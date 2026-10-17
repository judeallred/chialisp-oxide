//! Production-path Cargo authorization, real Rust execution, exact byte hashing.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::{FailureCode, FailureKind, Outcome};
use chialisp_oxide_ir::Program;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn native(fixture: &Fixture) {
    success(
        Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .output()
            .unwrap(),
    );
}
fn atom(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "()".into()
    } else {
        format!("0x{}", hex(bytes))
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn wanted(bytes: &[u8]) -> Outcome {
    Outcome::Accepted {
        output_hex: format!("a0{}", hex(bytes)),
    }
}
fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|p| p.name.ends_with(&format!("::{name}")))
        .unwrap()
}
fn vectors() -> Vec<Vec<u8>> {
    let mut out = vec![
        vec![],
        vec![0],
        vec![0, 0],
        vec![1],
        vec![0, 1],
        vec![255],
        b"abc".to_vec(),
    ];
    for n in [
        31, 32, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 255, 256, 4096, 8192,
    ] {
        out.push((0..n).map(|i| (i * 37 + 129) as u8).collect());
    }
    out
}

#[test]
fn actual_cargo_hashes_native_bytes_and_preserves_strict_effects() {
    let fixture = Fixture::with_sdk_alias("digest_api");
    let source = include_str!("hashing/library.rs").replace("chialisp_oxide", "digest_api");
    let vectors = vectors();
    // The Cargo-native path calls the exact public exports against RustCrypto's
    // independently computed expected bytes, not a copied scalar implementation.
    let rows = vectors
        .iter()
        .map(|bytes| format!("(&{bytes:?}[..],{:?})", digest(bytes)))
        .collect::<Vec<_>>()
        .join(",");
    fixture.source(&format!("{source}\n#[cfg(test)] mod native {{use super::*;#[test] fn exact_bytes() {{for (bytes,wanted) in [{rows}] {{assert_eq!(one(&bytes),wanted);assert_eq!(parts(&bytes,&[],&[]),wanted);assert_eq!(nested(&bytes),digest_api::sha256(&wanted));assert_eq!(ignored_argument(&bytes),7);assert_eq!(unused(&bytes),7);}}assert_eq!(fixed([0;32]),digest_api::sha256(&[0;32]));assert_eq!(snapshot([1;32],[2;32]),(digest_api::sha256(&[1;32]),digest_api::sha256(&[2;32])));}}}}"));
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 18);
    for name in [
        "one",
        "parts",
        "nested",
        "prefixed",
        "pair_hash",
        "fixed",
        "snapshot",
        "raw",
        "lazy",
        "ignored_argument",
        "unused",
        "hash_before_reject",
    ] {
        assert!(
            find(&programs, name).uses_runtime(),
            "hash requires strict bindings: {name}"
        );
    }
    for optimize in [false, true] {
        let names = [
            "one",
            "parts",
            "nested",
            "prefixed",
            "pair_hash",
            "fixed",
            "snapshot",
            "raw",
            "lazy",
            "ignored_argument",
            "unused",
            "no_hash",
            "hash_before_reject",
            "reject_then_spin",
            "spin_then_reject",
            "fake_one",
            "fake_parts",
        ];
        let codes = names
            .into_iter()
            .map(|name| {
                (
                    name,
                    chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for bytes in &vectors {
            let input = format!("({})", atom(bytes));
            let hash = digest(bytes);
            for name in ["one", "parts", "raw"] {
                let input = if name == "parts" {
                    format!("({} () ())", atom(bytes))
                } else if name == "raw" {
                    atom(bytes)
                } else {
                    input.clone()
                };
                assert_eq!(
                    vm::execute(&codes[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    wanted(&hash),
                    "{name} optimize={optimize} length={}",
                    bytes.len()
                );
            }
            assert_eq!(
                vm::execute(&codes["nested"], &input, 10_000_000)
                    .unwrap()
                    .outcome,
                wanted(&digest(&hash))
            );
            let mut prefixed = vec![1];
            prefixed.extend(bytes);
            assert_eq!(
                vm::execute(&codes["prefixed"], &input, 10_000_000)
                    .unwrap()
                    .outcome,
                wanted(&digest(&prefixed))
            );
            for name in ["unused", "ignored_argument", "no_hash"] {
                assert_eq!(
                    vm::execute(&codes[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    Outcome::Accepted {
                        output_hex: "07".into()
                    }
                );
            }
            for (name, value) in [("fake_one", 9), ("fake_parts", 8)] {
                assert_eq!(
                    vm::execute(&codes[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    wanted(&[value; 32])
                );
            }
        }
        for (a, b, c) in [
            (b"a".as_slice(), b"b".as_slice(), b"c".as_slice()),
            (&[][..], &[0][..], &[0, 1][..]),
            (&[255][..], &[128][..], &[][..]),
        ] {
            let mut all = a.to_vec();
            all.extend(b);
            all.extend(c);
            assert_eq!(
                vm::execute(
                    &codes["parts"],
                    &format!("({} {} {})", atom(a), atom(b), atom(c)),
                    10_000_000
                )
                .unwrap()
                .outcome,
                wanted(&digest(&all))
            );
        }
        let left = digest(&[1; 32]);
        let right = digest(&[2; 32]);
        let mut pair = vec![2];
        pair.extend(left);
        pair.extend(right);
        assert_eq!(
            vm::execute(
                &codes["pair_hash"],
                &format!("({} {})", atom(&[1; 32]), atom(&[2; 32])),
                10_000_000
            )
            .unwrap()
            .outcome,
            wanted(&digest(&pair))
        );
        assert_eq!(
            vm::execute(
                &codes["fixed"],
                &format!("({})", atom(&[0; 32])),
                10_000_000
            )
            .unwrap()
            .outcome,
            wanted(&digest(&[0; 32]))
        );
        assert_eq!(
            vm::execute(
                &codes["snapshot"],
                &format!("({} {})", atom(&[1; 32]), atom(&[2; 32])),
                10_000_000
            )
            .unwrap()
            .outcome,
            Outcome::Accepted {
                output_hex: format!("ffa0{}ffa0{}80", hex(&left), hex(&right))
            }
        );
        assert_eq!(
            vm::execute(&codes["lazy"], "(1 . 2)", 10_000_000)
                .unwrap()
                .outcome,
            wanted(&[7; 32])
        );
        let coercion = chialisp_oxide_ir::compile(find(&programs, "as_tree"), optimize).unwrap();
        for input in ["0x0000", "0x0001", "0x80ff"] {
            assert_eq!(
                vm::execute(&coercion, &format!("({input})"), 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Accepted {
                    output_hex: format!("82{}", &input[2..])
                }
            );
        }
        for input in ["(())", "(0x00)", "(0x000000)", "((1 . 2))"] {
            assert_eq!(
                vm::execute(&coercion, input, 10_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
        for input in ["()", "(1 . 2)", "(1 2)", "((1 . 2))"] {
            assert_eq!(
                vm::execute(&codes["one"], input, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        for (name, outcome, code, kind) in [
            (
                "reject_then_spin",
                Outcome::Rejected,
                FailureCode::Raise,
                FailureKind::Program,
            ),
            (
                "spin_then_reject",
                Outcome::Inconclusive,
                FailureCode::CostExceeded,
                FailureKind::Resource,
            ),
        ] {
            let run =
                chialisp_oxide_backend_experiment::execute(&codes[name], "()", 100_000).unwrap();
            assert_eq!(run.outcome, outcome, "{name} optimize={optimize}");
            assert_eq!(run.failure.as_ref().unwrap().code(), code);
            assert_eq!(run.failure.unwrap().kind(), kind);
        }
        let big = format!("({})", atom(&vec![42; 8192]));
        let plain = vm::execute(&codes["no_hash"], &big, 10_000_000)
            .unwrap()
            .cost
            .unwrap();
        let hashing = vm::execute(&codes["unused"], &big, 10_000_000)
            .unwrap()
            .cost
            .unwrap();
        assert!(hashing > plain + 1000);
        for name in ["unused", "ignored_argument", "hash_before_reject"] {
            let run = chialisp_oxide_backend_experiment::execute(&codes[name], &big, plain + 1000)
                .unwrap();
            assert_eq!(
                run.outcome,
                Outcome::Inconclusive,
                "retained hash {name} optimize={optimize}"
            );
            assert_eq!(run.failure.unwrap().code(), FailureCode::CostExceeded);
        }
    }
}

fn counterfeit(source: &str, root: &str) -> Fixture {
    let fixture = Fixture::new();
    let sdk = fixture.0.join("relocated-reviewed-sdk");
    std::fs::create_dir_all(sdk.join("src")).unwrap();
    std::fs::write(
        sdk.join("Cargo.toml"),
        "[package]\nname=\"chialisp-oxide\"\nversion=\"0.7.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(sdk.join("src/moved.rs"), source).unwrap();
    std::fs::write(sdk.join("src/lib.rs"), root).unwrap();
    std::fs::write(fixture.0.join("Cargo.toml"),"[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\nrenamed={package=\"chialisp-oxide\",path=\"relocated-reviewed-sdk\"}\n").unwrap();
    fixture.source("#[doc=\"__oxide_puzzle_v1\"] pub fn entry(bytes:&[u8])->[u8;32]{renamed::sha256_parts(bytes,&[],&[])}\n#[cfg(test)] mod native {#[test] fn callable(){let _=super::entry(b\"abc\");}}");
    fixture
}
fn ctfe_temporary(source: &str, statement: &str) -> String {
    assert_eq!(source.matches("const K: [u32; 64] = [").count(), 1);
    source
        .replacen(
            "const K: [u32; 64] = [",
            &format!("const K: [u32; 64] = {{{statement}["),
            1,
        )
        .replacen("    ];", "    ]};", 1)
}
fn kernel() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../chialisp-oxide/src/hashing.rs"),
    )
    .unwrap()
}

#[test]
fn production_authorization_is_relocation_safe_and_never_self_authorizes_mutations() {
    let original = kernel();
    let root = "#![no_std]\nmod moved;pub use moved::sha256_parts;\n";
    let fixture = counterfeit(&original, root);
    native(&fixture);
    success(fixture.cargo(true));
    let exports = fixture.exports();
    assert_eq!(exports.len(), 1);
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&exports[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&code, "(0x616263)", 10_000_000)
                .unwrap()
                .outcome,
            wanted(&digest(b"abc"))
        );
    }
    let native_mutants = [
        (
            "round_constant",
            original.replacen("0x428a2f98", "0x428a2f99", 1),
        ),
        (
            "inline_attribute",
            original.replacen(
                "pub fn sha256_parts",
                "#[inline(always)]\npub fn sha256_parts",
                1,
            ),
        ),
        (
            "static_constants",
            original.replacen("const K:", "static K:", 1),
        ),
        (
            "pointer_constants",
            original.replacen("const K: [u32; 64] = [", "const K: &[u32; 64] = &[", 1),
        ),
        (
            "transitive_helper",
            original.replacen("    digest\n}", "    changed(digest)\n}", 1)
                + "\nfn changed(mut value:[u8;32])->[u8;32]{value[0]^=1;value}\n",
        ),
        (
            "ctfe_large_temporary",
            ctfe_temporary(&original, "let _unused=[0u8;4097];"),
        ),
        (
            "ctfe_nested_temporary",
            ctfe_temporary(&original, "let _unused=[[0u8;1024];1024];"),
        ),
        (
            "ctfe_control_flow",
            ctfe_temporary(&original, "let mut counter=0;while counter<1 {counter+=1;}"),
        ),
        (
            "same_name_zero_body",
            "pub fn sha256_parts(_: &[u8],_: &[u8],_: &[u8])->[u8;32]{[0;32]}".into(),
        ),
        (
            "wrong_signature",
            "pub fn sha256_parts(_: &[u8],_: &[u8],_: &[u8])->[u8;31]{[0;31]}".into(),
        ),
    ];
    for (name, mutant) in native_mutants {
        assert_ne!(mutant, original, "nonvacuous source mutant {name}");
        let fixture = counterfeit(&mutant, root);
        if name == "wrong_signature" {
            fixture.source("#[doc=\"__oxide_puzzle_v1\"] pub fn entry(bytes:&[u8])->[u8;31]{renamed::sha256_parts(bytes,&[],&[])}");
        }
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "mutant admitted {name}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("reviewed native SHA256 primitive rejected"),
            "{name}: {error}"
        );
        assert!(
            fixture.exports().is_empty(),
            "failed reserved candidate exported IR: {name}"
        );
    }
    // A wrapper has no intrinsic authority: its real native body still returns9.
    let altered = original.replacen("sha256_parts(bytes, &[], &[])", "{let _=bytes;[9;32]}", 1);
    assert_ne!(altered, original);
    let fixture = counterfeit(
        &altered,
        "#![no_std]\nmod moved;pub use moved::{sha256,sha256_parts};\n",
    );
    fixture.source(
        "#[doc=\"__oxide_puzzle_v1\"]pub fn entry(bytes:&[u8])->[u8;32]{renamed::sha256(bytes)}",
    );
    success(fixture.cargo(false));
    success(fixture.cargo(true));
    let exports = fixture.exports();
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&exports[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&code, "(0x616263)", 10_000_000)
                .unwrap()
                .outcome,
            wanted(&[9; 32])
        );
    }
}

fn configured(fixture: &Fixture, flags: &str, wrapped: bool) -> Output {
    let mut command = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"));
    command
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env("RUSTFLAGS", flags)
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["check", "--lib", "--offline", "--jobs", "1", "--target-dir"])
        .arg(fixture.0.join(if wrapped {
            "configured-extract"
        } else {
            "configured-native"
        }));
    if wrapped {
        command
            .args(["--target", &fixture.host()])
            .env("RUSTC_WRAPPER", env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .env("CHIALISP_OXIDE_CARGO_MODE", "1")
            .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", "wrapper_fixture")
            .env("CHIALISP_OXIDE_OUTPUT_DIR", fixture.0.join("exports"));
    }
    command.output().unwrap()
}
#[test]
fn changed_profile_and_unreviewed_array_coercions_fail_closed() {
    // The existing driver policy appends checked arithmetic/debug assertions.
    // These raw requests therefore keep the reviewed effective profile.
    for flags in ["-Coverflow-checks=off", "-Cdebug-assertions=off"] {
        let fixture = Fixture::new();
        fixture.source("#[chialisp_oxide::puzzle]pub fn entry(bytes:&[u8])->[u8;32]{chialisp_oxide::sha256(bytes)}");
        success(configured(&fixture, flags, false));
        success(configured(&fixture, flags, true));
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&fixture.exports()[0], optimize).unwrap();
            assert_eq!(
                vm::execute(&code, "(0x616263)", 10_000_000)
                    .unwrap()
                    .outcome,
                wanted(&digest(b"abc"))
            );
        }
    }
    {
        let flags = "-Cpanic=abort";
        let fixture = Fixture::new();
        fixture.source("#[chialisp_oxide::puzzle]pub fn entry(bytes:&[u8])->[u8;32]{chialisp_oxide::sha256(bytes)}");
        success(configured(&fixture, flags, false));
        let output = configured(&fixture, flags, true);
        assert!(!output.status.success(), "changed profile {flags} admitted");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(
                "profile requires overflow-checks=yes, debug-assertions=yes and panic=unwind"
            ),
            "{flags}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fixture.exports().is_empty());
    }
    for source in [
        "#[chialisp_oxide::puzzle]pub fn entry(bytes:&mut [u8;32])->[u8;32]{chialisp_oxide::sha256(bytes)}",
        "#[chialisp_oxide::puzzle]pub fn entry(bytes:[u8;257])->[u8;32]{chialisp_oxide::sha256(&bytes)}",
        "#[chialisp_oxide::puzzle]pub fn entry(bytes:[u16;32])->bool{let slice:&[u16]=&bytes;slice==slice}",
    ] {
        let fixture = Fixture::new();
        fixture.source(source);
        success(fixture.cargo(false));
        assert!(
            !fixture.cargo(true).status.success(),
            "unsupported coercion admitted: {source}"
        );
        assert!(fixture.exports().is_empty());
    }
}

#[test]
fn public_sdk_reexport_authorizes_the_actual_cross_crate_body_not_its_location() {
    let original = kernel();
    let fixture = counterfeit(
        &original,
        "#![no_std]\npub use kernel_impl::sha256_parts;\n",
    );
    let sdk = fixture.0.join("relocated-reviewed-sdk");
    let dependency = sdk.join("kernel_impl");
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    std::fs::write(
        dependency.join("Cargo.toml"),
        "[package]\nname=\"kernel_impl\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    let source = dependency.join("src/lib.rs");
    std::fs::write(&source, &original).unwrap();
    let manifest = sdk.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\n[dependencies]\nkernel_impl={{path=\"kernel_impl\"}}\n"),
    )
    .unwrap();
    success(fixture.cargo(false));
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&code, "(0x616263)", 10_000_000)
                .unwrap()
                .outcome,
            wanted(&digest(b"abc"))
        );
    }
    std::fs::remove_dir_all(fixture.0.join("exports")).unwrap();
    let changed = original.replacen("0x428a2f98", "0x428a2f99", 1);
    assert_ne!(changed, original);
    std::fs::write(&source, changed).unwrap();
    success(fixture.cargo(false));
    let output = fixture.cargo(true);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("typed-MIR closure/profile fingerprint mismatch"),
        "{error}"
    );
    assert!(fixture.exports().is_empty());
}
