//! Explicitly reviewed native primitive bridge. Public spelling only selects a
//! candidate; the actual complete typed-MIR closure and profile authorize it.
pub(super) mod canonical;

use super::Lowerer;
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
    fn hashing_candidates(&self) -> Result<Vec<DefId>, String> {
        if let Some(result) = self.hash_candidates.borrow().as_ref() {
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
                    if child.ident.name.as_str() == "sha256_parts"
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
        *self.hash_candidates.borrow_mut() = Some(result.clone());
        result
    }

    fn hash_signature(&self, id: DefId) -> Result<(), String> {
        if !matches!(self.tcx.def_kind(id), rustc_hir::def::DefKind::Fn)
            || !self.lifetime_parameters_only(id)
            || self.tcx.constness(id) != rustc_hir::Constness::NotConst
        {
            return Err("entry must be an ordinary non-const, non-generic function".into());
        }
        let sig = self.tcx.fn_sig(id).instantiate_identity().skip_binder();
        let atom = |t: Ty<'tcx>| {
            matches!(t.kind(), ty::Ref(_, inner, ty::Mutability::Not)
            if matches!(inner.kind(),ty::Slice(element) if *element==self.tcx.types.u8))
        };
        // Do not execute a candidate's CTFE merely to choose this signature.
        // The reviewed return width is an already materialized usize literal.
        let literal32 = |count: ty::Const<'tcx>| match count.kind() {
            ty::ConstKind::Value(value) if value.ty == self.tcx.types.usize => {
                matches!(**value.valtree,ty::ValTreeKind::Leaf(number)
                    if number.size()==self.tcx.data_layout.pointer_size()
                        && number.to_bits(number.size())==32)
            }
            _ => false,
        };
        let bytes32 = matches!(sig.output().kind(),ty::Array(element,count)
            if *element==self.tcx.types.u8 && literal32(*count));
        if sig.safety().is_unsafe()
            || sig.abi() != rustc_abi::ExternAbi::Rust
            || sig.c_variadic()
            || sig.inputs().len() != 3
            || !sig.inputs().iter().all(|t| atom(*t))
            || !bytes32
        {
            return Err("entry requires exactly fn(&[u8], &[u8], &[u8]) -> [u8;32]".into());
        }
        Ok(())
    }

    fn authorize_hash(&self, id: DefId) -> Result<(), String> {
        if let Some(result) = self.hash_primitives.borrow().get(&id) {
            return result.clone();
        }
        let result = (|| {
            if env!("CHIALISP_OXIDE_RUSTC_COMMIT") != PINNED_COMMIT {
                return Err("installed compiler differs from the reviewed compiler".into());
            }
            self.hash_signature(id)?;
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
                "aarch64-apple-darwin" => {
                    "886df9e0681fe58bdbe6f60f197018f1534861c066c5c53375e1820f4e0e99a3"
                }
                "x86_64-unknown-linux-gnu" => {
                    "5e4048457a73963d8ab42e84fe6a6547552b5028203b5af13848d00398ddde48"
                }
                _ => {
                    return Err(
                        "target has no independently reviewed native SHA256 fingerprint".into(),
                    );
                }
            };
            let bytes = canonical::bytes(self.tcx, id)?;
            let actual = format!("{:x}", Sha256::digest(bytes));
            if actual != expected {
                return Err(format!(
                    "typed-MIR closure/profile fingerprint mismatch: expected {expected}, observed {actual}"
                ));
            }
            Ok(())
        })();
        if self.hash_primitives.borrow().len() >= MAX_PRIMITIVES {
            return Err("reviewed native primitive cache limit exceeded".into());
        }
        self.hash_primitives.borrow_mut().insert(id, result.clone());
        result
    }

    /// False for unrelated same-named functions; reserved SDK entry changes
    /// reject instead of silently reverting to unsupported native kernel loops.
    /// SHA remains an effect, including unused results and resource failures.
    pub(super) fn hash_primitive(
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
        if !self.hashing_candidates()?.contains(id) {
            return Ok(false);
        }
        let prefix = |reason| {
            self.error(
                span,
                format!("reviewed native SHA256 primitive rejected: {reason}"),
            )
        };
        if args.len() != 3
            || generic
                .skip_binder()
                .iter()
                .any(|a| a.as_region().is_none())
        {
            return Err(prefix("unsupported actual call signature".into()));
        }
        self.authorize_hash(*id).map_err(&prefix)?;
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
            return Err(prefix(format!(
                "actual operand/result types differ from reviewed signature: operands={:?}, expected={:?}, result={:?}, expected_result={:?}",
                args.iter()
                    .map(|arg| arg.node.ty(&body.local_decls, self.tcx))
                    .collect::<Vec<_>>(),
                signature.inputs(),
                destination.ty(&body.local_decls, self.tcx).ty,
                signature.output()
            )));
        }
        Ok(true)
    }
}
