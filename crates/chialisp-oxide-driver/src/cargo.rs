//! Cargo wrapper protocol. Host units and read-only probes do not publish IR.
use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use rustc_driver::{Callbacks, Compilation};
use rustc_middle::ty::TyCtxt;
use rustc_session::{EarlyDiagCtxt, config::rustc_optgroups};

use crate::{Driver, configure, finish, toolchain};

struct MetadataOnly {
    route: toolchain::Route,
}
impl Callbacks for MetadataOnly {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        configure(config, true, Some(&self.route));
    }

    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        if let Err(error) = toolchain::loaded(tcx) {
            tcx.dcx().fatal(error);
        }
        Compilation::Continue
    }
}

fn pass_through(compiler: &str, args: &[String]) -> ExitCode {
    match Command::new(compiler).args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("chialisp-oxide-driver: run Cargo's rustc: {error}");
            ExitCode::FAILURE
        }
    }
}

fn codegen_help(flags: &[String]) -> bool {
    let mut cpu = None;
    // Pinned rustc's parse_target_feature appends a comma whenever its existing
    // string is nonempty. Model only the three states needed for `== "help"`,
    // without allocating a duplicate concatenation of arbitrary feature flags.
    let mut feature = "";
    for flag in flags {
        let (key, value) = flag
            .split_once('=')
            .map_or((flag.as_str(), None), |(key, value)| (key, Some(value)));
        match key {
            "target-cpu" | "target_cpu" => cpu = value,
            "target-feature" | "target_feature" => {
                feature = match (feature, value) {
                    ("", Some("")) => "",
                    ("", Some("help")) => "help",
                    _ => "other",
                };
            }
            _ => {}
        }
    }
    cpu == Some("help") || feature == "help"
}

pub fn run(args: Vec<String>) -> ExitCode {
    // Expansion errors are ordinary compiler diagnostics, not an uncaught fatal
    // panic. The outer catch also covers failures before run_compiler starts.
    let mut status = ExitCode::FAILURE;
    let caught = rustc_driver::catch_with_exit_code(|| status = run_inner(args));
    if caught == ExitCode::SUCCESS {
        status
    } else {
        caught
    }
}

fn run_inner(mut args: Vec<String>) -> ExitCode {
    if args.is_empty() {
        eprintln!("chialisp-oxide-driver wrapper requires Cargo's rustc executable");
        return ExitCode::FAILURE;
    }
    let compiler = args.remove(0);
    let early = EarlyDiagCtxt::new(Default::default());
    // This copy is only for routing. Pass the ORIGINAL arguments onward so a
    // literal @ token inside a response file is not expanded a second time.
    // Routing reads response files separately from compilation; their stability
    // is trusted build configuration, not an authenticated filesystem snapshot.
    let routing = rustc_driver::args::arg_expand_all(&early, &args);
    let mut options = rustc_session::getopts::Options::new();
    for group in rustc_optgroups() {
        group.apply(&mut options);
    }
    let parsed = options
        .parse(&routing)
        .unwrap_or_else(|error| early.early_fatal(error.to_string()));
    let target = parsed.opt_str("target");
    if let Some(target) = &target
        && let Err(error) = toolchain::early_target(target, parsed.opt_str("sysroot").as_deref())
    {
        early.early_fatal(error);
    }
    // Some native early exits (-vV and -Cpasses=list) load a backend before
    // Callbacks::config. Reject target-unit overrides before invoking them too.
    if target.is_some()
        && parsed.opt_strs("Z").iter().any(|flag| {
            flag.starts_with("codegen-backend=") || flag.starts_with("codegen_backend=")
        })
    {
        early.early_fatal("unsupported toolchain configuration: explicit codegen backend overrides are not supported");
    }
    let probe = codegen_help(&parsed.opt_strs("C"))
        || parsed.opt_present("version")
        || parsed.opt_present("help")
        || parsed.opt_present("h")
        || parsed.opt_present("explain")
        || parsed
            .opt_strs("C")
            .iter()
            .any(|flag| matches!(flag.as_str(), "help" | "passes=list"))
        || parsed.opt_strs("Z").iter().any(|flag| flag == "help")
        || ["allow", "warn", "force-warn", "deny", "forbid"]
            .iter()
            .any(|level| parsed.opt_strs(level).iter().any(|flag| flag == "help"))
        || parsed.opt_strs("W").iter().any(|flag| flag == "all")
        || parsed.opt_strs("print").iter().any(|request| {
            // These two print requests continue compilation. Other requests
            // stop before analysis, including mixed requests with either one.
            !matches!(
                request.split('=').next(),
                Some("native-static-libs" | "link-args")
            )
        });
    if probe || target.is_none() {
        return pass_through(&compiler, &args);
    }
    let version = match Command::new(&compiler).arg("-vV").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => {
            eprintln!("chialisp-oxide-driver: unable to query Cargo's compiler version");
            return ExitCode::FAILURE;
        }
    };
    if !version
        .lines()
        .any(|line| line.strip_prefix("commit-hash: ") == Some(env!("CHIALISP_OXIDE_RUSTC_COMMIT")))
    {
        eprintln!(
            "chialisp-oxide-driver: compiler commit mismatch; use the pinned compiler that built chialisp-oxide-driver"
        );
        return ExitCode::FAILURE;
    }
    let selected_name = parsed.opt_str("crate-name");
    let sdk = selected_name.as_deref() == Some("chialisp_oxide");
    let selected = std::env::var_os("CARGO_PRIMARY_PACKAGE").is_some()
        && std::env::var("CHIALISP_OXIDE_PRIMARY_PACKAGES")
            .ok()
            .is_none_or(|names| {
                names
                    .split(',')
                    .any(|name| Some(name.replace('-', "_").as_str()) == selected_name.as_deref())
            });
    let output = if selected {
        match std::env::var_os("CHIALISP_OXIDE_OUTPUT_DIR") {
            Some(path) => Some(PathBuf::from(path)),
            None => {
                eprintln!(
                    "chialisp-oxide-driver: CHIALISP_OXIDE_OUTPUT_DIR is required for selected exports"
                );
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    // All target dependencies retain checked arithmetic and encode MIR.
    args.extend([
        "-Coverflow-checks=yes".into(),
        "-Cdebug-assertions=yes".into(),
    ]);
    if selected || sdk {
        args.extend(["-Funsafe-code".into(), "-Funstable-features".into()]);
    }
    let route = toolchain::Route {
        target: target.unwrap(),
        crate_name: selected_name,
    };
    args.insert(0, compiler);
    if let Some(output) = output {
        let mut driver = Driver {
            entry: String::new(),
            output: None,
            cargo_output: Some(output),
            route: Some(route),
            result: Err("Rust checking did not complete".into()),
        };
        finish(args, &mut driver)
    } else {
        rustc_driver::catch_with_exit_code(|| {
            rustc_driver::run_compiler(&args, &mut MetadataOnly { route })
        })
    }
}
