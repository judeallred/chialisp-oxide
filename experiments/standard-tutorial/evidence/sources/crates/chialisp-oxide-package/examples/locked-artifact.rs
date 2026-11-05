//! Host-only consumer of exact bytes resolved from a portable dependency lock.
//!
//! WIP. This executes arbitrary local CLVM with bounded host resources. It is
//! neither a typed on-chain Rust dependency nor a complete coin-spend check.
use chialisp_oxide_package::{
    Error, ErrorKind, Limits,
    artifact::{Encoding, inspect_artifact},
    dependency::read_lock,
    store::Registry,
};
use chialisp_oxide_vm::{FAILURE_PROTOCOL, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::json;
use std::{env, ffi::OsString, path::Path, process::ExitCode};

const MAX_SOLUTION_BYTES: usize = 64 * 1024;
const MAX_VM_COST: u64 = 10_000_000;
const MAX_ATOM_HEAP: usize = 8 * 1024 * 1024;

fn argument(arguments: &mut impl Iterator<Item = OsString>) -> Result<OsString, &'static str> {
    arguments
        .next()
        .ok_or("usage: locked-artifact LOCK_PATH REGISTRY_DIRECTORY DEPENDENCY_NAME SOLUTION_HEX")
}

fn execute() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let lock_path = argument(&mut arguments)?;
    let registry_path = argument(&mut arguments)?;
    let name = argument(&mut arguments)?;
    let solution_hex = argument(&mut arguments)?;
    if arguments.next().is_some() {
        return Err("expected exactly four arguments".into());
    }
    let name = name.to_str().ok_or("dependency name must be UTF-8")?;
    let solution_hex = solution_hex.to_str().ok_or("solution hex must be UTF-8")?;
    if solution_hex.len() > MAX_SOLUTION_BYTES * 2 {
        return Err("solution exceeds the host example's 64 KiB limit".into());
    }
    let solution_bytes = hex::decode(solution_hex)?;
    let limits = Limits {
        max_blob_bytes: MAX_SOLUTION_BYTES,
        max_atom_bytes: MAX_SOLUTION_BYTES,
        max_expanded_bytes: MAX_SOLUTION_BYTES,
        max_depth: 256,
        ..Limits::default()
    };
    // Reuse bounded complete canonical-tree admission before the VM decoder.
    let solution = inspect_artifact(&solution_bytes, Encoding::Plain, &limits)?;
    let registry = Registry::open(Path::new(&registry_path), Limits::default())?;
    let lock = read_lock(Path::new(&lock_path))?;
    let resolved = lock.resolve(&registry)?;
    let dependency = resolved.get(name).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownObject,
            "dependency alias is absent from the lock",
        )
    })?;

    let mut allocator = Allocator::new_limited(MAX_ATOM_HEAP);
    // These are the actual retrieved bytes. There is no embedded puzzle or
    // fallback construction when the locked object is missing or corrupted.
    let program = node_from_bytes(&mut allocator, dependency.artifact().bytes())?;
    let environment = node_from_bytes(&mut allocator, solution.bytes())?;
    let common = json!({
        "dependency": name,
        "object_id": dependency.object_id(),
        "artifact": dependency.artifact().identity(),
        "max_vm_cost": MAX_VM_COST,
        "vm_failure_protocol": FAILURE_PROTOCOL,
        "scope": "host_vm_execution_of_exact_locked_artifact",
    });
    let mut output = common.as_object().expect("constructed object").clone();
    let status = match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        MAX_VM_COST,
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
    Ok(status)
}

fn main() -> ExitCode {
    match execute() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("locked-artifact: {error}");
            let kind = error.downcast_ref::<Error>().map(|error| error.kind());
            println!(
                "{}",
                json!({"status": "host_error", "kind": kind,
                                  "error": error.to_string()})
            );
            ExitCode::FAILURE
        }
    }
}
