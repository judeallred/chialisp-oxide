//! Reviewed bridge from the SDK's reserved `apply` export to CLVM opcode 2.
//!
//! Like the SHA256 bridge, a candidate is located as a public root export of the
//! `chialisp_oxide` crate, then admitted only by its exact signature and the
//! pinned canonical fingerprint of its typed-MIR body (a bare `panic!()` without
//! the `vm` feature). The export name grants nothing: an ordinary local function
//! named `apply` keeps its real body, and a modified reserved body fails closed.
use super::Lowerer;
use super::trees::Ownership;
use rustc_hir::def_id::{CRATE_DEF_INDEX, DefId, LOCAL_CRATE};
use rustc_middle::{
    mir::{Body, Operand, Place},
    ty::{self, Ty},
};
use rustc_span::{Span, Spanned};
use sha2::{Digest, Sha256};

const MAX_ROOT_CHILDREN: usize = 4_096;
const MAX_PRIMITIVES: usize = 64;
const PINNED_COMMIT: &str = "daf2e5e18b3abc4311d9da3d96fd64138f43d508";

impl<'tcx> Lowerer<'tcx> {
    /// Recognize a call of the reserved `apply` export. `Ok(true)` means the
    /// caller must bind `Expr::Apply` strictly; `Ok(false)` means an ordinary
    /// call; `Err` means a candidate was called but is not admissible.
    pub(super) fn apply_primitive(
        &self,
        body: &Body<'tcx>,
        func: &Operand<'tcx>,
        args: &[Spanned<Operand<'tcx>>],
        destination: Place<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        let ty::FnDef(id, generic) = func.ty(&body.local_decls, self.tcx).kind() else {
            return Ok(false);
        };
        if !self.apply_candidates()?.contains(id) {
            return Ok(false);
        }
        let prefix = |reason| {
            self.error(
                span,
                format!("reviewed native apply primitive rejected: {reason}"),
            )
        };
        if args.len() != 2
            || generic
                .skip_binder()
                .iter()
                .any(|a| a.as_region().is_none())
        {
            return Err(prefix("unsupported actual call signature".into()));
        }
        self.authorize_apply(*id).map_err(&prefix)?;
        let signature = self.tcx.instantiate_bound_regions_with_erased(
            self.tcx.fn_sig(*id).instantiate_identity().skip_norm_wip(),
        );
        let erase = |t| self.tcx.erase_and_anonymize_regions(t);
        if args
            .iter()
            .zip(signature.inputs())
            .any(|(arg, ty)| erase(arg.node.ty(&body.local_decls, self.tcx)) != erase(*ty))
            || erase(destination.ty(&body.local_decls, self.tcx).ty) != erase(signature.output())
        {
            return Err(prefix(
                "actual operand/result types differ from reviewed signature".into(),
            ));
        }
        // The operands must be the admitted borrowed tree and the result that
        // same tree by value; structural admission, never the type's name.
        let tree = |t: Ty<'tcx>| match t.kind() {
            ty::Ref(_, inner, ty::Mutability::Not) => self
                .tree_shape(*inner, span)
                .ok()
                .flatten()
                .is_some_and(|shape| shape.ownership == Ownership::BorrowedRefs),
            _ => false,
        };
        let result = self
            .tree_shape(signature.output(), span)
            .ok()
            .flatten()
            .is_some_and(|shape| shape.ownership == Ownership::BorrowedRefs);
        if !signature.inputs().iter().all(|t| tree(*t)) || !result {
            return Err(prefix(
                "apply requires exactly fn(&Tree, &Tree) -> Tree over the admitted borrowed tree"
                    .into(),
            ));
        }
        Ok(true)
    }

    fn apply_candidates(&self) -> Result<Vec<DefId>, String> {
        if let Some(result) = self.apply_candidates.borrow().as_ref() {
            return result.clone();
        }
        let result = (|| {
            let crates = self.tcx.crates(());
            if crates.len() > 1_024 {
                return Err("reviewed native primitive crate graph exceeds 1,024 crates".into());
            }
            let mut candidates = Vec::new();
            for krate in std::iter::once(LOCAL_CRATE).chain(crates.iter().copied()) {
                if self.tcx.crate_name(krate).as_str() != "chialisp_oxide" {
                    continue;
                }
                let root = DefId {
                    krate,
                    index: CRATE_DEF_INDEX,
                };
                let children = if let Some(local) = root.as_local() {
                    self.tcx.module_children_local(local)
                } else {
                    self.tcx.module_children(root)
                };
                if children.len() > MAX_ROOT_CHILDREN {
                    return Err("reviewed native primitive module exceeds 4,096 exports".into());
                }
                for child in children {
                    if child.ident.name.as_str() == "apply"
                        && child.vis.is_public()
                        && let Some(id) = child.res.opt_def_id()
                    {
                        if candidates.len() >= MAX_PRIMITIVES {
                            return Err("reviewed native primitive candidate limit exceeded".into());
                        }
                        candidates.push(id);
                    }
                }
            }
            Ok(candidates)
        })();
        *self.apply_candidates.borrow_mut() = Some(result.clone());
        result
    }

    fn apply_signature(&self, id: DefId) -> Result<(), String> {
        if !matches!(self.tcx.def_kind(id), rustc_hir::def::DefKind::Fn)
            || !self.lifetime_parameters_only(id)
            || self.tcx.constness(id) != rustc_hir::Constness::NotConst
        {
            return Err("entry must be an ordinary non-const, lifetime-only function".into());
        }
        let sig = self.tcx.fn_sig(id).instantiate_identity().skip_binder();
        if sig.safety().is_unsafe()
            || sig.abi() != rustc_abi::ExternAbi::Rust
            || sig.c_variadic()
            || sig.inputs().len() != 2
        {
            return Err("entry requires exactly fn(&Tree, &Tree) -> Tree".into());
        }
        Ok(())
    }

    fn authorize_apply(&self, id: DefId) -> Result<(), String> {
        if let Some(result) = self.apply_primitives.borrow().get(&id) {
            return result.clone();
        }
        let result = (|| {
            if env!("CHIALISP_OXIDE_RUSTC_COMMIT") != PINNED_COMMIT {
                return Err("installed compiler differs from the reviewed compiler".into());
            }
            self.apply_signature(id)?;
            if !self.tcx.sess.overflow_checks()
                || !self.tcx.sess.opts.debug_assertions
                || self.tcx.sess.panic_strategy() != rustc_target::spec::PanicStrategy::Unwind
            {
                return Err(format!(
                    "profile requires overflow-checks=yes, debug-assertions=yes and panic=unwind; observed overflow-checks={}, debug-assertions={}, panic={:?}",
                    self.tcx.sess.overflow_checks(),
                    self.tcx.sess.opts.debug_assertions,
                    self.tcx.sess.panic_strategy()
                ));
            }
            let expected = match self.tcx.sess.opts.target_triple.tuple() {
                "aarch64-apple-darwin" => APPLY_FINGERPRINT_AARCH64_APPLE_DARWIN,
                "x86_64-unknown-linux-gnu" => APPLY_FINGERPRINT_X86_64_UNKNOWN_LINUX_GNU,
                _ => {
                    return Err(
                        "target has no independently reviewed native apply fingerprint".into(),
                    );
                }
            };
            let bytes = super::hashing::canonical::bytes(self.tcx, id)?;
            let actual = format!("{:x}", Sha256::digest(bytes));
            if actual != expected {
                return Err(format!(
                    "typed-MIR closure/profile fingerprint mismatch: expected {expected}, observed {actual}"
                ));
            }
            Ok(())
        })();
        self.apply_primitives
            .borrow_mut()
            .insert(id, result.clone());
        result
    }
}

/// Reviewed fingerprints of the reserved `apply` body (a bare `panic!()` whose
/// only callee is the installed core `panic` leaf) per supported host.
const APPLY_FINGERPRINT_AARCH64_APPLE_DARWIN: &str =
    "bc05265c4f2b3bce00bfcc22690fc3387ab3eb0665bb73af3c10af4da201f110";
const APPLY_FINGERPRINT_X86_64_UNKNOWN_LINUX_GNU: &str =
    "a3f20a0dcc8b8dff8e12187fe7f58a15a545862873feb0cc0f97cb120b173129";
