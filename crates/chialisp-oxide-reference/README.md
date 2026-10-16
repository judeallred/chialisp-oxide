# Independent Chialisp reference compiler

`compile(source: &str) -> anyhow::Result<Vec<u8>>` calls Chia Network's Rust
Chialisp compiler and returns serialized CLVM. It needs no Python installation,
subprocess compiler, or handwritten lowering that could share bugs with our
Rust backend. Fixture source must be trusted: upstream file includes are not an
isolation boundary.

The compiler is pinned to `chialisp 0.4.6`
([upstream revision](https://github.com/Chia-Network/chialisp/tree/32254c8402a3b277b43b1bdad743781f15433b80)),
the successor to `clvm_tools_rs`. Its internal `clvmr 0.16.2` allocator never
crosses the crate boundary. Tests deserialize the result and execute it with the
project's `clvmr 0.21.0` and `MEMPOOL_MODE`, so a reference comparison does not
accidentally execute each compiler's output on a different VM.

Traditional `mod` syntax and the modern `*standard-cl-21*` include are both
covered. `cargo test -p chialisp-oxide-reference` also checks deterministic output,
arithmetic, guarded conditions, rejection, malformed source/input, and lazy
branches. A reference compilation passing these tests does not establish that
the puzzle is safe or equivalent to a Rust puzzle; that requires shared-input
differential tests and coin-spend validation.
