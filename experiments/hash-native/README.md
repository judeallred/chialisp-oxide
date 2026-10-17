# Native SHA-256 experiment

This WIP experiment executes the actual SDK module at
`crates/chialisp-oxide/src/hashing.rs`. The ordinary Rust bridge imports that
file with `#[path]`; it does not copy a positive implementation. It uses the
repository's dated rustc directly, without Cargo, dependencies or shared targets.

```sh
python3 experiments/hash-native/check-native.py \
  --work-dir /private/tmp/chialisp-oxide-native-sha-sdk-run \
  --output experiments/hash-native/results.json --self-test

python3 experiments/hash-native/check-native.py \
  --check --self-test --output experiments/hash-native/results.json
```

The work directory must be fresh and outside the repository. Keep it when using
`--check`: the checker requires the actual retained binaries, mutant sources and
rlibs. `--rustc /absolute/path/to/rustc` supports a separately installed matching
compiler; its live commit must match the dated repository toolchain. This runner
does not install a compiler or launch Docker. When run in an emulated worker, use
`--execution-note` to record that explicitly.

Generated host reports are ignored by Git. They contain absolute compiler,
source and binary bindings, so a Mac report is not a portable Linux freshness
baseline. CI executes a fresh run into `$RUNNER_TEMP/hash-native.json`, checks
it in that same environment, and retains the report and native work directory
as artifacts. A new checkout must execute its own run.

Each checked debug and checked optimized executable processes 671 deterministic
requests: fixed empty/`abc`/two-block SHA-256 answers, multipart splits around
padding and block boundaries, bytes including zero/high-bit values, generated
messages and one-megabyte inputs. It also exercises the single-part wrapper when
the other parts are empty. The fixed answers are separate from the generated
Python `hashlib` oracle. The [NIST worked examples](https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA256.pdf)
cover `abc` and the two-block message; [FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final)
specifies SHA-256. These tests are not CAVP validation.

There are also 55 `/usr/bin/openssl` command comparisons and six real compiled
mutants. Each mutation must replace exactly one occurrence in the current SDK
source: a round constant, padding marker, encoded bit length, choose operands,
digest byte order or the second input's length. All mutants must compile and
produce a concrete wrong digest. Multipart hashing intentionally concatenates
without framing: callers must choose domain tags and unambiguous field lengths.

The report binds source/script/vector/toolchain declaration hashes; live rustc,
installed Rust libraries, Python/linker/OpenSSL identities; full build commands;
retained executables/rlibs; all observed digests and mutation witnesses. Freshness
mode recomputes the required input set and compares it to the report. It does not
compile or rerun kernel code. Its corruption tests check missing/stale inputs,
outputs, commands, witnesses and claims, including a temporarily missing binary.

This is finite native evidence. It proves neither universal kernel correctness,
SHA security, compiler preservation, nor the project's native-to-CLVM bridge.
The installed rustc/standard library, linker, OS/runtime, native object generation
and mutable files remain trusted. Hashes cannot authenticate that a recorded run
occurred. Python `hashlib` may share OpenSSL; the original isolated RustCrypto dev
oracle is from the same family used by clvmr. Agreement is not an independent
cryptographic proof. Earlier isolated kernel and Linux-emulation records remain
separate research evidence; this harness rechecks the SDK source itself.
