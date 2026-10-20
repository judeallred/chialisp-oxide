//! Exact finite-word representation for programs containing integer bitwise
//! operations. Other programs retain their original integer-only queries.
use anyhow::{Result, bail};
use chialisp_oxide_ir::{BinaryOp, Expr, Type};
use num_bigint::BigInt;

use crate::{Translation, Value, bounds};

/// Called after full validation and runtime-model rejection. Traverse every
/// expression, including dead branches, without cloning it or changing scope.
pub(super) fn contains_integer_bitwise(body: &Expr) -> bool {
    let mut pending = vec![body];
    while let Some(expr) = pending.pop() {
        match expr {
            Expr::Binary {
                kind: BinaryOp::Bitand | BinaryOp::Bitor | BinaryOp::Bitxor,
                ty: Type::Int { .. },
                ..
            } => return true,
            Expr::Binary { left, right, .. } => {
                pending.push(left);
                pending.push(right);
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                pending.push(condition);
                pending.push(then_value);
                pending.push(else_value);
            }
            Expr::Sequence { first, then_value } => {
                pending.push(first);
                pending.push(then_value);
            }
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
                pending.push(first);
                pending.push(rest);
            }
            Expr::Let { value, body, .. }
            | Expr::Apply {
                program: value,
                environment: body,
            } => {
                pending.push(value);
                pending.push(body);
            }
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => pending.extend(items),
            Expr::OptionalSome { value }
            | Expr::Not { value }
            | Expr::Field { value, .. }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value }
            | Expr::TreeRest { value }
            | Expr::AtomIsEmpty { value } => pending.push(value),
            Expr::OptionalNone { .. }
            | Expr::Local { .. }
            | Expr::Atom { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Bytes { .. }
            | Expr::Arg { .. }
            | Expr::Reject { .. } => (),
        }
    }
    false
}

impl Translation {
    fn integer_from_bits(&mut self, bits: u16, signed: bool, value: &str) -> String {
        let unsigned = self.define("Int", &format!("(bv2int {value})"));
        if !signed {
            return unsigned;
        }
        let modulus = BigInt::from(1u8) << usize::from(bits);
        let half = &modulus >> 1usize;
        self.define(
            "Int",
            &format!("(ite (>= {unsigned} {half}) (- {unsigned} {modulus}) {unsigned})"),
        )
    }

    pub(super) fn bitvector_input(&mut self, ty: &Type, name: &str) -> Result<Value> {
        bounds(ty)?;
        let Type::Int { bits, signed } = *ty else {
            bail!("expected fixed-width integer input")
        };
        let value = format!("{name}_bits");
        self.text
            .push_str(&format!("(declare-const {value} (_ BitVec {bits}))\n"));
        let interpreted = self.integer_from_bits(bits, signed, &value);
        self.text
            .push_str(&format!("(define-fun {name} () Int {interpreted})\n"));
        self.symbols.push(name.into());
        self.integer_bitvectors
            .insert((name.into(), bits, signed), value);
        // Every BV word denotes exactly one typed integer, and every integer
        // in the original input bounds has exactly one such word. No new
        // constraint, axiom, input sample or cryptographic assumption is added.
        Ok(Value::Int(name.into()))
    }

    fn bits_for_integer(&mut self, value: &str, bits: u16, signed: bool) -> String {
        let key = (value.to_owned(), bits, signed);
        if let Some(known) = self.integer_bitvectors.get(&key) {
            return known.clone();
        }
        // Checked arithmetic and other integer expressions remain mathematical
        // integers. Convert their value modulo2^width only at a bitwise use;
        // their separately propagated success/overflow conditions still apply.
        let result = self.define(
            &format!("(_ BitVec {bits})"),
            &format!("((_ int2bv {bits}) {value})"),
        );
        self.integer_bitvectors.insert(key, result.clone());
        result
    }

    pub(super) fn bitwise(
        &mut self,
        kind: BinaryOp,
        left: &str,
        right: &str,
        ty: &Type,
    ) -> Result<Value> {
        let op = match kind {
            BinaryOp::Bitand => "and",
            BinaryOp::Bitor => "or",
            BinaryOp::Bitxor => "xor",
            _ => bail!("expected a bitwise operation"),
        };
        if *ty == Type::Bool {
            return Ok(Value::Bool(
                self.define("Bool", &format!("({op} {left} {right})")),
            ));
        }
        bounds(ty)?;
        let Type::Int { bits, signed } = *ty else {
            bail!("bitwise SMT requires fixed-width integer or bool")
        };
        let a = self.bits_for_integer(left, bits, signed);
        let b = self.bits_for_integer(right, bits, signed);
        let bitvector = self.define(&format!("(_ BitVec {bits})"), &format!("(bv{op} {a} {b})"));
        let value = self.integer_from_bits(bits, signed, &bitvector);
        self.integer_bitvectors
            .insert((value.clone(), bits, signed), bitvector);
        Ok(Value::Int(value))
    }
}
