# Offline package registry

**WIP prototype. Not ready for use with real funds or in production.** The
registry stores immutable local packages and checks their bytes, structural
metadata and declared attachments. These checks do not establish source or
compiler authenticity, source/IR/CLVM equivalence, authorization, an audit or
production safety.

This extends the local storage increment of
[issue #21](https://github.com/judeallred/chialisp-oxide/issues/21). It does not
download dependencies, query a node or execute a puzzle. Registry commands do
not launch Cargo, rustup, the compiler, a driver, a build script, a VM or a
network client. Building a new puzzle remains a separate operation.

## Build with Cargo, then import an immutable bundle

Author puzzles in a normal Cargo library using `chialisp-oxide` and annotated
exports. Keep native Rust checks separate from the additional CLVM subset
check and build. Start with the
[Cargo quickstart](https://judeallred.github.io/chialisp-oxide/start/quickstart/)
for the pinned toolchain and authoring setup.

The following checkout commands use placeholder consumer and output paths.
Run them from the chialisp-oxide checkout after installing its pinned Rust
toolchain and components:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
cargo check --manifest-path /path/to/puzzle/Cargo.toml --locked
cargo test --manifest-path /path/to/puzzle/Cargo.toml --locked
target/debug/cargo-chialisp-oxide check --manifest-path /path/to/puzzle/Cargo.toml --locked
target/debug/cargo-chialisp-oxide build --manifest-path /path/to/puzzle/Cargo.toml --locked --output-dir /path/to/artifacts
```

The CLVM build publishes a new immutable bundle directory and prints its
`manifest.json` path. By default, it also records bounded selected-library
compiler-read files and named launcher snapshots automatically. Use that exact
path for import. Replace `IMMUTABLE_RUN`
and `my_puzzles::payment::puzzle` below with the actual directory and fully
qualified export name from your build:

```sh
target/debug/cargo-chialisp-oxide registry publish --bundle /path/to/artifacts/IMMUTABLE_RUN/manifest.json --entry my_puzzles::payment::puzzle --registry .local-puzzles --json
```

Import admits the complete bounded export set before selecting the requested
entry. It checks every listed IR program, export identity, serialized CLVM
artifact and associated receipt, including unselected exports. An invalid
unselected export prevents publication. The import requires the immutable
`manifest.json`; it does not follow `latest.json` or rebuild missing files.
Undeclared adjacent disassembly and presentation files are not imported.

The selected package retains its exact IR JSON, original complete bundle
manifest JSON, selected artifact bytes and all declared source files when the
bundle has a source receipt. It does not retain all unselected
IR files or reconstruct the original source tree. Later package verification
checks the retained attachments; it cannot authenticate the earlier complete
bundle read or prove which process produced those files.

If `cargo-chialisp-oxide` is already installed and on `PATH`, the normal Cargo
subcommand spelling is equivalent:

```sh
cargo chialisp-oxide registry publish --bundle /path/to/artifacts/IMMUTABLE_RUN/manifest.json --entry my_puzzles::payment::puzzle --registry .local-puzzles --json
```

The direct executable form is useful when Cargo is unavailable on the machine
that only verifies or stores an existing package. All project crates remain
unpublished; a local executable build or reviewed pinned checkout is required.

## Verify, look up and retrieve packages

Publication prints a JSON summary with `object_id`, `module_tree_hash` and
`blob_sha256`. Replace `OBJECT_ID` and `MODULE_TREE_HASH` below with the actual
64-character lowercase hexadecimal values. They identify different things.

```sh
target/debug/cargo-chialisp-oxide registry verify --package .local-puzzles/objects/OBJECT_ID.json --json
target/debug/cargo-chialisp-oxide registry lookup --module-hash MODULE_TREE_HASH --registry .local-puzzles --json
target/debug/cargo-chialisp-oxide registry get --object-id OBJECT_ID --registry .local-puzzles --json
```

The corresponding installed Cargo forms are:

```sh
cargo chialisp-oxide registry verify --package .local-puzzles/objects/OBJECT_ID.json --json
cargo chialisp-oxide registry lookup --module-hash MODULE_TREE_HASH --registry .local-puzzles --json
cargo chialisp-oxide registry get --object-id OBJECT_ID --registry .local-puzzles --json
```

`verify` checks an envelope file without opening a registry. `lookup` opens an
existing local registry and returns every admitted package variant with the
requested module tree hash, ordered by object identity. It checks all objects
within its scan limits; a corrupt object cannot hide behind a different
claimed module hash. A module hash does not choose a unique ABI, source claim
or compiler mode. `get` retrieves and verifies one object by its exact envelope
identity. These CLI commands print summaries; `get` does not emit the full
envelope. Its exact bytes remain in `objects/OBJECT_ID.json`.

To copy a complete package into another explicitly selected local registry:

```sh
target/debug/cargo-chialisp-oxide registry publish --package .local-puzzles/objects/OBJECT_ID.json --registry .another-local-registry --json
```

This package-only publication rechecks the envelope and needs no original
bundle, source checkout or compiler. `--entry` belongs only to `--bundle`
import. Publication creates the selected local registry if needed; it does not
publish to GitHub, crates.io, a chain or a remote service. An exact repeat
returns `already_present` after verifying the existing object. Different
packages for one module hash remain separate immutable objects.

Use `registry --help` for the actual command syntax. With `--json`, stdout
contains machine-readable results or a structured error; diagnostics also use
stderr and failed commands return an error exit status. Without `--json`, the
CLI prints a readable result and the verification scope.

## Keep the identities separate

- **Object ID:** SHA-256 of the complete exact package-envelope bytes. There is
  no self-referential object ID inside the envelope. Changing whitespace,
  metadata or an attachment changes this identity. Readers do not claim a
  canonical JSON representation.
- **Blob SHA-256:** SHA-256 of the exact serialized CLVM bytes. It identifies
  the byte artifact, independently of its decoded tree or attached claims.
- **Module tree hash:** the CLVM tree hash of the admitted decoded artifact.
  This is the key used by `lookup`. The field name does not prove that the tree
  is an uncurried module.
- **Curried puzzle hash:** the tree hash after arguments are committed into a
  puzzle instance. It normally differs from the template hash. If the stored
  artifact is already curried, its recorded `module_tree_hash` is that
  instance's tree hash. This increment does not uncurry it or recover arguments.
- **Attachment digests:** separate SHA-256 identities cover exact retained IR,
  bundle-manifest bytes and each attached source file. They establish content
  consistency, not authorship, compilation or equivalent behavior.

Two packages may share a module tree hash while carrying different IR, build
declarations, source declarations or provenance labels. Such a package remains
a different object. A matching hash does not authorize replacing a puzzle,
selecting an ABI automatically or spending a coin.

## Structural ABI is a description of the retained IR

A Rust export records the actual IR wire version and an ABI derived from its
validated retained `Program`: environment policy, parameter names and logical
types, and result type. It preserves strict versus ignored versus raw
environments, integer width and signedness, fixed byte lengths, proper tuples,
explicit pairs and optional-output structure.

These distinctions matter. A strict nullary entry expects an empty argument
list; an ignored nullary entry deliberately accepts any environment. A raw
entry receives the whole CLVM tree. A tuple is a proper list, whereas an
explicit `Cons` pair may have an improper tail. Optional output distinguishes
`None` from `Some(())`. Package admission retains the compiler's existing IR
capability restrictions; it does not add optional inputs or general Rust enums.

Nominal Rust domain names do not survive in this descriptor. `Amount` and
`Timestamp` can lower to the same logical integer type; `PuzzleHash` and
`CoinId` can lower to the same fixed byte type. Parameter spelling cannot
recover these meanings. The registry does not infer a safe binding API from
them. The attached IR's ABI also does not prove that the attached CLVM artifact
implements that ABI or the same policy.

## Automatically declared files and build context

Ordinary `cargo chialisp-oxide build` emits bundle schema 2 with a closed source
receipt. Its registry import produces `source_scope: "declared_files_only"`
and retains the exact bytes of every declared logical file. There is no
handwritten source JSON step. Import validates the complete source inventory
alongside all exports, then requires exact receipt/attachment correspondence
when verifying the resulting package. It does not reopen the original checkout
or reconstruct those files on the host filesystem.

The receipt distinguishes these observations:

- Successful selected-library compiler text/binary file reads under the selected
  package (`package/`) and its actual Cargo `OUT_DIR` (`generated/`), with exact
  returned bytes and successful read counts. The exact metadata-selected library
  root may instead use `entry/library.rs` when outside those roots.
- Launcher snapshots of the selected/workspace manifests and exact build-script
  root. These must remain byte-identical before and after the Cargo invocation.
  An external build-script root uses `entry/build-script.rs`; its surrounding
  directory is not captured. A root snapshot does not prove Cargo reran the
  script or capture its transitive inputs.
- The workspace lockfile after successful Cargo, if present. Normal unlocked
  builds may create or update it. This is a post-Cargo declaration, not a trace
  of the exact dependency resolution bytes consumed earlier.

Other successful external reads have bounded omission counters. Their paths
and contents are absent. Dependency compiler units, build-script execution,
arbitrary subprocesses, environment reads and the complete effective dependency
graph remain outside this scope. The unit records its parsed feature cfg set,
not all custom cfg or environment values. Raw data preserves empty files,
non-UTF8 binary bytes, BOMs and line endings; text normalization is not applied.

Compiler text, driver digest, toolchain, target, Rue revision, ordered Cargo
options and package identities remain recorded declarations. The receipt binds
them and every exact emitted IR file to its enclosing bundle. These consistency
checks do not authenticate the compiler or show that attached source, IR and
CLVM have equivalent behavior. Rehashing a changed, internally consistent
declaration creates a different package identity rather than proving its author
told the truth.

Capture uses fixed host resource limits. Missing, conflicting, nonportable or
oversized observations fail packaging without publishing a partial bundle or
silently discarding sources. A deliberately artifact-only build uses:

```sh
target/debug/cargo-chialisp-oxide build --manifest-path /path/to/puzzle/Cargo.toml --locked --artifact-only --output-dir /path/to/artifacts
```

`--artifact-only` emits legacy bundle schema 1, whose default import keeps
`source_scope: "not_recorded"` and an empty source list. `check` performs no
capture and rejects that build-only flag. Existing immutable schema-1 bundles
remain accepted unchanged. Old readers that do not support embedded bundle
schema 2 reject it; the outer package-envelope schema remains 1.

The package schema can also carry explicit source attachments with
`source_scope: "declared_files_only"`. Each attachment has a unique normalized
relative path label, exact contents encoded as hexadecimal and a checked
SHA-256 digest. This means only that these declared bytes are retained. It
does not establish that they compiled into the artifact, cover all dependencies
or generated files, or represent a complete effective build environment.
Path labels are not extracted onto the host filesystem. Optional provenance
URLs, revisions and license labels remain declarations too.

The CLI's `publish --package` accepts an already constructed valid envelope;
it does not add or discover source files. Host applications can construct a
`package::PackageManifest` and call `package::encode_manifest` to obtain an
independently admitted envelope.

## Host library API

The unpublished `chialisp-oxide-package` crate is host tooling, separate from
the `chialisp-oxide` puzzle-authoring facade. Use a path dependency into a
reviewed checkout or a reviewed pinned Git revision. Its registry operations
are not newly admitted operations inside an on-chain Rust puzzle.

The main library calls are:

```rust
use std::path::Path;
use chialisp_oxide_package::{Limits, Result, bundle::import_bundle, store::Registry};

fn store_export() -> Result<()> {
    let limits = Limits::default();
    let package = import_bundle(
        Path::new("/path/to/artifacts/IMMUTABLE_RUN/manifest.json"),
        "my_puzzles::payment::puzzle",
        &limits,
    )?;
    let registry = Registry::create(Path::new(".local-puzzles"), limits)?;
    let publication = registry.publish(&package)?;
    let variants = registry.lookup(package.module_tree_hash())?;
    let exact = registry.get(publication.object_id)?;
    assert_eq!(exact.object_id(), package.object_id());
    assert!(variants.iter().any(|item| item.object_id() == exact.object_id()));
    Ok(())
}
```

`verify_package` admits in-memory envelope bytes;
`verify_package_file` reads a bounded regular file. `VerifiedPackage::bytes()`
returns its exact complete envelope and `summary()` describes its checked
identities and scope. `artifact::inspect_artifact` and `verify_artifact`
perform bounded serialization and identity checks without execution. A raw
`clvm_artifact` package can explicitly omit Rust IR and its unknown typed ABI;
the registry does not derive Rust types from raw CLVM bytes.

## Serialization, resources and filesystem limits

The only supported artifact encoding is `clvm-legacy-plain/v1`: one complete
canonical legacy plain CLVM tree. Truncated input, trailing bytes and
nonminimal atom encodings are rejected. Backreferences and serde-2026
serialization are explicitly unsupported; admission does not retry a
different decoder. Marker bytes inside atom payloads remain ordinary payload
bytes. No disassembly or VM execution is needed to verify an artifact.

Caller-selected `Limits` bound blob size, atom size and heap accounting, tree
nodes and depth, expanded bytes, work, JSON size/depth/nodes, IR and source
attachments, export/object counts and aggregate reads. CLI commands use the
library defaults. Package contents cannot raise these limits. Resource and
I/O errors are host tooling outcomes, not evidence of puzzle rejection or
consensus safety.

The store uses complete synced staging files and a no-replace hard-link
commit on the same local filesystem. It verifies existing objects instead of
overwriting them. Interrupted staging files are not returned by lookup. An I/O
error after commit can leave a complete object visible; retry verifies it
before reporting `already_present`. Directory synchronization requests
durability on supported hosts; other hosts promise visibility only.

Readers reject symlink components and nonregular files before opening them.
The implementation assumes trusted local directories and supported local
filesystem hard-link/locking semantics. It is not a sandbox against hostile
concurrent directory replacement and does not promise remote-filesystem or
power-loss behavior beyond those explicit assumptions.

## Ecosystem context

This limited survey consulted project-maintained repositories and documentation
on 2026-10-20. These links describe existing tooling; they are not compatibility
pins, executed integrations or an exhaustive registry survey.

- Chia Network's [chia_puzzles](https://github.com/Chia-Network/chia_puzzles#readme)
  distributes standard puzzle source, compiled hex and generated Rust/Python
  bindings. Its [binding generator](https://github.com/Chia-Network/chia_puzzles/blob/main/generate_chialisp_constants.py)
  explicitly lists puzzle names, artifact paths and tree hashes. Our design
  consequence is to import reviewed canonical bytes and their provenance,
  preserving their hashes separately from newly compiled Rust implementations.
- The Wallet SDK's [Layer trait](https://github.com/xch-dev/chia-wallet-sdk/blob/main/crates/chia-sdk-driver/src/layer.rs)
  already separates typed solution parsing/construction from puzzle-layer
  parsing/construction. Its [StandardLayer](https://github.com/xch-dev/chia-wallet-sdk/blob/main/crates/chia-sdk-driver/src/layers/standard_layer.rs)
  recognizes the standard module hash, decodes `StandardArgs`, constructs a
  curry and computes its tree hash. Its [Puzzle representation](https://github.com/xch-dev/chia-wallet-sdk/blob/main/crates/chia-sdk-driver/src/puzzle.rs)
  distinguishes curried and raw puzzles. Future registry bindings should reuse
  reviewed schemas and pin actual modules, with unknown modules left opaque;
  an arbitrary hash or erased IR type cannot select a nominal builder safely.
- Rue's [installation guide](https://rue-lang.com/docs/installation/) describes
  `rue init` and `rue build`; its [module guide](https://rue-lang.com/docs/modules/)
  describes local file discovery, imports and re-exports. Those inspected pages
  do not establish a hash-addressed dependency or package-exchange contract.
  Rust builds therefore continue through Cargo, while this separate envelope
  records artifacts and declarations. Rue registry interoperability remains
  unverified rather than inferred from source-module support.
- [Coinset CLI](https://github.com/coinset-org/cli#readme) documents full-node
  puzzle/solution retrieval and local tree hashing, curry and uncurry tools.
  [Chia Dev Tools](https://github.com/Chia-Network/chia-dev-tools#readme) also
  supplies Chialisp build, hash, curry, source retrieval and inspection commands.
  These provide precedents for later reveal inspection. A network observation,
  reveal-hash verification, standard-curry decoding and package attachment
  verification remain separate steps; this offline increment performs no chain
  query or third-party command invocation.

The inspected scope was these guides, READMEs and named source files, plus
targeted public searches for Chia puzzle registries and Chialisp/CLVM package
managers. It did not verify a common public package protocol, all independent
registry proposals, current hosting policies or a real mainnet reveal. No
external tools were executed to establish interoperability. Those questions
remain research and integration work, not evidence that other registries do
not exist.

## Work still required for issue #21

This local registry does not complete the broader package tooling goal.
Actual mainnet observation, standard-curry argument decoding, reviewed nominal
bindings and a dependency-by-hash authoring example remain unfinished. Broader ecosystem
coverage and issue acceptance also remain open. Package identity checks add
no native execution, VM execution, simulator spending, model proof or
compiler-preservation evidence by themselves.
