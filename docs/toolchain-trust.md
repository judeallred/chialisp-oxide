# Trusted compiler and standard library

The supported build assumes the installed `nightly-2026-07-14` compiler and its
matching, unmodified standard library. Compiler-private metadata, source
extraction, the reviewed primitive bridges and the backend are trusted parts
of the current system. The proof models do not establish their preservation.

Rust language and diagnostic item identities identify operations within the
selected compilation. They do not authenticate the bytes of a replacement
`core`, `alloc` or `std`. A rebuilt core library can retain those identities
while changing a method implementation. Comparing rustc's version or commit
does not establish that the selected standard library has the reviewed semantics.

The Cargo integration preserves normal Cargo features, dependencies, build
scripts and compiler arguments within its supported host-analysis contract.
Target units now reject unsupported sysroots, standard-library locations and
backend overrides before exporting IR or continuing dependency metadata.
Native host units and read-only probes do not export puzzle IR. Build scripts,
procedural macros, compiler binaries, installed library bytes and the mutable
filesystem remain trusted build inputs.

The declared-export loader verifies the live driver's digest, full compiler
identity, requested artifact mode, declared local source fingerprints and
complete export bundle. These are useful identity/freshness checks. They are
not authenticated build provenance, a fingerprint of the complete transitive
build environment or proof of standard-library semantics.

## Enforced configuration and loaded-library checks

The driver uses both configuration checks and actual loaded-crate checks:

1. Common extraction/metadata callbacks resolve an absent sysroot to the
   driver's pinned installation. Canonical relative and symlink aliases of
   that same installation work; relocated or missing roots reject with an
   `unsupported toolchain configuration` diagnostic. The actual parsed target
   must be the built-in host target, not a JSON file with the same name.
2. Parsed reserved `--extern` paths for `core`, `alloc` and `std` must resolve
   to files directly in the pinned target-library directory. Compiling a target
   unit with one of those reserved crate names rejects, including a name chosen
   by a source attribute. Ordinary SDK aliases remain supported.
3. Before IR export or dependency-metadata continuation, `tcx.crates` freezes
   the loaded graph, which is limited to 1,024 crates. The guard checks every
   loaded core/alloc/std crate and the actual language/diagnostic item owners
   used by the primitive bridges. Every recorded metadata, archive, dynamic
   library and shared-library-interface source must resolve directly inside
   the pinned target-library directory. Aliases and transitive `-L` searches
   do not replace this check with an argument-name heuristic.
4. The wrapper parses a rustc-expanded copy for routing and passes the original
   arguments to rustc. A literal `@` line inside a response file is therefore
   not expanded again as another file. Parsed target/crate identity must match
   the routed unit before analysis. Compiling print modes such as
   `--print=native-static-libs` and `--print=link-args` still receive the checks.
5. Target-unit sysroot/target checks and rejection of explicit code-generation
   backend overrides run before delegation to early probes too. In the pinned
   compiler, verbose version and LLVM-pass queries can load a backend before
   the normal configuration callback. JSON targets may select a backend as
   well; rejecting only an explicit `-Zcodegen-backend` would miss that route.

Location checks still trust the contents and integrity of the installed toolchain.
They are not cryptographic authentication of that installation.

Routing and real compilation read response files separately. Parsed identity
checks detect target/crate routing changes; this is not an atomic snapshot of
all response-file contents or a defense against concurrent build-input changes.
Host-side native code and its plugins remain outside the CLVM subset checker.
Other compiler-plugin and LLVM configuration remains trusted; rejecting a
codegen-backend override is not a general plugin sandbox.

## Regression evidence

`crates/chialisp-oxide-driver/tests/toolchain.rs` exercises absent, split,
equals-form, relative and symlink sysroots; explicit pinned core metadata;
copied metadata and metadata/archive pairs; renamed, searched and transitive core dependencies;
metadata-only target units; response routing and literal `@` input; reserved
names; host delegation; compiling print modes; custom JSON targets and early
backend probes. Copied library bytes are unchanged. The relevant negative
controls must first compile with ordinary rustc and, where applicable, prove
the copied file was actually loaded through binary dependency information.
They test the supported-location policy without pretending to demonstrate a
changed standard-library algorithm or a real coin exploit.
When rustc coalesces same-hash searched/transitive libraries back to the pinned
file, the tests require that observed pinned location and successful checking.
They do not label an unselected copy as a rejected replacement.
An explicitly native-only, unstable `no_core` dependency fixture loads a renamed
copy without an injected pinned core to coalesce with it. This isolates the
loaded-graph guard in the metadata-only callback. It does not establish that
`no_core` source is accepted as an on-chain Rust export.

```sh
cargo test --locked -p chialisp-oxide-driver --test toolchain
```

Existing Cargo tests retain renamed SDK dependencies, features, generated
source and native-only code. Rejection must produce no new IR and must not be
an internal compiler error. The [rustc command-line reference](https://doc.rust-lang.org/rustc/command-line-arguments.html)
documents sysroots, external crate aliases and dependency search paths; the
[backend reference](https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/codegen-backend.html)
explains why a backend override is executable compiler configuration.

The preservation check compares every existing puzzle's IR/artifacts, byte
outputs, typed failures, costs and ledger observations under the supported
pinned configuration. Source-derived model reports continue to state their
compiler/standard-library assumptions. These guards are neither a compiler
preservation theorem nor a sandbox for hostile build tools.
