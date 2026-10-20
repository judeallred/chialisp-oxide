# Currying

**WIP prototype. A curried instance hash commits a coin to a program; it does
not make that program safe, and nothing here proves compiler preservation.**

Chia's standard curried form of a module `M` with a parameter `P` is

```text
(a (q . M) (c (q . P) 1))
```

Applying the quoted module to an environment that prepends the quoted
parameter to the caller's solution. The [Currying](https://chialisp.com/chialisp-currying/)
tutorial curries `(mod (first second) (* first second))` with `2`, producing
`(a (q 18 2 5) (c (q . 2) 1))`, a doubler.

## Two routes in the SDK

`chialisp_oxide::curry` provides both halves of currying as ordinary Rust:

- **Hash arithmetic.** `curried_puzzle_hash(module_hash, parameter_hash)` and
  the two- and three-parameter variants port the pinned upstream
  `curry-and-treehash.clib` relation over the reviewed SHA-256 bridge, using
  only fixed 32-byte arrays. Coins commit to puzzle hashes, so a parent that
  creates a child with a curried inner puzzle needs this without materializing
  the program. The bodies are plain functions the compiler lowers through its
  existing bridge; names grant nothing.
- **Owned construction.** With the `alloc` feature, `curry(module, parameter)`
  materializes the standard form as an `OwnedTree`, and
  `curried_environment` prepends one quoted parameter to an environment tree.
  Nesting the latter builds multi-parameter environments.

Native unit tests check all three hash arities against the independent
`clvm-utils` oracle and the owned form against a hand-spelled serialization of
the tutorial's curried result.

## The compiled witness

`puzzles/curry-hash` is an original teaching fixture: a strict two-argument
export over `[u8; 32]` tree hashes returning the one-parameter curried puzzle
hash. Its Chialisp comparison embeds the pinned `curry-and-treehash.clib`
include behind an explicit two-argument adapter, so the independent compiler
evaluates upstream's own `puzzle-hash-of-curried-function`; the Rue
comparison spells the hash relation directly. Fourteen named cases agree
across native Rust and the five CLVM builds, including the tutorial module
curried with `2`, boundary digests, swapped arguments, and every malformed
argument shape. The expected digest of the tutorial case is also reproduced by
an independent Python SHA-256 spelling outside the project.

## The canonical module and its instances

`puzzles/currying-multiply` ports the Currying page's own program,
`(mod (first second) (* first second))`, as a strict signed 64-bit adapter
with every original/adapter difference declared per case, exactly like the
Using Modules ports. Its manifest additionally declares `curried_examples`:
for the page arguments `2` and `5`, the corpus builds the standard form of
every build's artifact independently with clvmr, requires the SDK's owned
`curry` constructor to produce the same bytes and `curried_puzzle_hash` to
produce the instance's tree hash, and applies each instance to page and
adversarial solutions in all five builds. Those cases must equal native Rust
on the uncurried environment `(A . solution)`; no divergence may be declared.
A page-spelled curried result binds only the Chialisp build, whose module is
the independently compiled original. The Rust instances are different
programs with different hashes; the Rue translation happens to compile to the
same seven bytes here, but no identity is claimed for it. `corpus/validate_tutorial_examples.py` and
the site evidence gate check this contract.

## What remains

The owned `curry` constructor is exercised host-side by the corpus, not by a
compiled program factory returning a curried program; that needs its own
second-stage application relation. The pinned `curry-and-treehash.clib` is
upstream's deprecated reversed-argument API (the newer `curry.clib` is not
pinned). No curried-instance identity is claimed for any Rust module: Rust
artifacts still differ from canonical Chialisp bytes, so a curried Rust module
has a different puzzle hash than its canonical counterpart. Inner-puzzle hash
commitments, signature wrapping and singleton lineage checks are separate
work.
