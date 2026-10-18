# Rue LIR source import

Source: https://github.com/xch-dev/rue/tree/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir
Revision: 245c107a8e795229ea08097142f005a4c0d91075. License: Apache-2.0 (included).
The upstream repository had no NOTICE file at this revision.

Local changes:
- Cargo manifest expands workspace-inherited dependencies; rust lints remain
  governed by local CI. Source starts as an exact import of upstream src/.
- src/optimize/ops.rs: fix the empty identity-product case in opt_mul, which
  indexed result[0] when result was empty. Reproduced by optimizing 1 * 1.
  An empty residual product is the multiplicative identity, so return atom 1.
  Regression lives in crates/oxide-ir/tests/semantics.rs.

This patch applies to both our direct backend and Rue's source compiler through
Cargo's source patch. Reconcile with upstream before upgrading Rue. Never omit
this provenance or the license when redistributing the imported source.

Upstream report and regression/fix patch: https://github.com/xch-dev/rue/issues/108
Local patch file: ../../patches/rue-lir-identity-product.patch

Second local fix: src/optimize/truthy.rs preserves strict evaluation of Cons
fields during truthiness analysis. Otherwise an inlined typed Rue list with a
failing element could become an accepted constant branch after optimization.
The patch includes execution-based regression coverage for both Cons fields and
five truthiness contexts, with optimization on/off.
Upstream issue: https://github.com/xch-dev/rue/issues/109
Patch: ../../patches/rue-truthiness-preserves-failure.patch

Third local fix: src/optimize/ops.rs retains matching first/rest child paths
inside Cons. Returning their parent directly dropped required pair checks and
accepted atom inputs that unoptimized CLVM rejected. Execution-based controls
cover root/nested paths, explicit projections/direct paths, padded atoms, and
proper/improper pairs, with optimization on/off.
Upstream issue: https://github.com/xch-dev/rue/issues/110
Patch: ../../patches/rue-reconstruction-preserves-failure.patch
Scope and reproduction: ../../patches/rue-reconstruction.md

Fourth local fix: src/optimize/ops.rs retains the explicit nil environment
when executing a quoted program. Removing it let the quoted body read outer
data instead, changing returned values and sometimes dropping projection
failure. Execution-based controls cover nil-bound identity/projections,
same-environment execution, and constant programs with optimization off/on.
The valid Rue lambda witness is recorded in the reproduction note.
Upstream issue: https://github.com/xch-dev/rue/issues/111
Patch: ../../patches/rue-quoted-run-nil.patch
Scope and reproduction: ../../patches/rue-quoted-run-nil.md

Fifth local fix: src/optimize/ops.rs retains nested Logand/Logior reductions.
Flattening them deferred an inner integer check until every outer operand had
evaluated. For a malformed external pair passed to a Rue Int entrypoint, that
could change immediate InvalidOpArg rejection into CostExceeded from a recursive
sibling. The conservative fix removes only those two flattening arms and keeps
literal folding. Tests preserve rightmost-first evaluation, exact finite numeric
results, malformed pair rejection, and well-typed recursive controls. Adapter
regressions also exercise explicit source type guards. This does not establish
a mismatch on well-typed inputs or a vulnerability in the Rust typed ABI.
Upstream issue: https://github.com/xch-dev/rue/issues/112
Patch: ../../patches/rue-nested-bitwise-failure-order.patch
Scope and reproduction: ../../patches/rue-nested-bitwise-failure-order.md

Sixth local fix: src/optimize/ops.rs removes double Not only when the underlying
producer returns canonical Boolean bytes (nil or atom 1). Unknown paths and
integer masks retain normalization. Without this check, valid Rue `value != 0`
could return raw atom 2, and comparing it with true could change the answer.
A separate truthiness-only rewrite in src/optimize/truthy.rs removes double Not
when exact bytes are unobserved, while evaluating the original value once.
Execution tests cover returned values, comparisons, tuple fields, Boolean
operations, condition-only artifact equality, and required producer failures.
This is not a general proof of arbitrary-LIR or other truthiness rewrites.
Upstream issue: https://github.com/xch-dev/rue/issues/113
Patch: ../../patches/rue-canonical-boolean-values.patch
Scope and reproduction: ../../patches/rue-canonical-boolean-values.md

Seventh local fix: src/codegen.rs emits programs and static pair values through
explicit worklists. The original debug emitter overflows a 2 MiB native thread
stack for 80 nested Not nodes (depth 81, only 321 output bytes). The fix preserves
operand order, repeated DAG emission and Closure allocation interleaving; cycles
return a controlled codegen error. It does not impose a general heap or expansion
bound. Our IR lowering retains its separate expansion limits. Child-process
regressions cover default-sized stacks, deep programs/static data, shared nodes,
cycles and tight-allocator Closure order. This changes compiler robustness,
not CLVM execution cost by itself.
Upstream issue: https://github.com/xch-dev/rue/issues/114
Patch: ../../patches/rue-codegen-stack.patch
Scope and reproduction: ../../patches/rue-codegen-stack.md
