# Hash primitive boundary

- Read docs/hashing.md before changing hash admission or the SDK kernel. SHA is an explicit narrow bridge, not general external-library or loop support.
- An actual reserved public export DefId only selects a candidate. Require its real signature, closed typed-MIR implementation, independently reviewed target/profile pin, and installed-toolchain checks. Never authorize by a name, marker, path, or digest computed from the candidate itself.
- Bound raw type/constant/dependency graphs and literal CTFE locals/repeat sizes before normalization, resolution or evaluation. Preserve the fail-closed unsupported cases and exact per-target/profile headers.
- Keep SHA strict, once-only, non-total and in version-4 runtime lowering, including unused results. Check graph discovery, lowering and totality together. Preserve failure order against divergence and lazy branches.
- Immutable fixed-u8-array unsizing must retain actual rustc types, existing array bounds and value snapshots. It does not admit mutable references, arbitrary pointers, lengths or non-u8 slices.
- Keep kernel rotations/padding scalar unless a new closed implementation is independently reviewed. The narrow Clippy allowances explain that choice; changing calls changes the reviewed body.
- Run production driver hashing tests, IR SHA tests and SHA-model rejection tests. Re-run the actual SDK native harness after any hashed input changes; retain checked native builds, independent vectors/OpenSSL checks and compiled mutants. Research freshness records must bind current source/binary/toolchain inputs and their fixed case set.
- Native reports contain host/path-bound retained binaries; generate fresh CI reports and keep them out of Git. Linux emulation is explicit, and cross-target MIR analysis is separate from native Linux execution.
- New hash ports require exact original source/license/commit/tree/include/artifact provenance, independent raw-tree expectations, actual native Cargo functions, both Rust/Rue modes, Chialisp, retained-byte replay and appropriate consensus scenarios.
- Keep source/dependency/native input closures fresh. Host codec limits and VM resource exhaustion are inconclusive, never policy-rejection evidence. Preserve every prior observation through the closed reviewed migration gate.
- All current proof APIs reject SHA, including unused nodes. Do not add collision-free, injective or uninterpreted-hash axioms without a separate explicit model and reviewed assumptions.
- Implementation identity, finite tests and source freshness do not prove SHA security, native object authentication, compiler preservation or production readiness.
