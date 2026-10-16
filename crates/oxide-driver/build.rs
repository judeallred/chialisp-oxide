use std::process::Command;

fn main() {
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(&compiler)
        .args(["--print", "sysroot"])
        .output()
        .expect("the pinned rustc must be installed");
    assert!(output.status.success());
    let sysroot = String::from_utf8(output.stdout).expect("UTF-8 sysroot");
    let version = Command::new(&compiler)
        .arg("-vV")
        .output()
        .expect("rustc version");
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).expect("UTF-8 rustc version");
    let commit = version
        .lines()
        .find_map(|line| line.strip_prefix("commit-hash: "))
        .expect("rustc commit hash");
    let compiler_version = version.lines().next().expect("rustc version line");
    // rustc_driver is a compiler-private dynamic library, not a Cargo dependency.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", sysroot.trim());
    println!("cargo:rustc-env=OXIDE_SYSROOT={}", sysroot.trim());
    println!("cargo:rustc-env=OXIDE_RUSTC_COMMIT={commit}");
    println!("cargo:rustc-env=OXIDE_RUSTC_VERSION={compiler_version}");
    println!("cargo:rerun-if-changed=build.rs");
}
