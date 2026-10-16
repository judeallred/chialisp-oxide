# Working on chialisp-oxide

Build toward goals.md; record current capabilities and gaps without shrinking the goals.
Use docs/compiler-approach.md for architectural context and docs/verification.md for claim scope.

- The source language is real Rust checked by rustc. Never substitute textual MIR parsing or a Rust-looking DSL while claiming Rust semantics.
- Keep rustc internals in oxide-driver and Rue internals in oxide-ir/backend adapters.
- Reject unsupported constructs with useful diagnostics. Preserve strict failure behavior, integer width/overflow rules, and exact ABI validation.
- Treat all solutions as attacker-controlled CLVM trees. Test malformed shapes, padded numbers, oversized values, authorization failures, and optimization on/off.
- Differential tests must use independently compiled Chialisp and native Rust where available. Never change a canonical module hash to hide incompatibility.
- Distinguish executed tests, bounded models, model-level proofs, and compiler-preservation proofs. Never claim absence of all security breaches.
- For each new puzzle: add Rust, original/reference source and provenance, positive and adversarial vectors, compiled artifacts/costs, and mutation controls when meaningful.
- The documentation site belongs in www, uses Astro + pnpm, and deploys with GitHub Pages. Build it around verified working examples and generated validation data.
- The owner made the repository public and authorized GitHub Pages publication. Preserve that visibility and deploy www through GitHub Actions.
- Use subagents for independent compiler, corpus, verification, or site tasks; coordinate ownership before editing shared files.
- Do not commit target, node_modules, build output, secrets, or temporary artifacts. Commit intentional reproducible reports.
- Before pushing Rust changes: cargo fmt --all --check; cargo test --workspace --locked; cargo clippy --workspace --all-targets --locked -- -D warnings.
- Run python3 scripts/check-proofs.py for proof/model changes. Run pnpm site checks/build when www changes.
- Push completed, verified work to origin; inspect CI and fix failures. Record remaining substantive work for continuation.
- When finding a Rue bug, search upstream issues first, then file or update an xch-dev/rue issue with pinned revision, minimal repro, observed/expected behavior, and a patch containing a failing regression test plus fix. Link local patches to the issue. The user explicitly authorized upstream bug reports; no extra confirmation is needed.
- Regenerate corpus evidence only after formatting Rust sources: build workspace, run `target/debug/oxide validate --output corpus/validation.json`, then `python3 scripts/check-ir-proofs.py --output corpus/proofs.json`. The site rejects stale source/manifests/proof hashes.
- Keep Pages publication gated on successful compiler verification for the exact main commit, and verify the live evidence endpoint after deployment.

- Keep the landing page and README prominent about WIP prototype status and not ready for use. Do not market current tests or model checks as production readiness.
- Prefer researched Rust/Cargo authoring conventions. Do not present the standalone-file harness as the final user workflow or invent a rustc CLVM target before a real backend supports it.
- Lead new authoring examples with normal Cargo libraries, oxide-sdk, and annotated exports. Keep native `cargo check/test` and the additional CLVM subset check distinct. Preserve Cargo's features, build scripts, generated code, and compiler arguments.
- Keep compiler-private integration on a tested dated nightly with matching components; do not restore RUSTC_BOOTSTRAP as a supported stable-toolchain workaround.
- Cross-crate lowering currently admits only structurally validated oxide-sdk types/helpers. Do not broaden external calls until overflow flags, dependency MIR, generics, and trait resolution have explicit semantics and regression tests. Cargo integration tests must compare exact outputs, not acceptance alone.
- Write puzzle logic with named condition constructors, nominal domain types, explicit amount/time units, and named policy constants. Keep fixture key bytes in named supporting data. Retain ordinary Rust arithmetic and assertions; raw opcode tuples belong in SDK internals and independent wire-format tests.
- The paired corpus is the real `oxide-puzzles` Cargo library. Validate annotated Cargo exports and link that same library for native execution; do not compile copied policies or silently route corpus checks through the standalone research driver. Maintain `puzzles/authoring.json` and fail on stale declared native/compiler inputs.
- Preserve canonical tutorial source exactly when porting examples. Record finite Rust integer bounds, exact ABI differences, and module-hash incompatibilities explicitly; a wrapper that changes the original's accepted inputs is an adapter, not the unchanged canonical program.
