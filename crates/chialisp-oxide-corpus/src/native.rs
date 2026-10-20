//! Reference execution calls the actual Rust sources also supplied to rustc-driver.
#![allow(clippy::type_complexity)] // Heterogeneous condition tuples are the current teaching ABI.
use anyhow::{Result, bail, ensure};
use chialisp_oxide::{Amount, Cons, Message32, PublicKey, PuzzleHash, Timestamp, Tree};
use chialisp_oxide_puzzles::{
    boolean_gate as gate, bounded_counter as counter, capped_payment as capped, currying_multiply,
    fixed_payment as fixed, intro_add, intro_conditional, intro_list, module_add, module_factorial,
    module_times_ten, recovery_payment as recovery, signature_payment as signature,
    split_payment as split, timelocked_payment as timelock, two_signature_payment as multisig,
};
use clvmr::{Allocator, NodePtr, allocator::SExp, serde::node_to_bytes};
use num_bigint::{BigInt, Sign};

trait Encode {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr>;
}
impl Encode for bool {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        if *self {
            Ok(a.new_atom(&[1])?)
        } else {
            Ok(NodePtr::NIL)
        }
    }
}
impl Encode for u64 {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let n = BigInt::from(*self);
        let bytes = if *self == 0 {
            vec![]
        } else {
            n.to_signed_bytes_be()
        };
        Ok(a.new_atom(&bytes)?)
    }
}
impl Encode for i64 {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        // Minimal two's complement bytes; zero is the empty atom.
        let bytes = if *self == 0 {
            vec![]
        } else {
            BigInt::from(*self).to_signed_bytes_be()
        };
        Ok(a.new_atom(&bytes)?)
    }
}
impl<const N: usize> Encode for [u8; N] {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        Ok(a.new_atom(self)?)
    }
}
impl<First: Encode, Rest: Encode> Encode for Cons<First, Rest> {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let first = self.first.encode(a)?;
        let rest = self.rest.encode(a)?;
        Ok(a.new_pair(first, rest)?)
    }
}
fn list(a: &mut Allocator, items: &[NodePtr]) -> Result<NodePtr> {
    let mut out = NodePtr::NIL;
    for item in items.iter().rev() {
        out = a.new_pair(*item, out)?;
    }
    Ok(out)
}
impl<A: Encode> Encode for (A,) {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let x = self.0.encode(a)?;
        list(a, &[x])
    }
}
impl<A: Encode, B: Encode> Encode for (A, B) {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let x = self.0.encode(a)?;
        let y = self.1.encode(a)?;
        list(a, &[x, y])
    }
}
impl<A: Encode, B: Encode, C: Encode> Encode for (A, B, C) {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let x = self.0.encode(a)?;
        let y = self.1.encode(a)?;
        let z = self.2.encode(a)?;
        list(a, &[x, y, z])
    }
}
impl<A: Encode, B: Encode, C: Encode, D: Encode> Encode for (A, B, C, D) {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let w = self.0.encode(a)?;
        let x = self.1.encode(a)?;
        let y = self.2.encode(a)?;
        let z = self.3.encode(a)?;
        list(a, &[w, x, y, z])
    }
}
// Encode domain values independently of the compiler's newtype erasure.
impl Encode for Amount {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.mojos().encode(a)
    }
}
impl Encode for Timestamp {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.unix_seconds().encode(a)
    }
}
impl Encode for PuzzleHash {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.0.encode(a)
    }
}
impl Encode for PublicKey {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.0.encode(a)
    }
}
impl Encode for Message32 {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.0.encode(a)
    }
}

fn output(v: impl Encode) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let node = v.encode(&mut a)?;
    Ok(node_to_bytes(&a, node)?)
}
impl Encode for Tree<'_> {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        super::native_tree::encode(a, self)
    }
}
impl Encode for &Tree<'_> {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        super::native_tree::encode(a, self)
    }
}

// A host testing bound, never an added policy assertion or rejection outcome.
fn raw_native_depth(mut tree: &Tree<'_>, label: &str) -> Result<()> {
    let mut depth = 0;
    while let Tree::Pair(_, rest) = tree {
        depth += 1;
        ensure!(
            depth <= 128,
            "{label} native oracle right-tail depth exceeds 128"
        );
        tree = rest;
    }
    Ok(())
}

fn args(a: &Allocator, mut n: NodePtr) -> Result<Vec<NodePtr>> {
    let mut out = vec![];
    loop {
        match a.sexp(n) {
            SExp::Atom => {
                ensure!(a.atom(n).as_ref().is_empty(), "improper solution");
                return Ok(out);
            }
            SExp::Pair(f, r) => {
                out.push(f);
                ensure!(out.len() <= 8, "too many arguments");
                n = r;
            }
        }
    }
}
fn atom(a: &Allocator, n: NodePtr) -> Result<Vec<u8>> {
    ensure!(matches!(a.sexp(n), SExp::Atom), "expected atom");
    Ok(a.atom(n).as_ref().to_vec())
}
fn uint(a: &Allocator, n: NodePtr) -> Result<u64> {
    let bytes = atom(a, n)?;
    let integer = BigInt::from_signed_bytes_be(&bytes);
    ensure!(integer.sign() != Sign::Minus, "negative u64");
    let value: u64 = integer.clone().try_into()?;
    let canonical = if value == 0 {
        vec![]
    } else {
        integer.to_signed_bytes_be()
    };
    ensure!(bytes == canonical, "noncanonical integer");
    Ok(value)
}
fn int64(a: &Allocator, n: NodePtr) -> Result<i64> {
    // Canonical signed CLVM integer: minimal two's complement, nil for zero,
    // and the value must fit the declared Rust width.
    let bytes = atom(a, n)?;
    let integer = BigInt::from_signed_bytes_be(&bytes);
    let value: i64 = integer.clone().try_into()?;
    let canonical = if value == 0 {
        vec![]
    } else {
        integer.to_signed_bytes_be()
    };
    ensure!(bytes == canonical, "noncanonical integer");
    Ok(value)
}
fn boolean(a: &Allocator, n: NodePtr) -> Result<bool> {
    match atom(a, n)?.as_slice() {
        [] => Ok(false),
        [1] => Ok(true),
        _ => bail!("noncanonical bool"),
    }
}
fn bytes32(a: &Allocator, n: NodePtr) -> Result<[u8; 32]> {
    atom(a, n)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes"))
}
thread_local! { static EXPECTED_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
struct NativePanicScope(bool);
impl NativePanicScope {
    fn enter() -> Self {
        static HOOK: std::sync::Once = std::sync::Once::new();
        HOOK.call_once(|| {
            let original = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                if !EXPECTED_PANIC.with(|flag| flag.get()) {
                    original(info);
                }
            }));
        });
        Self(EXPECTED_PANIC.with(|flag| flag.replace(true)))
    }
}
impl Drop for NativePanicScope {
    fn drop(&mut self) {
        EXPECTED_PANIC.with(|flag| flag.set(self.0));
    }
}

pub fn execute(slug: &str, solution: &str) -> Result<Option<Vec<u8>>> {
    // Assertions are an expected oracle result. Suppress their default hook only
    // within this thread and scope, leaving unrelated panics visible.
    let _scope = NativePanicScope::enter();
    let mut a = Allocator::new();
    let env = rue_clvm::assemble(&mut a, solution)?;
    if slug == "sha256tree" {
        let arena = bumpalo::Bump::new();
        // Hash both branches. Bound all decoded occurrences and payloads before
        // allocation; host exhaustion must propagate as Err, never rejection.
        let tree = super::native_hash::decode(&arena, &a, env)?;
        return Ok(Some(output(chialisp_oxide_puzzles::sha256tree::puzzle(
            tree,
        ))?));
    }
    if slug == "p2-delegated-conditions" {
        let arena = bumpalo::Bump::new();
        // Decode and preflight the complete environment before the policy
        // panic adapter, including arbitrary key trees and ignored tails.
        let tree = super::native_hash::decode(&arena, &a, env)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::p2_delegated_conditions::puzzle(tree)
        }));
        return match result {
            Ok(value) => {
                super::p2_delegated_conditions::output_preflight(&value)?;
                Ok(Some(output(value)?))
            }
            Err(_) => Ok(None),
        };
    }
    if slug == "password-coin" {
        let arena = bumpalo::Bump::new();
        // Bound the whole attacker-controlled input, including ignored tails,
        // before allocation. Host exhaustion propagates outside the panic catch.
        let tree = super::native_hash::decode(&arena, &a, env)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::password_coin::puzzle(tree)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(_) => Ok(None),
        };
    }
    if slug == "p2-singleton" {
        let arena = bumpalo::Bump::new();
        // Complete input preflight precedes the policy catch, including ignored tails.
        let tree = super::native_hash::decode(&arena, &a, env)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::p2_singleton::puzzle(tree)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(_) => Ok(None),
        };
    }
    if slug == "singleton-launcher" {
        let arena = bumpalo::Bump::new();
        // Include the ignored tail in the host budget, while preserving the
        // policy's raw first-three-projection and arbitrary-field contract.
        let tree = super::native_hash::decode(&arena, &a, env)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::singleton_launcher::puzzle(tree)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(_) => Ok(None),
        };
    }
    if slug == "atom-not-equal" {
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 2 => arguments,
            _ => return Ok(None),
        };
        if arguments
            .iter()
            .any(|argument| !matches!(a.sexp(*argument), SExp::Atom))
        {
            return Ok(None);
        }
        let left = a.atom(arguments[0]);
        let right = a.atom(arguments[1]);
        let result = std::panic::catch_unwind(|| {
            chialisp_oxide_puzzles::atom_not_equal::puzzle(left.as_ref(), right.as_ref())
        });
        // Keep output allocation/codec failures outside policy rejection.
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(_) => Ok(None),
        };
    }
    if slug == "prefix-list" {
        // Cover both argument trees and all branches before creating Boxes.
        // These resource failures are inconclusive host errors, never policy.
        super::native_owned_tree::preflight(&a, env)?;
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 2 => arguments,
            _ => return Ok(None),
        };
        super::native_owned_tree::preflight_prefix(&a, arguments[0], arguments[1])?;
        let arena = bumpalo::Bump::new();
        let list = super::native_owned_tree::decode(&arena, &a, arguments[0])?;
        let tail = super::native_owned_tree::decode(&arena, &a, arguments[1])?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::prefix_list::puzzle(list, tail)
        }));
        return match result {
            Ok(value) => {
                let mut output = Allocator::new();
                let node = super::native_owned_tree::encode(&mut output, value)?;
                Ok(Some(node_to_bytes(&output, node)?))
            }
            Err(_) => Ok(None),
        };
    }
    if slug == "insertion-sort-by-keys" {
        // Bound the whole environment before allocating Boxes, including data
        // ignored by a base case. Host limits are inconclusive oracle errors.
        super::native_owned_tree::preflight(&a, env)?;
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 2 => arguments,
            _ => return Ok(None),
        };
        super::native_owned_tree::preflight_sort(&a, arguments[0], arguments[1])?;
        let arena = bumpalo::Bump::new();
        let sorted = super::native_owned_tree::decode(&arena, &a, arguments[0])?;
        let insertions = super::native_owned_tree::decode(&arena, &a, arguments[1])?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::insertion_sort_by_keys::puzzle(sorted, insertions)
        }));
        return match result {
            Ok(value) => {
                let mut output = Allocator::new();
                let node = super::native_owned_tree::encode(&mut output, value)?;
                Ok(Some(node_to_bytes(&output, node)?))
            }
            Err(_) => Ok(None),
        };
    }
    if slug == "curry-factory" {
        // Bound the whole environment before creating Boxes; host limits are
        // inconclusive oracle errors, never policy rejections.
        super::native_owned_tree::preflight(&a, env)?;
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 2 => arguments,
            _ => return Ok(None),
        };
        let arena = bumpalo::Bump::new();
        let module = super::native_tree::decode(&arena, &a, arguments[0])?;
        let parameter = super::native_tree::decode(&arena, &a, arguments[1])?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::curry_factory::puzzle(module, parameter)
        }));
        return match result {
            Ok(value) => {
                let mut output = Allocator::new();
                let node = super::native_owned_tree::encode(&mut output, value)?;
                Ok(Some(node_to_bytes(&output, node)?))
            }
            Err(_) => Ok(None),
        };
    }
    if slug == "delegated-puzzle" {
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 2 => arguments,
            _ => return Ok(None),
        };
        let arena = bumpalo::Bump::new();
        let inner_puzzle = super::native_tree::decode(&arena, &a, arguments[0])?;
        let inner_solution = super::native_tree::decode(&arena, &a, arguments[1])?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::delegated_puzzle::puzzle(inner_puzzle, inner_solution)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(payload) => {
                // The native evaluator's host ceiling is inconclusive, never a
                // policy rejection; an applied program's own failure rejects.
                ensure!(
                    payload
                        .downcast_ref::<chialisp_oxide::ResourceExhausted>()
                        .is_none(),
                    "native apply evaluator exhausted its host ceiling"
                );
                Ok(None)
            }
        };
    }
    if slug == "rightmost-tree" {
        let arena = bumpalo::Bump::new();
        let tree = super::native_tree::decode(&arena, &a, env)?;
        // Keep resource-bound errors outside the policy panic/Result adapter.
        // They must propagate to the caller, not become Outcome::Rejected.
        raw_native_depth(tree, "rightmost")?;
        return Ok(Some(output(
            chialisp_oxide_puzzles::rightmost_tree::puzzle(tree),
        )?));
    }
    if slug == "last-list-element" {
        let arena = bumpalo::Bump::new();
        let tree = super::native_tree::decode(&arena, &a, env)?;
        // Bounds/codec failures are host errors, never policy rejections.
        raw_native_depth(tree, "last-list-element")?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::last_list_element::puzzle(tree)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(_) => Ok(None),
        };
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Vec<u8>> {
        // These annotated nullary exports explicitly ignore every assembled tree.
        // Keep the default exact argument decoder for all other policies.
        match slug {
            "p2-conditions" => {
                let arena = bumpalo::Bump::new();
                let tree = super::native_tree::decode(&arena, &a, env)?;
                return output(chialisp_oxide_puzzles::p2_conditions::puzzle(tree));
            }
            "intro-list" => return output(intro_list::puzzle()),
            "intro-add" => return output(intro_add::puzzle()),
            "intro-conditional" => return output(intro_conditional::puzzle()),
            _ => (),
        }
        let args = args(&a, env)?;
        match slug {
            "fixed-payment" => {
                ensure!(args.is_empty(), "arity");
                output(fixed::puzzle())
            }
            "capped-payment" => {
                ensure!(args.len() == 1, "arity");
                output(capped::puzzle(Amount::from_mojos(uint(&a, args[0])?)))
            }
            "split-payment" => {
                ensure!(args.len() == 1, "arity");
                output(split::puzzle(Amount::from_mojos(uint(&a, args[0])?)))
            }
            "timelocked-payment" => {
                ensure!(args.is_empty(), "arity");
                output(timelock::puzzle())
            }
            "boolean-gate" => {
                ensure!(args.len() == 1, "arity");
                output(gate::puzzle(boolean(&a, args[0])?))
            }
            "bounded-counter" => {
                ensure!(args.len() == 1, "arity");
                output(counter::puzzle(uint(&a, args[0])?))
            }
            "signature-payment" => {
                ensure!(args.len() == 1, "arity");
                output(signature::puzzle(Message32(bytes32(&a, args[0])?)))
            }
            "two-signature-payment" => {
                ensure!(args.len() == 1, "arity");
                output(multisig::puzzle(Message32(bytes32(&a, args[0])?)))
            }
            "curry-hash" => {
                ensure!(args.len() == 2, "arity");
                output(chialisp_oxide_puzzles::curry_hash::puzzle(
                    bytes32(&a, args[0])?,
                    bytes32(&a, args[1])?,
                ))
            }
            "recovery-payment" => {
                ensure!(args.len() == 2, "arity");
                output(recovery::puzzle(
                    boolean(&a, args[0])?,
                    Message32(bytes32(&a, args[1])?),
                ))
            }
            "module-add" => {
                ensure!(args.len() == 2, "arity");
                output(module_add::puzzle(int64(&a, args[0])?, int64(&a, args[1])?))
            }
            "currying-multiply" => {
                ensure!(args.len() == 2, "arity");
                output(currying_multiply::puzzle(
                    int64(&a, args[0])?,
                    int64(&a, args[1])?,
                ))
            }
            "module-times-ten" => {
                ensure!(args.len() == 1, "arity");
                output(module_times_ten::puzzle(int64(&a, args[0])?))
            }
            "module-factorial" => {
                ensure!(args.len() == 1, "arity");
                output(module_factorial::puzzle(int64(&a, args[0])?))
            }
            _ => bail!("unknown native fixture {slug}"),
        }
    }));
    Ok(result.ok().and_then(Result::ok))
}

#[cfg(test)]
mod tests {
    use super::execute;

    #[test]
    fn actual_hash_ports_preserve_digest_width_raw_fields_and_ignored_tail() {
        // Independent Python hashlib known answers, not this SDK's SHA kernel.
        for (input, digest) in [
            (
                "()",
                "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
            ),
            (
                "0x00d9",
                "00e146297686edb230b0e85eb3bbbef74d745dcd89d98469db35aaad8d2e08db",
            ),
        ] {
            assert_eq!(
                hex::encode(execute("sha256tree", input).unwrap().unwrap()),
                format!("a0{digest}")
            );
        }
        let ordinary = "ffff33ff01ff0280ffff3cffa0bcd55bcd0daebba8cb158547e8480dc968570faf958f1e31a9887d6ae3dba5918080";
        for input in ["(1 2 3)", "(1 2 3 4)", "(1 2 3 . 0x00)"] {
            assert_eq!(
                hex::encode(execute("singleton-launcher", input).unwrap().unwrap()),
                ordinary
            );
        }
        assert_eq!(
            hex::encode(
                execute("singleton-launcher", "((1 . 2) (3 . 4) (5 . 6))")
                    .unwrap()
                    .unwrap()
            ),
            "ffff33ffff0102ffff030480ffff3cffa00fb2608fa2ee7246193af0180083fbfd95e1186dd341c64986c7f44b90e507148080"
        );
        for input in ["()", "0x00", "(1)", "(1 2)", "(1 2 . 0x00)"] {
            assert_eq!(execute("singleton-launcher", input).unwrap(), None);
        }
    }

    #[test]
    fn recursive_hash_host_limit_is_error_even_in_launcher_ignored_tail() {
        let mut tree = "0x0001".to_string();
        for _ in 0..257 {
            tree = format!("({tree} . 0x00)");
        }
        for (slug, input) in [
            ("sha256tree", tree.clone()),
            ("singleton-launcher", format!("(1 2 3 . {tree})")),
        ] {
            let error = execute(slug, &input).unwrap_err().to_string();
            assert!(
                error.contains("native hash input depth exceeds"),
                "{slug}: {error}"
            );
        }
    }

    #[test]
    fn actual_rightmost_rust_preserves_atom_bytes_and_returns_proper_list_nil() {
        for (input, expected) in [
            ("()", "80"),
            ("0x00", "00"),
            ("0x0001", "820001"),
            ("(1 2 3)", "80"),
            ("(1 . 0x0001)", "820001"),
            ("((8) . 0xffff)", "82ffff"),
        ] {
            assert_eq!(
                hex::encode(execute("rightmost-tree", input).unwrap().unwrap()),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn native_recursion_limit_is_host_error_and_never_policy_rejection() {
        let mut tree = "0x0001".to_string();
        for _ in 0..128 {
            tree = format!("(0x00 . {tree})");
        }
        assert_eq!(
            hex::encode(execute("rightmost-tree", &tree).unwrap().unwrap()),
            "820001"
        );
        tree = format!("(0x00 . {tree})");
        let error = execute("rightmost-tree", &tree).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("native oracle right-tail depth exceeds 128")
        );
    }

    #[test]
    fn actual_last_helper_preserves_final_trees_and_rejects_nonempty_zero_tails() {
        for (input, expected) in [
            ("(())", "80"),
            ("(0x00)", "00"),
            ("(0x0001)", "820001"),
            ("(1 2 3)", "03"),
            ("(1 (2 . 3))", "ff0203"),
            ("((8))", "ff0880"),
            ("((1 . 2) (0x0001 . 0x0000))", "ff820001820000"),
        ] {
            assert_eq!(
                hex::encode(execute("last-list-element", input).unwrap().unwrap()),
                expected,
                "{input}"
            );
        }
        for input in [
            "()",
            "0x00",
            "0x0000",
            "(1 . 2)",
            "(1 . 0x00)",
            "(1 . 0x0000)",
            "(1 . 0x0001)",
        ] {
            assert_eq!(
                execute("last-list-element", input).unwrap(),
                None,
                "{input}"
            );
        }
    }

    #[test]
    fn last_helper_native_limit_propagates_as_host_error() {
        let mut tree = "()".to_string();
        for _ in 0..128 {
            tree = format!("(0x0001 . {tree})");
        }
        assert_eq!(
            hex::encode(execute("last-list-element", &tree).unwrap().unwrap()),
            "820001"
        );
        tree = format!("(0x0001 . {tree})");
        assert!(
            execute("last-list-element", &tree)
                .unwrap_err()
                .to_string()
                .contains("native oracle right-tail depth exceeds 128")
        );
    }
}
