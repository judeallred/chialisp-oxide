---
title: Extend the compiler with evidence
description: Add a puzzle or language feature with reproducible checks and honest source provenance.
---

Read `AGENTS.md`, `goals.md`, and the compiler proposal before changing the
pipeline. Keep the real-Rust requirement and the full tutorial/CHIP migration
goal visible; a smaller supported subset is a current implementation boundary,
not a redefinition of the goals.

## Add an executable puzzle

1. Create `puzzles/your-puzzle/puzzle.rs`, `reference.clsp`, and `puzzle.json`.
   Import the real SDK, annotate the entry, and register its module in
   `puzzles/lib.rs`. Add its export/source mapping and local Cargo inputs to
   `puzzles/authoring.json`.
2. Record whether the reference is canonical upstream source or an authored
   teaching comparison. Pin copied source revisions and verify licenses.
3. Add valid boundaries, policy failures, malformed shapes, noncanonical
   integers, and adversarial combinations to the manifest.
4. Register native-oracle decoding around the same Cargo library function and
   independent Chialisp comparisons; do not copy the Rust policy into a harness.
5. Add ledger scenarios when the result is a condition-producing coin puzzle.
6. Add a meaningful mutation control: remove the check you claim matters and
   require the validation system to catch it.
7. Build the workspace, then regenerate `corpus/validation.json` and the proof
   report so their declared Cargo input hashes agree. Inspect bytes, costs,
   outcomes, hashes, and assumptions before building the site.

The lab reads the generated report, including source text. Do not hand-edit
generated metrics or claim an unrelated teaching fixture completes a CHIP.

## Add a language feature

Specify native semantics, accepted ABI encoding, rejection behavior, strictness,
and width rules first. Extend the driver only for MIR forms with implemented
lowering. Keep compiler-private rustc code in `oxide-driver` and Rue-specific
lowering in the backend. Include compile-fail diagnostics for adjacent
unsupported constructs and tests that cannot pass by mirroring the same bug.

## Before pushing

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/check-proofs.py
python3 corpus/validate_inventory.py
python3 corpus/validate_tutorial_examples.py
cd www
pnpm check
pnpm build
pnpm test:site
```

Keep generated binaries, `target`, `node_modules`, and site build output out of
git. Commit intentional reports and dependency locks. Review upstream version
changes against the complete semantic corpus, not only a successful build.
