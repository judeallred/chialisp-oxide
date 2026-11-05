//! Constructed metadata controls using a real pinned-rustc TyCtxt. These are
//! not native-valid Rust ways to forge closure args or frontend acceptance tests.
use super::*;
use rustc_driver::{Callbacks, Compilation};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
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
        let mut casts = Vec::new();
        let mut captured = None;
        let mut parent = None;
        for owner in tcx.hir_body_owners() {
            if tcx.def_kind(owner) == DefKind::Fn {
                let body = tcx.optimized_mir(owner);
                for block in body.basic_blocks.iter() {
                    for s in &block.statements {
                        if let StatementKind::Assign(a) = &s.kind {
                            match &a.1 {
                                Rvalue::Cast(
                                    CastKind::PointerCoercion(
                                        ty::adjustment::PointerCoercion::ClosureFnPointer(
                                            rustc_hir::Safety::Safe,
                                        ),
                                        _,
                                    ),
                                    op,
                                    target,
                                ) => {
                                    casts.push((
                                        op.ty(&body.local_decls, tcx),
                                        *target,
                                        s.source_info.span,
                                    ));
                                    parent = Some(owner.to_def_id());
                                }
                                Rvalue::Aggregate(k, _) => {
                                    if let AggregateKind::Closure(id, args) = &**k
                                        && !args.as_closure().upvar_tys().is_empty()
                                    {
                                        captured = Some(Ty::new_closure(tcx, *id, args));
                                    }
                                }
                                _ => (),
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(casts.len(), 2);
        let (source, target, span) = casts[0];
        let lower = Lowerer::new(tcx);
        let (id, _, _) = lower.zero_closure_coercion(source, target, span).unwrap();
        let ty::Closure(_, args) = source.kind() else {
            unreachable!()
        };
        assert!(
            !tcx.promoted_mir(id).is_empty(),
            "first actual closure has a promoted byte literal"
        );
        let identity = ty::GenericArgs::identity_for_item(tcx, id);
        let original = mir::UnevaluatedConst {
            def: id,
            args: identity,
            promoted: Some(mir::Promoted::from_usize(0)),
        };
        assert!(lower.zero_closure_promoted_args(original, span).unwrap());
        let actual = mir::UnevaluatedConst { args, ..original };
        assert!(!lower.zero_closure_promoted_args(actual, span).unwrap());
        lower
            .active_zero_promoted
            .borrow_mut()
            .push(super::promoted::Context { def: id, args });
        assert!(lower.zero_closure_promoted_args(actual, span).unwrap());
        lower.active_zero_promoted.borrow_mut().pop();
        let source = &tcx.promoted_mir(id)[mir::Promoted::from_usize(0)];
        lower.preflight_promoted_clone(source, args, span).unwrap();
        let mut wide = source.clone();
        wide.basic_blocks.as_mut()[mir::START_BLOCK].statements = vec![
            mir::Statement::new(
                mir::SourceInfo {
                    span,
                    scope: mir::OUTERMOST_SOURCE_SCOPE
                },
                StatementKind::Nop
            );
            8193
        ];
        assert!(
            lower
                .preflight_promoted_clone(&wide, args, span)
                .unwrap_err()
                .contains("statement bound")
        );
        let mut deep = source.clone();
        let mut ty = tcx.types.u8;
        for _ in 0..40 {
            ty = Ty::new_imm_ref(tcx, tcx.lifetimes.re_erased, ty);
        }
        deep.local_decls[mir::RETURN_PLACE].ty = ty;
        assert!(
            lower
                .preflight_promoted_clone(&deep, args, span)
                .unwrap_err()
                .contains("type preflight")
        );
        let mut mismatches = Vec::new();
        for (index, replacement) in [
            (0, tcx.types.i16),
            (1, target),
            (2, Ty::new_tup(tcx, &[tcx.types.u64])),
        ] {
            let mut changed = identity.iter().collect::<Vec<_>>();
            changed[index] = replacement.into();
            mismatches.push(tcx.mk_args(&changed));
        }
        let mut truncated = identity.iter().collect::<Vec<_>>();
        truncated.pop();
        mismatches.push(tcx.mk_args(&truncated));
        let mut extended = identity.iter().collect::<Vec<_>>();
        extended.push(tcx.types.u8.into());
        mismatches.push(tcx.mk_args(&extended));
        for args in mismatches {
            assert!(
                !lower
                    .zero_closure_promoted_args(mir::UnevaluatedConst { args, ..original }, span)
                    .unwrap()
            );
        }
        assert!(
            !lower
                .zero_closure_promoted_args(
                    mir::UnevaluatedConst {
                        promoted: None,
                        ..original
                    },
                    span
                )
                .unwrap()
        );
        assert!(
            !lower
                .zero_closure_promoted_args(
                    mir::UnevaluatedConst {
                        def: parent.unwrap(),
                        ..original
                    },
                    span
                )
                .unwrap()
        );
        let ty::Closure(second, second_args) = casts[1].0.kind() else {
            unreachable!()
        };
        assert!(
            !lower
                .zero_closure_promoted_args(
                    mir::UnevaluatedConst {
                        def: *second,
                        args: second_args,
                        ..original
                    },
                    span
                )
                .unwrap()
        );
        let captured = captured.expect("actual scalar capture present");
        assert!(
            lower
                .zero_closure_coercion(captured, target, span)
                .unwrap_err()
                .contains("noncapturing")
        );
        let oversized = tcx.mk_args(&vec![tcx.types.u8.into(); 68]);
        assert!(
            lower
                .zero_closure_promoted_args(
                    mir::UnevaluatedConst {
                        args: oversized,
                        ..original
                    },
                    span
                )
                .unwrap_err()
                .contains("bounded")
        );
        self.checked = true;
        Compilation::Stop
    }
}
#[test]
fn promoted_closure_metadata_requires_exact_prevalidated_owner_and_all_args() {
    let directory = std::env::temp_dir().join(format!(
        "closures54-metadata-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.rs");
    fs::write(&source,"pub fn entry(x:u64)->u64 {let one:fn(u64)->u64=|v|{let data=&[1u8];v+data[0] as u64};let two:fn(u64)->u64=|v|v+2;let base=x;let capture=|v:u64|base+v;one(x)+two(x)+capture(0)}").unwrap();
    let args = vec![
        "rustc".into(),
        "--edition=2024".into(),
        "--crate-type=rlib".into(),
        "--emit=metadata".into(),
        "--sysroot".into(),
        env!("CHIALISP_OXIDE_SYSROOT").into(),
        "-Coverflow-checks=yes".into(),
        "--out-dir".into(),
        directory.display().to_string(),
        source.display().to_string(),
    ];
    let mut callback = Controls { checked: false };
    let code =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut callback));
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    assert!(callback.checked);
    fs::remove_dir_all(directory).unwrap();
}
