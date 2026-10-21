use super::*;

#[test]
fn observed_standard_reveal_has_exact_independent_pins_and_recurry_bytes() {
    let source = bytes(OBSERVED_REVEAL_HEX);
    assert_eq!(source.len(), 291);
    let expected = "545e9487959a1b2f6c50b5567dc63193eba648d8007896ec52bcde427f310499"
        .parse()
        .unwrap();
    let result = inspect_reveal(&source, &Limits::default(), Some(expected)).unwrap();
    assert_eq!(result.report().form, RevealForm::CanonicalCurry);
    assert!(result.report().module_recognized);
    assert_eq!(
        result.reveal().identity().blob_sha256.to_string(),
        "1e0bb524a16dde1f7916a26f2d59ff47883ccae2ff0cc4720bd640a04cc617e6"
    );
    let module = result.module().unwrap();
    assert_eq!(module.bytes(), bytes(STANDARD_HEX));
    assert_eq!(module.identity().bytes, 227);
    assert_eq!(
        module.identity().blob_sha256.to_string(),
        "21747ef070376012f22f7b426edaa36a244a77f8825d13d5529963b921ad1da7"
    );
    assert_eq!(
        module.identity().module_tree_hash.to_string(),
        "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"
    );
    assert_eq!(result.arguments()[0].bytes(), atom(&bytes(KEY_HEX)));
    assert_eq!(
        result.arguments()[0]
            .identity()
            .module_tree_hash
            .to_string(),
        "61cdb8338f0fbfe905af96d851e9c6dd23cca0ae78fa4b04f995d389f3bf73c3"
    );
    assert_eq!(
        curried(module.bytes(), &[result.arguments()[0].bytes().to_vec()]),
        source
    );
    let report = serde_json::to_value(&result).unwrap();
    assert_eq!(
        report["standard_arguments"],
        json!({"status":"decoded","synthetic_public_key_hex":KEY_HEX,"role":"synthetic_public_key","key_is_infinity":false})
    );
    assert_eq!(
        report["known_module"]["source_revision"],
        "d122456feeef55708d354a065623037bfa010714"
    );
    kind(
        inspect_reveal(&source, &Limits::default(), Some(Digest32::new([0; 32]))),
        ErrorKind::DigestMismatch,
    );
    kind(
        inspect_reveal(module.bytes(), &Limits::default(), Some(expected)),
        ErrorKind::DigestMismatch,
    );
}

#[test]
fn exact_raw_module_precedes_its_curry_shaped_implementation_constants() {
    let raw = bytes(STANDARD_HEX);
    let result = inspect(&raw);
    assert_eq!(result.report().form, RevealForm::RecognizedRawModule);
    assert!(result.report().module_recognized);
    assert!(result.arguments().is_empty());
    assert!(std::ptr::eq(result.reveal(), result.module().unwrap()));
    assert_eq!(
        result.report().module.as_ref().unwrap().clvm_hex,
        STANDARD_HEX
    );
    assert!(matches!(
        result.report().standard_arguments,
        StandardArguments::NotCurried
    ));
    assert!(standard_module_matches(result.module().unwrap()));
    // Alter one admitted opcode atom, retaining byte count and tree shape.
    let mut other = raw;
    other[1] = 3;
    let altered = inspect(&curried(&other, &[atom(&bytes(KEY_HEX))]));
    assert!(!altered.report().module_recognized);
    assert!(altered.report().known_module.is_none());
    assert!(matches!(
        altered.report().standard_arguments,
        StandardArguments::NotApplicable
    ));
}

#[test]
fn standard_schema_errors_are_successful_inspections_with_separate_statuses() {
    let module = bytes(STANDARD_HEX);
    let key = atom(&bytes(KEY_HEX));
    for (args, reason) in [
        (vec![], StandardArgumentError::WrongArity),
        (vec![key.clone(), key], StandardArgumentError::WrongArity),
        (vec![bytes("ff0102")], StandardArgumentError::ExpectedAtom),
        (
            vec![atom(&[1; 47])],
            StandardArgumentError::WrongPublicKeyLength,
        ),
        (
            vec![atom(&[1; 49])],
            StandardArgumentError::WrongPublicKeyLength,
        ),
        (
            vec![atom(&[0; 48])],
            StandardArgumentError::InvalidPublicKey,
        ),
        (
            vec![atom(&[0xff; 48])],
            StandardArgumentError::InvalidPublicKey,
        ),
    ] {
        let source = curried(&module, &args);
        let result = inspect(&source);
        assert_eq!(result.report().form, RevealForm::CanonicalCurry);
        assert!(result.report().module_recognized);
        match &result.report().standard_arguments {
            StandardArguments::Invalid { reason: observed } => assert_eq!(*observed, reason),
            other => panic!("unexpected argument status {other:?}"),
        }
        assert_eq!(result.reveal().bytes(), source);
    }
}

#[test]
fn parser_accepted_infinity_is_visible_without_an_owner_or_signature_claim() {
    let mut infinity = [0u8; 48];
    infinity[0] = 0xc0;
    let result = inspect(&curried(&bytes(STANDARD_HEX), &[atom(&infinity)]));
    let report = serde_json::to_value(&result).unwrap();
    assert_eq!(report["standard_arguments"]["status"], "decoded");
    assert_eq!(report["standard_arguments"]["key_is_infinity"], true);
    let other = inspect(&curried(
        &bytes(STANDARD_HEX),
        &[atom(&chia_bls::PublicKey::generator().to_bytes())],
    ));
    assert_eq!(
        result.module().unwrap().identity(),
        other.module().unwrap().identity()
    );
    assert_ne!(
        result.reveal().identity().module_tree_hash,
        other.reveal().identity().module_tree_hash
    );
}

#[test]
fn nested_curried_program_is_not_recursively_relabelled_as_the_standard_module() {
    let inner = curried(&bytes(STANDARD_HEX), &[atom(&bytes(KEY_HEX))]);
    let outer = curried(&inner, &[bytes("80")]);
    let result = inspect(&outer);
    assert_eq!(result.report().form, RevealForm::CanonicalCurry);
    assert_eq!(result.module().unwrap().bytes(), inner);
    assert!(!result.report().module_recognized);
    assert!(matches!(
        result.report().standard_arguments,
        StandardArguments::NotApplicable
    ));
}
