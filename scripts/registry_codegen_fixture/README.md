This is a WIP evidence template, not a ready-to-build user example. The gate
copies its exact sources into a fresh target-owned Cargo project, records the
explicit manifest path rewrite, then runs the current offline generator against
an explicit admitted registry and dependency lock. Generated modules are never
copied from the private compiler probes.

The multiplier is independently compiled from the small Chialisp module in
`examples/compile-reference.rs`. The standard module and fixture synthetic-key
bytes are pinned by `crates/chialisp-oxide-package/tests/fixtures/mainnet-standard`;
that retained directory contains its source/license and Coinset provenance.
The observation is not chain authentication. The 255/256-byte quote programs are
independently encoded test data. Native tests, subset checking, source receipts
and actual CLVM execution are separate gates. No signatures or spends are tested.
