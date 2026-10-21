//! Offline package operations dispatch before any compiler/wrapper inspection.
use chialisp_oxide_package::{
    Digest32, Error, ErrorKind, Limits, Result,
    bundle::import_bundle,
    dependency::{self, add_local},
    inspect,
    package::{VERIFICATION_SCOPE, verify_package_file},
    store::Registry,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

const USAGE: &str = concat!(
    "WIP prototype. Not ready for use or real funds.\n\n",
    "cargo chialisp-oxide registry publish --bundle IMMUTABLE/manifest.json --entry RUST_EXPORT --registry DIR [--json]\n",
    "cargo chialisp-oxide registry publish --package ENVELOPE.json --registry DIR [--json]\n",
    "cargo chialisp-oxide registry verify --package ENVELOPE.json [--json]\n",
    "cargo chialisp-oxide registry lookup --module-hash LOWERCASE_HEX --registry DIR [--json]\n",
    "cargo chialisp-oxide registry get --object-id LOWERCASE_HEX --registry DIR [--json]\n",
    "cargo chialisp-oxide registry add --name ASCII_ALIAS --module-hash LOWERCASE_HEX [--object-id LOWERCASE_HEX] --registry DIR --lock PATH [--json]\n",
    "cargo chialisp-oxide registry inspect --clvm-hex-file PATH [--expected-puzzle-hash LOWERCASE_HEX] [--registry DIR] [--json]\n\n",
    "Publish writes only the explicitly selected local registry. ",
    "Add pins one exact local object in the explicitly selected consumer lock; ambiguous hashes require --object-id. ",
    "Add writes are reviewed for macOS/Linux local POSIX filesystems; non-Unix add writes are unsupported. ",
    "Add does not create a registry, change Cargo.toml/Cargo.lock, infer a binding or affect build/check. ",
    "Inspect reads an explicit local reveal. An optional --registry looks up only its extracted or recognized module; no store is opened for an unclassified reveal without a module. ",
    "The full reveal puzzle hash and an extracted module hash are distinct; recognition does not verify a chain observation or spend. ",
    "No registry operation runs Cargo, rustup, a compiler, VM, build script or network client. ",
    "Bundle metadata and optional source attachments are declarations; identity checks do not prove source/IR/CLVM equivalence or security.",
);

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}
struct Options {
    command: String,
    values: BTreeMap<String, OsString>,
    json: bool,
}
impl Options {
    fn parse(args: &[OsString]) -> Result<Self> {
        let command = args
            .first()
            .ok_or_else(|| invalid("registry needs a command"))?
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath, "registry command must be UTF-8"))?
            .to_owned();
        if !["publish", "verify", "lookup", "get", "add", "inspect"].contains(&command.as_str()) {
            return Err(invalid(format!("unknown registry command {command}")));
        }
        let mut values = BTreeMap::new();
        let mut machine = false;
        let mut args = args.iter().skip(1);
        while let Some(raw_arg) = args.next() {
            let arg = raw_arg.to_str().ok_or_else(|| {
                Error::new(ErrorKind::InvalidPath, "registry option name must be UTF-8")
            })?;
            if arg == "--json" {
                if machine {
                    return Err(invalid("--json supplied twice"));
                }
                machine = true;
                continue;
            }
            if ![
                "--bundle",
                "--entry",
                "--registry",
                "--package",
                "--module-hash",
                "--object-id",
                "--name",
                "--lock",
                "--clvm-hex-file",
                "--expected-puzzle-hash",
            ]
            .contains(&arg)
            {
                return Err(invalid(format!("unknown registry option {arg}")));
            }
            let value = args
                .next()
                .filter(|v| !v.to_str().is_some_and(|s| s.starts_with("--")))
                .ok_or_else(|| invalid(format!("{arg} needs a value")))?;
            if value.is_empty() || values.insert(arg.to_owned(), value.clone()).is_some() {
                return Err(invalid(format!("empty or duplicate registry option {arg}")));
            }
        }
        let allowed: &[&str] = match command.as_str() {
            "publish" => &["--bundle", "--entry", "--registry", "--package"],
            "verify" => &["--package"],
            "lookup" => &["--module-hash", "--registry"],
            "get" => &["--object-id", "--registry"],
            "add" => &[
                "--name",
                "--module-hash",
                "--object-id",
                "--registry",
                "--lock",
            ],
            "inspect" => &["--clvm-hex-file", "--expected-puzzle-hash", "--registry"],
            _ => unreachable!(),
        };
        if values.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(invalid("option does not belong to this registry command"));
        }
        Ok(Self {
            command,
            values,
            json: machine,
        })
    }
    fn value(&self, name: &str) -> Result<&str> {
        self.values
            .get(name)
            .ok_or_else(|| invalid(format!("{} requires {name}", self.command)))?
            .to_str()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidPath,
                    format!("{name} identity must be UTF-8"),
                )
            })
    }
    fn path(&self, name: &str) -> Result<PathBuf> {
        self.values
            .get(name)
            .map(PathBuf::from)
            .ok_or_else(|| invalid(format!("{} requires {name}", self.command)))
    }
}
fn execute(options: &Options) -> Result<Value> {
    let limits = Limits::default();
    match options.command.as_str() {
        "verify" => {
            let package = verify_package_file(&options.path("--package")?, &limits)?;
            Ok(json!({"status":"verified","package":package.summary()}))
        }
        "publish" => {
            let from_bundle = options.values.contains_key("--bundle");
            let from_package = options.values.contains_key("--package");
            if from_bundle == from_package {
                return Err(invalid(
                    "publish requires exactly one of --bundle or --package",
                ));
            }
            if !from_bundle && options.values.contains_key("--entry") {
                return Err(invalid("--entry belongs only to --bundle import"));
            }
            // Read/admit the entire input before creating the explicitly chosen
            // local registry. No failed import may publish a partial envelope.
            let package = if from_bundle {
                import_bundle(
                    &options.path("--bundle")?,
                    options.value("--entry")?,
                    &limits,
                )?
            } else {
                verify_package_file(&options.path("--package")?, &limits)?
            };
            let registry = Registry::create(&options.path("--registry")?, limits)?;
            let outcome = registry.publish(&package)?;
            Ok(
                json!({"status":"published_or_already_present","publication":outcome,"package":package.summary()}),
            )
        }
        "lookup" => {
            let hash: Digest32 = options.value("--module-hash")?.parse()?;
            let registry = Registry::open(&options.path("--registry")?, limits)?;
            let packages = registry.lookup(hash)?;
            Ok(
                json!({"status":"lookup_complete","module_tree_hash":hash,"packages":packages.iter().map(|p|p.summary()).collect::<Vec<_>>(),"scope":VERIFICATION_SCOPE}),
            )
        }
        "get" => {
            let id: Digest32 = options.value("--object-id")?.parse()?;
            let registry = Registry::open(&options.path("--registry")?, limits)?;
            Ok(json!({"status":"verified","package":registry.get(id)?.summary()}))
        }
        "inspect" => {
            let expected = options
                .values
                .contains_key("--expected-puzzle-hash")
                .then(|| options.value("--expected-puzzle-hash")?.parse::<Digest32>())
                .transpose()?;
            let bytes = inspect::read_hex_file(&options.path("--clvm-hex-file")?, &limits)?;
            let inspection = inspect::inspect_reveal(&bytes, &limits, expected)?;
            // Verify the complete reveal and optional full puzzle hash before
            // consulting an explicit store. Module lookup never aliases the
            // full curried puzzle hash to a module-only package.
            let module_lookup = if let Some(module) = inspection.module()
                && options.values.contains_key("--registry")
            {
                let module_hash = module.identity().module_tree_hash;
                let registry = Registry::open(&options.path("--registry")?, limits)?;
                let packages = registry.lookup(module_hash)?;
                Some(json!({"module_tree_hash":module_hash,
                    "packages":packages.iter().map(|package|package.summary()).collect::<Vec<_>>()}))
            } else {
                None
            };
            Ok(json!({"status":"inspected","inspection":inspection,"module_lookup":module_lookup}))
        }
        "add" => {
            let name = options.value("--name")?;
            let hash: Digest32 = options.value("--module-hash")?.parse()?;
            let object_id = options
                .values
                .contains_key("--object-id")
                .then(|| options.value("--object-id")?.parse::<Digest32>())
                .transpose()?;
            let lock = options.path("--lock")?;
            let registry = Registry::open(&options.path("--registry")?, limits)?;
            let outcome = add_local(&registry, &lock, name, hash, object_id)?;
            Ok(json!({"status":outcome.status,"pin":outcome.pin,
                "lock_sha256":outcome.lock_sha256,"dependency_count":outcome.dependency_count,
                "scope":dependency::VERIFICATION_SCOPE}))
        }
        _ => Err(invalid("unknown registry command")),
    }
}
/// Return None for the existing build/check path; this function has no compiler
/// or driver side effects, including in help and error paths.
pub(super) fn dispatch(args: &[OsString]) -> Option<anyhow::Result<()>> {
    let args = if args.first().is_some_and(|a| a == "chialisp-oxide") {
        &args[1..]
    } else {
        args
    };
    if args.first().is_none_or(|arg| arg != "registry") {
        return None;
    }
    let args = &args[1..];
    let machine = args.iter().any(|arg| arg == "--json");
    if args.is_empty()
        || args.first().is_some_and(|arg| arg == "help")
        || args
            .iter()
            .any(|arg| matches!(arg.to_str(), Some("--help" | "-h")))
    {
        if machine {
            println!("{}", json!({"status":"help","usage":USAGE}));
        } else {
            println!("{USAGE}");
        }
        return Some(Ok(()));
    }
    let result = Options::parse(args)
        .and_then(|options| execute(&options).map(|value| (value, options.json)));
    Some(match result {
        Ok((value, machine)) => {
            if machine {
                println!("{value}");
            } else {
                println!(
                    "{}\n{VERIFICATION_SCOPE}",
                    serde_json::to_string_pretty(&value).expect("registry result is JSON")
                );
            }
            Ok(())
        }
        Err(error) => {
            if machine {
                println!(
                    "{}",
                    json!({"status":"error","kind":error.kind(),"message":error.to_string(),"scope":VERIFICATION_SCOPE})
                );
            }
            Err(anyhow::Error::new(error))
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(|v| (*v).into()).collect()
    }
    #[test]
    fn parsing_closes_command_flags_and_duplicates() {
        for values in [
            vec!["verify", "--registry", "x"],
            vec!["lookup", "--json", "--json"],
            vec!["publish", "--bundle", "x", "--bundle", "y"],
            vec!["get", "--object-id"],
            vec!["unknown"],
            vec!["verify", "--wat", "x"],
            vec!["add", "--name", "payment", "--name", "other"],
            vec!["add", "--lock"],
            vec!["add", "--entry", "payment"],
            vec!["add", "--binding", "guessed_type"],
            vec!["lookup", "--lock", "consumer.lock"],
            vec!["get", "--name", "payment"],
            vec!["inspect", "--lock", "consumer.lock"],
            vec!["inspect", "--package", "input.json"],
            vec!["inspect", "--clvm-hex-file"],
            vec![
                "inspect",
                "--clvm-hex-file",
                "a.hex",
                "--clvm-hex-file",
                "b.hex",
            ],
            vec!["inspect", "--expected-puzzle-hash"],
            vec!["lookup", "--clvm-hex-file", "input.hex"],
        ] {
            assert!(Options::parse(&args(&values)).is_err(), "{values:?}");
        }
        assert!(
            Options::parse(&args(&[
                "lookup",
                "--module-hash",
                "00",
                "--registry",
                "local",
                "--json"
            ]))
            .unwrap()
            .json
        );
        assert!(dispatch(&args(&["build", "--json"])).is_none());
        let add = Options::parse(&args(&[
            "add",
            "--name",
            "payment",
            "--module-hash",
            "00",
            "--registry",
            "local",
            "--lock",
            "consumer.lock",
            "--json",
        ]))
        .unwrap();
        assert_eq!(add.command, "add");
        assert_eq!(add.value("--name").unwrap(), "payment");
        assert_eq!(add.path("--lock").unwrap(), PathBuf::from("consumer.lock"));
        assert!(!add.values.contains_key("--object-id"));
        assert!(add.json);
        let inspect = Options::parse(&args(&[
            "inspect",
            "--clvm-hex-file",
            "input.hex",
            "--json",
        ]))
        .unwrap();
        assert_eq!(inspect.command, "inspect");
        assert_eq!(
            inspect.path("--clvm-hex-file").unwrap(),
            PathBuf::from("input.hex")
        );
        assert!(!inspect.values.contains_key("--registry"));
    }
}
