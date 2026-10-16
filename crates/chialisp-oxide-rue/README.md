# Independent Rue source compiler

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

`compile_file(path: &Path, optimize: bool) -> anyhow::Result<Vec<u8>>` compiles
an actual Rue source file and returns serialized CLVM. It runs the pinned Rue
frontend, its own type checking/lowering, the optional LIR optimizer, and CLVM
code generation. It neither translates Rust nor wraps emitted bytecode.

The pin is
[`245c107a8e795229ea08097142f005a4c0d91075`](https://github.com/xch-dev/rue/tree/245c107a8e795229ea08097142f005a4c0d91075),
with the workspace's documented identity-product and strict-truthiness patches.
Both optimizer modes keep `auto_inline = true`, `optimize_static_pairs = false`,
and `debug_symbols = false`. The boolean changes the real `optimize_lir` option;
it is not a label applied to one compiled program. Compiler errors are checked
after file compilation and again after entrypoint lowering/code generation.
See the pinned [compiler options](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-options/src/compiler_options.rs).

The standard library comes from that same compiler revision. Each of the twelve
current `puzzles/*/puzzle.rue` comparisons is self-contained and uses no local
import files. They are authored Rue translations, not claimed as canonical
upstream Rue modules. Condition results use Rue's [named standard structs](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-compiler/src/std/conditions/types.rue).
The canonical Intro references remain the exact original Chialisp expressions;
their Rue translations deliberately ignore the entire environment.

The original teaching fixtures receive the whole untrusted environment as
`Any` and check exact list shape, arity, booleans, and byte lengths. Rue `Int`
is unbounded: amount/counter sources explicitly check nonnegative `u64` bounds
and minimal signed-byte encodings to match the chosen Rust ABI. These are
source-level compatibility checks, not a claim that a plain Rue type annotation
automatically enforces that ABI. Public fixture keys have known secret seeds.

`cargo test -p chialisp-oxide-rue --locked` compiles every current Rue comparison
in both modes and runs all recorded named cases through the project's
`clvmr 0.21.0` executor with `MEMPOOL_MODE`, comparing exact output bytes or
rejection to the native record. A separate regression verifies that the actual
LIR toggle changes a nonconstant policy's program without changing its tested
behavior. Full corpus generation additionally performs the generated sweeps and
complete-spend comparisons. Finite evidence is not a compiler-correctness proof
or a production security audit.

Only compile trusted local source. Rue imports can read files; this in-process
adapter is not a sandbox. Declared fixture/dependency hashes and the compiler
pin describe recorded inputs, not a complete automatically discovered dependency
fingerprint.
