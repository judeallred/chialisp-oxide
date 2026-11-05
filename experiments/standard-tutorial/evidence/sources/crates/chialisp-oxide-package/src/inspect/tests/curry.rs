use super::*;

#[test]
fn arbitrary_exact_arguments_keep_order_markers_and_improper_tails() {
    let module = bytes("ff0102");
    let arguments = [
        bytes("80"),
        bytes("00"),
        bytes("820001"),
        bytes("ff0102"),
        bytes("86fdff32303236"),
        bytes("82feff"),
    ];
    let source = curried(&module, &arguments);
    let result = inspect(&source);
    assert_eq!(result.report().form, RevealForm::CanonicalCurry);
    assert!(!result.report().module_recognized);
    assert_eq!(result.module().unwrap().bytes(), module);
    assert_eq!(result.arguments().len(), arguments.len());
    for (i, expected) in arguments.iter().enumerate() {
        assert_eq!(result.arguments()[i].bytes(), expected);
        assert_eq!(result.report().arguments[i].clvm_hex, hex::encode(expected));
        assert_eq!(
            result.report().arguments[i].identity.blob_sha256,
            crate::digest(expected)
        );
    }
    assert_ne!(
        result.arguments()[0].identity().module_tree_hash,
        result.arguments()[1].identity().module_tree_hash
    );
    let recurry = curried(
        result.module().unwrap().bytes(),
        &result
            .arguments()
            .iter()
            .map(|a| a.bytes().to_vec())
            .collect::<Vec<_>>(),
    );
    assert_eq!(recurry, source);
    assert!(matches!(
        result.report().standard_arguments,
        StandardArguments::NotApplicable
    ));
}

#[test]
fn only_exact_opcode_atoms_proper_outer_lists_and_terminal_one_classify() {
    let module = [1];
    let arg = [0x80];
    let valid = curried(&module, &[arg.to_vec()]);
    assert_eq!(inspect(&valid).report().form, RevealForm::CanonicalCurry);
    let quote = pair(&[1], &module);
    let quoted_arg = pair(&[1], &arg);
    let canonical_args = list(&[&[4], &quoted_arg, &[1]]);
    let mut bad = vec![
        list(&[&[3], &quote, &canonical_args]),
        list(&[&bytes("820002"), &quote, &canonical_args]),
        list(&[&[2], &pair(&[2], &module), &canonical_args]),
        list(&[&[2], &pair(&bytes("820001"), &module), &canonical_args]),
        list(&[&[2], &[1], &canonical_args]),
        list(&[&[2], &quote, &list(&[&[5], &quoted_arg, &[1]])]),
        list(&[&[2], &quote, &list(&[&bytes("820004"), &quoted_arg, &[1]])]),
        list(&[
            &[2],
            &quote,
            &list(&[&[4], &pair(&bytes("820001"), &arg), &[1]]),
        ]),
        list(&[&[2], &quote, &list(&[&[4], &quoted_arg, &[0x80]])]),
        list(&[&[2], &quote, &list(&[&[4], &quoted_arg, &[0]])]),
        list(&[&[2], &quote, &list(&[&[4], &quoted_arg, &bytes("820001")])]),
        list(&[&[2], &quote, &canonical_args, &[0x80]]),
        list(&[&[2], &quote]),
    ];
    // A nonnil proper-list terminator is a valid arbitrary tree, but is not
    // the closed three-field curry expression's required list shape.
    let mut improper = valid;
    *improper.last_mut().unwrap() = 0;
    bad.push(improper);
    for source in bad {
        let result = inspect(&source);
        assert_eq!(result.report().form, RevealForm::Unclassified);
        assert!(result.module().is_none());
        assert!(result.arguments().is_empty());
        assert!(!result.report().module_recognized);
    }
}

#[test]
fn quoted_proper_lists_remain_literal_data_instead_of_being_reinterpreted() {
    // (q MODULE) is (q . (MODULE)), an exact quote of a different module.
    // Accept canonical shape with that arbitrary datum; never erase its list.
    let inner = list(&[&[1]]);
    let source = curried(&inner, &[list(&[&[2], &[3]])]);
    let result = inspect(&source);
    assert_eq!(result.report().form, RevealForm::CanonicalCurry);
    assert_eq!(result.module().unwrap().bytes(), inner);
    assert_eq!(result.arguments()[0].bytes(), list(&[&[2], &[3]]));
}

#[test]
fn ordinary_unclassified_programs_are_not_execution_failures() {
    for source in [
        bytes("80"),
        bytes("01"),
        bytes("ff0102"),
        bytes("ff01ff0280"),
    ] {
        let result = inspect(&source);
        assert_eq!(result.report().form, RevealForm::Unclassified);
        assert_eq!(result.reveal().bytes(), source);
        assert!(result.module().is_none());
        let report = serde_json::to_value(&result).unwrap();
        assert_eq!(report["form"], "unclassified");
        assert_eq!(
            report["standard_arguments"],
            json!({"status":"not_applicable"})
        );
    }
}
