//! Closed finite recipe witnesses. Expected codes are not inferred from a run.
use super::{
    validation::{destination_atom, expected, list, pair},
    *,
};
use chialisp_oxide_vm::{FailureCode, VmFailure};

pub(super) fn expected_outcome(
    lane: &str,
    case: &str,
    environment: &[u8],
    destination: &[u8; 32],
) -> Result<VmOutcome> {
    ensure!(
        matches!(lane, "rust_off" | "rust_on" | "chialisp_reference"),
        "unknown finite VM lane"
    );
    let atom = destination_atom(destination);
    let (wire, accepted) = match case {
        "destination_atom32" => (list(&[&atom]), true),
        "missing" => (vec![0x80], false),
        "whole_atom" => (vec![1], false),
        "improper_tail" => (pair(&atom, &[1]), false),
        "extra_argument" => (list(&[&atom, &[0x80]]), false),
        "pair_destination" => (list(&[&pair(&[1], &[0x80])]), false),
        "short_destination" => (list(&[&[0x9f; 32]]), false),
        "long_destination" => (list(&[&[0xa1; 34]]), false),
        "curried_nil_solution" => (vec![0x80], true),
        "curried_extra_solution" => (vec![0xff, 1, 0x80], false),
        "curried_atom_solution" => (vec![1], false),
        _ => anyhow::bail!("unknown finite VM witness"),
    };
    ensure!(wire == environment, "finite VM witness environment differs");
    if accepted {
        return Ok(VmOutcome::Accepted {
            output_hex: hex::encode(expected(destination)),
        });
    }
    // Existing named_conditions.rs126–136/273–296 and boomerang's finite
    // decoder witnesses record Eq(pair,nil) InvalidOpArg in Rust/off. The
    // optimized nil truthiness guard raises. Both exact environments above
    // were independently replayed against a02's retained modules/curries.
    // The independent reference's nine Raise witnesses were executed for
    // three destinations in validation's reference controls, not generalized
    // from Rust or from a failure-kind comparison.
    let code = if lane == "rust_off" && matches!(case, "extra_argument" | "curried_extra_solution")
    {
        FailureCode::InvalidOpArg
    } else {
        FailureCode::Raise
    };
    Ok(VmOutcome::Rejected {
        failure: VmFailure::from_code(code),
    })
}

pub(super) fn verify(observation: &VmObservation, destination: &[u8; 32]) -> Result<()> {
    let environment = hex::decode(&observation.environment_hex)?;
    let expected = expected_outcome(
        &observation.lane,
        &observation.case,
        &environment,
        destination,
    )?;
    ensure!(
        observation.expected_outcome == expected,
        "finite VM expected-outcome metadata changed"
    );
    ensure!(
        observation.outcome == expected,
        "{}/{} env {}: expected {}, observed {}",
        observation.lane,
        observation.case,
        observation.environment_hex,
        serde_json::to_string(&expected)?,
        serde_json::to_string(&observation.outcome)?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pair_tail_codes_are_exactly_lane_case_and_environment_scoped() {
        let destination = [0x23; 32];
        for (case, env) in [
            (
                "extra_argument",
                list(&[&destination_atom(&destination), &[0x80]]),
            ),
            ("curried_extra_solution", vec![0xff, 1, 0x80]),
        ] {
            for lane in ["rust_off", "rust_on", "chialisp_reference"] {
                let expected = expected_outcome(lane, case, &env, &destination).unwrap();
                let mut observation = VmObservation {
                    lane: lane.into(),
                    case: case.into(),
                    environment_hex: hex::encode(&env),
                    cost: None,
                    outcome: expected.clone(),
                    expected_outcome: expected,
                };
                verify(&observation, &destination).unwrap();
                observation.outcome = VmOutcome::Rejected {
                    failure: VmFailure::from_code(if lane == "rust_off" {
                        FailureCode::Raise
                    } else {
                        FailureCode::InvalidOpArg
                    }),
                };
                assert!(verify(&observation, &destination).is_err());
                observation.environment_hex = "80".into();
                assert!(verify(&observation, &destination).is_err());
            }
        }
        assert!(expected_outcome("rust_off", "unlisted", &[0x80], &destination).is_err());
    }
    #[test]
    fn other_program_resource_and_infrastructure_codes_cannot_substitute() {
        let destination = [0x23; 32];
        for code in [
            FailureCode::InvalidOpArg,
            FailureCode::PathIntoAtom,
            FailureCode::CostExceeded,
            FailureCode::InternalError,
        ] {
            let expected = expected_outcome("rust_off", "missing", &[0x80], &destination).unwrap();
            let failure = VmFailure::from_code(code);
            let outcome = if failure.is_program_rejection() {
                VmOutcome::Rejected { failure }
            } else {
                VmOutcome::Inconclusive { failure }
            };
            let observation = VmObservation {
                lane: "rust_off".into(),
                case: "missing".into(),
                environment_hex: "80".into(),
                cost: None,
                outcome,
                expected_outcome: expected,
            };
            assert!(verify(&observation, &destination).is_err());
        }
    }
}
