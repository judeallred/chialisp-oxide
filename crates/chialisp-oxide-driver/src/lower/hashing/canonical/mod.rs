//! Exhaustive, fail-closed canonical bytes for the reviewed native kernel.
//!
//! This is not rustc's noncryptographic query hash, source-text authorization,
//! native-object attestation, or a rustc-query resource sandbox. Unsupported
//! typed MIR/attributes/constants reject before digest comparison.
mod body;
mod bounds;
mod types;
mod values;
use rustc_hir::{
    attrs::{InlineAttr, OptimizeAttr},
    def_id::DefId,
};
use rustc_middle::{
    middle::codegen_fn_attrs::InstrumentFnAttr,
    mir::{
        self,
        interpret::{GlobalAlloc, alloc_range},
    },
    ty::{self, Ty, TyCtxt},
};
use rustc_target::spec::PanicStrategy as rustc_target_panic;
use std::collections::HashMap;
type R<T = ()> = Result<T, String>;
const MAX_NODES: usize = 128;
const MAX_BLOCKS: usize = 4096;
const MAX_STATEMENTS: usize = 16384;
const MAX_LOCALS: usize = 4096;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_WORK: usize = 500_000;
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Node<'tcx> {
    Function(ty::Instance<'tcx>),
    Constant(mir::UnevaluatedConst<'tcx>),
}
#[derive(Clone, Copy)]
enum Raw<'tcx> {
    Ty(Ty<'tcx>),
    Const(ty::Const<'tcx>),
    Args(ty::GenericArgsRef<'tcx>),
}
struct Encode<'tcx> {
    tcx: TyCtxt<'tcx>,
    bytes: Vec<u8>,
    nodes: Vec<Node<'tcx>>,
    ids: HashMap<Node<'tcx>, usize>,
    work: usize,
    blocks: usize,
    statements: usize,
    constants: usize,
    leaf_calls: usize,
    instance: Option<ty::Instance<'tcx>>,
}
impl<'tcx> Encode<'tcx> {
    pub(super) fn charge(&mut self, n: usize) -> R {
        self.work = self.work.checked_add(n).ok_or("work overflow")?;
        if self.work > MAX_WORK {
            return Err("canonical work budget".into());
        }
        Ok(())
    }
    pub(super) fn raw(&mut self, b: &[u8]) -> R {
        self.charge(1)?;
        if b.len() > MAX_BYTES - self.bytes.len() {
            return Err("canonical byte budget".into());
        }
        self.bytes.extend_from_slice(b);
        Ok(())
    }
    pub(super) fn tag(&mut self, t: u8) -> R {
        self.raw(&[t])
    }
    pub(super) fn u64(&mut self, n: u64) -> R {
        self.raw(&n.to_le_bytes())
    }
    pub(super) fn usize(&mut self, n: usize) -> R {
        self.u64(n.try_into().map_err(|_| "usize encoding")?)
    }
    pub(super) fn boolean(&mut self, b: bool) -> R {
        self.tag(u8::from(b))
    }
    pub(super) fn string(&mut self, s: &str) -> R {
        self.usize(s.len())?;
        self.raw(s.as_bytes())
    }
    pub(super) fn id(&mut self, node: Node<'tcx>) -> R<usize> {
        if let Some(id) = self.ids.get(&node) {
            return Ok(*id);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err("canonical graph budget".into());
        }
        let id = self.nodes.len();
        self.nodes.push(node);
        self.ids.insert(node, id);
        Ok(id)
    }
    // Inspect borrowed interned input graphs before asking rustc to normalize.
    // This bounds our own traversal/allocation; it is not a rustc-query sandbox.
    pub(super) fn encode(mut self, entry: ty::Instance<'tcx>) -> R<Self> {
        self.string("isolated-reviewed-native-kernel-typed-mir-v1")?;
        self.string("rustc-daf2e5e18b3abc4311d9da3d96fd64138f43d508")?;
        self.string(self.tcx.sess.opts.target_triple.tuple())?;
        self.string(&self.tcx.sess.target.data_layout)?;
        self.boolean(self.tcx.sess.overflow_checks())?;
        self.boolean(self.tcx.sess.opts.debug_assertions)?;
        self.tag(match self.tcx.sess.panic_strategy() {
            rustc_target_panic::Abort => 0,
            rustc_target_panic::Unwind => 1,
            rustc_target_panic::ImmediateAbort => 2,
        })?;
        self.id(Node::Function(entry))?;
        let mut next = 0;
        while next < self.nodes.len() {
            let node = self.nodes[next];
            self.usize(next)?;
            match node {
                Node::Function(instance) => {
                    self.instance = Some(instance);
                    self.tag(0)?;
                    if !matches!(
                        self.tcx.def_kind(instance.def_id()),
                        rustc_hir::def::DefKind::Fn | rustc_hir::def::DefKind::AssocFn
                    ) {
                        return Err("unencoded function kind".into());
                    }
                    self.attrs(instance.def_id())?;
                    self.signature(instance)?;
                    if !self.tcx.is_mir_available(instance.def_id()) {
                        return Err("missing external MIR".into());
                    }
                    let source = self.tcx.instance_mir(instance.def);
                    self.preflight(source)?;
                    if !matches!(
                        source.phase,
                        mir::MirPhase::Runtime(mir::RuntimePhase::Optimized)
                    ) {
                        return Err("unexpected function MIR phase".into());
                    }
                    self.body(source)?
                }
                Node::Constant(uv) => {
                    self.tag(1)?;
                    let instance = ty::Instance::new_raw(uv.def, uv.args);
                    self.instance = Some(instance);
                    let source = if let Some(promoted) = uv.promoted {
                        &self.tcx.promoted_mir(uv.def)[promoted]
                    } else {
                        if !matches!(
                            self.tcx.def_kind(uv.def),
                            rustc_hir::def::DefKind::Const { .. }
                                | rustc_hir::def::DefKind::AnonConst
                                | rustc_hir::def::DefKind::AssocConst { .. }
                        ) {
                            return Err("static constant source unencoded".into());
                        }
                        self.tcx.mir_for_ctfe(uv.def)
                    };
                    self.preflight(source)?;
                    self.body(source)?
                }
            }
            next += 1;
        }
        self.usize(self.nodes.len())?;
        Ok(self)
    }
}

/// The caller has already enforced the installed pinned compiler/stdlib policy.
pub(super) fn bytes<'tcx>(tcx: TyCtxt<'tcx>, id: DefId) -> R<Vec<u8>> {
    if env!("CHIALISP_OXIDE_RUSTC_COMMIT") != "daf2e5e18b3abc4311d9da3d96fd64138f43d508" {
        return Err("reviewed native primitive requires the pinned compiler commit".into());
    }
    if tcx.generics_of(id).requires_monomorphization(tcx) {
        return Err("native primitive entry cannot have type or const generics".into());
    }
    let encoder = Encode {
        tcx,
        bytes: vec![],
        nodes: vec![],
        ids: HashMap::new(),
        work: 0,
        blocks: 0,
        statements: 0,
        constants: 0,
        leaf_calls: 0,
        instance: None,
    };
    Ok(encoder.encode(ty::Instance::mono(tcx, id))?.bytes)
}
