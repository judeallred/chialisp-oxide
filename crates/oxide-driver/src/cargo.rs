//! Cargo wrapper protocol. Host units and compiler probes remain normal rustc.
use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use rustc_driver::Callbacks;

use crate::{Driver, configure, finish};

struct MetadataOnly;
impl Callbacks for MetadataOnly {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        configure(config, true);
    }
}

fn argument<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, argument)| {
        if argument == name {
            args.get(index + 1).map(String::as_str)
        } else {
            argument
                .strip_prefix(name)
                .and_then(|value| value.strip_prefix('='))
        }
    })
}

fn pass_through(compiler: &str, args: &[String]) -> ExitCode {
    match Command::new(compiler).args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("oxide-driver: run Cargo's rustc: {error}");
            ExitCode::FAILURE
        }
    }
}

pub fn run(mut args: Vec<String>) -> ExitCode {
    if args.is_empty() {
        eprintln!("oxide-driver wrapper requires Cargo's rustc executable");
        return ExitCode::FAILURE;
    }
    let compiler = args.remove(0);
    let probe = args.iter().any(|arg| {
        arg.starts_with("--print") || matches!(arg.as_str(), "-vV" | "-V" | "--version" | "--help")
    });
    if probe || argument(&args, "--target").is_none() {
        return pass_through(&compiler, &args);
    }
    let version = match Command::new(&compiler).arg("-vV").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => {
            eprintln!("oxide-driver: unable to query Cargo's compiler version");
            return ExitCode::FAILURE;
        }
    };
    if !version
        .lines()
        .any(|line| line.strip_prefix("commit-hash: ") == Some(env!("OXIDE_RUSTC_COMMIT")))
    {
        eprintln!(
            "oxide-driver: compiler commit mismatch; use the pinned compiler that built oxide-driver"
        );
        return ExitCode::FAILURE;
    }
    let selected_name = argument(&args, "--crate-name").unwrap_or("");
    let sdk = selected_name == "oxide_sdk";
    let selected = std::env::var_os("CARGO_PRIMARY_PACKAGE").is_some()
        && std::env::var("OXIDE_PRIMARY_PACKAGES")
            .ok()
            .is_none_or(|names| {
                names
                    .split(',')
                    .any(|name| name.replace('-', "_") == selected_name)
            });
    let output = if selected {
        match std::env::var_os("OXIDE_OUTPUT_DIR") {
            Some(path) => Some(PathBuf::from(path)),
            None => {
                eprintln!("oxide-driver: OXIDE_OUTPUT_DIR is required for selected exports");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    // All target dependencies must retain the same checked arithmetic semantics
    // and encode MIR, including an SDK outside the user's workspace.
    args.extend([
        "-Coverflow-checks=yes".into(),
        "-Cdebug-assertions=yes".into(),
    ]);
    if selected || sdk {
        args.extend(["-Funsafe-code".into(), "-Funstable-features".into()]);
    }
    if argument(&args, "--sysroot").is_none() {
        args.extend(["--sysroot".into(), env!("OXIDE_SYSROOT").into()]);
    }
    args.insert(0, compiler);
    if let Some(output) = output {
        let mut driver = Driver {
            entry: String::new(),
            output: None,
            cargo_output: Some(output),
            result: Err("Rust checking did not complete".into()),
        };
        finish(args, &mut driver)
    } else {
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut MetadataOnly))
    }
}
