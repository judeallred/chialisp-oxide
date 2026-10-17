---
title: Compiler trust and standard libraries
description: Understand which Rust toolchain configurations the prototype checks, reproduce native-valid rejection controls, and keep location checks separate from compiler proofs.
---

**WIP prototype. Not ready for use with funds or production puzzles.**

Ordinary `cargo check` and `cargo test` check your native Rust program.
`cargo chialisp-oxide` adds the supported CLVM subset and backend checks.
Both depend on a compiler and standard library with the expected semantics.

The prototype uses `nightly-2026-07-14` and its matching installed libraries.
Some Rust operations, including primitive byte comparison and restricted `Box`
construction, have reviewed mappings into CLVM. Compiler language items and
trait identities identify those operations within a compilation. They do not
authenticate a replacement library that retains those identities.

## Keep the supported configuration explicit

The driver checks target configuration before analysis and the libraries
rustc actually loaded before exporting IR or continuing target dependency
metadata. It requires the built-in host target and the pinned sysroot.
Relative paths and symlink aliases that resolve to that same installation
work. A relocated root or custom JSON target does not become supported merely
because it resembles the original.

Reserved `core`, `alloc` and `std` extern paths must resolve to the installed
target-library directory. The loaded-graph check also follows renamed and
transitive dependencies and actual primitive language/diagnostic item owners.
It checks metadata, archive, dynamic-library and shared-library-interface
sources. Looking only for an argument spelled `--extern core=...` would miss
other routes documented by [rustc's library search rules](https://doc.rust-lang.org/rustc/command-line-arguments.html).

Target units cannot use explicit code-generation backend overrides. Early
target probes also receive sysroot, target and backend checks: a verbose
version or LLVM-pass query can load a backend before the usual compiler
callback. Custom target descriptions can select a backend too. Such a backend
is [executable compiler configuration](https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/codegen-backend.html),
not ordinary puzzle data.

## Reproduce the rejection controls

Build the local tools, then run the dedicated native/wrapper comparisons:

```sh
cargo build --workspace --locked
cargo test --locked -p chialisp-oxide-driver --test toolchain
```

The tests use byte-identical copies of installed core metadata and
metadata/archive pairs.
Ordinary rustc must accept the relevant examples; binary dependency information
must show that the copied file was actually loaded. Our wrapper must then
reject the unsupported location and emit no puzzle IR. Renamed and transitive
dependencies exercise the loaded-library check rather than only an argument
filter. When rustc coalesces same-hash searched/transitive libraries back to the
installed file, that observed resolution is a positive control. An unselected
copy is not presented as a rejected replacement. Relocated sysroots and
metadata-only target units have separate controls.

One explicitly native-only dependency fixture uses unstable `no_core` to load
a renamed copy without an injected pinned core. Its dependency information
must prove the copy was loaded, and the metadata-only guard must reject it.
This isolates the graph check from reserved-extern argument checks; it does
not claim that `no_core` source can be compiled as an on-chain Rust export.

These tests establish a configuration boundary. They do not demonstrate a
changed library algorithm, an on-chain exploit, or proof that all malicious
compilers are detectable. The ordinary Cargo regressions additionally retain
SDK aliases, features, generated source, build scripts and native-only code.

## Preserve response-file and native behavior

The router reads an expanded argument copy, while rustc receives the original
arguments for compilation. A literal `@` filename inside a response file
retains rustc's once-only expansion behavior. Parsed target and crate name must
match the routed unit. A print option that continues compilation still gets
normal analysis; it is not classified as an informational exit.
The separate research source-file harness rejects `@response-file` input.
Use Cargo for compiler response files, or `./@name.rs` for a literal filename.

Native host units and informational probes do not publish target IR. Host
build scripts and procedural macros remain executable, trusted build inputs.
Routing and compilation read response files separately, so this mechanism is
not an atomic filesystem snapshot or a sandbox for hostile build processes.

## Separate enforcement from proof

Canonical locations still trust the installed compiler and library bytes.
Replacing files inside that trusted installation, changing dynamic-loading
configuration, or racing mutable build inputs is outside this protection.
Other compiler-plugin and LLVM options remain trusted; rejecting one backend
override mechanism is not a general plugin sandbox.
The source-derived proof reports identify source inputs and check fresh local
compiler/driver identities; they are not authenticated binary attestations.

The [byte-policy checker](/chialisp-oxide/verification/byte-proofs/) proves its
stated property in an IR model and replays selected native/CLVM executions.
It does not prove rustc extraction, the primitive bridges, the emitted decoder,
Rue optimization or final CLVM semantics for all inputs. These configuration
checks do not close that preservation gap or establish security of a coin.

See the [supported Rust subset](/chialisp-oxide/reference/supported-rust/),
[formal checks](/chialisp-oxide/verification/formal/), and the
[implementation trust notes](https://github.com/judeallred/chialisp-oxide/blob/main/docs/toolchain-trust.md)
for the remaining boundaries.
