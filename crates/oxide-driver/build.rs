use std::process::Command;

fn main() {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("the pinned rustc must be installed");
    assert!(output.status.success());
    let sysroot = String::from_utf8(output.stdout).expect("UTF-8 sysroot");
    // rustc_driver is a compiler-private dynamic library, not a Cargo dependency.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", sysroot.trim());
    println!("cargo:rerun-if-changed=build.rs");
}
