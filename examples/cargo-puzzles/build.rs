use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_STRICT_CAP");
    let limit = if env::var_os("CARGO_FEATURE_STRICT_CAP").is_some() {
        100
    } else {
        1000
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"));
    fs::write(
        output.join("limits.rs"),
        format!("pub const PAYMENT_LIMIT: u64 = {limit};\n"),
    )
    .expect("write the generated puzzle limit");
}
