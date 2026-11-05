//! Internal callable capability scans and canonical closed target discovery.
use crate::{Expr, Program, Type};
use anyhow::{Result, ensure};

const MAX_SCAN: usize = 500_000;

fn contains(ty: &Type, visited: &mut usize) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        *visited = visited.saturating_add(1);
        if *visited > 20_000 {
            return true;
        }
        let count = match ty {
            Type::Callable { .. } => return true,
            Type::Tuple { items } => items.len(),
            Type::Pair { .. } => 2,
            Type::Optional { .. } => 1,
            _ => 0,
        };
        if count > 20_000usize.saturating_sub(pending.len() + *visited) {
            return true;
        }
        match ty {
            Type::Tuple { items } => pending.extend(items),
            Type::Pair { first, rest } => pending.extend([first.as_ref(), rest.as_ref()]),
            Type::Optional { item } => pending.push(item),
            _ => (),
        }
    }
    false
}
impl Type {
    /// Conservative bounded discovery, safe before validation.
    pub fn contains_callable(&self) -> bool {
        contains(self, &mut 0)
    }
}

impl Program {
    /// Retained types, dead branches and unused table bodies all require v9.
    pub fn uses_callables(&self) -> bool {
        if self.functions.len() > 64
            || self.parameters.len() > 64
            || self.functions.iter().any(|f| f.parameters.len() > 64)
        {
            return true;
        }
        let mut types = 0usize;
        if contains(&self.result, &mut types)
            || self.parameters.iter().any(|p| contains(&p.ty, &mut types))
            || self.functions.iter().any(|f| {
                contains(&f.result, &mut types)
                    || f.parameters.iter().any(|p| contains(&p.ty, &mut types))
            })
        {
            return true;
        }
        let mut pending = vec![&self.body];
        pending.extend(self.functions.iter().map(|f| &f.body));
        let mut seen = 0usize;
        while let Some(expr) = pending.pop() {
            seen += 1;
            if seen > 20_000 {
                return true;
            }
            match expr {
                Expr::FunctionRef { .. } | Expr::CallIndirect { .. } => return true,
                Expr::Let { ty, .. } | Expr::Int { ty, .. } | Expr::Binary { ty, .. }
                    if contains(ty, &mut types) =>
                {
                    return true;
                }
                Expr::OptionalNone { item } if contains(item, &mut types) => return true,
                Expr::IntCast(crate::IntegerCast { source, target, .. })
                    if contains(source, &mut types) || contains(target, &mut types) =>
                {
                    return true;
                }
                _ => (),
            }
            if !crate::validate::capability_children(expr, &mut pending, seen) {
                return true;
            }
        }
        false
    }

    /// Canonical targets are derived from every FunctionRef in the validated
    /// program, never from an unverified call-site allowlist or input bytes.
    pub fn callable_targets(&self, callable: &Type) -> Result<Vec<usize>> {
        let mut work = 0usize;
        let mut charge = |n| {
            work = work.saturating_add(n);
            ensure!(work <= MAX_SCAN, "callable target work budget exceeded");
            Ok(())
        };
        let references = self.callable_references(&mut charge)?;
        self.match_callable_targets(callable, &references, charge)
    }

    pub(crate) fn callable_references(
        &self,
        mut charge: impl FnMut(usize) -> Result<()>,
    ) -> Result<Vec<usize>> {
        ensure!(
            self.functions.len() <= 64,
            "callable function table bound exceeded"
        );
        charge(1 + self.functions.len())?;
        let mut pending = vec![&self.body];
        pending.extend(self.functions.iter().map(|f| &f.body));
        let mut ids = [false; 64];
        let mut visited = 0usize;
        while let Some(expr) = pending.pop() {
            visited += 1;
            ensure!(visited <= 20_000, "callable reference scan budget exceeded");
            let count = crate::validate::child_count(expr);
            charge(1usize.saturating_add(count))?;
            if let Expr::FunctionRef { function_id } = expr {
                ensure!(
                    *function_id < self.functions.len(),
                    "invalid callable function ID"
                );
                ids[*function_id] = true;
            }
            ensure!(
                crate::validate::capability_children(expr, &mut pending, visited),
                "callable scan queue budget exceeded"
            );
        }
        charge(self.functions.len())?;
        Ok(ids
            .into_iter()
            .enumerate()
            .filter_map(|(id, used)| used.then_some(id))
            .collect())
    }

    pub(crate) fn match_callable_targets(
        &self,
        callable: &Type,
        references: &[usize],
        mut charge: impl FnMut(usize) -> Result<()>,
    ) -> Result<Vec<usize>> {
        let Type::Callable { parameters, result } = callable else {
            anyhow::bail!("indirect callee must be callable");
        };
        ensure!(
            self.functions.len() <= 64 && parameters.len() <= 64 && references.len() <= 64,
            "callable target signature bound exceeded"
        );
        charge(references.len())?;
        let mut targets = Vec::new();
        let mut work = 0usize;
        for index in references {
            let function = self
                .functions
                .get(*index)
                .ok_or_else(|| anyhow::anyhow!("invalid reified target"))?;
            ensure!(
                function.parameters.len() <= 64,
                "callable parameter bound exceeded"
            );
            charge(2 + parameters.len() + function.parameters.len())?;
            let mut types = vec![(result.as_ref(), 0), (&function.result, 0)];
            types.extend(parameters.iter().map(|ty| (ty, 0)));
            types.extend(function.parameters.iter().map(|p| (&p.ty, 0)));
            while let Some((ty, depth)) = types.pop() {
                work += 1;
                charge(1)?;
                ensure!(
                    work <= MAX_SCAN && depth < 32,
                    "callable signature comparison bound exceeded"
                );
                match ty {
                    Type::Tuple { items } => {
                        ensure!(
                            items.len() <= 256
                                && items.len() <= MAX_SCAN.saturating_sub(work + types.len()),
                            "callable signature queue budget exceeded"
                        );
                        charge(items.len())?;
                        types.extend(items.iter().map(|ty| (ty, depth + 1)));
                    }
                    Type::Pair { first, rest } => {
                        ensure!(
                            2 <= MAX_SCAN.saturating_sub(work + types.len()),
                            "callable signature queue budget exceeded"
                        );
                        charge(2)?;
                        types.extend([(first.as_ref(), depth + 1), (rest.as_ref(), depth + 1)]);
                    }
                    Type::Optional { item } => {
                        ensure!(
                            work + types.len() < MAX_SCAN,
                            "callable signature queue budget exceeded"
                        );
                        charge(1)?;
                        types.push((item, depth + 1));
                    }
                    Type::Callable { .. } => {
                        anyhow::bail!("reified target signatures cannot contain callables")
                    }
                    _ => (),
                }
            }
            // Both compared structures have been bounded/charged completely.
            if function.parameters.len() == parameters.len()
                && function
                    .parameters
                    .iter()
                    .zip(parameters)
                    .all(|(p, t)| p.ty == *t)
                && function.result == **result
            {
                targets.push(*index);
            }
        }
        Ok(targets)
    }
}
