# Current evidence and historical checkpoints

- Validate the complete active corpus, native source identities, typed outcomes,
  protocol records, raw IR and requested fresh Cargo bundles before projecting
  any historical report. Find puzzles by unique slug, never by last-row position.
- Historical projectors are pure data transformations. Never submit their output
  to a current-source validator or use it as fresh execution evidence.
- Keep signed and unsigned checkpoints explicit. The source-cost M1 checkpoint
  is reviewed but unsigned; E20 is a signed anchor. The old pending guarded
  profile remains pending. Do not manufacture a commit or rewrite its status.
- The active root is the generic corpus append (`scripts/corpus_append.py`,
  profile `corpus/corpus-append.json`): the retained delegated-conditions
  checkpoint has 23 puzzles, the composition checkpoint 22, and the current
  corpus lists every added slug after the preserved order. Keep the whole
  recorded checkpoints immutable and bind each transition with its own reviewed
  profile. A new export extends `added` and is re-recorded with
  `check-corpus-append.py --record` after the final host build; it does not add
  a migration module, CLI or checkpoint copy. `current22_migration` is a
  compatibility name, not permission to validate a restored report as current.
- A manifest case may declare a divergence only for Chialisp and Rue, with an
  independently specified outcome that differs from native Rust; Rust CLVM is
  never declared. Readers check declared backends against the declaration and
  everything else against native Rust.
- The reserved `apply` export is admitted like the SHA kernel: exact signature
  plus a per-host fingerprint of its `vm`-less panic body. Never widen the
  canonical encoder beyond the reviewed leaves (core u8 slice length, core
  `panic(&str)`), string-slice constants and the structurally checked borrowed
  tree without re-pinning both hosts; the corpus oracle and puzzles
  dev-dependency enable `vm`, extraction never does.
- `scripts/capture-current-corpus.sh` performs the complete capture in the only
  consistent order (LAST host build, validation twice, bundles, sidecar,
  `--record` and pin, delegated packet, derived reports, then a host rebuild
  and the fresh-bundle gates before any builder relinks the driver). Use it
  instead of ad hoc sequences; it still requires the pinned Lean binary.
- Recapture `corpus/corpus-append-existing-p2-controls.json` with
  `scripts/capture-p2-controls.py` whenever any workspace source changes; its
  window hashes the complete reviewed workspace.
- Run current checks with the actual current compiler and driver identities,
  complete bundles in both modes, and the exact requested Cargo options.
  Repeated corpus reports must be byte-identical. Hash consistency does not
  authenticate binaries or prove compiler preservation.
- Keep the delegated-conditions source-control lane distinct: one native test
  group contains three paired observations; its 12 VM runs add no SDK spends.
  The 200 native raw cases, 1,000 cross-language VM runs and 60 dedicated SDK
  attempts are separately scoped evidence, not interchangeable coverage counts.
- Finish native policy checks, then the LAST host build, before fresh extraction
  and VM capture. Keep source and tool files quiet throughout each capture.
  Preserve failed runs and use new output directories for corrected attempts.
- Freeze `scripts/vm_failures.py` before capturing public source controls: the
  packet hashes that source. Its proof closure may read packet file identities
  but must not embed the packet hash into its own source or call a producer.
- Follow scripts/check-corpus-append.py, scripts/check-p2dc-append.py,
  scripts/check-runtime-composition.py and scripts/check-p2dc-source-controls.py.
  Recorded readers, constructed corruption controls and actual execution are
  separate claims. Preserve the WIP and not-ready-for-use notices.
- Retained P2, P2dc, boomerang and acyclic-scalar source-control packets deliberately
  request `--artifact-only` and keep their schema-1 inventories and stronger
  reviewed source windows. Their readers accept only exact historical or exact
  explicit artifact-only extraction commands, consistently across both modes.
  Do not credit those packets with automatic source capture. Fresh ordinary
  corpus/proof bundles use the complete schema-2 reader; narrow automatic
  attachments do not replace their independently declared source checks.
