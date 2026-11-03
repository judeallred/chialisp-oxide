use super::Lowerer;
use chialisp_oxide_ir::{Expr, Type};
use rustc_abi::Size;
use rustc_middle::{
    mir::{
        Const, ConstValue,
        interpret::{GlobalAlloc, alloc_range},
    },
    ty::{self, Ty},
};
use rustc_span::Span;

impl<'tcx> Lowerer<'tcx> {
    /// Accept only a value already validated as a fixed byte array or a chain
    /// of supported transparent wrappers. Rustc evaluates the constant; we do
    /// not reinterpret source syntax or follow pointers to other constants.
    pub(super) fn bytes_constant(
        &self,
        constant: Const<'tcx>,
        rust_ty: Ty<'tcx>,
        len: usize,
        span: Span,
    ) -> Result<Expr, String> {
        let env = ty::TypingEnv::fully_monomorphized();
        let size = Size::from_bytes(len);
        let layout = self
            .tcx
            .layout_of(env.as_query_input(rust_ty))
            .map_err(|_| self.error(span, "unsupported fixed-byte constant layout"))?;
        if len > 256 || layout.size != size {
            return Err(self.error(
                span,
                "fixed-byte constant must have its exact bounded byte-array layout",
            ));
        }
        let value = constant
            .eval(self.tcx, env, span)
            .map_err(|_| self.error(span, "unable to evaluate fixed-byte constant"))?;
        if matches!(value, ConstValue::ZeroSized) && len == 0 {
            return Ok(Expr::Bytes { hex: String::new() });
        }
        let ConstValue::Indirect { alloc_id, offset } = value else {
            return Err(self.error(span, "fixed-byte constant requires a by-value byte allocation; references and slices are unsupported"));
        };
        let Some(GlobalAlloc::Memory(allocation)) = self.tcx.try_get_global_alloc(alloc_id) else {
            return Err(self.error(span, "fixed-byte constant requires initialized memory"));
        };
        let allocation = allocation.inner();
        if offset
            .bytes()
            .checked_add(size.bytes())
            .is_none_or(|end| end > allocation.size().bytes())
        {
            return Err(self.error(
                span,
                "fixed-byte constant allocation is shorter than its declared layout",
            ));
        }
        let range = alloc_range(offset, size);
        if allocation.init_mask().is_range_initialized(range).is_err() {
            return Err(self.error(span, "uninitialized fixed-byte constants are unsupported"));
        }
        if !allocation.provenance().range_empty(range, &self.tcx) {
            return Err(self.error(
                span,
                "fixed-byte constants with pointer provenance are unsupported",
            ));
        }
        let bytes = allocation
            .get_bytes_strip_provenance(&self.tcx, range)
            .map_err(|_| {
                self.error(
                    span,
                    "fixed-byte constant contains invalid bytes or pointer provenance",
                )
            })?;
        Ok(Expr::Bytes {
            hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }
}

// A compiler-owned decoding plan. Native field offsets come from rustc, never
// logical tuple order. Padding is deliberately absent from the leaves we read.
struct AggregatePlan {
    size: Size,
    kind: AggregateKind,
}
enum AggregateKind {
    Unit,
    Scalar(Type),
    Bytes(usize),
    Tuple(Vec<(Size, AggregatePlan)>),
    Pair(Box<[(Size, AggregatePlan); 2]>),
    Wrapper(Box<(Size, AggregatePlan)>),
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn aggregate_constant(
        &self,
        constant: Const<'tcx>,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Expr, String> {
        // Validate the complete ORIGINAL type graph, including zero-size leaves,
        // before any value evaluation. Logical erasure would hide references,
        // Option niches, boxes, and arbitrary same-named containers.
        let mut pending = vec![(rust_ty, 0usize)];
        let mut nodes = 0usize;
        while let Some((current, depth)) = pending.pop() {
            nodes += 1;
            self.expression_work
                .set(self.expression_work.get().saturating_add(1));
            if nodes > 4096
                || depth >= 32
                || self.expression_work.get() > super::MAX_EXPRESSION_WORK
            {
                return Err(self.error(span, "aggregate constant type/work limit exceeded"));
            }
            let children = match current.kind() {
                ty::Bool | ty::Int(_) | ty::Uint(_) => {
                    self.ty(current, span)?;
                    vec![]
                }
                ty::Tuple(fields) => {
                    if fields.len() > 256 || fields.len() > 4096usize.saturating_sub(nodes + pending.len()) {
                        return Err(self.error(span, "aggregate constant type expansion limit exceeded"));
                    }
                    fields.iter().collect::<Vec<_>>()
                },
                ty::Array(element, len) => {
                    let length = len.try_to_target_usize(self.tcx).filter(|n| *n <= 256)
                        .ok_or_else(|| self.error(span, "aggregate constant arrays are limited to 256 elements"))? as usize;
                    if *element == self.tcx.types.u8 {
                        vec![]
                    } else {
                        // Even an empty array must not hide forbidden element
                        // types. Charge expansion before constructing the queue.
                        let expanded = length.max(1);
                        if expanded > 4096usize.saturating_sub(nodes + pending.len()) {
                            return Err(self.error(span, "aggregate constant type expansion limit exceeded"));
                        }
                        vec![*element; expanded]
                    }
                }
                ty::Adt(..) => {
                    if let Some(fields) = self.cons_fields(current, span)? {
                        fields.to_vec()
                    } else if let Some(fields) = self.named_struct_fields(current, span)? {
                        fields
                    } else if self.optional_shape(current, span)?.is_some() || self.tree_shape(current, span)?.is_some() {
                        return Err(self.error(span, "aggregate constants do not admit Option or Tree payloads"));
                    } else {
                        vec![self.transparent_inner(current, span)?.ok_or_else(|| self.error(span, "unsupported aggregate constant container"))?]
                    }
                }
                _ => return Err(self.error(span, "aggregate constants require drop-free fixed values; references, pointers and general containers are unsupported")),
            };
            if children.len() > 256
                || children.len() > 4096usize.saturating_sub(nodes + pending.len())
            {
                return Err(self.error(span, "aggregate constant type expansion limit exceeded"));
            }
            pending.extend(children.into_iter().map(|child| (child, depth + 1)));
        }
        let env = ty::TypingEnv::fully_monomorphized();
        if rust_ty.needs_drop(self.tcx, env) {
            return Err(self.error(span, "aggregate constants must be drop-free"));
        }
        // Account for the bounded second traversal before constructing its plan.
        let work = self
            .expression_work
            .get()
            .saturating_add(nodes.saturating_mul(2));
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(span, "aggregate constant plan work limit exceeded"));
        }
        self.expression_work.set(work);
        let plan = self.aggregate_plan(rust_ty, span)?;
        let value = constant
            .eval(self.tcx, env, span)
            .map_err(|_| self.error(span, "unable to evaluate aggregate constant"))?;
        self.decode_aggregate(&plan, value, plan.size, Size::ZERO, span)
    }

    fn aggregate_plan(&self, rust_ty: Ty<'tcx>, span: Span) -> Result<AggregatePlan, String> {
        let env = ty::TypingEnv::fully_monomorphized();
        let layout = self
            .tcx
            .layout_of(env.as_query_input(rust_ty))
            .map_err(|_| self.error(span, "unsupported aggregate constant layout"))?;
        if layout.size.bytes() > 65536 {
            return Err(self.error(
                span,
                "aggregate constant storage limit exceeded (65,536 bytes)",
            ));
        }
        let field = |index, ty| -> Result<_, String> {
            Ok((layout.fields.offset(index), self.aggregate_plan(ty, span)?))
        };
        let kind = match rust_ty.kind() {
            ty::Tuple(fields) if fields.is_empty() => AggregateKind::Unit,
            ty::Tuple(fields) => AggregateKind::Tuple(
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, t)| field(i, t))
                    .collect::<Result<_, _>>()?,
            ),
            ty::Array(element, len) if *element == self.tcx.types.u8 => AggregateKind::Bytes(
                len.try_to_target_usize(self.tcx)
                    .ok_or_else(|| self.error(span, "unknown aggregate byte-array length"))?
                    as usize,
            ),
            ty::Array(element, len) => {
                let length = len
                    .try_to_target_usize(self.tcx)
                    .ok_or_else(|| self.error(span, "unknown aggregate array length"))?
                    as usize;
                AggregateKind::Tuple(
                    (0..length)
                        .map(|index| field(index, *element))
                        .collect::<Result<_, _>>()?,
                )
            }
            ty::Bool | ty::Int(_) | ty::Uint(_) => AggregateKind::Scalar(self.ty(rust_ty, span)?),
            ty::Adt(..) => {
                if let Some([first, rest]) = self.cons_fields(rust_ty, span)? {
                    AggregateKind::Pair(Box::new([field(0, first)?, field(1, rest)?]))
                } else if let Some(fields) = self.named_struct_fields(rust_ty, span)? {
                    AggregateKind::Tuple(
                        fields
                            .into_iter()
                            .enumerate()
                            .map(|(index, ty)| field(index, ty))
                            .collect::<Result<_, _>>()?,
                    )
                } else {
                    let inner = self.transparent_inner(rust_ty, span)?.ok_or_else(|| {
                        self.error(span, "unsupported aggregate constant wrapper")
                    })?;
                    AggregateKind::Wrapper(Box::new(field(0, inner)?))
                }
            }
            _ => return Err(self.error(span, "aggregate constant type changed after preflight")),
        };
        Ok(AggregatePlan {
            size: layout.size,
            kind,
        })
    }

    fn aggregate_leaf(
        &self,
        value: ConstValue,
        root_size: Size,
        offset: Size,
        size: Size,
        span: Span,
    ) -> Result<Vec<u8>, String> {
        let end = offset
            .bytes()
            .checked_add(size.bytes())
            .ok_or_else(|| self.error(span, "aggregate constant offset overflow"))?;
        if end > root_size.bytes() || size.bytes() > 256 {
            return Err(self.error(span, "aggregate constant leaf is outside its layout"));
        }
        match value {
            ConstValue::Indirect {
                alloc_id,
                offset: base,
            } => {
                let Some(GlobalAlloc::Memory(allocation)) = self.tcx.try_get_global_alloc(alloc_id)
                else {
                    return Err(self.error(span, "aggregate constant requires initialized memory"));
                };
                let allocation = allocation.inner();
                let start = base.bytes().checked_add(offset.bytes()).ok_or_else(|| {
                    self.error(span, "aggregate constant allocation offset overflow")
                })?;
                if start
                    .checked_add(size.bytes())
                    .is_none_or(|end| end > allocation.size().bytes())
                {
                    return Err(self.error(
                        span,
                        "aggregate constant allocation is shorter than its layout",
                    ));
                }
                let range = alloc_range(Size::from_bytes(start), size);
                if allocation.init_mask().is_range_initialized(range).is_err()
                    || !allocation.provenance().range_empty(range, &self.tcx)
                {
                    return Err(self.error(span, "aggregate constant leaf contains uninitialized bytes or pointer provenance"));
                }
                Ok(allocation
                    .get_bytes_strip_provenance(&self.tcx, range)
                    .map_err(|_| self.error(span, "invalid aggregate constant bytes"))?
                    .to_vec())
            }
            ConstValue::Scalar(scalar) => {
                let scalar = scalar
                    .try_to_scalar_int()
                    .map_err(|_| self.error(span, "aggregate constants cannot contain pointers"))?;
                if scalar.size() != root_size || root_size.bytes() > 16 {
                    return Err(self.error(span, "aggregate scalar constant layout mismatch"));
                }
                let bits = scalar.to_bits(root_size);
                let bytes = if self.tcx.data_layout.endian == rustc_abi::Endian::Little {
                    bits.to_le_bytes()
                } else {
                    bits.to_be_bytes()
                };
                let base = if self.tcx.data_layout.endian == rustc_abi::Endian::Little {
                    0
                } else {
                    16 - root_size.bytes() as usize
                };
                Ok(bytes[base + offset.bytes() as usize..base + end as usize].to_vec())
            }
            ConstValue::ZeroSized if size == Size::ZERO => Ok(vec![]),
            _ => Err(self.error(span, "unsupported aggregate constant storage")),
        }
    }

    fn decode_aggregate(
        &self,
        plan: &AggregatePlan,
        value: ConstValue,
        root_size: Size,
        offset: Size,
        span: Span,
    ) -> Result<Expr, String> {
        let child = |(relative, plan): &(Size, AggregatePlan)| {
            let start = offset
                .bytes()
                .checked_add(relative.bytes())
                .ok_or_else(|| self.error(span, "aggregate field offset overflow"))?;
            self.decode_aggregate(plan, value, root_size, Size::from_bytes(start), span)
        };
        Ok(match &plan.kind {
            AggregateKind::Unit => Expr::Unit,
            AggregateKind::Tuple(fields) => Expr::Tuple {
                items: fields.iter().map(child).collect::<Result<_, _>>()?,
            },
            AggregateKind::Pair(fields) => Expr::Pair {
                first: Box::new(child(&fields[0])?),
                rest: Box::new(child(&fields[1])?),
            },
            AggregateKind::Wrapper(field) => child(field)?,
            AggregateKind::Bytes(len) => Expr::Bytes {
                hex: self
                    .aggregate_leaf(value, root_size, offset, Size::from_bytes(*len), span)?
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            },
            AggregateKind::Scalar(ty) => {
                let bytes = self.aggregate_leaf(value, root_size, offset, plan.size, span)?;
                let mut bits = 0u128;
                if self.tcx.data_layout.endian == rustc_abi::Endian::Little {
                    for (index, byte) in bytes.iter().enumerate() {
                        bits |= u128::from(*byte) << (index * 8);
                    }
                } else {
                    for byte in bytes {
                        bits = (bits << 8) | u128::from(byte);
                    }
                }
                if *ty == Type::Bool && bits > 1 {
                    return Err(self.error(span, "invalid Boolean aggregate constant"));
                }
                self.scalar(bits, ty, span)?
            }
        })
    }
}
