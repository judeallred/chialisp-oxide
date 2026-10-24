//! Explicit local reference compilation / primary VM phases; no SDK execution.
use chialisp_oxide_vm::classify_failure;
use clvmr::{
    Allocator, ChiaDialect,
    chia_dialect::MEMPOOL_MODE,
    run_program,
    serde::{node_from_bytes, node_to_bytes_limit},
};
use serde_json::{Value, json};
use std::{env, fs, io::Read, path::Path};

const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_COST: u64 = 50_000_000;

fn read(path: &Path) -> Vec<u8> {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert!(metadata.len() <= MAX_FILE as u64);
    let mut bytes = Vec::new();
    fs::File::open(path)
        .unwrap()
        .take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= MAX_FILE);
    bytes
}

// Canonical plain serialization preflight before CLVM node allocation. The
// compiled transport profile is distinct from the narrower source-vector one.
fn preflight(raw: &[u8], max_bytes: usize, max_nodes: usize, max_depth: usize) {
    assert!(!raw.is_empty() && raw.len() <= max_bytes);
    let mut pending = vec![1usize];
    let mut cursor = 0usize;
    let mut nodes = 0usize;
    let mut work = raw.len();
    while let Some(depth) = pending.pop() {
        nodes += 1;
        work = work.checked_add(192).unwrap();
        assert!(nodes <= max_nodes && depth <= max_depth && work <= 16 * 1024 * 1024);
        let head = *raw.get(cursor).unwrap();
        cursor += 1;
        if head == 255 {
            assert!(depth < max_depth && nodes + pending.len() + 2 <= max_nodes);
            pending.extend([depth + 1, depth + 1]);
        } else if head >= 128 {
            let width = head.leading_ones() as usize;
            assert!(width <= 5);
            let mut length = (head & (127 >> width)) as usize;
            for _ in 1..width {
                length = length
                    .checked_mul(256)
                    .unwrap()
                    .checked_add(*raw.get(cursor).unwrap() as usize)
                    .unwrap();
                cursor += 1;
            }
            assert!(length <= max_bytes && cursor.checked_add(length).unwrap() <= raw.len());
            let minimum = [0, 0, 64, 8192, 1_048_576, 134_217_728][width];
            assert!(length >= minimum);
            assert!(!(length == 1 && raw[cursor] < 128));
            work = work.checked_add(length).unwrap();
            assert!(work <= 16 * 1024 * 1024);
            cursor += length;
        }
    }
    assert_eq!(cursor, raw.len(), "trailing serialized bytes");
}

fn decode_hex(value: &Value, bytes: usize, nodes: usize, depth: usize) -> Vec<u8> {
    let text = value.as_str().unwrap();
    assert!(!text.is_empty() && text.len() <= bytes * 2 && text.len().is_multiple_of(2));
    assert!(
        text.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let raw = hex::decode(text).unwrap();
    preflight(&raw, bytes, nodes, depth);
    raw
}

fn main() {
    let args: Vec<_> = env::args().collect();
    assert_eq!(args.len(), 3);
    match args[1].as_str() {
        "compile-reference" => {
            let root = Path::new(&args[2]);
            let mut modules = Vec::new();
            for export in ["increment", "reset"] {
                let source = String::from_utf8(read(&root.join(format!("{export}.clsp")))).unwrap();
                let module = chialisp_oxide_reference::compile(&source).unwrap();
                preflight(&module, 1024 * 1024, 65536, 512);
                modules.push(json!({"export":export,"clvm_hex":hex::encode(module)}));
            }
            println!(
                "{}",
                json!({"compiler":chialisp_oxide_reference::COMPILER_VERSION,"artifacts":modules})
            );
        }
        "compile-rue" => {
            let root = Path::new(&args[2]);
            let mut artifacts = Vec::new();
            for (mode, optimize) in [("rue-off", false), ("rue-on", true)] {
                for export in ["increment", "reset"] {
                    let source = root.join(format!("counter-{export}/puzzle.rue"));
                    let program = chialisp_oxide_rue::compile_file(&source, optimize).unwrap();
                    preflight(&program, 1024 * 1024, 65536, 512);
                    artifacts.push(json!({"mode":mode,"export":export,"optimize_lir":optimize,"clvm_hex":hex::encode(program)}));
                }
            }
            println!(
                "{}",
                json!({"rue_revision":chialisp_oxide_rue::RUE_REVISION,"artifacts":artifacts})
            );
        }
        "replay" => {
            let request: Value = serde_json::from_slice(&read(Path::new(&args[2]))).unwrap();
            assert_eq!(request["max_cost"].as_u64(), Some(MAX_COST));
            let modules = request["modules"].as_array().unwrap();
            let vectors = request["vectors"].as_array().unwrap();
            assert_eq!(modules.len(), 18);
            assert_eq!(vectors.len(), 99);
            let mut observations = Vec::new();
            for module in modules {
                let program = decode_hex(&module["clvm_hex"], 1024 * 1024, 65536, 512);
                for vector in vectors.iter().filter(|v| v["export"] == module["export"]) {
                    let environment = decode_hex(&vector["environment_hex"], 65536, 8192, 128);
                    let mut allocator = Allocator::new_limited(32 * 1024 * 1024);
                    let code = node_from_bytes(&mut allocator, &program).unwrap();
                    let args = node_from_bytes(&mut allocator, &environment).unwrap();
                    let execution = match run_program(
                        &mut allocator,
                        &ChiaDialect::new(MEMPOOL_MODE),
                        code,
                        args,
                        MAX_COST,
                    ) {
                        Ok(result) => {
                            // Codec/resource failures fail the harness; never policy rejection.
                            let output = node_to_bytes_limit(&allocator, result.1, 65536).unwrap();
                            preflight(&output, 65536, 8192, 128);
                            json!({"outcome":{"status":"accepted","output_hex":hex::encode(output)},"cost":result.0,"error":null})
                        }
                        Err(error) => {
                            let failure = classify_failure(&error);
                            json!({"outcome":{"status":if failure.is_program_rejection(){"rejected"}else{"inconclusive"}},"cost":null,"error":format!("{error:?}"),"failure":failure})
                        }
                    };
                    observations.push(json!({"module_id":module["id"],"export":vector["export"],"name":vector["name"],"execution":execution}));
                }
            }
            assert_eq!(observations.len(), 891);
            println!(
                "{}",
                json!({"protocol":"chialisp-oxide-action-counter-raw-vm/v1","vm_failure_protocol":chialisp_oxide_vm::FAILURE_PROTOCOL,"vm":"clvmr 0.21.0","flags":MEMPOOL_MODE.bits(),"max_cost":MAX_COST,"allocator_limit_bytes":32*1024*1024,"observations":observations})
            );
        }
        _ => panic!("explicit compile-reference, compile-rue or replay phase required"),
    }
}
