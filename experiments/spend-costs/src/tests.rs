use super::measurement::measure;
use chialisp_oxide_simulator::{
    SecretKey, Signature, Simulator, SpendBundle, coin_spend, create_coin, sign_transaction,
};

#[test]
fn actual_signed_bundle_distinguishes_all_three_charge_rules() {
    let key = SecretKey::from_seed(&[0x31; 32]);
    let source = format!(
        "(mod () (list (list 50 0x{} 0x010203)))",
        hex::encode(key.public_key().to_bytes())
    );
    let reveal = chialisp_oxide_reference::compile(&source).unwrap();
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, &reveal, 1).unwrap();
    let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
    let signature = sign_transaction(std::slice::from_ref(&spend), &[key]).unwrap();
    let bundle = SpendBundle::new(vec![spend], signature);
    simulator.new_transaction(bundle.clone()).unwrap();
    let costs = measure(&bundle, 0).unwrap();
    assert_eq!(costs["helper"]["signature_verified"], false);
    let plain = &costs["generators"][0];
    let compressed = &costs["generators"][1];
    assert_eq!(
        plain["metrics"]["total_cost"].as_u64().unwrap(),
        costs["helper"]["total_cost"].as_u64().unwrap() + 24_020
    );
    for encoding in [plain, compressed] {
        assert_eq!(encoding["metrics"]["signature_verified"], true);
        assert_eq!(encoding["signature_controls"].as_array().unwrap().len(), 2);
    }
    let mut unsigned = bundle;
    unsigned.aggregated_signature = Signature::default();
    assert!(
        measure(&unsigned, 0).is_err(),
        "helper success must not authorize an unsigned measurement"
    );
}

#[test]
fn unsigned_bundle_without_signature_conditions_still_has_a_negative_control() {
    let reveal = chialisp_oxide_reference::compile("(mod () ())").unwrap();
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, &reveal, 1).unwrap();
    let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
    let bundle = SpendBundle::new(vec![spend], Signature::default());
    simulator.new_transaction(bundle.clone()).unwrap();
    let costs = measure(&bundle, 0).unwrap();
    for encoding in costs["generators"].as_array().unwrap() {
        assert_eq!(encoding["signature_controls"].as_array().unwrap().len(), 1);
        assert_eq!(
            encoding["signature_controls"][0]["signature"],
            "unrelated_nonempty"
        );
    }
}
