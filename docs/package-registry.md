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

## Pin an exact local dependency

After reviewing lookup's variants, select a package and give it a local name:

```sh
target/debug/cargo-chialisp-oxide registry add --name payment --module-hash MODULE_TREE_HASH --object-id OBJECT_ID --registry .local-puzzles --lock chialisp-oxide.lock --json
```

The installed form is `cargo chialisp-oxide registry add` with the same flags.
`--object-id` may be omitted only when the complete admitted store has exactly
one package for that tree hash. Multiple envelopes remain ambiguous even when
their CLVM bytes match. Use `lookup`, review the declarations and select an
exact object; the tool never chooses the first variant.

`add` writes use local POSIX semantics, reviewed on macOS/Linux;
non-Unix platforms fail explicitly before mutation. Reading a portable lock and
resolving its packages remain independent of this writer restriction.
`--lock` is explicit and its parent must already exist. The tool does not
discover Cargo workspaces or change `Cargo.toml` or `Cargo.lock`. Those output
basenames are reserved, including Unicode lowercase equivalents. A lock cannot be placed
inside its selected registry. Commit a project-local `chialisp-oxide.lock`
alongside the consumer code; choose a registry separately when resolving it.

The closed `chialisp-oxide-dependencies/v1` protocol records sorted unique
names, exact envelope IDs, artifact encoding, byte length, serialized-blob
digest and decoded tree hash. Each pin has an explicit `binding: null`:
neither its name nor the stored structural ABI grants a typed callable API.
Names use `[a-z][a-z0-9_]{0,63}` and do not become filesystem paths or Rust
module names. A lock has at most 128 dependencies and 1 MiB of JSON, with
fixed depth and node bounds. It contains no absolute registry path, executable
bytes or source tree, so copying a lock also requires copying its exact packages.

An add checks every registry object once within the shared scan budget and
verifies all existing pins before writing. An identical pin for an existing
name returns `already_present` and preserves the original lock bytes, including
whitespace. A different pin under that name is a conflict; replacement is not
implicit. Missing, changed or corrupt dependencies prevent the update.

Writers use a nonblocking sibling advisory lock, compare the original exact
file snapshot, then write, sync and recheck a private staging file before
committing the complete replacement. Contention or a changed snapshot returns
a conflict. Precommit failures preserve the old destination; an I/O failure
after commit can leave the complete new lock visible. Retry rechecks that state;
if an orphaned hard-link stage could not be removed, its extra link must be
cleaned up before another update. The persistent, empty sibling writer-lock
file is coordination state. This protocol
coordinates cooperating writers on supported local filesystems; it is not a
hostile-directory sandbox or a power-loss guarantee.

## Resolve a lock without building or writing

Check a copied lock and its selected local store independently of Rust source
generation or execution:

```sh
target/debug/cargo-chialisp-oxide registry resolve --lock chialisp-oxide.lock --registry .local-puzzles --json
target/debug/cargo-chialisp-oxide registry resolve --lock chialisp-oxide.lock --registry .local-puzzles --name payment --json
```

The installed form is `cargo chialisp-oxide registry resolve` with the same
flags. Both forms admit every locked pin and every object in the bounded store
scan. `--name` filters only the displayed dependencies after this complete
check; an unrelated corrupt object or missing unselected pin still fails the
command. An unknown selected name fails after admission too.

Successful JSON contains `status: "resolved"`, `lock_sha256`, `dependencies`
and `scope`. The digest covers the exact checked lock bytes, including
whitespace. Each returned dependency has its name, exact `object_id`, complete
`artifact` identity and explicit `binding: null`. Without `--name`, the array
contains all pins in name order; with it, the array contains one pin. Module
bytes and source attachments are not included in this result.

Resolution creates no registry, lock, sidecar or generated source and runs no
compiler or VM. Its artifact admission limits are separate from the narrower
static Rust generator's limits: a valid stored program can resolve even when
it cannot be embedded by `generate-rust`. This is a checked local read, not a
reservation against later filesystem changes or an authenticity claim.

## Consume the actual locked artifact from Rust

The package library resolves every pin against a selected local store before
returning executable bytes:

```rust
use std::path::Path;
use chialisp_oxide_package::{
    Error, ErrorKind, Limits, Result, dependency::read_lock, store::Registry,
};

fn payment_bytes() -> Result<Vec<u8>> {
    let registry = Registry::open(Path::new(".local-puzzles"), Limits::default())?;
    let lock = read_lock(Path::new("chialisp-oxide.lock"))?;
    let dependencies = lock.resolve(&registry)?;
    let payment = dependencies.get("payment").ok_or_else(|| {
        Error::new(ErrorKind::UnknownObject, "payment dependency is absent")
    })?;
    Ok(payment.artifact().bytes().to_vec())
}
```

The checkout includes a runnable host example which executes those retrieved
bytes with an explicit serialized environment. There is no embedded puzzle or
fallback when retrieval fails. For a lock whose `payment` pin is the corpus's
`chialisp_oxide_puzzles::capped_payment::puzzle`, `ff8201f480` encodes its `(500)`
solution:

```sh
cargo run --locked -p chialisp-oxide-package --example locked-artifact -- chialisp-oxide.lock .local-puzzles payment ff8201f480
```

This example bounds the solution to 64 KiB, its depth to 256, atom heap to
8 MiB and VM work to 10,000,000 cost units. Its successful JSON includes the
actual object/artifact identities, result bytes and cost. VM errors carry the
pinned typed failure code; resource exhaustion is inconclusive. Running a
module in a host VM does not validate a coin spend or supply a nominal solution
schema. This is an opaque host dependency. The generated-source workflow below
adds an on-chain call through ordinary local Rust; reviewed nominal argument
and solution bindings remain work for issue #21.

To reproduce the actual host workflow from this checkout, build the tools and
extract the corpus in both modes into fresh output directories:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver -p chialisp-oxide-package --bins --examples
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir target/registry-source-on
target/debug/cargo-chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir target/registry-source-off
python3 scripts/check-registry-dependencies.py --cli target/debug/cargo-chialisp-oxide --consumer target/debug/examples/locked-artifact --bundle-on target/registry-source-on --bundle-off target/registry-source-off --output target/registry-replay
```

The replay selects only the capped-payment export after complete bundle
admission, publishes its two compiler variants and pins them in a local lock.
It checks the actual Rust consumer's expected output and typed failures,
byte-preserving idempotence, tampering, missing objects and copied-store
resolution. Child tool commands run with an empty `PATH` and invalid
Cargo/compiler overrides. The output directory must be new and retains the
command results and report. This is an executable seed recipe for those two
packages; it does not compile or seed the entire published-source catalog.

## Generate local Rust that calls a locked program

After pinning `payment`, generate a local module from its exact retrieved
artifact. Run these commands from the consumer project with the unpublished
Cargo subcommand installed from a reviewed checkout:

```sh
cargo chialisp-oxide registry generate-rust --lock chialisp-oxide.lock --registry .local-puzzles --name payment --output src/payment_module.rs --json
```

The command checks every locked dependency and every object in the selected
bounded registry scan before selecting `payment`. It generates from the
admitted program bytes, including unknown and already-curried programs. A
catalog name, stored ABI description or source attachment does not grant
compiler behavior. Generation runs no compiler or VM.

Include the result as an ordinary local Rust module. The author decides which
functions to export and how to frame the called program's environment:

```rust
use chialisp_oxide::Tree;

mod payment_module;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn foreign_payment<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    payment_module::run(environment)
}
```

This raw export forwards the complete caller tree to the selected program.
The generated helper constructs the exact program and calls the existing
reviewed `apply` bridge. The Rust compiler checks its local functions and
borrows; the CLVM subset checker lowers their actual bodies. Generated names
and provenance comments receive no special trust.

The helper accepts an opaque `Tree`, so the author must understand the selected
program's input contract. The capped-payment corpus export, for example,
expects its existing one-argument proper-list environment. A standard module
instead needs its declared key and solution fields framed explicitly. The
generator adds no argument decoder, key derivation, curry operation or nominal
Rust parameter types. Malformed trees and improper tails reach the selected
program unchanged. The enclosing Rust export has its own compiled artifact and
puzzle hash; it is not interchangeable with the selected module or a previously
funded canonical instance.

Keep the generated source and exact dependency lock in version control. Check
that they still agree before ordinary Rust tests and CLVM builds:

```sh
cargo chialisp-oxide registry generate-rust --lock chialisp-oxide.lock --registry .local-puzzles --name payment --output src/payment_module.rs --check --json
cargo check --locked
cargo test --locked
cargo chialisp-oxide check --locked
cargo chialisp-oxide build --locked --output-dir target/clvm-on
cargo chialisp-oxide build --locked --no-optimize --output-dir target/clvm-off
```

Generated items carry formatter-only `#[rustfmt::skip]` attributes, so ordinary
`cargo fmt` preserves their exact regeneration bytes. Edit the author wrapper
normally; an edited generated body must fail the regeneration check.

Use the facade's `vm` feature only in the native test/dev dependency when
testing an application; ordinary extraction uses its reviewed VM-less body.
For a local consumer, replace the checkout path in both dependency declarations:

```toml
[dependencies]
chialisp-oxide = { path = "../chialisp-oxide/crates/chialisp-oxide" }

[dev-dependencies]
chialisp-oxide = { path = "../chialisp-oxide/crates/chialisp-oxide", features = ["vm"] }
```

The [application guide](https://github.com/judeallred/chialisp-oxide/blob/main/docs/dynamic-apply.md) explains the reviewed native/compiler
boundary. A successful generation or native build does not
establish that either CLVM compilation mode succeeds. Test the actual emitted
artifacts against the original program with valid and malformed environments.

Output is new-or-identical: a different existing file returns a conflict rather
than overwriting edits. `--check` only regenerates and compares. For an
intentional dependency update, generate to a new explicit filename and review
the source and exact pin changes. The POSIX writer stages, syncs and rechecks
the complete bounded source before atomic no-replace publication. It rejects
Cargo files, the supplied lock, registry descendants and nonregular or aliased
outputs. A postcommit I/O error can leave the complete new source visible;
this is not a hostile-directory or power-loss guarantee.

The initial source profile admits at most 128 pairs, 128 distinct atoms,
256 bytes per atom, 8 KiB of distinct atom payloads, tree depth 64 and five
live pair references. It emits eight pairs per helper, at most 16 helpers and
256 KiB of Rust source within a bounded work budget. These are generation
limits, not consensus rules or a promise that every generated program fits an
author's complete compiler call graph. A larger atom is explicitly unsupported
by this route; splitting it would change the program. Nil, padded integers,
non-UTF-8 atoms and improper tails retain their exact bytes and shape.

To execute the checkout's end-to-end generated-source gate, use a new output
directory after building the host tools:

```sh
cargo build --workspace --bins --examples --locked
python3 scripts/check-registry-codegen.py --execute --output target/registry-codegen
```

The gate generates from four actual pinned packages and materializes an ordinary
Cargo consumer with five annotated exports. It checks formatting, native tests,
an independently compiled Chialisp multiplier, a retained standard module,
255/256-byte atoms and a strict discarded application. Both Rust compilation
modes retain complete source receipts and run against canonical outputs or
typed failures. The report separates native tests, an ordinary Rust-valid
source mutation, package admissions and VM observations. Failed or inconclusive
runs retain their commands and partial reports; none adds signature, ledger,
chain-authentication or compiler-preservation evidence.

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
The [reveal inspection and standard binding guide](https://judeallred.github.io/chialisp-oxide/reference/reveal-inspection/)
covers the retained mainnet observation, local standard-curry decoding and the
first reviewed host binding. Broader nominal bindings and an on-chain
dependency-by-hash authoring example remain unfinished.
Local exact pins and host VM consumers do not complete those requirements. Broader ecosystem
coverage and issue acceptance also remain open. Package identity checks add
no native execution, VM execution, simulator spending, model proof or
compiler-preservation evidence by themselves.
