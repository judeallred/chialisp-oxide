//! Native Rust export observations; host ABI adapter rows stay separate.
use bumpalo::Bump;
use chialisp_oxide::{PublicKey, Tree};
use chialisp_oxide_action_counter::{StateTruth, increment, reset};
use clvmr::{Allocator, NodePtr, SExp, serde::node_from_bytes};
use serde_json::{Value, json};

// Both actual return types use the same host wire encoder. This trait is a
// native-only test adapter, not a compiler/SDK intrinsic or policy substitute.
trait SignatureOutput {
    fn signature(&self) -> Option<chialisp_oxide::AggSigMe>;
}
impl SignatureOutput for () {
    fn signature(&self) -> Option<chialisp_oxide::AggSigMe> {
        None
    }
}
impl SignatureOutput for (chialisp_oxide::AggSigMe,) {
    fn signature(&self) -> Option<chialisp_oxide::AggSigMe> {
        Some(self.0)
    }
}

fn integer(n: u64) -> Vec<u8> {
    if n == 0 {
        return Vec::new();
    }
    let raw = n.to_be_bytes();
    let first = raw.iter().position(|b| *b != 0).unwrap();
    let mut result = Vec::new();
    if raw[first] & 128 != 0 {
        result.push(0);
    }
    result.extend_from_slice(&raw[first..]);
    result
}
fn atom(raw: &[u8]) -> Vec<u8> {
    assert!(raw.len() <= 8192);
    let mut output = if raw.len() == 1 && raw[0] < 128 {
        Vec::new()
    } else if raw.len() < 64 {
        vec![128 + raw.len() as u8]
    } else if raw.len() < 8192 {
        vec![192 + (raw.len() >> 8) as u8, raw.len() as u8]
    } else {
        vec![224, 32, 0]
    };
    output.extend_from_slice(raw);
    output
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    assert!(first.len() + rest.len() < 65536);
    [vec![255], first.to_vec(), rest.to_vec()].concat()
}
fn list(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .rev()
        .fold(vec![128], |tail, value| pair(value, &tail))
}
fn borrowed<'a>(
    a: &Allocator,
    n: NodePtr,
    arena: &'a Bump,
    depth: usize,
    remaining: &mut usize,
) -> &'a Tree<'a> {
    assert!(
        depth <= 128 && *remaining > 0,
        "host codec bound, outside policy adapter"
    );
    *remaining -= 1;
    let value = match a.sexp(n) {
        SExp::Atom => Tree::Atom(arena.alloc_slice_copy(a.atom(n).as_ref())),
        SExp::Pair(f, r) => Tree::Pair(
            borrowed(a, f, arena, depth + 1, remaining),
            borrowed(a, r, arena, depth + 1, remaining),
        ),
    };
    arena.alloc(value)
}
fn encoded(tree: &Tree<'_>, depth: usize) -> Vec<u8> {
    assert!(depth <= 128);
    match tree {
        Tree::Atom(bytes) => atom(bytes),
        Tree::Pair(f, r) => pair(&encoded(f, depth + 1), &encoded(r, depth + 1)),
    }
}
fn fields<'a>(tree: &'a Tree<'a>) -> Option<Vec<&'a Tree<'a>>> {
    let mut pending = tree;
    let mut output = Vec::new();
    loop {
        match pending {
            Tree::Pair(f, r) => {
                if output.len() >= 3 {
                    return None;
                }
                output.push(*f);
                pending = r;
            }
            Tree::Atom([]) => return Some(output),
            Tree::Atom(_) => return None,
        }
    }
}
fn count(tree: &Tree<'_>) -> Option<u64> {
    let Tree::Atom(raw) = tree else {
        return None;
    };
    if raw.len() > 9 || raw.first().is_some_and(|b| b & 128 != 0) {
        return None;
    }
    let mut n = 0u128;
    for byte in *raw {
        n = n * 256 + u128::from(*byte);
    }
    if n > u128::from(u64::MAX) {
        return None;
    }
    let n = n as u64;
    (integer(n) == *raw).then_some(n)
}

#[test]
fn actual_exports_and_separate_host_abi_adapter_match_independent_vectors() {
    let data: Value = serde_json::from_str(include_str!("../vectors.json")).unwrap();
    let rows = data["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 99);
    for row in rows {
        let raw = hex::decode(row["environment_hex"].as_str().unwrap()).unwrap();
        assert!(raw.len() <= 65536);
        let mut a = Allocator::new_limited(8 * 1024 * 1024);
        let root = node_from_bytes(&mut a, &raw).unwrap();
        let arena = Bump::new();
        let environment = borrowed(&a, root, &arena, 1, &mut 8192);
        let export = row["export"].as_str().unwrap();
        let decoded = (|| {
            let values = fields(environment)?;
            if values.len() != if export == "increment" { 1 } else { 2 } {
                return None;
            }
            let owner = if export == "reset" {
                let Tree::Atom(bytes) = values[0] else {
                    return None;
                };
                Some(PublicKey((*bytes).try_into().ok()?))
            } else {
                None
            };
            let Tree::Pair(ephemeral, n) = values[values.len() - 1] else {
                return None;
            };
            Some((owner, *ephemeral, count(n)?))
        })();
        let Some((owner, ephemeral, n)) = decoded else {
            assert_eq!(row["expected"], json!({"status":"rejected","stage":"abi"}));
            println!(
                "ACTION_COUNTER_NATIVE_ADAPTER {}",
                json!({"export":export,"name":row["name"],"status":"abi_rejected","scope":"Host adapter only; export not called"})
            );
            continue;
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if export == "increment" {
                let output = increment(StateTruth {
                    first: *ephemeral,
                    rest: n,
                });
                (output.first.first, output.first.rest, None)
            } else {
                let output = reset(
                    owner.unwrap(),
                    StateTruth {
                        first: *ephemeral,
                        rest: n,
                    },
                );
                (
                    output.first.first,
                    output.first.rest,
                    SignatureOutput::signature(&output.rest),
                )
            }
        }));
        let actual = match result {
            Ok((ephemeral, count, signature)) => {
                // Output codec/resource failures fail the harness, outside the
                // policy panic adapter and without rejection credit.
                let conditions = if let Some(signature) = signature {
                    list(&[list(&[
                        atom(&integer(signature.0)),
                        atom(&signature.1.0),
                        atom(&signature.2.0),
                    ])])
                } else {
                    vec![128]
                };
                let bytes = pair(
                    &pair(&encoded(&ephemeral, 1), &atom(&integer(count))),
                    &conditions,
                );
                let actual = json!({"status":"accepted","output_hex":hex::encode(bytes)});
                assert_eq!(actual, row["expected"]);
                actual
            }
            Err(_) => {
                assert_eq!(
                    row["expected"],
                    json!({"status":"rejected","stage":"policy"})
                );
                json!({"status":"policy_panic"})
            }
        };
        println!(
            "ACTION_COUNTER_NATIVE_EXPORT {}",
            json!({"export":export,"name":row["name"],"actual":actual})
        );
    }
}
