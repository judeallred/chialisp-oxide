//! Native witnesses invoke the generated, actual-source policy modules.
use chialisp_oxide::{
    AggSigMe, AssertCoinAnnouncement, AssertMyAmount, AssertMyCoinId, CoinId, CreateCoin,
    CreateCoinAnnouncement, Message32, PublicKey, PuzzleHash,
};
use chialisp_oxide_precommit_controls::{
    claim_drop_age, claim_original, registry_drop_signature, registry_original,
};
use serde_json::{Value, json};

type ClaimPolicy =
    fn((CoinId, Message32, PuzzleHash), bool, CoinId) -> claim_original::PrecommitConditions;

fn atom(value: &[u8]) -> Vec<u8> {
    assert!(value.len() < 64, "bounded fixed-witness codec");
    if value.len() == 1 && value[0] < 128 {
        return value.to_vec();
    }
    let mut encoded = vec![0x80 | u8::try_from(value.len()).unwrap()];
    encoded.extend_from_slice(value);
    encoded
}

fn integer(value: u64) -> Vec<u8> {
    if value == 0 {
        return atom(&[]);
    }
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap();
    let mut payload = Vec::new();
    if bytes[first] & 0x80 != 0 {
        payload.push(0);
    }
    payload.extend_from_slice(&bytes[first..]);
    atom(&payload)
}

fn list(values: &[Vec<u8>]) -> Vec<u8> {
    assert!(values.len() <= 6, "bounded fixed-witness list");
    let mut result = Vec::new();
    for value in values {
        assert!(value.len() <= 1024, "bounded fixed-witness item");
        result.push(0xff);
        result.extend_from_slice(value);
    }
    result.push(0x80);
    result
}

fn claim_output(policy: ClaimPolicy) -> Vec<u8> {
    let output = policy(
        (
            CoinId([0x11; 32]),
            Message32([0x22; 32]),
            PuzzleHash([0x33; 32]),
        ),
        true,
        CoinId([0x44; 32]),
    );
    let mut conditions = vec![
        list(&[integer(output.first.0), atom(&output.first.1.0)]),
        list(&[integer(output.rest.first.0), integer(output.rest.first.1.0)]),
        list(&[
            integer(output.rest.rest.first.0),
            integer(u64::from(output.rest.rest.first.1.0)),
        ]),
        list(&[
            integer(output.rest.rest.rest.first.0),
            atom(&output.rest.rest.rest.first.1.0),
            integer(output.rest.rest.rest.first.2.0),
        ]),
        list(&[
            integer(output.rest.rest.rest.rest.first.0),
            atom(&output.rest.rest.rest.rest.first.1.0),
        ]),
    ];
    if let Some(acceptance) = output.rest.rest.rest.rest.rest {
        conditions.push(list(&[integer(acceptance.0), atom(&acceptance.1.0)]));
    }
    list(&conditions)
}

fn registry_output(
    me: AssertMyCoinId,
    amount: AssertMyAmount,
    ready: AssertCoinAnnouncement,
    accept: CreateCoinAnnouncement,
    signature: Option<AggSigMe>,
    receipt: CreateCoin,
) -> Vec<u8> {
    let mut conditions = vec![
        list(&[integer(me.0), atom(&me.1.0)]),
        list(&[integer(amount.0), integer(amount.1.0)]),
        list(&[integer(ready.0), atom(&ready.1.0)]),
        list(&[integer(accept.0), atom(&accept.1.0)]),
    ];
    if let Some(signature) = signature {
        conditions.push(list(&[
            integer(signature.0),
            atom(&signature.1.0),
            atom(&signature.2.0),
        ]));
    }
    conditions.push(list(&[
        integer(receipt.0),
        atom(&receipt.1.0),
        integer(receipt.2.0),
    ]));
    list(&conditions)
}

fn observe(name: &str) -> Value {
    // These fixed inputs and encoding bounds are host harness checks, outside
    // any policy-panic adapter. This harness never labels a host error rejection.
    let output = match name {
        "claim_original" => claim_output(claim_original::puzzle),
        "claim_drop_age" => claim_output(claim_drop_age::puzzle),
        "registry_original" => {
            let output = registry_original::puzzle(
                PuzzleHash([0x55; 32]),
                CoinId([0x11; 32]),
                CoinId([0x66; 32]),
                PuzzleHash([0x33; 32]),
                (PublicKey([0x77; 48]), PuzzleHash([0x88; 32]), [0x99; 32]),
            );
            registry_output(
                output.0,
                output.1,
                output.2,
                output.3,
                Some(output.4),
                output.5,
            )
        }
        "registry_drop_signature" => {
            let output = registry_drop_signature::puzzle(
                PuzzleHash([0x55; 32]),
                CoinId([0x11; 32]),
                CoinId([0x66; 32]),
                PuzzleHash([0x33; 32]),
                (PublicKey([0x77; 48]), PuzzleHash([0x88; 32]), [0x99; 32]),
            );
            registry_output(output.0, output.1, output.2, output.3, None, output.4)
        }
        other => panic!("unknown source control {other}"),
    };
    json!({"status":"accepted", "output_hex":hex::encode(output)})
}

#[test]
fn actual_derived_policies_match_independent_output_witnesses() {
    let witnesses: Value = serde_json::from_str(include_str!("../witnesses.json")).unwrap();
    let rows = witnesses.as_array().unwrap();
    assert_eq!(rows.len(), 4);
    let names = [
        "claim_original",
        "claim_drop_age",
        "registry_original",
        "registry_drop_signature",
    ];
    for (row, name) in rows.iter().zip(names) {
        assert_eq!(row["name"], name);
        let actual = observe(name);
        assert_eq!(actual, row["expected"], "{name}");
        println!(
            "PRECOMMIT_CONTROL_NATIVE {}",
            json!({"name":name, "status":actual["status"], "output_hex":actual["output_hex"]})
        );
    }
}
