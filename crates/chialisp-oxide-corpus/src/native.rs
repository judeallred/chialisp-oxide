//! Reference execution calls the actual Rust sources also supplied to rustc-driver.
#![allow(clippy::type_complexity)] // Heterogeneous condition tuples are the current teaching ABI.
use anyhow::{Result, bail, ensure};
use chialisp_oxide::{Amount, Message32, PublicKey, PuzzleHash, Timestamp};
use chialisp_oxide_puzzles::{
    boolean_gate as gate, bounded_counter as counter, capped_payment as capped,
    fixed_payment as fixed, intro_add, intro_conditional, intro_list, recovery_payment as recovery,
    signature_payment as signature, split_payment as split, timelocked_payment as timelock,
    two_signature_payment as multisig,
};
use clvmr::{Allocator, NodePtr, allocator::SExp, serde::node_to_bytes};
use num_bigint::{BigInt, Sign};

trait Encode {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr>;
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
impl<const N: usize> Encode for [u8; N] {
    fn encode(&self, a: &mut Allocator) -> Result<NodePtr> {
        Ok(a.new_atom(self)?)
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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Vec<u8>> {
        // These annotated nullary exports explicitly ignore every assembled tree.
        // Keep the default exact argument decoder for all other policies.
        match slug {
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
            "recovery-payment" => {
                ensure!(args.len() == 2, "arity");
                output(recovery::puzzle(
                    boolean(&a, args[0])?,
                    Message32(bytes32(&a, args[1])?),
                ))
            }
            _ => bail!("unknown native fixture {slug}"),
        }
    }));
    Ok(result.ok().and_then(Result::ok))
}
