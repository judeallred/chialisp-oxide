use anyhow::{Context, Result, bail, ensure};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Options {
    pub check: bool,
    pub optimize: bool,
    pub output: Option<PathBuf>,
    pub package: Option<String>,
    pub manifest: Option<PathBuf>,
    pub cargo: Vec<String>,
    pub metadata: Vec<String>,
    pub clean: Vec<String>,
}

impl Options {
    pub fn parse(mut args: Vec<String>) -> Result<Option<Self>> {
        if args.first().is_some_and(|arg| arg == "oxide") {
            args.remove(0);
        }
        let Some(command) = args.first().map(String::as_str) else {
            return Ok(None);
        };
        if matches!(command, "--help" | "-h" | "help") {
            return Ok(None);
        }
        ensure!(
            matches!(command, "build" | "check"),
            "expected build or check"
        );
        let mut options = Self {
            check: command == "check",
            optimize: true,
            output: None,
            package: None,
            manifest: None,
            cargo: vec![],
            metadata: vec![],
            clean: vec![],
        };
        let mut args = args.into_iter().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => return Ok(None),
                "--lib" => {} // This prototype always extracts a library target.
                "--no-optimize" => options.optimize = false,
                "--output-dir" => {
                    ensure!(options.output.is_none(), "--output-dir supplied twice");
                    options.output = Some(args.next().context("--output-dir needs a path")?.into());
                }
                "-p" | "--package" => {
                    ensure!(options.package.is_none(), "select exactly one package");
                    let value = args.next().context("--package needs a package name")?;
                    options.package = Some(value.clone());
                    options.cargo.extend(["--package".into(), value]);
                }
                "--manifest-path" => {
                    ensure!(options.manifest.is_none(), "--manifest-path supplied twice");
                    let value = args.next().context("--manifest-path needs a path")?;
                    options.manifest = Some(PathBuf::from(&value));
                    options.cargo.extend([arg.clone(), value.clone()]);
                    options.metadata.extend([arg, value]);
                }
                "--features" | "-F" | "--color" => {
                    let value = args
                        .next()
                        .with_context(|| format!("{arg} needs a value"))?;
                    options.cargo.extend([arg.clone(), value.clone()]);
                    options.metadata.extend([arg, value]);
                }
                "--profile" => {
                    let value = args.next().context("--profile needs a value")?;
                    options.cargo.extend([arg.clone(), value.clone()]);
                    options.clean.extend([arg, value]);
                }
                "--jobs" | "-j" => {
                    let value = args
                        .next()
                        .with_context(|| format!("{arg} needs a value"))?;
                    options.cargo.extend([arg, value]);
                }
                "--all-features" | "--no-default-features" => {
                    options.cargo.push(arg.clone());
                    options.metadata.push(arg);
                }
                "--locked" | "--offline" | "--frozen" => {
                    options.cargo.push(arg.clone());
                    options.metadata.push(arg.clone());
                    options.clean.push(arg);
                }
                "--release" => {
                    options.cargo.push(arg.clone());
                    options.clean.push(arg);
                }
                "--verbose" | "-v" | "-vv" | "--quiet" | "-q" => {
                    options.cargo.push(arg);
                }
                _ => bail!(
                    "unsupported cargo oxide option `{arg}`; this prototype selects one library \
                     package and supports the Cargo options shown by --help"
                ),
            }
        }
        Ok(Some(options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>> {
        Options::parse(args.iter().map(|value| (*value).into()).collect())
    }

    #[test]
    fn cargo_and_direct_invocation_preserve_graph_options() {
        for prefix in [vec!["oxide", "build"], vec!["build"]] {
            let mut args = prefix;
            args.extend([
                "-p",
                "payments",
                "--features",
                "recovery,strict",
                "--locked",
                "--lib",
                "--output-dir",
                "artifacts with spaces",
                "--no-optimize",
            ]);
            let options = parse(&args).unwrap().unwrap();
            assert_eq!(options.package.as_deref(), Some("payments"));
            assert_eq!(
                options.output.unwrap(),
                PathBuf::from("artifacts with spaces")
            );
            assert!(!options.optimize);
            assert_eq!(
                options.metadata,
                ["--features", "recovery,strict", "--locked"]
            );
            assert!(
                options
                    .cargo
                    .windows(2)
                    .any(|pair| pair == ["--package", "payments"])
            );
        }
    }

    #[test]
    fn rejects_ambiguous_or_unimplemented_target_modes() {
        for args in [
            vec!["build", "--target", "clvm"],
            vec!["build", "--workspace"],
            vec!["build", "-p", "a", "-p", "b"],
            vec!["check", "--manifest-path"],
            vec!["build", "--", "-Zanything"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
