#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

mod lower;

use std::{path::PathBuf, process::Command};

use oxide_ir::{Expr, Parameter, Program};
use rustc_driver::{Callbacks, Compilation};
use rustc_middle::ty::TyCtxt;

struct Driver {
    entry: String,
    output: Option<PathBuf>,
    result: Result<(), String>,
}

impl Callbacks for Driver {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        // Configure this internal pass directly rather than accepting arbitrary
        // -Z flags or enabling unstable language features in user puzzle crates.
        config.opts.unstable_opts.mir_opt_level = Some(0);
    }

    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        // The driver invokes analysis before this callback. Explicitly force every
        // local body's borrow-check query too; MIR extraction must never become a
        // shortcut around ordinary Rust checking after a rustc upgrade.
        for owner in tcx.hir_body_owners() {
            tcx.ensure_ok().mir_borrowck(owner);
        }
        tcx.dcx().abort_if_errors();
        self.result = self.extract(tcx);
        Compilation::Stop
    }
}

impl Driver {
    fn extract<'tcx>(&self, tcx: TyCtxt<'tcx>) -> Result<(), String> {
        let entries: Vec<_> = tcx
            .hir_body_owners()
            .filter(|id| {
                tcx.def_kind(*id) == rustc_hir::def::DefKind::Fn
                    && tcx.item_name(id.to_def_id()).as_str() == self.entry
            })
            .collect();
        if entries.len() != 1 {
            return Err(format!(
                "expected exactly one free function named `{}`, found {}",
                self.entry,
                entries.len()
            ));
        }
        let id = entries[0].to_def_id();
        let body = tcx.optimized_mir(id);
        let lowerer = lower::Lowerer::new(tcx);
        let parameters = body
            .args_iter()
            .enumerate()
            .map(|(index, local)| {
                Ok(Parameter {
                    name: body
                        .var_debug_info
                        .iter()
                        .find_map(|info| match &info.value {
                            rustc_middle::mir::VarDebugInfoContents::Place(place)
                                if place.local == local && place.projection.is_empty() =>
                            {
                                Some(info.name.to_string())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| format!("arg{index}")),
                    ty: lowerer.ty(body.local_decls[local].ty, body.span)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let program = Program {
            name: self.entry.clone(),
            result: lowerer.ty(body.return_ty(), body.span)?,
            body: lowerer.function(
                id,
                (0..parameters.len())
                    .map(|index| Expr::Arg { index })
                    .collect(),
                &mut Vec::new(),
            )?,
            parameters,
        };
        program.validate().map_err(|error| {
            format!(
                "{}: unsupported lowered program: {error:#}",
                tcx.sess.source_map().span_to_diagnostic_string(body.span)
            )
        })?;
        let json = serde_json::to_string_pretty(&program).map_err(|error| error.to_string())?;
        if let Some(path) = &self.output {
            std::fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())?;
        } else {
            println!("{json}");
        }
        Ok(())
    }
}

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);
    let Some(source) = arguments.next() else {
        usage();
    };
    if source == "--help" || source == "-h" {
        usage();
    }
    let mut driver = Driver {
        entry: "puzzle".into(),
        output: None,
        result: Err("Rust checking did not complete".into()),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--entry" => driver.entry = arguments.next().unwrap_or_else(|| usage()),
            "--output" => {
                driver.output = Some(arguments.next().unwrap_or_else(|| usage()).into());
            }
            _ => {
                eprintln!("unsupported driver argument: {argument}");
                usage();
            }
        }
    }
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("the pinned rustc must be available");
    if !sysroot.status.success() {
        eprintln!("unable to locate pinned rustc sysroot");
        std::process::exit(1);
    }
    let args = vec![
        "oxide-driver".into(),
        source,
        "--crate-name=oxide_puzzle".into(),
        "--crate-type=lib".into(),
        "--edition=2024".into(),
        "--sysroot".into(),
        String::from_utf8(sysroot.stdout).unwrap().trim().into(),
        "-Cpanic=abort".into(),
        "-Coverflow-checks=yes".into(),
        "-Cdebug-assertions=yes".into(),
        "-Funsafe-code".into(),
        "--emit=metadata".into(),
    ];
    let status =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut driver));
    if status != std::process::ExitCode::SUCCESS {
        return status;
    }
    if let Err(error) = driver.result {
        eprintln!("error: chialisp-oxide: {error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn usage() -> ! {
    eprintln!("usage: oxide-driver SOURCE.rs [--entry puzzle] [--output program.json]");
    std::process::exit(2)
}
