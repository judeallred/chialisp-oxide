use crate::{BinaryOp, Environment, Expr, Parameter, Program, Type};
use anyhow::{Result, bail, ensure};
use num_bigint::BigInt;

const MAX_FUNCTIONS: usize = 64;
const MAX_PARAMETERS: usize = 64;
const MAX_EXPRESSION_NODES: usize = 20_000;
const MAX_EXPRESSION_DEPTH: usize = 256;
const MAX_TYPE_NODES: usize = 20_000;
const MAX_VALIDATION_WORK: usize = 500_000;

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

#[derive(Default)]
struct Budget {
    expressions: usize,
    types: usize,
    work: usize,
}
impl Budget {
    fn work(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(
            self.work <= MAX_VALIDATION_WORK,
            "IR validation work budget exceeded"
        );
        Ok(())
    }
    fn validate_type(&mut self, ty: &Type, depth: usize) -> Result<()> {
        ensure!(depth < 32, "type nesting limit");
        self.types += 1;
        ensure!(self.types <= MAX_TYPE_NODES, "IR type node budget exceeded");
        self.work(1)?;
        match ty {
            Type::Int { .. } => {
                bounds(ty)?;
            }
            Type::Bytes { len } => ensure!(*len <= 4096, "byte array too large"),
            Type::Tuple { items } => {
                ensure!(items.len() <= 256, "tuple too large");
                for item in items {
                    self.validate_type(item, depth + 1)?;
                }
            }
            Type::Pair { first, rest } => {
                self.validate_type(first, depth + 1)?;
                self.validate_type(rest, depth + 1)?;
            }
            Type::Atom | Type::Tree | Type::Bool | Type::Unit => (),
        }
        Ok(())
    }
    fn clone_type(&mut self, ty: &Type) -> Result<Type> {
        // Type forwarding can otherwise copy a large aggregate many times even
        // when the expression-node budget is small.
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.work(1)?;
            if let Type::Tuple { items } = ty {
                pending.extend(items);
            } else if let Type::Pair { first, rest } = ty {
                pending.extend([first.as_ref(), rest.as_ref()]);
            }
        }
        Ok(ty.clone())
    }
    fn signature(&mut self, name: &str, parameters: &[Parameter], result: &Type) -> Result<()> {
        ensure!(name.len() <= 256, "program/function name too long");
        ensure!(parameters.len() <= MAX_PARAMETERS, "too many parameters");
        for parameter in parameters {
            ensure!(parameter.name.len() <= 256, "parameter name too long");
            self.validate_type(&parameter.ty, 0)?;
        }
        self.validate_type(result, 0)
    }
}

pub(crate) fn children<'a>(expr: &'a Expr, pending: &mut Vec<&'a Expr>) {
    match expr {
        Expr::Tuple { items }
        | Expr::Sha256 { parts: items }
        | Expr::Call {
            arguments: items, ..
        } => pending.extend(items),
        Expr::Apply {
            program: left,
            environment: right,
        }
        | Expr::Binary { left, right, .. } => {
            pending.push(left);
            pending.push(right);
        }
        Expr::AtomIsEmpty { value }
        | Expr::Not { value }
        | Expr::Field { value, .. }
        | Expr::TreeAtom { value }
        | Expr::TreeAtomValue { value }
        | Expr::TreeIsPair { value }
        | Expr::TreeFirst { value }
        | Expr::PairFirst { value }
        | Expr::PairRest { value }
        | Expr::TreeRest { value } => pending.push(value),
        Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
            pending.push(first);
            pending.push(rest);
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
        Expr::Let { value, body, .. } => {
            pending.push(value);
            pending.push(body);
        }
        Expr::Atom { .. }
        | Expr::Bytes { .. }
        | Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Arg { .. }
        | Expr::Local { .. }
        | Expr::Unit
        | Expr::Reject { .. } => (),
    }
}

impl Program {
    /// A bounded conservative capability scan, also safe before validation.
    /// Invalid oversized inputs report capability rather than hiding new nodes;
    /// full validation must still succeed before compiling or optimizing them.
    pub fn uses_pairs(&self) -> bool {
        if self.functions.len() > MAX_FUNCTIONS || self.parameters.len() > MAX_PARAMETERS {
            return true;
        }
        let mut types = vec![&self.result];
        types.extend(self.parameters.iter().map(|p| &p.ty));
        let mut expressions = vec![&self.body];
        for function in &self.functions {
            if function.parameters.len() > MAX_PARAMETERS {
                return true;
            }
            types.extend(function.parameters.iter().map(|p| &p.ty));
            types.push(&function.result);
            expressions.push(&function.body);
        }
        let mut visited = 0usize;
        while let Some(expression) = expressions.pop() {
            visited += 1;
            if visited > MAX_EXPRESSION_NODES {
                return true;
            }
            match expression {
                Expr::Pair { .. } | Expr::PairFirst { .. } | Expr::PairRest { .. } => return true,
                Expr::Let { ty, .. } | Expr::Binary { ty, .. } | Expr::Int { ty, .. } => {
                    types.push(ty)
                }
                Expr::Tuple { items } if items.len() > 256 => return true,
                Expr::Sha256 { parts } if parts.len() > 64 => return true,
                Expr::Call { arguments, .. } if arguments.len() > MAX_PARAMETERS => return true,
                _ => (),
            }
            children(expression, &mut expressions);
        }
        visited = 0;
        while let Some(ty) = types.pop() {
            visited += 1;
            if visited > MAX_TYPE_NODES {
                return true;
            }
            match ty {
                Type::Pair { .. } => return true,
                Type::Tuple { items } => {
                    if items.len() > 256 {
                        return true;
                    }
                    types.extend(items);
                }
                _ => (),
            }
        }
        false
    }
    /// Runtime constructs and strict multipart hashes require version 4,
    /// regardless of entry policy. This traversal never follows calls.
    pub fn uses_runtime(&self) -> bool {
        if !self.functions.is_empty() || self.uses_pairs() {
            return true;
        }
        let mut pending = vec![&self.body];
        while let Some(expr) = pending.pop() {
            if matches!(
                expr,
                Expr::Call { .. }
                    | Expr::Let { .. }
                    | Expr::Local { .. }
                    | Expr::Sha256 { .. }
                    | Expr::Apply { .. }
            ) {
                return true;
            }
            children(expr, &mut pending);
        }
        false
    }
    /// Hash semantics require a separate model; inspect dead arms and every
    /// function table entry as well as the entry expression.
    /// Dynamic application has no model: inspect dead arms and every function.
    pub fn uses_apply(&self) -> bool {
        let mut pending = vec![&self.body];
        pending.extend(self.functions.iter().map(|function| &function.body));
        while let Some(expression) = pending.pop() {
            if matches!(expression, Expr::Apply { .. }) {
                return true;
            }
            children(expression, &mut pending);
        }
        false
    }
    pub fn uses_sha256(&self) -> bool {
        let mut pending = vec![&self.body];
        pending.extend(self.functions.iter().map(|function| &function.body));
        while let Some(expression) = pending.pop() {
            if matches!(expression, Expr::Sha256 { .. }) {
                return true;
            }
            children(expression, &mut pending);
        }
        false
    }
    pub(crate) fn validate_environment(&self) -> Result<()> {
        ensure!(
            self.environment != Environment::Ignored || self.parameters.is_empty(),
            "ignored environment requires zero parameters"
        );
        ensure!(
            self.environment != Environment::Raw
                || (self.parameters.len() == 1 && self.parameters[0].ty == Type::Tree),
            "raw environment requires exactly one tree parameter"
        );
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_environment()?;
        ensure!(self.functions.len() <= MAX_FUNCTIONS, "too many functions");
        let mut validator = Validator {
            program: self,
            budget: Budget::default(),
        };
        validator
            .budget
            .signature(&self.name, &self.parameters, &self.result)?;
        // Collect every signature first. Recursive calls use declared types,
        // never recursive validation or optimistic termination assumptions.
        for (index, function) in self.functions.iter().enumerate() {
            ensure!(
                function.id == index,
                "function IDs must be dense table indices"
            );
            validator
                .budget
                .signature(&function.name, &function.parameters, &function.result)?;
        }
        let ty = validator.infer(&self.body, &self.parameters, &mut Vec::new(), 0)?;
        ensure!(
            ty.is_none() || ty.as_ref() == Some(&self.result),
            "body/result type mismatch"
        );
        for function in &self.functions {
            let ty = validator.infer(&function.body, &function.parameters, &mut Vec::new(), 0)?;
            ensure!(
                ty.is_none() || ty.as_ref() == Some(&function.result),
                "function {} body/result type mismatch",
                function.id
            );
        }
        Ok(())
    }
    pub fn type_of(&self, expr: &Expr) -> Result<Option<Type>> {
        self.type_of_scoped(expr, &self.parameters, &[])
    }
    /// Infer in one function's argument scope and the supplied active lexical
    /// bindings. No caller-local bindings are implicitly available to callees.
    pub fn type_of_scoped(
        &self,
        expr: &Expr,
        parameters: &[Parameter],
        locals: &[(usize, Type)],
    ) -> Result<Option<Type>> {
        ensure!(parameters.len() <= MAX_PARAMETERS, "too many parameters");
        ensure!(
            locals.len() < MAX_EXPRESSION_DEPTH,
            "too many active locals"
        );
        let mut validator = Validator {
            program: self,
            budget: Budget::default(),
        };
        for parameter in parameters {
            validator.budget.validate_type(&parameter.ty, 0)?;
        }
        for (index, (id, ty)) in locals.iter().enumerate() {
            ensure!(
                !locals[..index].iter().any(|(other, _)| other == id),
                "duplicate active local {id}"
            );
            validator.budget.validate_type(ty, 0)?;
        }
        validator.infer(expr, parameters, &mut locals.to_vec(), 0)
    }
}

struct Validator<'a> {
    program: &'a Program,
    budget: Budget,
}
impl Validator<'_> {
    fn infer(
        &mut self,
        expr: &Expr,
        parameters: &[Parameter],
        locals: &mut Vec<(usize, Type)>,
        depth: usize,
    ) -> Result<Option<Type>> {
        ensure!(depth < MAX_EXPRESSION_DEPTH, "expression nesting limit");
        self.budget.expressions += 1;
        ensure!(
            self.budget.expressions <= MAX_EXPRESSION_NODES,
            "IR expression node budget exceeded"
        );
        self.budget.work(1 + locals.len())?;
        // Pair-only chains must not retain the larger legacy inference frame.
        // Every node still pays the same depth/node/work checks above exactly
        // once, and every child re-enters this dispatcher in its own scope.
        match expr {
            Expr::Pair { first, rest } => self.infer_pair(first, rest, parameters, locals, depth),
            Expr::PairFirst { value } | Expr::PairRest { value } => self.infer_pair_projection(
                value,
                matches!(expr, Expr::PairFirst { .. }),
                parameters,
                locals,
                depth,
            ),
            _ => self.infer_non_pair(expr, parameters, locals, depth),
        }
    }

    fn infer_non_pair(
        &mut self,
        expr: &Expr,
        parameters: &[Parameter],
        locals: &mut Vec<(usize, Type)>,
        depth: usize,
    ) -> Result<Option<Type>> {
        let ty = match expr {
            Expr::Pair { .. } | Expr::PairFirst { .. } | Expr::PairRest { .. } => {
                unreachable!("pair nodes enter their dedicated inference helpers")
            }
            Expr::Sha256 { parts } => {
                ensure!(parts.len() <= 64, "too many SHA256 parts");
                for part in parts {
                    let ty = self.infer(part, parameters, locals, depth + 1)?;
                    ensure!(
                        ty.is_none() || matches!(ty, Some(Type::Atom | Type::Bytes { .. })),
                        "SHA256 parts must be atom bytes"
                    );
                }
                Type::Bytes { len: 32 }
            }
            Expr::Apply {
                program,
                environment,
            } => {
                for operand in [program, environment] {
                    let ty = self.infer(operand, parameters, locals, depth + 1)?;
                    ensure!(
                        ty.is_none() || matches!(ty, Some(Type::Tree)),
                        "apply operands must be arbitrary trees"
                    );
                }
                Type::Tree
            }
            Expr::Call {
                function_id,
                arguments,
            } => {
                let program = self.program;
                let function = program
                    .functions
                    .get(*function_id)
                    .ok_or_else(|| anyhow::anyhow!("invalid function {function_id}"))?;
                ensure!(
                    function.id == *function_id,
                    "function IDs must be dense table indices"
                );
                ensure!(
                    arguments.len() == function.parameters.len(),
                    "call argument count mismatch for function {function_id}"
                );
                for (argument, parameter) in arguments.iter().zip(&function.parameters) {
                    let ty = self.infer(argument, parameters, locals, depth + 1)?;
                    ensure!(
                        ty.is_none() || ty.as_ref() == Some(&parameter.ty),
                        "call argument type mismatch for function {function_id}"
                    );
                }
                self.budget.clone_type(&function.result)?
            }
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                self.budget.validate_type(ty, 0)?;
                ensure!(
                    !locals.iter().any(|(id, _)| id == local_id),
                    "duplicate active local {local_id}"
                );
                // The initializer sees only the outer scope. Even a Reject
                // initializer does not exempt the body from type checking.
                let value_ty = self.infer(value, parameters, locals, depth + 1)?;
                ensure!(
                    value_ty.is_none() || value_ty.as_ref() == Some(ty),
                    "local initializer type mismatch"
                );
                locals.push((*local_id, self.budget.clone_type(ty)?));
                let result = self.infer(body, parameters, locals, depth + 1);
                locals.pop();
                return result;
            }
            Expr::Local { local_id } => {
                let (_, ty) = locals
                    .iter()
                    .rev()
                    .find(|(id, _)| id == local_id)
                    .ok_or_else(|| anyhow::anyhow!("unbound local {local_id}"))?;
                self.budget.clone_type(ty)?
            }
            Expr::Atom { hex } => {
                ensure!(hex.len() <= 8192, "byte literal too large");
                ensure!(hex::decode(hex)?.len() <= 4096, "atom literal too large");
                Type::Atom
            }
            Expr::AtomIsEmpty { value } => {
                let ty = self.infer(value, parameters, locals, depth + 1)?;
                ensure!(
                    ty.is_none() || ty == Some(Type::Atom),
                    "atom emptiness test requires an atom"
                );
                Type::Bool
            }
            Expr::TreeAtom { value } => {
                let ty = self.infer(value, parameters, locals, depth + 1)?;
                ensure!(
                    ty.is_none() || matches!(ty, Some(Type::Atom | Type::Bytes { .. })),
                    "tree atom requires atom bytes"
                );
                if ty.is_none() {
                    return Ok(None);
                }
                Type::Tree
            }
            Expr::TreePair { first, rest } => {
                let first = self.infer(first, parameters, locals, depth + 1)?;
                let rest = self.infer(rest, parameters, locals, depth + 1)?;
                ensure!(
                    first.is_none() || first == Some(Type::Tree),
                    "tree pair first requires a tree"
                );
                ensure!(
                    rest.is_none() || rest == Some(Type::Tree),
                    "tree pair rest requires a tree"
                );
                if first.is_none() || rest.is_none() {
                    return Ok(None);
                }
                Type::Tree
            }
            Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::TreeRest { value }
            | Expr::TreeAtomValue { value } => {
                let ty = self.infer(value, parameters, locals, depth + 1)?;
                ensure!(
                    ty.is_none() || ty == Some(Type::Tree),
                    "tree operation requires a tree"
                );
                if ty.is_none() {
                    return Ok(None);
                }
                match expr {
                    Expr::TreeIsPair { .. } => Type::Bool,
                    Expr::TreeAtomValue { .. } => Type::Atom,
                    _ => Type::Tree,
                }
            }
            Expr::Unit => Type::Unit,
            Expr::Bool { .. } => Type::Bool,
            Expr::Int { value, ty } => {
                ensure!(value.len() <= 64, "integer literal text too large");
                self.budget.validate_type(ty, 0)?;
                let (min, max) = bounds(ty)?;
                let number = value.parse::<BigInt>()?;
                ensure!(
                    number >= min && number <= max,
                    "integer literal out of range"
                );
                self.budget.clone_type(ty)?
            }
            Expr::Bytes { hex } => {
                ensure!(hex.len() <= 8192, "byte literal too large");
                let bytes = hex::decode(hex)?;
                Type::Bytes { len: bytes.len() }
            }
            Expr::Arg { index } => {
                let parameter = parameters
                    .get(*index)
                    .ok_or_else(|| anyhow::anyhow!("invalid argument {index}"))?;
                self.budget.clone_type(&parameter.ty)?
            }
            Expr::Reject { message } => {
                ensure!(message.len() <= 4096, "rejection message too large");
                return Ok(None);
            }
            Expr::Tuple { items } => {
                ensure!(items.len() <= 256, "tuple too large");
                let mut types = Vec::with_capacity(items.len());
                for item in items {
                    types.push(
                        self.infer(item, parameters, locals, depth + 1)?
                            .ok_or_else(|| anyhow::anyhow!("diverging tuple field"))?,
                    );
                }
                Type::Tuple { items: types }
            }
            Expr::Field { value, index } => {
                match self.infer(value, parameters, locals, depth + 1)? {
                    Some(Type::Tuple { items }) => {
                        let ty = items
                            .get(*index)
                            .ok_or_else(|| anyhow::anyhow!("invalid tuple field"))?;
                        self.budget.clone_type(ty)?
                    }
                    Some(Type::Bytes { len }) if *index < len => Type::Int {
                        bits: 8,
                        signed: false,
                    },
                    _ => bail!("field projection requires tuple or bytes"),
                }
            }
            Expr::Sequence { first, then_value } => {
                self.infer(first, parameters, locals, depth + 1)?;
                return self.infer(then_value, parameters, locals, depth + 1);
            }
            Expr::Not { value } => {
                let operand = self.infer(value, parameters, locals, depth + 1)?;
                ensure!(
                    operand.is_none() || operand == Some(Type::Bool),
                    "not requires bool"
                );
                Type::Bool
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                ensure!(
                    self.infer(condition, parameters, locals, depth + 1)? == Some(Type::Bool),
                    "if requires bool"
                );
                let left = self.infer(then_value, parameters, locals, depth + 1)?;
                let right = self.infer(else_value, parameters, locals, depth + 1)?;
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
                self.budget.validate_type(ty, 0)?;
                let l = self.infer(left, parameters, locals, depth + 1)?;
                let r = self.infer(right, parameters, locals, depth + 1)?;
                ensure!(
                    l.as_ref().is_none_or(|left| left == ty)
                        && r.as_ref().is_none_or(|right| right == ty),
                    "binary operand type mismatch"
                );
                // The explicit operand type also ascribes a rejecting child,
                // which has no returned value. Still inspect both children and
                // validate the operation/type below, even when both reject.
                match kind {
                    BinaryOp::Eq | BinaryOp::Ne => {
                        ensure!(
                            matches!(
                                ty,
                                Type::Int { .. } | Type::Bool | Type::Bytes { .. } | Type::Atom
                            ),
                            "equality requires scalar"
                        );
                        Type::Bool
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !matches!(ty, Type::Atom | Type::Bytes { .. }) {
                            bounds(ty)?;
                        }
                        Type::Bool
                    }
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                        bounds(ty)?;
                        self.budget.clone_type(ty)?
                    }
                    BinaryOp::Bitand | BinaryOp::Bitor | BinaryOp::Bitxor => {
                        if *ty != Type::Bool {
                            bounds(ty)?;
                        }
                        self.budget.clone_type(ty)?
                    }
                    _ => bail!("operation {kind:?} has no validated lowering yet"),
                }
            }
        };
        Ok(Some(ty))
    }

    // Keep pair result temporaries outside the general recursive inference
    // frame. Existing deep input must reach the same typed nesting error on
    // the default native stack after adding new expression variants.
    fn infer_pair(
        &mut self,
        first: &Expr,
        rest: &Expr,
        parameters: &[Parameter],
        locals: &mut Vec<(usize, Type)>,
        depth: usize,
    ) -> Result<Option<Type>> {
        let first = self.infer(first, parameters, locals, depth + 1)?;
        let rest = self.infer(rest, parameters, locals, depth + 1)?;
        // Even a bottom first child never exempts the rest from checking.
        let (Some(first), Some(rest)) = (first, rest) else {
            return Ok(None);
        };
        Ok(Some(Type::Pair {
            first: Box::new(first),
            rest: Box::new(rest),
        }))
    }

    fn infer_pair_projection(
        &mut self,
        value: &Expr,
        first: bool,
        parameters: &[Parameter],
        locals: &mut Vec<(usize, Type)>,
        depth: usize,
    ) -> Result<Option<Type>> {
        let Some(ty) = self.infer(value, parameters, locals, depth + 1)? else {
            return Ok(None);
        };
        let Type::Pair { first: head, rest } = ty else {
            bail!("typed pair projection requires a pair");
        };
        Ok(Some(if first { *head } else { *rest }))
    }
}
