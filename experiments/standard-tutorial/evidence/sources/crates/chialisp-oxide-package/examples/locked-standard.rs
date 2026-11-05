//! Host-only standard binding around an actual retrieved inner program.
//! WIP: VM conditions are not signature validation or a complete coin spend.
use chialisp_oxide_package::{
    Error, ErrorKind, Limits,
    artifact::{Encoding, inspect_artifact},
    dependency::read_lock,
    standard::{StandardModule, SyntheticPublicKey},
    store::Registry,
};
use chialisp_oxide_vm::{FAILURE_PROTOCOL, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::json;
use std::{env, path::Path, process::ExitCode};

fn execute() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let arguments: Vec<_> = env::args_os().skip(1).take(7).collect();
    if arguments.len() != 6 {
        return Err("usage: locked-standard LOCK REGISTRY STANDARD_ALIAS INNER_ALIAS SYNTHETIC_KEY_HEX INNER_SOLUTION_HEX".into());
    }
    let standard_alias = arguments[2].to_str().ok_or("alias must be UTF-8")?;
    let inner_alias = arguments[3].to_str().ok_or("alias must be UTF-8")?;
    let key_hex = arguments[4].to_str().ok_or("key hex must be UTF-8")?;
    let solution_hex = arguments[5].to_str().ok_or("solution hex must be UTF-8")?;
    if key_hex.len() != 96 || solution_hex.len() > 128 * 1024 {
        return Err(Error::new(
            ErrorKind::InvalidManifest,
            "key or solution transport length is invalid",
        )
        .into());
    }
    let key = SyntheticPublicKey::from_bytes(
        hex::decode(key_hex)?
            .try_into()
            .map_err(|_| "expected 48 synthetic public key bytes")?,
    )?;
    let limits = Limits::default();
    let registry = Registry::open(Path::new(&arguments[1]), limits.clone())?;
    let lock = read_lock(Path::new(&arguments[0]))?;
    let dependencies = lock.resolve(&registry)?;
    let find = |alias| {
        dependencies.get(alias).ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownObject,
                "requested dependency alias is absent",
            )
        })
    };
    let standard = find(standard_alias)?;
    let inner = find(inner_alias)?;
    let module = StandardModule::from_package(standard, &limits)?;
    let instance = module.curry(&key, &limits)?;
    let solution_profile = Limits {
        max_blob_bytes: 64 * 1024,
        max_atom_bytes: 64 * 1024,
        max_expanded_bytes: 64 * 1024,
        max_depth: 256,
        ..limits.clone()
    };
    let inner_solution = inspect_artifact(
        &hex::decode(solution_hex)?,
        Encoding::Plain,
        &solution_profile,
    )?;
    let solution = instance.delegated_solution(inner.artifact(), &inner_solution, &limits)?;

    let mut allocator = Allocator::new_limited(8 * 1024 * 1024);
    let program = node_from_bytes(&mut allocator, instance.artifact().bytes())?;
    let environment = node_from_bytes(&mut allocator, solution.bytes())?;
    let common = json!({
        "module_object_id": module.object_id(),
        "inner_object_id": inner.object_id(),
        "synthetic_public_key_hex": hex::encode(key.to_bytes()),
        "reveal": instance.artifact().identity(),
        "solution": solution.identity(),
        "reveal_hex": hex::encode(instance.artifact().bytes()),
        "solution_hex": hex::encode(solution.bytes()),
        "vm_failure_protocol": FAILURE_PROTOCOL,
        "scope": "host_standard_binding_vm_only",
    });
    let mut output = common.as_object().expect("constructed object").clone();
    let exit = match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        10_000_000,
    ) {
        Ok(result) => {
            output.insert("status".into(), json!("executed"));
            output.insert("cost".into(), json!(result.0));
            output.insert(
                "result_hex".into(),
                json!(hex::encode(node_to_bytes(&allocator, result.1)?)),
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            output.insert("status".into(), json!("vm_error"));
            output.insert("failure".into(), json!(classify_failure(&error)));
            ExitCode::FAILURE
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(exit)
}

fn main() -> ExitCode {
    match execute() {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("locked-standard: {error}");
            let kind = error.downcast_ref::<Error>().map(|error| error.kind());
            println!(
                "{}",
                json!({"status": "host_error", "kind": kind, "error": error.to_string()})
            );
            ExitCode::FAILURE
        }
    }
}
