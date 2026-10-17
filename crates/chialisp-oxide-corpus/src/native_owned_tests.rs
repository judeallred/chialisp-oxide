//! Integration boundaries of the actual native prefix adapter. Host resource
//! errors remain Result::Err; malformed ABI/policy failures are Ok(None).
use super::{native, native_owned_tree};
use clvmr::{Allocator, serde::node_to_bytes};

fn spine(depth: usize, left: bool, leaf: &str) -> String {
    (0..depth).fold(leaf.into(), |rest, _| {
        if left {
            format!("({rest} . ())")
        } else {
            format!("(0x00 . {rest})")
        }
    })
}

fn serialized(tree: &str) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let node = rue_clvm::assemble(&mut allocator, tree).unwrap();
    node_to_bytes(&allocator, node).unwrap()
}

fn fits_alone(tree: &str) {
    let mut allocator = Allocator::new();
    let node = rue_clvm::assemble(&mut allocator, tree).unwrap();
    native_owned_tree::preflight(&allocator, node).unwrap();
}

fn host_error(environment: &str, expected: &str) {
    let error = native::execute("prefix-list", environment)
        .expect_err("host limits must not become policy rejection or success");
    assert!(error.to_string().contains(expected), "{error:#}");
}

#[test]
fn prefix_checks_deep_first_children_and_arbitrary_final_tails_before_boxes() {
    let deep = spine(128, true, "0x0001");
    fits_alone(&deep);
    // A one-element list has a shallow right spine, but its head is deep.
    host_error(&format!("(({deep}) ())"), "depth");
    // Empty L returns all of P, including its deep first branches.
    host_error(&format!("(() {deep})"), "depth");
}

#[test]
fn complete_environment_node_budget_is_not_reset_for_each_argument() {
    let mut wide = "()".to_string();
    for _ in 0..12 {
        wide = format!("({wide} . {wide})");
    }
    let list = format!("({wide})");
    // Each argument fits independently; their expanded occurrence counts plus
    // the strict argument-list cells exceed the combined 16,384-node limit.
    fits_alone(&list);
    fits_alone(&wide);
    host_error(&format!("({list} {wide})"), "nodes");
}

#[test]
fn complete_environment_atom_budget_is_not_reset_for_each_argument() {
    let atom = format!("0x{}", "11".repeat(600_000));
    let list = format!("({atom})");
    fits_alone(&list);
    fits_alone(&atom);
    host_error(&format!("({list} {atom})"), "atom bytes");
}

#[test]
fn prefix_output_depth_is_checked_separately_from_each_input_and_full_environment() {
    let list = spine(64, false, "()");
    let tail = spine(65, true, "0x0001");
    let environment = format!("({list} {tail})");
    fits_alone(&list);
    fits_alone(&tail);
    fits_alone(&environment);
    host_error(&environment, "output depth");
}

#[test]
fn exact_output_depth_boundary_preserves_all_tail_bytes() {
    let list = spine(64, false, "()");
    let tail = spine(64, true, "0x0001");
    let expected = spine(64, false, &tail);
    let output = native::execute("prefix-list", &format!("({list} {tail})")).unwrap();
    assert_eq!(output, Some(serialized(&expected)));
    // The final tail need not be a proper list or a canonical integer atom.
    for tail in ["0x00", "0x0001", "(2 1)", "(0xff . 0x0000)"] {
        assert_eq!(
            native::execute("prefix-list", &format!("(() {tail})")).unwrap(),
            Some(serialized(tail))
        );
    }
}

#[test]
fn malformed_abi_and_nonempty_zero_list_terminators_are_policy_rejections() {
    for input in [
        "()",
        "1",
        "(())",
        "(() () 1)",
        "(() () . 0x00)",
        "(() () . 0x0000)",
        "(0x00 ())",
        "((1 . 0x00) ())",
        "((1 . 0x0000) ())",
    ] {
        assert_eq!(
            native::execute("prefix-list", input).unwrap(),
            None,
            "{input}"
        );
    }
    assert_eq!(
        native::execute("prefix-list", "(() 0x0000)").unwrap(),
        Some(serialized("0x0000"))
    );
}
