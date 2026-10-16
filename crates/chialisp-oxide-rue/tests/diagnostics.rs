use std::path::Path;

use chialisp_oxide_rue::compile_file;

#[test]
fn reports_syntax_type_and_missing_entry_errors_instead_of_publishing_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for fixture in ["syntax_error.rue", "type_error.rue", "missing_main.rue"] {
        for optimize in [false, true] {
            let error = compile_file(&root.join(fixture), optimize).unwrap_err();
            assert!(!error.to_string().is_empty(), "{fixture}");
        }
    }
}
