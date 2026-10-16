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
