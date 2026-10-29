# Local registry Rust source-control fixtures

**Testing policies; not ready for use with real funds.** This unpublished package
contains four variants of this repository's own original fixed alpha/beta
teaching policies. It has no wallet, network client or external-system code.
Native checks have run; the package's paired frontend, template VM and SDK
source-control captures are separate evidence lanes.

The original production puzzle functions and shared `SetOutput` are unchanged.
Each variant retains the five-argument input ABI, initialization/ephemeral/old-value
predicates, peer preservation, state transition and signature intent. It removes
exactly one returned condition and uses a module-local two-condition return type:

- `alpha_drop_send` / `beta_drop_send`: `(CreateCoinWithHint, AggSigMe)`.
- `alpha_drop_create` / `beta_drop_create`: `(SendPuzzleMessage, AggSigMe)`.

Only the identified returned constructor, return alias and imports required by
those changes differ. The source headers identify these as testing variants.
`fixtures/source-recipes.json` records the original source hashes, exact edits and
formatted variant identities. The pure `verify_sources.py` checker applies every
exact one-match edit and notice prefix to the original, then requires byte equality
with each formatted variant. It verifies the unchanged recipe copy too. A textual
recipe is evidence of source derivation; behavioral observations have their own
native, VM and SDK records.

`src/registry_recipe.rs` is an exact copy of the current original recipe. A native
test compares those source bytes. There is no shared-alias change that could alter
the other original action.

The package has its own `[workspace]`, no default features, and explicit
`native-vm` support for ordinary native tests. Extraction must keep the empty
profile; the actual puzzle-library baseline is a **dev-dependency only**.
`root-lock.seed` retains the current reviewed root lock. The wallet integration
adds `bigdecimal 0.4.11` and `chia-sdk-driver 0.36.0` to the root dependency graph;
neither is added to this package. Its 127 resolved external tuples remain
unchanged and a subset of the updated seed. Updating this declaration requires
fresh native, LAST-build and control capture records. The package's own
`Cargo.lock` was reconciled by explicit offline workspace resolution: all 127
external `(name, version, source, checksum)` tuples matched the root lock, without
upgrades or new external packages. Its workspace membership differs from the
root seed, so the complete lockfiles are not asserted byte-identical. Future
resolution must preserve reviewed external pins and record its actual command.

The native unit harness calls all four actual variant exports and calls the real
`chialisp-oxide-puzzles` alpha/beta exports for four baseline observations. It
constructs each package's distinct nominal entry type from the same fixture
bytes and compares complete independently encoded output bytes against
constructed expectations from independent baseline vectors. It does not call the
main native adapter or assume each result has three conditions. Those eight calls
are finite typed native observations, not CLVM input decoding or SDK submissions.

The dated-nightly native a02 run passed formatting, Cargo check, both tests and
Clippy with the locked offline dependency set. The tests exercised four baseline
calls and four variant calls, plus the exact recipe-copy check. Its before/after
437-source-member and four-tool window was quiet; the local receipt and archived
executables remain under `target/action14-registry-controls-native-a02`. These
host-bound records are not authenticated execution or compiler-preservation
proofs. The earlier a01 formatting stop remains preserved separately.

`fixtures/native-vectors.json` contains exact environment assembly/bytes for later
VM replay and typed fields for native tests. Original output expectations come
from independent existing vectors; variant expectations delete the specific
returned SEND or CREATE condition. The fixture's construction label describes
its provenance; actual observations and command outcomes belong to capture
receipts. The public fixture key is not a custody key.

`scaffold-inventory.json` is a current source inventory, not execution evidence.
It lists the package's admitted source/data members except itself after the
reviewed source edits; the original scaffold snapshot is preserved in target.
Changing that inventory does not refresh or authenticate a native/VM/SDK receipt.
A source change requires the relevant actual gates and a fresh capture window.

Complete schema-2 control extraction, Program validation/recompilation,
retained-byte template comparison, own-root SDK funding journals, byte-identical
repeat and the portable source-control reader remain pending until their actual
captures pass. Root's complete 40-export base evidence remains separate and gains
no mutant credit from these fixtures. Research Chialisp controls add no Rust
mutant credit. None of these records establishes live funding, a BLS/security
proof, compiler preservation, issue14 completion, or issue15 implementation.
