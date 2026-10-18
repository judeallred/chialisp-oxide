# Rue codegen native stack robustness

Upstream report: https://github.com/xch-dev/rue/issues/114

Pinned upstream: `245c107a8e795229ea08097142f005a4c0d91075` (`rue-lir 0.11.0`). The original codegen source in signed downstream commit `405792113242aed8d94d3dc4805ed68fc667b3d1` is byte-identical to that upstream file (SHA256 `00801d11a02ae27fea5e4aeb50dc63f6826c90168c74cdc2961b5081435c53d0`). Existing six downstream patches are not part of this patch.

An 80-level chain of `Lir::Not` over `Path(1)` aborts the original debug codegen on an explicitly 2 MiB Rust thread. Both untouched upstream and the downstream tree reproduced the failure. The regression runs the small-stack compiler in a child process so an abort does not terminate unrelated tests. Expected serialization is independently assembled (321 bytes).

The fix uses heap worklists for program emission and static-data traversal. It emits repeated DAG references independently, preserves CLVM child/allocation order, and reports cyclic arenas as a controlled codegen error. Closure emission is staged so `q1` allocation and each capture wrapper occur before the next capture. Tight-heap controls pass on old recursive code, fail on a retained naive postorder implementation, and pass on the staged fix; they check pair counts 1/16 and heap bytes 0/64 before the intentional OOM.

Executed validation: downstream 66 tests, clean pinned upstream plus only this patch 49 tests, downstream all-target Clippy with `-D warnings`, and clean upstream patch apply/content/mode checks all pass. The new suite has six tests, including 4096-deep program/static-data emission on the same 2 MiB thread. Both Cargo.lock files remain unchanged.

Scope: this establishes the reported native debug-build compiler robustness fix on aarch64 macOS with dated rustc nightly-2026-07-14. It does not establish a universal heap resource bound, release-profile failure, cross-platform failure, or any new CLVM runtime semantics/proof.

Full upstream-applicable patch: `rue-codegen-stack.patch` (SHA256 `b769f2a7128891448e2a7a78b7ecf8b633824ef9b40c32d5b64ee262ed076745`). The issue includes its complete regression and fix. External research provenance and test-first/final logs are preserved under `/private/tmp/chialisp-oxide-rue-codegen-*`; these host paths are not portable proof inputs.
