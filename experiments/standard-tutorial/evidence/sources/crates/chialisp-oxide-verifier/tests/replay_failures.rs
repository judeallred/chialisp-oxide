use chialisp_oxide_ir::{Environment, Expr, Program, Type};
use chialisp_oxide_verifier::{
    ReplayFailure, ReplayOutcome, Witness, classify_replay_failure, replay,
};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{allocator::NodePtr, error::EvalErr};
use serde_json::json;
use std::error::Error;

#[test]
fn resource_and_infrastructure_errors_cannot_become_rejection_evidence() {
    // These are explicitly injected classifier-boundary controls, not claims
    // that the real VM's large stack limits were exhausted during this test.
    let failures = [
        EvalErr::CostExceeded,
        EvalErr::OutOfMemory,
        EvalErr::TooManyAtoms,
        EvalErr::TooManyPairs,
        EvalErr::Timeout,
        EvalErr::ValueStackLimitReached(NodePtr::NIL),
        EvalErr::EnvironmentStackLimitReached(NodePtr::NIL),
        EvalErr::SoftforkStackDepthExceeded,
        EvalErr::SerializationError,
        EvalErr::NonCanonicalSerialization,
        EvalErr::SerializationBackreferenceError,
        EvalErr::InternalError(NodePtr::NIL, "injected internal VM failure".into()),
    ];
    for original in failures {
        let expected = classify_failure(&original);
        assert!(!expected.is_program_rejection());
        let error = classify_replay_failure(original).unwrap_err();
        let typed = error.downcast_ref::<ReplayFailure>().unwrap();
        assert_eq!(typed.failure, expected);
        let source = typed.source().unwrap().downcast_ref::<EvalErr>().unwrap();
        assert_eq!(classify_failure(source), expected);
    }
}

#[test]
fn actual_assertion_replay_records_typed_raise_in_both_compilation_modes() {
    let program = Program {
        functions: vec![],
        name: "typed-rejection-evidence".into(),
        environment: Environment::Strict,
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Reject {
            message: "independently specified rejecting policy".into(),
        },
    };
    let outcomes = replay(&program, &Witness { arguments: vec![] }, 1_000_000).unwrap();
    assert_eq!(outcomes.len(), 2);
    for row in outcomes {
        let ReplayOutcome::Rejected { failure, .. } = &row.outcome else {
            panic!("a rejecting policy must not return a successful value")
        };
        assert_eq!(failure.kind(), FailureKind::Program);
        assert_eq!(failure.code(), FailureCode::Raise);
        let wire = serde_json::to_value(&row.outcome).unwrap();
        assert_eq!(wire["failure"], json!({"kind":"program", "code":"raise"}));
        let decoded: ReplayOutcome = serde_json::from_value(wire).unwrap();
        assert!(
            matches!(decoded, ReplayOutcome::Rejected { failure, .. } if failure.code() == FailureCode::Raise)
        );
    }
}

#[test]
fn rejection_decoder_rejects_valid_resource_metadata_and_untyped_diagnostics() {
    // A diagnostic spelling "Raise" must not override authoritative metadata.
    for failure in [
        json!({"kind":"resource", "code":"cost_exceeded"}),
        json!({"kind":"infrastructure", "code":"internal_error"}),
        json!({"kind":"program", "code":"cost_exceeded"}),
        json!({"kind":"program", "code":"future_unknown_error"}),
    ] {
        let wire = json!({"status":"rejected", "error":"Raise(NodePtr::NIL)", "failure":failure});
        assert!(serde_json::from_value::<ReplayOutcome>(wire).is_err());
    }
    assert!(
        serde_json::from_value::<ReplayOutcome>(json!({
            "status":"rejected", "error":"Raise(NodePtr::NIL)"
        }))
        .is_err()
    );
    // Public enum construction cannot bypass the wire invariant either.
    let forged = ReplayOutcome::Rejected {
        error: "Raise(NodePtr::NIL)".into(),
        failure: classify_failure(&EvalErr::CostExceeded),
    };
    assert!(serde_json::to_value(forged).is_err());
    let projection = classify_replay_failure(EvalErr::PathIntoAtom).unwrap();
    let ReplayOutcome::Rejected { failure, .. } = projection else {
        unreachable!()
    };
    assert_eq!(failure.kind(), FailureKind::Program);
    assert_eq!(failure.code(), FailureCode::PathIntoAtom);
    assert_ne!(failure.code(), FailureCode::Raise);
}
