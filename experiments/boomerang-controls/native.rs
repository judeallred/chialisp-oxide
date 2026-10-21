//! Native output witnesses invoke the generated actual-source package itself.
use chialisp_oxide::{Amount, PublicKey, PuzzleHash, Timestamp};
use chialisp_oxide_boomerang_controls::{
    drop_claim_signature, early_refund, equivalent, late_claim, original,
};
use serde_json::{Value, json};

type Policy = fn(original::Terms, bool) -> original::BoomerangConditions;

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
    assert!(values.len() <= 4);
    let mut result = Vec::new();
    for value in values {
        assert!(value.len() <= 1024);
        result.push(0xff);
        result.extend_from_slice(value);
    }
    result.push(0x80);
    result
}

fn observe(policy: Policy, claim: bool, key: [u8; 48]) -> Value {
    let terms = (
        PublicKey(key),
        PuzzleHash([0x42; 32]),
        PuzzleHash([0x43; 32]),
        Amount::from_mojos(1_000),
        Timestamp::from_unix_seconds(100),
    );
    // Fixed native inputs and codec bounds are outside any policy-panic adapter.
    // These constructors do not panic, and the harness does not turn host errors
    // into policy rejection evidence.
    let output = policy(terms, claim);
    let mut conditions = vec![
        list(&[integer(output.first.0), integer(output.first.1.0)]),
        list(&[integer(output.rest.first.0), integer(output.rest.first.1.0)]),
        list(&[
            integer(output.rest.rest.first.0),
            atom(&output.rest.rest.first.1.0),
            integer(output.rest.rest.first.2.0),
        ]),
    ];
    if let Some(signature) = output.rest.rest.rest {
        conditions.push(list(&[
            integer(signature.0),
            atom(&signature.1.0),
            atom(&signature.2.0),
        ]));
    }
    json!({"status":"accepted", "output_hex":hex::encode(list(&conditions))})
}

#[test]
fn actual_derived_policies_match_independent_output_witnesses() {
    let witnesses: Value = serde_json::from_str(include_str!("../witnesses.json")).unwrap();
    let rows = witnesses.as_array().unwrap();
    assert_eq!(rows.len(), 9);
    let key: [u8; 48] = hex::decode(
        "8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5",
    )
    .unwrap()
    .try_into()
    .unwrap();
    for row in rows {
        let policy: Policy = match row["name"].as_str().unwrap() {
            "original" => original::puzzle,
            "equivalent" => equivalent::puzzle,
            "drop_claim_signature" => drop_claim_signature::puzzle,
            "late_claim" => late_claim::puzzle,
            "early_refund" => early_refund::puzzle,
            other => panic!("unknown source control {other}"),
        };
        let actual = observe(policy, row["claim"].as_bool().unwrap(), key);
        assert_eq!(actual, row["expected"], "{}", row["name"]);
        println!(
            "BOOMERANG_MUTATION_NATIVE {}",
            json!({"name":row["name"], "witness":row["witness"], "actual":actual})
        );
    }
}
