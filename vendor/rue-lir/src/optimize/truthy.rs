use id_arena::Arena;

use crate::{Lir, LirId, optimize::optimize};

pub fn opt_truthy(arena: &mut Arena<Lir>, value: LirId) -> Result<bool, LirId> {
    match arena[value].clone() {
        Lir::Atom(atom) => Ok(!atom.is_empty()),
        // A constructed pair is truthy only after both fields have evaluated.
        // Keep that evaluation: either field may raise or otherwise fail.
        Lir::Cons(..) => Err(value),
        Lir::Listp(inner, atom_can_be_truthy) => {
            if atom_can_be_truthy {
                Err(value)
            } else {
                opt_truthy(arena, inner)
            }
        }
        Lir::Not(inner) => {
            // A truthiness-only consumer does not observe Bool's exact bytes.
            // Removing both nots still evaluates the original value once.
            if let Lir::Not(value) = arena[inner].clone() {
                return opt_truthy(arena, value);
            }

            if let Lir::Eq(left, right) = arena[inner].clone() {
                if let Lir::Atom(left) = arena[left].clone()
                    && left.is_empty()
                {
                    return Err(right);
                }

                if let Lir::Atom(right) = arena[right].clone()
                    && right.is_empty()
                {
                    return Err(left);
                }
            }

            Err(value)
        }
        Lir::Eq(left, right) => {
            if let Lir::Atom(left) = arena[left].clone()
                && left.is_empty()
            {
                let not = arena.alloc(Lir::Not(right));
                let not = optimize(arena, not);
                return opt_truthy(arena, not);
            }

            if let Lir::Atom(right) = arena[right].clone()
                && right.is_empty()
            {
                let not = arena.alloc(Lir::Not(left));
                let not = optimize(arena, not);
                return opt_truthy(arena, not);
            }

            Err(value)
        }
        _ => Err(value),
    }
}
