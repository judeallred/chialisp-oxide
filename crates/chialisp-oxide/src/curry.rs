#![forbid(unsafe_code)]
//! Standard currying as hash arithmetic and as owned tree construction.
//!
//! Chia's standard curried form of a module `M` with parameter `P` is
//! `(a (q . M) (c (q . P) 1))`: apply the quoted module to an environment that
//! prepends the quoted parameter to the caller's solution. The puzzle hash of
//! that form can be computed from the module's and parameter's tree hashes
//! alone, which is what coin commitments need. These helpers port the hash
//! arithmetic of the pinned upstream `curry-and-treehash.clib` include using
//! the reviewed SHA-256 bridge; they do not establish that a particular module
//! or instance is secure, and the compiler lowers their ordinary bodies, not
//! their names.
use crate::sha256_parts;

/// CLVM `a`.
pub const APPLY_OPCODE: u8 = 2;
/// CLVM `c`.
pub const CONS_OPCODE: u8 = 4;
/// CLVM `q`.
pub const QUOTE_OPCODE: u8 = 1;

/// Tree hash of an atom: `sha256(0x01 || bytes)`.
pub fn atom_hash(bytes: &[u8]) -> [u8; 32] {
    sha256_parts(&[1], bytes, &[])
}

/// Tree hash of a pair from its children's hashes: `sha256(0x02 || left || right)`.
pub fn pair_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    sha256_parts(&[2], left, right)
}

/// Tree hash of `(q . value)` given `value`'s tree hash.
pub fn quoted_hash(value_hash: &[u8; 32]) -> [u8; 32] {
    pair_hash(&atom_hash(&[QUOTE_OPCODE]), value_hash)
}

/// Tree hash of `(c (q . parameter) environment)` given both operands' hashes:
/// the environment with one more parameter curried in.
pub fn curried_environment_hash(
    parameter_hash: &[u8; 32],
    environment_hash: &[u8; 32],
) -> [u8; 32] {
    let quoted = quoted_hash(parameter_hash);
    let tail = pair_hash(environment_hash, &atom_hash(&[]));
    pair_hash(&atom_hash(&[CONS_OPCODE]), &pair_hash(&quoted, &tail))
}

/// Tree hash of `(a (q . function) environment)` given both operands' hashes.
pub fn applied_hash(function_hash: &[u8; 32], environment_hash: &[u8; 32]) -> [u8; 32] {
    let quoted = quoted_hash(function_hash);
    let tail = pair_hash(environment_hash, &atom_hash(&[]));
    pair_hash(&atom_hash(&[APPLY_OPCODE]), &pair_hash(&quoted, &tail))
}

/// Tree hash of the caller's whole solution, `1`, as the innermost environment.
pub fn solution_environment_hash() -> [u8; 32] {
    atom_hash(&[1])
}

/// Puzzle hash of `module` curried with one parameter.
pub fn curried_puzzle_hash(module_hash: &[u8; 32], parameter_hash: &[u8; 32]) -> [u8; 32] {
    applied_hash(
        module_hash,
        &curried_environment_hash(parameter_hash, &solution_environment_hash()),
    )
}

/// Puzzle hash of `module` curried with two parameters, first then second.
pub fn curried_puzzle_hash_2(
    module_hash: &[u8; 32],
    first_hash: &[u8; 32],
    second_hash: &[u8; 32],
) -> [u8; 32] {
    let inner = curried_environment_hash(second_hash, &solution_environment_hash());
    applied_hash(module_hash, &curried_environment_hash(first_hash, &inner))
}

/// Puzzle hash of `module` curried with three parameters in order.
pub fn curried_puzzle_hash_3(
    module_hash: &[u8; 32],
    first_hash: &[u8; 32],
    second_hash: &[u8; 32],
    third_hash: &[u8; 32],
) -> [u8; 32] {
    let inner = curried_environment_hash(third_hash, &solution_environment_hash());
    let middle = curried_environment_hash(second_hash, &inner);
    applied_hash(module_hash, &curried_environment_hash(first_hash, &middle))
}

#[cfg(feature = "alloc")]
mod owned {
    use super::{APPLY_OPCODE, CONS_OPCODE, QUOTE_OPCODE};
    use crate::OwnedTree;
    use alloc::boxed::Box;

    fn pair<'a>(first: OwnedTree<'a>, rest: OwnedTree<'a>) -> OwnedTree<'a> {
        OwnedTree::Pair(Box::new(first), Box::new(rest))
    }

    /// Prepend one quoted parameter to an environment tree: `(c (q . parameter) environment)`.
    pub fn curried_environment<'a>(
        parameter: OwnedTree<'a>,
        environment: OwnedTree<'a>,
    ) -> OwnedTree<'a> {
        let quoted = pair(OwnedTree::Atom(&[QUOTE_OPCODE]), parameter);
        let tail = pair(environment, OwnedTree::Atom(&[]));
        pair(OwnedTree::Atom(&[CONS_OPCODE]), pair(quoted, tail))
    }

    /// The standard curried program `(a (q . module) (c (q . parameter) 1))`.
    pub fn curry<'a>(module: OwnedTree<'a>, parameter: OwnedTree<'a>) -> OwnedTree<'a> {
        let environment = curried_environment(parameter, OwnedTree::Atom(&[1]));
        let quoted = pair(OwnedTree::Atom(&[QUOTE_OPCODE]), module);
        let tail = pair(environment, OwnedTree::Atom(&[]));
        pair(OwnedTree::Atom(&[APPLY_OPCODE]), pair(quoted, tail))
    }
}
#[cfg(feature = "alloc")]
pub use owned::{curried_environment, curry};

#[cfg(test)]
mod tests {
    extern crate alloc;
    extern crate std;
    use super::*;
    use alloc::boxed::Box;
    use clvm_utils::{TreeHash, curry_tree_hash, tree_hash};
    use oracle_clvmr::{Allocator, NodePtr, serde::node_to_bytes};
    use std::vec::Vec;

    fn hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
        tree_hash(a, node).to_bytes()
    }
    fn tree_hash_of(value: [u8; 32]) -> TreeHash {
        TreeHash::new(value)
    }
    fn list(a: &mut Allocator, items: &[NodePtr]) -> NodePtr {
        let mut out = NodePtr::NIL;
        for item in items.iter().rev() {
            out = a.new_pair(*item, out).unwrap();
        }
        out
    }

    #[test]
    fn hash_arithmetic_matches_the_independent_oracle_for_one_two_and_three_parameters() {
        let mut a = Allocator::new();
        // (* 2 5) as a module tree, plus assorted parameter trees.
        let module = {
            let op = a.new_atom(&[18]).unwrap();
            let two = a.new_atom(&[2]).unwrap();
            let five = a.new_atom(&[5]).unwrap();
            list(&mut a, &[op, two, five])
        };
        let parameters = [
            a.new_atom(&[2]).unwrap(),
            a.new_atom(&[]).unwrap(),
            a.new_atom(&[0, 1]).unwrap(),
            {
                let x = a.new_atom(b"abc").unwrap();
                let y = a.new_atom(&[0x80, 0xff]).unwrap();
                a.new_pair(x, y).unwrap()
            },
        ];
        let module_hash = hash(&a, module);
        for p in parameters {
            let ph = hash(&a, p);
            assert_eq!(
                curried_puzzle_hash(&module_hash, &ph),
                curry_tree_hash(tree_hash_of(module_hash), &[tree_hash_of(ph)]).to_bytes()
            );
            for q in parameters {
                let qh = hash(&a, q);
                assert_eq!(
                    curried_puzzle_hash_2(&module_hash, &ph, &qh),
                    curry_tree_hash(
                        tree_hash_of(module_hash),
                        &[tree_hash_of(ph), tree_hash_of(qh)]
                    )
                    .to_bytes()
                );
                assert_eq!(
                    curried_puzzle_hash_3(&module_hash, &ph, &qh, &ph),
                    curry_tree_hash(
                        tree_hash_of(module_hash),
                        &[tree_hash_of(ph), tree_hash_of(qh), tree_hash_of(ph)]
                    )
                    .to_bytes()
                );
            }
        }
        // Zero parameters: (a (q . M) 1) is not a curried form these helpers produce,
        // but the environment hash of `1` is the oracle's starting point.
        assert_eq!(
            solution_environment_hash(),
            clvm_utils::tree_hash_atom(&[1]).to_bytes()
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_curry_materializes_the_standard_form_and_hashes_consistently() {
        fn encode(a: &mut Allocator, tree: &crate::OwnedTree<'_>) -> NodePtr {
            match tree {
                crate::OwnedTree::Atom(bytes) => a.new_atom(bytes).unwrap(),
                crate::OwnedTree::Pair(first, rest) => {
                    let first = encode(a, first);
                    let rest = encode(a, rest);
                    a.new_pair(first, rest).unwrap()
                }
            }
        }
        use crate::OwnedTree;
        let mut a = Allocator::new();
        // The tutorial's multiply module (* 2 5) curried with 2: (a (q 18 2 5) (c (q . 2) 1)).
        let module = OwnedTree::Pair(
            Box::new(OwnedTree::Atom(&[18])),
            Box::new(OwnedTree::Pair(
                Box::new(OwnedTree::Atom(&[2])),
                Box::new(OwnedTree::Pair(
                    Box::new(OwnedTree::Atom(&[5])),
                    Box::new(OwnedTree::Atom(&[])),
                )),
            )),
        );
        let curried = curry(module, OwnedTree::Atom(&[2]));
        let node = encode(&mut a, &curried);
        let bytes: Vec<u8> = node_to_bytes(&a, node).unwrap();
        // Independently spelled expected serialization of the page's curried result.
        let expected = {
            let two = a.new_atom(&[2]).unwrap();
            let one = a.new_atom(&[1]).unwrap();
            let four = a.new_atom(&[4]).unwrap();
            let op = a.new_atom(&[18]).unwrap();
            let p2 = a.new_atom(&[2]).unwrap();
            let p5 = a.new_atom(&[5]).unwrap();
            let body = list(&mut a, &[op, p2, p5]);
            let quoted_module = a.new_pair(one, body).unwrap();
            let quoted_arg = a.new_pair(one, two).unwrap();
            let env = list(&mut a, &[four, quoted_arg, one]);
            let program = list(&mut a, &[two, quoted_module, env]);
            node_to_bytes(&a, program).unwrap()
        };
        assert_eq!(bytes, expected);
        // The hash-only route agrees with the materialized tree.
        let module_hash = {
            let op = a.new_atom(&[18]).unwrap();
            let p2 = a.new_atom(&[2]).unwrap();
            let p5 = a.new_atom(&[5]).unwrap();
            let body = list(&mut a, &[op, p2, p5]);
            hash(&a, body)
        };
        assert_eq!(
            curried_puzzle_hash(&module_hash, &atom_hash(&[2])),
            hash(&a, node)
        );
    }
}
