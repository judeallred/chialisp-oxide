//! Constructed metadata controls use actual pinned-rustc types and source MIR.
//! Forged-slot/body controls are not native-valid source programs.
use super::*;
use rustc_driver::{Callbacks, Compilation};
use std::time::{SystemTime, UNIX_EPOCH};
struct Controls {
    checked: bool,
}
impl Callbacks for Controls {
    fn config(&mut self, c: &mut rustc_interface::interface::Config) {
        c.opts.unstable_opts.mir_opt_level = Some(0);
    }
    fn after_analysis<'tcx>(
        &mut self,
        _: &rustc_interface::interface::Compiler,
        tcx: ty::TyCtxt<'tcx>,
    ) -> Compilation {
        for owner in tcx.hir_body_owners() {
            if !tcx.is_typeck_child(owner.to_def_id()) {
                tcx.ensure_ok().mir_borrowck(owner);
            }
        }
        tcx.dcx().abort_if_errors();
        let mut selected = None;
        for owner in tcx.hir_body_owners() {
            if tcx.def_kind(owner) != DefKind::Fn {
                continue;
            }
            let body = tcx.optimized_mir(owner);
            for data in body.basic_blocks.iter() {
                for statement in &data.statements {
                    if let mir::StatementKind::Assign(a) = &statement.kind
                        && let Rvalue::Aggregate(kind, ops) = &a.1
                        && let mir::AggregateKind::Closure(id, args) = &**kind
                        && !ops.is_empty()
                    {
                        selected = Some((body, *id, *args, statement.clone()));
                    }
                }
            }
        }
        let (parent, id, args, constructor) = selected.expect("actual captured closure");
        let span = parent.span;
        let lower = Lowerer::new(tcx);
        lower.admit_captured_closure(id, args, span).unwrap();
        let plan = lower.captured_plan(id).unwrap();
        assert_eq!(
            plan.environment,
            Type::Tuple {
                items: vec![Type::Int {
                    bits: 64,
                    signed: false
                }]
            }
        );
        let helper = lower.captured_body(id).unwrap().unwrap();
        assert_eq!(helper.arg_count, 2);
        let signature = lower.captured_signature(helper).unwrap().unwrap();
        assert_eq!(signature.0.len(), 2);
        for (slot, replacement) in [
            (0, tcx.types.bool),
            (1, tcx.types.bool),
            (2, tcx.types.bool),
            (2, Ty::new_tup(tcx, &[tcx.types.u8])),
        ] {
            let fresh = Lowerer::new(tcx);
            let mut changed = args.to_vec();
            changed[slot] = replacement.into();
            let changed = tcx.mk_args(&changed);
            assert!(
                fresh.admit_captured_closure(id, changed, span).is_err(),
                "slot{slot}"
            );
        }
        let mut deep = tcx.types.u64;
        for _ in 0..33 {
            deep = Ty::new_tup(tcx, &[deep]);
        }
        assert!(
            lower
                .capture_type(deep, parent.source.def_id(), span)
                .err()
                .unwrap()
                .contains("depth32")
        );
        let exhausted = Lowerer::new(tcx);
        exhausted
            .expression_work
            .set(super::super::MAX_EXPRESSION_WORK - 1);
        assert!(exhausted.admit_captured_closure(id, args, span).is_err());
        // Each cloned input is already bounded by captured_body/owned_plan; the
        // forged control grows by exactly one existing bounded statement.
        lower.owned_plan(parent).unwrap();
        lower
            .callable_charge(
                span,
                parent.basic_blocks[mir::START_BLOCK].statements.len() + 1,
            )
            .unwrap();
        let mut duplicated = parent.clone();
        duplicated.basic_blocks.as_mut()[mir::START_BLOCK]
            .statements
            .push(constructor);
        let duplicated = tcx.arena.alloc(duplicated);
        assert!(
            lower
                .captured_uses(duplicated)
                .err()
                .unwrap()
                .contains("one immutable definition")
        );
        let mut forged = helper.clone();
        let mut changed = false;
        for data in forged.basic_blocks.as_mut() {
            for statement in &mut data.statements {
                if let mir::StatementKind::Assign(a) = &mut statement.kind
                    && let Rvalue::Use(Operand::Copy(p), _) = &mut a.1
                    && p.local == Local::from_usize(1)
                {
                    p.projection = tcx.mk_place_elems(&[
                        ProjectionElem::Deref,
                        ProjectionElem::Field(rustc_abi::FieldIdx::from_usize(0), tcx.types.u8),
                    ]);
                    changed = true;
                }
            }
        }
        assert!(changed);
        assert!(lower.captured_receiver_uses(&forged, &plan).is_err());
        self.checked = true;
        Compilation::Continue
    }
}
#[test]
fn actual_capture_metadata_slots_receiver_fields_and_budgets_are_closed() {
    let root = std::env::temp_dir().join(format!(
        "captures57-metadata-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn puzzle(value:u64)->u64{let step=move |n:u64|value+n;step(1)}",
    )
    .unwrap();
    let args = vec![
        "rustc".into(),
        "--crate-name".into(),
        "captured_metadata_control".into(),
        "--edition=2024".into(),
        "--crate-type=lib".into(),
        "--emit=metadata".into(),
        "--sysroot".into(),
        env!("CHIALISP_OXIDE_SYSROOT").into(),
        "--out-dir".into(),
        root.display().to_string(),
        source.display().to_string(),
    ];
    let mut controls = Controls { checked: false };
    let code =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut controls));
    assert!(code == std::process::ExitCode::SUCCESS && controls.checked);
    std::fs::remove_dir_all(root).unwrap();
}
