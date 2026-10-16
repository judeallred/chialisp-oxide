#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

mod cargo;
mod exports;
mod lower;

use std::{io::Write, path::PathBuf};

use chialisp_oxide_ir::{Environment, Expr, Parameter, Program};
use rustc_driver::{Callbacks, Compilation};
use rustc_hir::def_id::DefId;
use rustc_middle::ty::TyCtxt;
use sha2::{Digest, Sha256};

struct Driver {
    entry: String,
    output: Option<PathBuf>,
    cargo_output: Option<PathBuf>,
    result: Result<(), String>,
}

fn configure(config: &mut rustc_interface::interface::Config, encode_dependencies: bool) {
    config.opts.unstable_opts.mir_opt_level = Some(0);
    config.opts.unstable_opts.always_encode_mir = encode_dependencies;
}

impl Callbacks for Driver {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        configure(config, self.cargo_output.is_some());
    }

    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        // Analysis runs before this callback. Explicitly force each typeck
        // root's borrow check; that query also checks all of its nested bodies.
        // Querying a closure directly violates rustc's mir_borrowck contract.
        for owner in tcx.hir_body_owners() {
            if !tcx.is_typeck_child(owner.to_def_id()) {
                tcx.ensure_ok().mir_borrowck(owner);
            }
        }
        tcx.dcx().abort_if_errors();
        self.result = self.extract(tcx);
        if self.cargo_output.is_some() && self.result.is_ok() {
            // Cargo needs the normal metadata for downstream crates. Artifacts
            // become publishable only after the whole Cargo command succeeds.
            Compilation::Continue
        } else {
            Compilation::Stop
        }
    }
}

impl Driver {
    fn extract<'tcx>(&self, tcx: TyCtxt<'tcx>) -> Result<(), String> {
        if let Some(output) = &self.cargo_output {
            let mut programs = Vec::new();
            for (id, environment) in exports::collect(tcx)? {
                let name = format!(
                    "{}::{}",
                    tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE),
                    tcx.def_path_str(id)
                );
                programs.push(self.extract_program(tcx, id, name, environment)?);
            }
            programs.sort_by(|a, b| a.name.cmp(&b.name));
            std::fs::create_dir_all(output).map_err(|error| error.to_string())?;
            for program in programs {
                // Hash the complete path rather than replacing separators:
                // `a__b` and `a::b` must never overwrite one another.
                let path = output.join(format!(
                    "{:x}.ir.json",
                    Sha256::digest(program.name.as_bytes())
                ));
                let json =
                    serde_json::to_vec_pretty(&program).map_err(|error| error.to_string())?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| format!("create unique export {}: {error}", path.display()))?;
                file.write_all(&json).map_err(|error| error.to_string())?;
            }
            return Ok(());
        }
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
        let environment = exports::environment(tcx, entries[0])?.unwrap_or_default();
        let program =
            self.extract_program(tcx, entries[0].to_def_id(), self.entry.clone(), environment)?;
        let json = serde_json::to_string_pretty(&program).map_err(|error| error.to_string())?;
        if let Some(path) = &self.output {
            std::fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())?;
        } else {
            println!("{json}");
        }
        Ok(())
    }

    fn extract_program<'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        id: DefId,
        name: String,
        environment: Environment,
    ) -> Result<Program, String> {
        let body = tcx.optimized_mir(id);
        if environment == Environment::Ignored && body.arg_count != 0 {
            return Err(format!(
                "{}: environment = \"ignored\" requires a function with no parameters",
                tcx.sess.source_map().span_to_diagnostic_string(body.span)
            ));
        }
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
            name,
            environment,
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
        Ok(program)
    }
}

fn main() -> std::process::ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() == 1 && arguments[0] == "--compiler-version" {
        println!("{}", env!("CHIALISP_OXIDE_RUSTC_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    if arguments.len() == 1 && arguments[0] == "--version" {
        println!(
            "chialisp-oxide-driver {} ({})",
            env!("CARGO_PKG_VERSION"),
            env!("CHIALISP_OXIDE_RUSTC_VERSION")
        );
        return std::process::ExitCode::SUCCESS;
    }
    let explicit_wrapper = arguments
        .first()
        .is_some_and(|arg| arg == "--cargo-wrapper");
    if explicit_wrapper {
        arguments.remove(0);
    }
    if explicit_wrapper || std::env::var_os("CHIALISP_OXIDE_CARGO_MODE").is_some() {
        return cargo::run(arguments);
    }
    standalone(arguments)
}

fn standalone(arguments: Vec<String>) -> std::process::ExitCode {
    let mut arguments = arguments.into_iter();
    let Some(source) = arguments.next() else {
        usage();
    };
    if source == "--help" || source == "-h" {
        usage();
    }
    let mut driver = Driver {
        entry: "puzzle".into(),
        output: None,
        cargo_output: None,
        result: Err("Rust checking did not complete".into()),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--entry" => driver.entry = arguments.next().unwrap_or_else(|| usage()),
            "--output" => driver.output = Some(arguments.next().unwrap_or_else(|| usage()).into()),
            _ => {
                eprintln!("unsupported driver argument: {argument}");
                usage();
            }
        }
    }
    let args = vec![
        "chialisp-oxide-driver".into(),
        source,
        "--crate-name=chialisp_oxide_puzzle".into(),
        "--crate-type=lib".into(),
        "--edition=2024".into(),
        "--sysroot".into(),
        env!("CHIALISP_OXIDE_SYSROOT").into(),
        "-Cpanic=abort".into(),
        "-Coverflow-checks=yes".into(),
        "-Cdebug-assertions=yes".into(),
        "-Funsafe-code".into(),
        "-Funstable-features".into(),
        "--emit=metadata".into(),
    ];
    finish(args, &mut driver)
}

fn finish(args: Vec<String>, driver: &mut Driver) -> std::process::ExitCode {
    let status = rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, driver));
    if status != std::process::ExitCode::SUCCESS {
        return status;
    }
    if let Err(error) = &driver.result {
        eprintln!("error: chialisp-oxide: {error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn usage() -> ! {
    eprintln!("usage: chialisp-oxide-driver SOURCE.rs [--entry puzzle] [--output program.json]");
    std::process::exit(2)
}
