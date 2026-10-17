//! Expand only lexical SMT let bindings in already size-bounded solver data.
//! This is not an evaluator for SMT operators: the tree/sequence grammar still
//! validates the resulting constructors, sorts and byte literals afterwards.
use crate::SExpression;
use anyhow::{Context, Result, ensure};

#[derive(Clone, Copy)]
struct Binding<'a> {
    name: &'a str,
    value: &'a SExpression,
    outer: usize,
}

pub(crate) fn expand(value: &SExpression) -> Result<SExpression> {
    fn visit<'a>(
        value: &'a SExpression,
        bindings: &[Binding<'a>],
        depth: usize,
        work: &mut usize,
    ) -> Result<SExpression> {
        ensure!(
            depth <= 128 && *work > 0,
            "solver let expansion depth/work limit"
        );
        *work -= 1;
        match value {
            SExpression::Atom(name) => {
                if let Some(binding) = bindings.iter().rev().find(|binding| binding.name == name) {
                    return visit(binding.value, &bindings[..binding.outer], depth + 1, work);
                }
                ensure!(name.len() <= 1024, "solver data token too large");
                Ok(SExpression::Atom(name.clone()))
            }
            SExpression::List(items) => {
                if matches!(items.first(), Some(SExpression::Atom(name)) if name == "let") {
                    let [_, SExpression::List(definitions), body] = items.as_slice() else {
                        anyhow::bail!("malformed solver let binding");
                    };
                    ensure!(
                        bindings.len().saturating_add(definitions.len()) <= 128,
                        "solver let binding limit"
                    );
                    ensure!(
                        bindings.len().saturating_add(definitions.len()) <= *work,
                        "solver let expansion work limit"
                    );
                    *work -= bindings.len() + definitions.len();
                    let outer = bindings.len();
                    let mut next = bindings.to_vec();
                    for definition in definitions {
                        let SExpression::List(pair) = definition else {
                            anyhow::bail!("malformed solver let definition")
                        };
                        let [SExpression::Atom(name), value] = pair.as_slice() else {
                            anyhow::bail!("malformed solver let definition")
                        };
                        ensure!(
                            !name.is_empty()
                                && name.len() <= 128
                                && !next[outer..].iter().any(|binding| binding.name == name),
                            "invalid/duplicate solver let symbol"
                        );
                        // SMT lets are simultaneous: each RHS sees only the
                        // environment before this complete group of bindings.
                        next.push(Binding { name, value, outer });
                    }
                    visit(body, &next, depth + 1, work)
                } else {
                    ensure!(items.len() <= *work, "solver let expansion work limit");
                    Ok(SExpression::List(
                        items
                            .iter()
                            .map(|item| visit(item, bindings, depth + 1, work))
                            .collect::<Result<_>>()?,
                    ))
                }
            }
        }
    }
    visit(value, &[], 0, &mut 100_000).context("invalid structural solver let data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_values;
    fn data(text: &str) -> SExpression {
        parse_values(&format!("((env {text}))"))
            .unwrap()
            .remove("env")
            .unwrap()
    }
    #[test]
    fn simultaneous_let_shadowing_uses_the_outer_environment() {
        let value = data("(let ((x 01)) (let ((x 02) (y x)) (pair x y)))");
        let expanded = expand(&value).unwrap();
        assert_eq!(
            format!("{expanded:?}"),
            format!("{:?}", data("(pair 02 01)"))
        );
    }
    #[test]
    fn malformed_duplicate_and_exponential_aliases_fail_closed() {
        for text in ["(let (x) x)", "(let ((x 01) (x 02)) x)", "(let ((x 01)))"] {
            assert!(expand(&data(text)).is_err());
        }
        // Build a valid nested alias DAG whose expansion would be exponential.
        let mut source = "x18".to_owned();
        for i in (1..=18).rev() {
            source = format!("(let ((x{i} (pair x{} x{}))) {source})", i - 1, i - 1);
        }
        source = format!(
            "(let ((x0 {})) {source})",
            "(atom (as seq.empty (Seq (_ BitVec 8))))"
        );
        assert!(expand(&data(&source)).is_err());
    }
}
