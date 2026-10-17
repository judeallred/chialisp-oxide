# Owned trees in the Rust subset

**WIP prototype; not ready for real funds or production use.** This feature
supports a restricted owned atom/pair representation. It does not make general
Rust allocation or the standard library available on CLVM.

Borrowed `Tree<'a>` is useful when a result refers to existing input data. A
function cannot return a reference to a newly created stack-local pair: ordinary
Rust rejects that escaping reference. An owned result expresses the intended
lifetime without weakening the borrow checker.

## Author an owned result

The unpublished facade has an optional `alloc` feature, disabled by default.
The corpus enables it through its ordinary Cargo dependency:

```toml
chialisp-oxide = { path = "../crates/chialisp-oxide", features = ["alloc"] }
```

`OwnedTree<'a>` has two ordinary Rust variants:

```rust
pub enum OwnedTree<'a> {
    Atom(&'a [u8]),
    Pair(Box<OwnedTree<'a>>, Box<OwnedTree<'a>>),
}
```

Pair nodes are owned; immutable atom bytes are borrowed. There is no additional
"borrowed subtree" variant with a tag that CLVM would erase. Every value is
unambiguously an atom or a pair. Dynamic creation of new atom storage remains
separate work.

The real [prefix fixture](../puzzles/prefix-list/puzzle.rs) takes two trees by
value and returns a new list spine ending in the arbitrary second tree. Its
recursive step moves the existing first-child Box and allocates the next spine
node with `Box::new`. The normal Rust compiler checks moves, lifetimes, and
borrows; the additional project checker validates the supported CLVM subset.
The default ABI requires exactly two arguments and a literal empty atom ending
the outer argument list. The second argument may itself be any atom or tree.

The three sources, malformed-input examples, and executed artifacts appear in
the [prefix lesson](https://judeallred.github.io/chialisp-oxide/tutorials/prefix-list/).
The Chialisp adapter retains the exact pinned upstream helper body and visibly
adds strict outer argument checks. This is a helper port, not an unchanged
canonical module or completion of its surrounding include file.

## Lower logical values, preserve evaluations

The driver recognizes the actual standard `Box` language item, its default
`Global` allocator type, and the actual `Box::new` diagnostic item. It validates
the two-variant recursive shape, including the absence of a custom destructor.
Local variant order and enum spelling do not grant special behavior. A
same-named local `Box::new` remains an ordinary checked function.

The pinned compiler expands moving from a Box into internal pointer metadata,
alignment/null guards, and cleanup. A bounded plan accounts for the complete
definition/use chain before interpreting it as a logical tree snapshot. It
keeps internal pointer values separate from ordinary typed locals and retains
the original MIR types. Dereferences use the value captured at the pointer's
origin, including after a later source move or reassignment. Only the validated
standard cleanup is erased.

`Box::new` does not erase its initializer. Checked overflow, assertions, recursive
calls, and unused fallible results keep their source evaluation order. Separate
tests require a program rejection when it occurs before an infinite call and
resource exhaustion when that call comes first. Nontermination controls run in
CLVM, never in the native oracle.

The representation lowers to the existing logical tree IR; it introduces no
serialized pointer address, allocator, or target-dependent layout. This is not
a proof that arbitrary MIR pointer operations are safe to erase. User pointers,
pointer identity, mutation, nested Box containers, custom allocators/destructors,
`leak`, and `forget` remain unsupported. Owned exports also reject explicit
global allocators visible in the crate graph. Downstream host configuration and
allocation observations are not a resource-equivalence guarantee.

## Keep host resources outside policy results

The native oracle checks the complete assembled environment before constructing
any owned values: depth at most 128, at most 16,384 expanded tree nodes, and at
most 1,048,576 total atom bytes. Both children matter, including ignored heads
and arbitrary final tails, because dropping owned nodes traverses them too.
It checks the prefix result's possible depth before calling the policy.

These are testing limits. They add no guard to the compiled puzzle. Exceeding a
host bound is an error and makes the comparison inconclusive; it cannot count
as a policy rejection. Decoding and output encoding stay outside the policy
panic adapter. Encoding consumes all owned result nodes iteratively, including
after an error, to avoid recursive destruction of an oversized result. Process
memory exhaustion is not claimed impossible.

The implementation and targeted regressions are in
[native_owned_tree.rs](../crates/chialisp-oxide-corpus/src/native_owned_tree.rs)
and [native_owned_tests.rs](../crates/chialisp-oxide-corpus/src/native_owned_tests.rs).
The compiler's real Cargo tests separately cover SDK metadata, local shapes,
ownership diagnostics, snapshots, unsupported operations, and strict evaluation.

## Separate algorithms from compiler proofs

The [Lean experiment](../experiments/tree-theorems) proves properties of manually
authored prefix and last-element algorithms for all finite trees. An independent
type oracle prevents replacing a claimed theorem with a weaker statement; axiom
audits reject unreviewed assumptions. A finite bridge checks model equalities
against named executed examples in all five compiler modes.

Those algorithm theorems and finite executions do not prove Rust-to-MIR,
MIR-to-IR, or IR-to-CLVM preservation, resource equivalence, or production safety.
The existing source-derived SMT models still reject recursive runtime IR.
