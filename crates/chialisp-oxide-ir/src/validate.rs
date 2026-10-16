use crate::{BinaryOp, Expr, Program, Type};
use anyhow::{Result, bail, ensure};
use num_bigint::BigInt;

pub fn bounds(ty: &Type) -> Result<(BigInt, BigInt)> {
    let Type::Int { bits, signed } = ty else {
        bail!("expected integer type")
    };
    ensure!(
        [8, 16, 32, 64, 128].contains(bits),
        "unsupported integer width {bits}"
    );
    let magnitude = BigInt::from(1) << usize::from(*bits - u16::from(*signed));
    Ok((
        if *signed {
            -&magnitude
        } else {
            BigInt::from(0)
        },
        magnitude - 1,
    ))
}
fn validate_type(ty: &Type, depth: usize) -> Result<()> {
    ensure!(depth < 32, "type nesting limit");
    match ty {
        Type::Int { .. } => {
            bounds(ty)?;
        }
        Type::Bytes { len } => ensure!(*len <= 4096, "byte array too large"),
        Type::Tuple { items } => {
            ensure!(items.len() <= 256, "tuple too large");
            for item in items {
                validate_type(item, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}
impl Program {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.parameters.len() <= 64, "too many parameters");
        ensure!(self.name.len() <= 256, "program name too long");
        let mut pending = vec![&self.body];
        let mut nodes = 0usize;
        while let Some(expr) = pending.pop() {
            nodes += 1;
            ensure!(nodes <= 20_000, "IR expression node budget exceeded");
            match expr {
                Expr::Int { value, .. } => {
                    ensure!(value.len() <= 64, "integer literal text too large")
                }
                Expr::Bytes { hex } => ensure!(hex.len() <= 8192, "byte literal too large"),
                Expr::Reject { message } => {
                    ensure!(message.len() <= 4096, "rejection message too large")
                }
                Expr::Tuple { items } => pending.extend(items),
                Expr::Binary { left, right, .. } => {
                    pending.push(left);
                    pending.push(right);
                }
                Expr::Not { value } | Expr::Field { value, .. } => pending.push(value),
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
                _ => (),
            }
        }
        for parameter in &self.parameters {
            validate_type(&parameter.ty, 0)?;
        }
        validate_type(&self.result, 0)?;
        let ty = self.infer(&self.body, 0)?;
        ensure!(
            ty.is_none() || ty == Some(self.result.clone()),
            "body/result type mismatch"
        );
        Ok(())
    }
    pub fn type_of(&self, expr: &Expr) -> Result<Option<Type>> {
        self.infer(expr, 0)
    }
    fn infer(&self, expr: &Expr, depth: usize) -> Result<Option<Type>> {
        ensure!(depth < 256, "expression nesting limit");
        let infer = |e: &Expr| self.infer(e, depth + 1);
        let ty = match expr {
            Expr::Unit => Type::Unit,
            Expr::Bool { .. } => Type::Bool,
            Expr::Int { value, ty } => {
                let (min, max) = bounds(ty)?;
                let number = value.parse::<BigInt>()?;
                ensure!(
                    number >= min && number <= max,
                    "integer literal out of range"
                );
                ty.clone()
            }
            Expr::Bytes { hex } => {
                let bytes = hex::decode(hex)?;
                ensure!(bytes.len() <= 4096, "byte literal too large");
                Type::Bytes { len: bytes.len() }
            }
            Expr::Arg { index } => self
                .parameters
                .get(*index)
                .ok_or_else(|| anyhow::anyhow!("invalid argument {index}"))?
                .ty
                .clone(),
            Expr::Reject { .. } => return Ok(None),
            Expr::Tuple { items } => {
                ensure!(items.len() <= 256, "tuple too large");
                Type::Tuple {
                    items: items
                        .iter()
                        .map(|e| infer(e)?.ok_or_else(|| anyhow::anyhow!("diverging tuple field")))
                        .collect::<Result<_>>()?,
                }
            }
            Expr::Field { value, index } => match infer(value)? {
                Some(Type::Tuple { items }) => items
                    .get(*index)
                    .ok_or_else(|| anyhow::anyhow!("invalid tuple field"))?
                    .clone(),
                Some(Type::Bytes { len }) if *index < len => Type::Int {
                    bits: 8,
                    signed: false,
                },
                _ => bail!("field projection requires tuple or bytes"),
            },
            Expr::Sequence { first, then_value } => {
                infer(first)?;
                return infer(then_value);
            }
            Expr::Not { value } => {
                ensure!(infer(value)? == Some(Type::Bool), "not requires bool");
                Type::Bool
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                ensure!(infer(condition)? == Some(Type::Bool), "if requires bool");
                let left = infer(then_value)?;
                let right = infer(else_value)?;
                ensure!(
                    left.is_none() || right.is_none() || left == right,
                    "branch type mismatch"
                );
                return Ok(left.or(right));
            }
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => {
                let l = infer(left)?;
                let r = infer(right)?;
                ensure!(
                    l == Some(ty.clone()) && l == r,
                    "binary operand type mismatch"
                );
                match kind {
                    BinaryOp::Eq | BinaryOp::Ne => {
                        ensure!(
                            matches!(ty, Type::Int { .. } | Type::Bool | Type::Bytes { .. }),
                            "equality requires scalar"
                        );
                        Type::Bool
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        bounds(ty)?;
                        Type::Bool
                    }
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                        bounds(ty)?;
                        ty.clone()
                    }
                    _ => bail!("operation {kind:?} has no validated lowering yet"),
                }
            }
        };
        Ok(Some(ty))
    }
}
