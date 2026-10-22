//! Reference execution calls the actual Rust sources also supplied to rustc-driver.
#![allow(clippy::type_complexity)] // Heterogeneous condition tuples are the current teaching ABI.
use anyhow::{Result, bail, ensure};
use chialisp_oxide::{
    Amount, BlockAge, CoinAnnouncementId, CoinId, Cons, Message32, PublicKey, PuzzleHash,
    Timestamp, Tree,
};
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
impl Encode for () {
    fn encode(&self, _a: &mut Allocator) -> Result<NodePtr> {
        Ok(NodePtr::NIL)
    }
}
impl<T: Encode> Encode for Option<T> {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        match self {
            None => Ok(NodePtr::NIL),
            Some(value) => {
                let item = value.encode(a)?;
                Ok(a.new_pair(item, NodePtr::NIL)?)
            }
        }
    }
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
impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode, F: Encode> Encode
    for (A, B, C, D, E, F)
{
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        let items = [
            self.0.encode(a)?,
            self.1.encode(a)?,
            self.2.encode(a)?,
            self.3.encode(a)?,
            self.4.encode(a)?,
            self.5.encode(a)?,
        ];
        list(a, &items)
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
impl Encode for CoinId {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.0.encode(a)
    }
}
impl Encode for CoinAnnouncementId {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        self.0.encode(a)
    }
}
impl Encode for BlockAge {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        u64::from(self.blocks()).encode(a)
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

// The boomerang decoder checks fixed widths before copying attacker input.
// Existing fixture decoders retain their independently recorded behavior.
fn fixed_atom<const N: usize>(a: &Allocator, n: NodePtr) -> Result<[u8; N]> {
    ensure!(matches!(a.sexp(n), SExp::Atom), "expected atom");
    a.atom(n)
        .as_ref()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected {N} bytes"))
}
fn public_key(a: &Allocator, n: NodePtr) -> Result<PublicKey> {
    Ok(PublicKey(fixed_atom(a, n)?))
}
fn boomerang_arguments(
    a: &Allocator,
    env: NodePtr,
) -> Result<((PublicKey, PuzzleHash, PuzzleHash, Amount, Timestamp), bool)> {
    let arguments = args(a, env)?;
    ensure!(arguments.len() == 2, "boomerang arity");
    let terms = args(a, arguments[0])?;
    ensure!(terms.len() == 5, "boomerang terms arity");
    let recipient_key = public_key(a, terms[0])?;
    let recipient = PuzzleHash(fixed_atom(a, terms[1])?);
    let refund_to = PuzzleHash(fixed_atom(a, terms[2])?);
    for n in [terms[3], terms[4]] {
        ensure!(matches!(a.sexp(n), SExp::Atom), "expected u64 atom");
        // A positive u64 needs at most eight bytes plus a leading sign byte.
        ensure!(a.atom(n).as_ref().len() <= 9, "u64 atom exceeds width");
    }
    let amount = Amount::from_mojos(uint(a, terms[3])?);
    let deadline = Timestamp::from_unix_seconds(uint(a, terms[4])?);
    ensure!(
        matches!(a.sexp(arguments[1]), SExp::Atom),
        "expected bool atom"
    );
    ensure!(
        a.atom(arguments[1]).as_ref().len() <= 1,
        "bool atom exceeds width"
    );
    let claim = boolean(a, arguments[1])?;
    Ok((
        (recipient_key, recipient, refund_to, amount, deadline),
        claim,
    ))
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
    if slug == "boomerang" {
        // Invalid typed arguments reject before source execution. This closed
        // decoder bounds every copied scalar; assembly errors still propagate.
        let Ok((terms, claim)) = boomerang_arguments(&a, env) else {
            return Ok(None);
        };
        let result =
            std::panic::catch_unwind(|| chialisp_oxide_puzzles::boomerang::puzzle(terms, claim));
        return match result {
            // Output allocation/codec errors are inconclusive host failures,
            // outside the source-policy panic adapter.
            Ok(value) => Ok(Some(output(value)?)),
            Err(payload) => {
                ensure!(
                    payload
                        .downcast_ref::<chialisp_oxide::ResourceExhausted>()
                        .is_none(),
                    "native boomerang policy exhausted its host ceiling"
                );
                Ok(None)
            }
        };
    }
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
    if slug == "inner-puzzle" {
        let arguments = match args(&a, env) {
            Ok(arguments) if arguments.len() == 3 => arguments,
            _ => return Ok(None),
        };
        let Ok(commitment) = bytes32(&a, arguments[0]) else {
            return Ok(None);
        };
        let arena = bumpalo::Bump::new();
        let inner_puzzle = super::native_hash::decode(&arena, &a, arguments[1])?;
        let inner_solution = super::native_tree::decode(&arena, &a, arguments[2])?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::inner_puzzle::puzzle(commitment, inner_puzzle, inner_solution)
        }));
        return match result {
            Ok(value) => Ok(Some(output(value)?)),
            Err(payload) => {
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
            "precommit-claim" => {
                ensure!(args.len() == 3, "arity");
                let terms = self::args(&a, args[0])?;
                ensure!(terms.len() == 3, "terms arity");
                output(chialisp_oxide_puzzles::precommit_claim::puzzle(
                    (
                        CoinId(fixed_atom(&a, terms[0])?),
                        Message32(fixed_atom(&a, terms[1])?),
                        PuzzleHash(fixed_atom(&a, terms[2])?),
                    ),
                    boolean(&a, args[1])?,
                    CoinId(fixed_atom(&a, args[2])?),
                ))
            }
            "precommit-registry" => {
                ensure!(args.len() == 5, "arity");
                let opening = self::args(&a, args[4])?;
                ensure!(opening.len() == 3, "opening arity");
                output(chialisp_oxide_puzzles::precommit_registry::puzzle(
                    PuzzleHash(fixed_atom(&a, args[0])?),
                    CoinId(fixed_atom(&a, args[1])?),
                    CoinId(fixed_atom(&a, args[2])?),
                    PuzzleHash(fixed_atom(&a, args[3])?),
                    (
                        public_key(&a, opening[0])?,
                        PuzzleHash(fixed_atom(&a, opening[1])?),
                        fixed_atom(&a, opening[2])?,
                    ),
                ))
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
    use super::{Encode, boomerang_arguments, execute, output, public_key};
    use chialisp_oxide::Cons;
    use clvmr::{Allocator, NodePtr};

    fn boomerang_terms() -> [String; 5] {
        [
            format!("0x{}", "07".repeat(48)),
            format!("0x{}", "42".repeat(32)),
            format!("0x{}", "43".repeat(32)),
            "1000".to_string(),
            "100".to_string(),
        ]
    }

    fn boomerang_solution(terms: &[String], claim: &str) -> String {
        format!("(({}) {claim})", terms.join(" "))
    }

    #[test]
    fn optional_outputs_preserve_nil_and_singleton_payloads() {
        for (actual, expected) in [
            (output(None::<()>).unwrap(), "80"),
            (output(Some(())).unwrap(), "ff8080"),
            (output(None::<u64>).unwrap(), "80"),
            (output(Some(0_u64)).unwrap(), "ff8080"),
            (output(Some(1_u64)).unwrap(), "ff0180"),
            (output(None::<Cons<u64, u64>>).unwrap(), "80"),
            (
                output(Some(Cons {
                    first: 1_u64,
                    rest: 2_u64,
                }))
                .unwrap(),
                "ffff010280",
            ),
            (
                output(Cons {
                    first: 1_u64,
                    rest: None::<u64>,
                })
                .unwrap(),
                "ff0180",
            ),
            (
                output(Cons {
                    first: 1_u64,
                    rest: Some(0_u64),
                })
                .unwrap(),
                "ff01ff8080",
            ),
        ] {
            assert_eq!(hex::encode(actual), expected);
        }
    }

    #[test]
    fn optional_payload_is_encoded_once_and_codec_errors_propagate() {
        struct Counted<'a>(&'a std::cell::Cell<usize>);
        impl Encode for Counted<'_> {
            fn encode(&self, _a: &mut Allocator) -> anyhow::Result<NodePtr> {
                self.0.set(self.0.get() + 1);
                Ok(NodePtr::NIL)
            }
        }
        let calls = std::cell::Cell::new(0);
        assert_eq!(output(Some(Counted(&calls))).unwrap(), [0xff, 0x80, 0x80]);
        assert_eq!(calls.get(), 1);

        struct FailedPayload;
        impl Encode for FailedPayload {
            fn encode(&self, _a: &mut Allocator) -> anyhow::Result<NodePtr> {
                anyhow::bail!("native output codec failed")
            }
        }
        assert!(
            output(Some(FailedPayload))
                .unwrap_err()
                .to_string()
                .contains("native output codec failed")
        );
        let mut limited = Allocator::new_limited(1);
        assert!(Some([0x80_u8; 48]).encode(&mut limited).is_err());
    }

    #[test]
    fn public_key_decoder_requires_exact_atom_width_and_preserves_bytes() {
        let mut a = Allocator::new();
        let key: [u8; 48] = std::array::from_fn(|index| index as u8);
        let node = a.new_atom(&key).unwrap();
        assert_eq!(public_key(&a, node).unwrap().0, key);
        for width in [0, 1, 47, 49, 1024] {
            let node = a.new_atom(&vec![7; width]).unwrap();
            assert!(public_key(&a, node).is_err(), "width {width}");
        }
        let pair = a.new_pair(node, NodePtr::NIL).unwrap();
        assert!(public_key(&a, pair).is_err());
    }

    #[test]
    fn boomerang_native_decodes_canonical_scalars_and_full_u64_domain() {
        let mut a = Allocator::new();
        for (scalar, expected) in [
            ("()", 0),
            ("1", 1),
            ("127", 127),
            ("128", 128),
            ("0x00ffffffffffffffff", u64::MAX),
        ] {
            for claim in ["()", "1"] {
                let mut terms = boomerang_terms();
                terms[3] = scalar.to_string();
                terms[4] = scalar.to_string();
                let solution = boomerang_solution(&terms, claim);
                let env = rue_clvm::assemble(&mut a, &solution).unwrap();
                let ((key, recipient, refund_to, amount, deadline), decoded_claim) =
                    boomerang_arguments(&a, env).unwrap();
                assert_eq!(key.0, [7; 48]);
                assert_eq!(recipient.0, [0x42; 32]);
                assert_eq!(refund_to.0, [0x43; 32]);
                assert_eq!(amount.mojos(), expected);
                assert_eq!(deadline.unix_seconds(), expected);
                assert_eq!(decoded_claim, claim == "1");
                assert!(execute("boomerang", &solution).unwrap().is_some());
            }
        }
    }

    #[test]
    fn boomerang_native_rejects_malformed_shapes_widths_and_scalars() {
        let terms = boomerang_terms();
        let term_list = format!("({})", terms.join(" "));
        let mut malformed = vec![
            "()".to_string(),
            "0x00".to_string(),
            format!("({term_list})"),
            format!("({term_list} 1 1)"),
            format!("({term_list} . 1)"),
            format!("({term_list} 1 . 0x00)"),
            "(1 1)".to_string(),
            boomerang_solution(&terms[..4], "1"),
            format!("(({} 1) 1)", terms.join(" ")),
            format!("(({} . 0x00) 1)", terms.join(" ")),
        ];
        for (index, widths) in [(0, [0, 47, 49]), (1, [0, 31, 33]), (2, [0, 31, 33])] {
            for width in widths {
                let mut changed = terms.clone();
                changed[index] = if width == 0 {
                    "()".to_string()
                } else {
                    format!("0x{}", "07".repeat(width))
                };
                malformed.push(boomerang_solution(&changed, "1"));
            }
        }
        for index in 0..5 {
            let mut changed = terms.clone();
            changed[index] = "(1 . 2)".to_string();
            malformed.push(boomerang_solution(&changed, "1"));
        }
        for index in [3, 4] {
            for scalar in [
                "0x00",     // Nonempty zero.
                "0x0001",   // Padded positive value.
                "0x000080", // Excess sign padding.
                "-1",
                "0x80",                   // Negative signed atom.
                "0x010000000000000000",   // u64::MAX + 1.
                "0x00010000000000000000", // Too wide before integer parsing.
            ] {
                let mut changed = terms.clone();
                changed[index] = scalar.to_string();
                malformed.push(boomerang_solution(&changed, "1"));
            }
        }
        for claim in ["2", "0x00", "0x0001", "(1)", "(1 . 2)"] {
            malformed.push(boomerang_solution(&terms, claim));
        }
        for solution in malformed {
            assert_eq!(execute("boomerang", &solution).unwrap(), None, "{solution}");
        }
        // Input assembly failures are codec errors, never policy rejection.
        assert!(execute("boomerang", "(").is_err());
    }

    #[test]
    fn boomerang_native_calls_actual_policy_with_exact_optional_signature_tail() {
        let terms = boomerang_terms();
        for (claim, expected) in [
            (
                "1",
                format!(
                    "((85 100) (73 1000) (51 {} 1000) (50 {} 0x{}))",
                    terms[1],
                    terms[0],
                    hex::encode(b"chialisp-oxide/boomerang/claim:1")
                ),
            ),
            ("()", format!("((81 100) (73 1000) (51 {} 1000))", terms[2])),
        ] {
            let mut a = Allocator::new();
            let expected = rue_clvm::assemble(&mut a, &expected).unwrap();
            let expected = clvmr::serde::node_to_bytes(&a, expected).unwrap();
            assert_eq!(
                execute("boomerang", &boomerang_solution(&terms, claim))
                    .unwrap()
                    .unwrap(),
                expected
            );
        }
    }

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
