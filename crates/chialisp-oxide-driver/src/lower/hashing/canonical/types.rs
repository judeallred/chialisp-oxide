use super::*;

impl<'tcx> Encode<'tcx> {
    pub(super) fn normal_ty(&mut self, t: Ty<'tcx>) -> R<Ty<'tcx>> {
        self.raw_graph(Raw::Ty(t))?;
        self.instance
            .ok_or("no current instance")?
            .try_instantiate_mir_and_normalize_erasing_regions(
                self.tcx,
                ty::TypingEnv::fully_monomorphized(),
                ty::EarlyBinder::bind(self.tcx, t),
            )
            .map_err(|_| "type normalization".into())
    }
    pub(super) fn normal_const(&mut self, c: mir::Const<'tcx>) -> R<mir::Const<'tcx>> {
        self.raw_graph(Raw::Ty(c.ty()))?;
        match c {
            mir::Const::Ty(_, value) => self.raw_graph(Raw::Const(value))?,
            mir::Const::Unevaluated(uv, _) => self.literal_ctfe_input(uv)?,
            mir::Const::Val(_, _) => (),
        }

        self.instance
            .ok_or("no current instance")?
            .try_instantiate_mir_and_normalize_erasing_regions(
                self.tcx,
                ty::TypingEnv::fully_monomorphized(),
                ty::EarlyBinder::bind(self.tcx, c),
            )
            .map_err(|_| "constant normalization".into())
    }
    pub(super) fn normal_ty_const(&mut self, c: ty::Const<'tcx>) -> R<ty::Const<'tcx>> {
        self.raw_graph(Raw::Const(c))?;
        self.instance
            .ok_or("no current instance")?
            .try_instantiate_mir_and_normalize_erasing_regions(
                self.tcx,
                ty::TypingEnv::fully_monomorphized(),
                ty::EarlyBinder::bind(self.tcx, c),
            )
            .map_err(|_| "type constant normalization".into())
    }
    /// Exactly the borrowed tree shape the frontend admits: an ordinary enum with
    /// one lifetime parameter, no destructor, variants `Atom(&[u8])` and
    /// `Pair(&Self, &Self)` in that order. Names and crates are irrelevant.
    pub(super) fn admitted_borrowed_tree(&self, t: Ty<'tcx>) -> bool {
        let ty::Adt(definition, args) = t.kind() else {
            return false;
        };
        if !definition.is_enum()
            || definition.has_dtor(self.tcx)
            || definition.variants().len() != 2
            || definition.repr().c()
            || definition.repr().int.is_some()
            || args.len() != 1
            || args.iter().any(|a| a.as_region().is_none())
        {
            return false;
        }
        let variants = definition.variants();
        let atom = &variants[rustc_abi::VariantIdx::from_u32(0)];
        let pair = &variants[rustc_abi::VariantIdx::from_u32(1)];
        if atom.fields.len() != 1 || pair.fields.len() != 2 {
            return false;
        }
        let field = |v: &ty::VariantDef, i: usize| {
            self.tcx
                .try_normalize_erasing_regions(
                    ty::TypingEnv::fully_monomorphized(),
                    v.fields[rustc_abi::FieldIdx::from_usize(i)].ty(self.tcx, args),
                )
                .ok()
        };
        let is_u8_slice_ref = |f: Ty<'tcx>| {
            matches!(f.kind(), ty::Ref(_, inner, ty::Mutability::Not)
                if matches!(inner.kind(), ty::Slice(e) if *e == self.tcx.types.u8))
        };
        let is_self_ref = |f: Ty<'tcx>| {
            matches!(f.kind(), ty::Ref(_, inner, ty::Mutability::Not)
                if matches!(inner.kind(), ty::Adt(inner_def, _) if inner_def.did() == definition.did()))
        };
        match (field(atom, 0), field(pair, 0), field(pair, 1)) {
            (Some(payload), Some(first), Some(rest)) => {
                is_u8_slice_ref(payload) && is_self_ref(first) && is_self_ref(rest)
            }
            _ => false,
        }
    }
    pub(super) fn ty(&mut self, t: Ty<'tcx>, depth: usize) -> R {
        self.charge(1)?;
        if depth >= 32 {
            return Err("type depth".into());
        }
        let t = self.normal_ty(t)?;
        match t.kind() {
            ty::Bool => self.tag(1)?,
            ty::Never => self.tag(8)?,
            // Only the core panic leaf's `&'static str` message reaches here.
            ty::Str => self.tag(10)?,
            ty::Int(i) => {
                self.tag(2)?;
                self.tag(match i {
                    ty::IntTy::I8 => 0,
                    ty::IntTy::I16 => 1,
                    ty::IntTy::I32 => 2,
                    ty::IntTy::I64 => 3,
                    ty::IntTy::I128 => 4,
                    ty::IntTy::Isize => 5,
                })?
            }
            ty::Uint(i) => {
                self.tag(3)?;
                self.tag(match i {
                    ty::UintTy::U8 => 0,
                    ty::UintTy::U16 => 1,
                    ty::UintTy::U32 => 2,
                    ty::UintTy::U64 => 3,
                    ty::UintTy::U128 => 4,
                    ty::UintTy::Usize => 5,
                })?
            }
            ty::Array(e, n) => {
                self.tag(4)?;
                let n = n
                    .try_to_target_usize(self.tcx)
                    .ok_or("nonconcrete array length")?;
                if n > 4096 {
                    return Err("array bound".into());
                }
                self.u64(n)?;
                self.ty(*e, depth + 1)?
            }
            ty::Slice(e) => {
                self.tag(5)?;
                self.ty(*e, depth + 1)?
            }
            ty::Ref(_, e, m) => {
                self.tag(6)?;
                self.boolean(m.is_mut())?;
                self.ty(*e, depth + 1)?
            }
            ty::Tuple(items) => {
                if items.len() > 64 {
                    return Err("tuple bound".into());
                }
                self.tag(7)?;
                self.usize(items.len())?;
                for t in items.iter() {
                    self.ty(t, depth + 1)?
                }
            }
            ty::Adt(..) if self.admitted_borrowed_tree(t) => {
                // Structural identity only: the shape check below fixes every
                // variant and field, so no definition path or name is encoded.
                self.tag(9)?;
                self.string("admitted_borrowed_tree")?
            }
            _ => return Err(format!("unencoded type: {t:?}")),
        }
        let layout = self
            .tcx
            .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(t))
            .map_err(|_| "layout unavailable")?;
        self.boolean(layout.is_sized())?;
        self.u64(layout.size.bytes())?;
        self.u64(layout.align.abi.bytes())?;
        let fields = layout.fields.count();
        if fields > 4096 {
            return Err("layout field bound".into());
        }
        self.usize(fields)?;
        for i in 0..fields {
            self.u64(layout.fields.offset(i).bytes())?
        }
        Ok(())
    }
    pub(super) fn attrs(&mut self, id: DefId) -> R {
        let a = self.tcx.codegen_fn_attrs(id);
        if a.flags.bits() != 0
            || !matches!(a.inline, InlineAttr::None)
            || !matches!(a.optimize, OptimizeAttr::Default)
            || a.symbol_name.is_some()
            || !a.foreign_item_symbol_aliases.is_empty()
            || a.link_ordinal.is_some()
            || !a.target_features.is_empty()
            || a.safe_target_features
            || a.linkage.is_some()
            || a.import_linkage.is_some()
            || a.link_section.is_some()
            || a.sanitizers != Default::default()
            || a.instruction_set.is_some()
            || a.alignment.is_some()
            || a.patchable_function_entry.is_some()
            || a.objc_class.is_some()
            || a.objc_selector.is_some()
            || !matches!(a.instrument_fn, InstrumentFnAttr::Default)
        {
            return Err("unreviewed function codegen attributes".into());
        }
        self.tag(0)
    }
    pub(super) fn signature(&mut self, instance: ty::Instance<'tcx>) -> R {
        let previous = self.instance.replace(instance);
        let result = (|| {
            self.raw_graph(Raw::Args(instance.args))?;
            let raw = self
                .tcx
                .fn_sig(instance.def_id())
                .instantiate_identity()
                .skip_norm_wip()
                .skip_binder();
            if raw.inputs().len() > 64 {
                return Err("raw signature arity bound".into());
            }
            for input in raw.inputs() {
                self.raw_graph(Raw::Ty(*input))?;
            }
            self.raw_graph(Raw::Ty(raw.output()))?;
            let sig = self
                .tcx
                .try_instantiate_and_normalize_erasing_regions(
                    instance.args,
                    ty::TypingEnv::fully_monomorphized(),
                    self.tcx.fn_sig(instance.def_id()),
                )
                .map_err(|_| "signature normalization")?
                .skip_binder();
            if sig.safety() != rustc_hir::Safety::Safe
                || sig.abi() != rustc_abi::ExternAbi::Rust
                || sig.c_variadic()
            {
                return Err("unreviewed function signature ABI/safety".into());
            }
            if sig.inputs().len() > 64 {
                return Err("signature argument bound".into());
            }
            self.usize(sig.inputs().len())?;
            for t in sig.inputs() {
                self.ty(*t, 0)?
            }
            self.ty(sig.output(), 0)
        })();
        self.instance = previous;
        result
    }
    /// Encode one reviewed leaf of the installed core library: the owner must be
    /// the actual `core` crate loaded from the pinned sysroot target directory.
    fn core_leaf(&mut self, instance: ty::Instance<'tcx>, label: &str) -> R {
        let did = instance.def_id();
        let core = self
            .tcx
            .lang_items()
            .eq_trait()
            .ok_or("missing core anchor")?
            .krate;
        if did.is_local() || did.krate != core || self.tcx.crate_name(core).as_str() != "core" {
            return Err(format!("{label}: language item has foreign owner"));
        }
        let sysroot = std::path::Path::new(env!("CHIALISP_OXIDE_SYSROOT"))
            .canonicalize()
            .map_err(|_| "pinned sysroot missing")?;
        let expected = sysroot
            .join("lib/rustlib")
            .join(self.tcx.sess.opts.target_triple.tuple())
            .join("lib")
            .canonicalize()
            .map_err(|_| "target libraries missing")?;
        let source = self.tcx.used_crate_source(core);
        let mut count = 0;
        for p in [
            &source.rmeta,
            &source.rlib,
            &source.dylib,
            &source.sdylib_interface,
        ]
        .into_iter()
        .flatten()
        {
            count += 1;
            let p = p.canonicalize().map_err(|_| "core source missing")?;
            if p.parent() != Some(expected.as_path()) {
                return Err("core leaf source outside pinned installation".into());
            }
        }
        if count == 0 {
            return Err("core source unavailable".into());
        }
        self.tag(1)?;
        self.string(label)?;
        self.signature(instance)?;
        self.leaf_calls += 1;
        Ok(())
    }
    pub(super) fn instance_ref(&mut self, instance: ty::Instance<'tcx>) -> R {
        self.raw_graph(Raw::Args(instance.args))?;
        let did = instance.def_id();
        if Some(did) == self.tcx.lang_items().panic_fn() {
            // The reserved `apply` export without a native evaluator is a bare
            // `panic!()`, which lowers to one call of the `panic` lang item with
            // a `&'static str`. Admit exactly that installed core leaf.
            self.core_leaf(instance, "installed_core_panic_str")?;
            let sig = self.tcx.instantiate_bound_regions_with_erased(
                self.tcx.fn_sig(did).instantiate_identity().skip_norm_wip(),
            );
            let str_ref = |t: Ty<'tcx>| matches!(t.kind(), ty::Ref(_, inner, ty::Mutability::Not) if inner.is_str());
            if !instance.args.is_empty()
                || sig.inputs().len() != 1
                || !str_ref(sig.inputs()[0])
                || !sig.output().is_never()
            {
                return Err("only the actual core panic(&str) -> ! is a reviewed leaf".into());
            }
            return Ok(());
        }
        if Some(did) == self.tcx.lang_items().slice_len_fn() {
            if instance.args.len() != 1 || instance.args.type_at(0) != self.tcx.types.u8 {
                return Err("only actual core u8 slice length is a reviewed leaf".into());
            }
            self.core_leaf(instance, "installed_core_slice_len_u8")
        } else {
            if self.tcx.crate_name(did.krate).as_str() == "core" {
                return Err(format!("unreviewed core leaf: {instance:?}"));
            }
            if !matches!(instance.def, ty::InstanceKind::Item(_)) {
                return Err("unencoded shim/virtual/intrinsic instance".into());
            }
            self.tag(0)?;
            let id = self.id(Node::Function(instance))?;
            self.usize(id)
        }
    }
}
