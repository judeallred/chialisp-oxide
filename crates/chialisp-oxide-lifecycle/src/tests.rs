//! Constructed adapter declarations test journal relations; these are not
//! executed compiler/native/VM, signature-verification or live-chain evidence.
use super::*;
use chia_protocol::{Bytes32, Coin};

fn plan() -> Plan {
    Plan {
        network: Network {
            testnet: "testnet11".into(),
            genesis_challenge: [9; 32],
        },
        amount: 1000,
        outputs: vec![Output {
            puzzle_hash: [4; 32],
            amount: 999,
        }],
    }
}

fn artifact() -> Artifact {
    Artifact {
        reveal: vec![1],
        solution: vec![0x80],
    }
}

fn observation(plan: &Plan, coin: &CoinRecord) -> Observation {
    Observation {
        network: plan.network.clone(),
        tip: Tip {
            height: 10,
            header_hash: [10; 32],
        },
        input_id: coin.id(),
        input: InputState::Unspent { created_height: 8 },
        children: vec![],
    }
}

fn checked() -> Journal {
    let mut journal = Journal::new(plan()).unwrap();
    journal
        .append(
            0,
            Event::Built {
                artifact: artifact(),
            },
        )
        .unwrap();
    journal
        .append(
            1,
            Event::Checked {
                record: CheckRecord {
                    artifact_digest: artifact_digest(&artifact()).unwrap(),
                    observed_outputs: plan().outputs,
                    native_receipt: b"constructed native declaration".to_vec(),
                    clvm_receipt: b"constructed VM declaration".to_vec(),
                },
            },
        )
        .unwrap();
    journal
}

fn funded() -> Journal {
    let mut journal = checked();
    let coin = CoinRecord {
        parent: [3; 32],
        puzzle_hash: artifact().puzzle_hash().unwrap(),
        amount: 1000,
    };
    let observation = observation(journal.plan(), &coin);
    journal
        .append(2, Event::Funded { coin, observation })
        .unwrap();
    journal
}

fn signed() -> Journal {
    let mut journal = funded();
    let request = journal.signing_request().unwrap();
    // Identity aggregate is only an unverified fixture; recording an external
    // validator assertion below does not turn this into a BLS verification test.
    journal
        .append(
            3,
            Event::SignatureReturned {
                receipt: SignReceipt {
                    request_id: request.id().unwrap(),
                    signed_bundle: request.unsigned_bundle,
                },
            },
        )
        .unwrap();
    journal
}

fn ready() -> Journal {
    let mut journal = signed();
    let request = journal.signing_request().unwrap();
    journal
        .append(
            4,
            Event::SignatureValidated {
                validation: SignatureValidation {
                    request_id: request.id().unwrap(),
                    signed_bundle_digest: digest(&request.unsigned_bundle),
                    receipt: b"constructed external validator assertion".to_vec(),
                },
            },
        )
        .unwrap();
    journal
}

fn confirmed_observation(journal: &Journal) -> Observation {
    let request = journal.signing_request().unwrap();
    Observation {
        network: journal.plan().network.clone(),
        tip: Tip {
            height: 11,
            header_hash: [11; 32],
        },
        input_id: request.input.id(),
        input: InputState::Spent {
            created_height: 8,
            spent_height: 11,
        },
        children: request
            .expected_children
            .into_iter()
            .map(|coin| ChildObservation {
                coin,
                created_height: 11,
            })
            .collect(),
    }
}

#[test]
fn sdk_coin_identity_includes_amount_boundaries() {
    for amount in [0, 127, 128, u64::MAX] {
        let record = CoinRecord {
            parent: [1; 32],
            puzzle_hash: [2; 32],
            amount,
        };
        let sdk = Coin::new(Bytes32::new([1; 32]), Bytes32::new([2; 32]), amount);
        assert_eq!(record.id(), sdk.coin_id().to_bytes());
    }
}

#[test]
fn unverified_receipt_cannot_unlock_submission_and_validation_binds_exact_bytes() {
    let mut journal = signed();
    assert_eq!(journal.stage(), Stage::AwaitingSignatureValidation);
    assert!(journal.submission_bytes().is_err());
    let request = journal.signing_request().unwrap();
    let before = journal.to_json().unwrap();
    assert!(
        journal
            .append(
                4,
                Event::SignatureValidated {
                    validation: SignatureValidation {
                        request_id: request.id().unwrap(),
                        signed_bundle_digest: [0; 32],
                        receipt: vec![1],
                    }
                }
            )
            .is_err()
    );
    assert_eq!(journal.to_json().unwrap(), before);
    assert!(
        journal
            .append(
                4,
                Event::Submitted {
                    request_id: request.id().unwrap(),
                    signed_bundle_digest: digest(&request.unsigned_bundle),
                    public_receipt: vec![1]
                }
            )
            .is_err()
    );
}

#[test]
fn signer_cannot_change_request_network_or_actual_spend_prefix() {
    let mut journal = funded();
    let request = journal.signing_request().unwrap();
    let mut different_network = request.clone();
    different_network.network.genesis_challenge = [8; 32];
    assert_ne!(request.id().unwrap(), different_network.id().unwrap());
    assert!(
        journal
            .append(
                3,
                Event::SignatureReturned {
                    receipt: SignReceipt {
                        request_id: different_network.id().unwrap(),
                        signed_bundle: request.unsigned_bundle.clone(),
                    }
                }
            )
            .is_err()
    );
    let mut changed = request.unsigned_bundle.clone();
    changed[0] ^= 1; // attacker-selected vector length, rejected before parsing
    assert!(
        journal
            .append(
                3,
                Event::SignatureReturned {
                    receipt: SignReceipt {
                        request_id: request.id().unwrap(),
                        signed_bundle: changed,
                    }
                }
            )
            .is_err()
    );
    assert_eq!(journal.revision(), 3);
}

#[test]
fn invalid_signature_point_can_be_retained_but_cannot_be_validated() {
    let mut journal = funded();
    let request = journal.signing_request().unwrap();
    let mut bytes = request.unsigned_bundle.clone();
    let signature_start = bytes.len() - 96;
    bytes[signature_start..].fill(0xff);
    journal
        .append(
            3,
            Event::SignatureReturned {
                receipt: SignReceipt {
                    request_id: request.id().unwrap(),
                    signed_bundle: bytes.clone(),
                },
            },
        )
        .unwrap();
    assert!(
        journal
            .append(
                4,
                Event::SignatureValidated {
                    validation: SignatureValidation {
                        request_id: request.id().unwrap(),
                        signed_bundle_digest: digest(&bytes),
                        receipt: vec![1],
                    }
                }
            )
            .is_err()
    );
    assert!(journal.submission_bytes().is_err());
}

#[test]
fn resume_replays_every_event_and_rejects_unknown_fields_revision_holes() {
    let journal = ready();
    let bytes = journal.to_json().unwrap();
    let resumed = Journal::from_json(&bytes).unwrap();
    assert_eq!(resumed.stage(), Stage::ReadyToSubmit);
    assert_eq!(
        resumed.submission_bytes().unwrap(),
        journal.submission_bytes().unwrap()
    );
    assert_eq!(resumed.to_json().unwrap(), bytes);
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["cached_stage"] = serde_json::json!("confirmed");
    assert!(Journal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    value.as_object_mut().unwrap().remove("cached_stage");
    value["entries"][1]["revision"] = serde_json::json!(3);
    assert!(Journal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn confirmation_requires_exact_complete_children_and_reorg_revokes_it() {
    let mut journal = ready();
    let request = journal.signing_request().unwrap();
    journal
        .append(
            5,
            Event::Submitted {
                request_id: request.id().unwrap(),
                signed_bundle_digest: digest(journal.submission_bytes().unwrap()),
                public_receipt: vec![1],
            },
        )
        .unwrap();
    let good = confirmed_observation(&journal);
    let mut incomplete = good.clone();
    incomplete.children.clear();
    journal
        .append(
            6,
            Event::Observed {
                observation: incomplete,
            },
        )
        .unwrap();
    assert_eq!(journal.stage(), Stage::Conflict);
    journal
        .append(7, Event::Observed { observation: good })
        .unwrap();
    assert_eq!(journal.stage(), Stage::Confirmed);
    assert!(journal.submission_bytes().is_err());
    let reorg = observation(journal.plan(), &request.input);
    journal
        .append(8, Event::Observed { observation: reorg })
        .unwrap();
    assert_eq!(journal.stage(), Stage::ReadyToSubmit);
    assert_eq!(journal.submission_bytes().unwrap(), request.unsigned_bundle);
}

#[test]
fn missing_funding_after_reorg_invalidates_old_signatures() {
    let mut journal = ready();
    let request = journal.signing_request().unwrap();
    let mut missing = observation(journal.plan(), &request.input);
    missing.input = InputState::Missing;
    journal
        .append(
            5,
            Event::Observed {
                observation: missing,
            },
        )
        .unwrap();
    assert_eq!(journal.stage(), Stage::AwaitingFunding);
    assert!(journal.signing_request().is_err());
    assert!(journal.submission_bytes().is_err());
    let coin = CoinRecord {
        parent: [99; 32],
        ..request.input.clone()
    };
    journal
        .append(
            6,
            Event::Funded {
                observation: observation(journal.plan(), &coin),
                coin,
            },
        )
        .unwrap();
    assert_ne!(
        journal.signing_request().unwrap().id().unwrap(),
        request.id().unwrap()
    );
    assert_eq!(journal.stage(), Stage::AwaitingSignature);
}

#[test]
fn reconciler_rejects_wrong_network_input_parent_and_creation_height_atomically() {
    let mut journal = ready();
    let good = confirmed_observation(&journal);
    let before = journal.to_json().unwrap();
    let mut variants = vec![good.clone(); 4];
    variants[0].network.genesis_challenge = [1; 32];
    variants[1].input_id = [0; 32];
    variants[2].children[0].coin.parent = [0; 32];
    variants[3].children[0].created_height = 9;
    for observation in variants {
        assert!(journal.append(5, Event::Observed { observation }).is_err());
        assert_eq!(journal.to_json().unwrap(), before);
    }
}

#[test]
fn lost_submission_ack_can_resume_from_matching_chain_outputs() {
    let mut journal = ready();
    let observed = confirmed_observation(&journal);
    journal
        .append(
            5,
            Event::Observed {
                observation: observed,
            },
        )
        .unwrap();
    assert_eq!(journal.stage(), Stage::Confirmed);
    let mut resumed = Journal::from_json(&journal.to_json().unwrap()).unwrap();
    assert_eq!(resumed.stage(), Stage::Confirmed);
    assert!(
        resumed
            .append(
                0,
                Event::Built {
                    artifact: artifact()
                }
            )
            .is_err()
    );
}

#[test]
fn output_overflow_duplicates_and_artifact_trailing_bytes_fail_closed() {
    let mut invalid = plan();
    invalid.outputs = vec![
        Output {
            puzzle_hash: [1; 32],
            amount: u64::MAX,
        },
        Output {
            puzzle_hash: [2; 32],
            amount: 1,
        },
    ];
    assert!(Journal::new(invalid).is_err());
    let mut invalid = plan();
    invalid.outputs.push(invalid.outputs[0].clone());
    assert!(Journal::new(invalid).is_err());
    let mut journal = Journal::new(plan()).unwrap();
    assert!(
        journal
            .append(
                0,
                Event::Built {
                    artifact: Artifact {
                        reveal: vec![1, 1],
                        solution: vec![0x80]
                    }
                }
            )
            .is_err()
    );
    assert_eq!(journal.stage(), Stage::AwaitingBuild);
}

#[test]
fn plain_admission_bounds_depth_nodes_and_rejects_backref_markers() {
    // 0xfe is legal within an atom payload; only a structural backref marker
    // is unsupported. These are local host admission failures, not VM policy
    // rejection or universal resource-safety proofs.
    assert!(
        artifact_digest(&Artifact {
            reveal: vec![0x81, 0xfe],
            solution: vec![0x80]
        })
        .is_ok()
    );
    let mut deep = [0xff, 0x80].repeat(512);
    deep.push(0x80);
    let mut wide = vec![0x80];
    for _ in 0..16 {
        let mut next = vec![0xff];
        next.extend_from_slice(&wide);
        next.extend_from_slice(&wide);
        wide = next;
    }
    for reveal in [deep, wide, vec![0xff, 1, 0xfe, 1]] {
        let mut journal = Journal::new(plan()).unwrap();
        assert!(
            journal
                .append(
                    0,
                    Event::Built {
                        artifact: Artifact {
                            reveal,
                            solution: vec![0x80]
                        }
                    }
                )
                .is_err()
        );
        assert_eq!(journal.revision(), 0);
    }
}

#[test]
fn signing_request_protocol_is_exact_before_serialization() {
    let mut request = funded().signing_request().unwrap();
    request.protocol = "unreviewed/v1".into();
    assert!(request.id().is_err());
    request.protocol = "x".repeat(2 * 1024 * 1024);
    assert!(request.id().is_err());
}
