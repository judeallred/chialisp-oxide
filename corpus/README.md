# Source and migration inventory

The validated teaching corpus is the real `oxide-puzzles` Cargo library under
[`puzzles`](../puzzles). [`puzzles/authoring.json`](../puzzles/authoring.json)
maps each paired source to its annotated export and declares local Cargo inputs.
The validation and proof reports record their hashes. Native execution links
that library; CLVM and proof IR are extracted through Cargo from the same source.
Run `./scripts/oxide validate` and `python3 scripts/check-ir-proofs.py` after
building the workspace. These original teaching fixtures do not count as
canonical tutorial or CHIP ports.

`inventory.json` records 98 source entries as of 2026-10-16: all 28 Markdown
documentation pages in the pinned Chialisp site, all 64 numbered CHIP index
entries, five grandfathered/obsolete standards, and the auction proposal listed
under consideration. This is a source inventory, not 98 implemented puzzles.

The eight tutorial pages cover the primer and concepts. The remaining Chialisp
pages cover language/VM references, costs, conditions, errors, attacks,
optimization, debugging, and seven protocol primitives. The original password
coin's insecurity must remain explicit when the lesson is migrated.

CHIP status is copied from the versioned index, including draft, review,
stagnant, withdrawn, obsolete, and grandfathered material. A proposal's status
does not imply that it contains a puzzle. Every entry still needs reference
discovery or review: links ending in `.clsp`, `.clvm`, or `.rue` were indexed as
leads, including upstream dependencies. They have not been imported or verified
as the definitive complete implementation. Branch links remain visibly
unpinned; their licenses are unset until inspected. The proposal/doc license
does not establish a linked implementation's license.

Source snapshots:

- [Chialisp documentation](https://github.com/Chia-Network/chialisp-web/tree/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/docs)
- [CHIP index](https://github.com/Chia-Network/chips/blob/6fa7625dfcc895177dbecac20c838498e6a60ceb/README.md)
- [Consensus condition opcodes](https://github.com/Chia-Network/chia_rs/blob/56f6b676a96de8659fe05d7e4f705df553f37fa5/crates/chia-consensus/src/opcodes.rs)

An original small fixture that tests a similar idea does not count as migrating
the corresponding tutorial or protocol. To promote an entry, identify each
example or module, inspect and pin the canonical source and license, add paired
local source paths, and attach reproducible differential/simulator evidence.
Reports must distinguish tested behavior from module-hash compatibility and
list remaining assumptions. A protocol that commits to an original module hash
cannot substitute behaviorally equivalent bytecode without further work.

Run `python3 corpus/validate_inventory.py`. The validator rejects unpinned
source snapshots, duplicate entries, broken evidence links, and validation
claims without implemented local sources and evidence. It does not verify
equivalence by itself; the compiler/VM/simulator suites generate that evidence.
All migration statuses initially remain `planned` / `not_run`.
