# chialisp-oxide

Rust → CLVM compiler tooling built on Rue, with executable tutorials,
differential puzzle tests, and an evidence-based study of prevented errors.

The first backend experiment is runnable. It compares Rue source compilation
with direct Rue LIR integration and checks both against native Rust policy logic
and a handwritten CLVM reference. Rust-source compilation is the next milestone.

```sh
cargo test --workspace --locked
cargo run --locked -p oxide-backend-experiment > /tmp/oxide-backend-report.json
python3 scripts/check-proofs.py  # requires Z3 on PATH
```

- [Backend experiment and measured results](experiments/backend/README.md)
- [Verification strategy and formal-methods scope](docs/verification.md)
- [Project goals](goals.md)
- [Compiler approach and implementation plan](docs/compiler-approach.md)
- [Chialisp reference and tutorials](https://chialisp.com/)

Tests include generated solution trees, malformed inputs, integer boundaries,
optimization comparisons, and an executable mutation control. Z3 checks a small
integer-policy model; this is not an end-to-end security proof. The sample puzzle
has no authorization and is an experiment, not a production contract.
