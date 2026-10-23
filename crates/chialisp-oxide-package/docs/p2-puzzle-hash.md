# Explicit host binding for canonical p2_puzzle_hash

**WIP prototype; not ready for production or real funds.**
The 11 focused binding tests execute independent canonical source compilation,
retrieved-byte comparisons, raw-tree preservation, typed VM failures and
resource controls. Run them with the package and workspace gates below; these
package observations do not credit a complete spend.

`chialisp_oxide_package::p2_puzzle_hash` is an explicit host-side binding for
one reviewed canonical module. It is not a newly admitted external call inside
an on-chain Rust export. Ordinary on-chain dependency authoring still uses the
separate registry-generated local Rust route and needs its own compiled/spend
evidence.

## Construct from actual resolved packages

First open the selected local `Registry`, read the explicit dependency lock,
and call `lock.resolve(&registry)`. This admits every locked dependency and the
complete bounded store before retrieving aliases through `resolved.get(name)`.
The lock remains version 1 with `binding: null`; choosing this Rust constructor
explicitly selects the reviewed schema. Package names, provenance labels and
structural ABI declarations cannot select it automatically.

The following host function takes the resulting admitted packages. It uses the
retrieved module and inner-program bytes without an embedded-program fallback:

```rust
use chialisp_oxide_package::{
    Limits, Result, VerifiedPackage,
    artifact::VerifiedArtifact,
    p2_puzzle_hash::{InnerPuzzleHash, P2PuzzleHashInstance, P2PuzzleHashModule},
};

fn prepare(
    module_package: &VerifiedPackage,
    inner_package: &VerifiedPackage,
    inner_environment: &VerifiedArtifact,
    limits: &Limits,
) -> Result<(P2PuzzleHashInstance, VerifiedArtifact)> {
    let module = P2PuzzleHashModule::from_package(module_package, limits)?;
    let inner_program = inner_package.artifact();
    let commitment = InnerPuzzleHash::from_artifact(inner_program);
    let instance = module.curry(commitment, limits)?;
    let solution = instance.solution(inner_program, inner_environment, limits)?;
    Ok((instance, solution))
}
```

`from_package` requires the exact reviewed module bytes, serialized digest and
tree hash, and re-admits them under the caller's limits. `curry` constructs the
canonical one-argument curry from those bytes. `solution` encodes the proper
outer list `(inner_program inner_environment)` and checks the program against
the instance's committed tree hash. The inner environment remains its complete
original tree, including binary atoms and improper tails; no extra argument-list
wrapper is inserted inside it.

For a separately reviewed commitment, use
`InnerPuzzleHash::from_tree_hash(digest)` explicitly. This declares the role of
those bytes; it does not prove their origin. Access `instance.artifact()` for
the complete curried reveal and `instance.commitment()` for its inner-tree
commitment. `module.object_id()` and `instance.module_object_id()` retain the
selected module package identity.

## Identities and failure scope

The package **object ID** hashes the exact envelope. The artifact **blob
SHA-256** hashes serialized CLVM bytes. The canonical **module tree hash**
identifies the decoded uncurried module. The **curried instance tree hash**
commits the selected inner-program hash and is the funding target. The
**inner-program tree hash** is the separate `InnerPuzzleHash` value. Do not
substitute a blob digest or package object ID for that commitment.

Only the outer canonical curry and two-field solution are reviewed here.
No nominal inner ABI, ownership, signature, conservation rule or safe condition
list is inferred. A matching inner program can fail, return arbitrary output,
or require authorization supplied elsewhere. Hash commitments rely on SHA-256
assumptions and do not authenticate source or authorship.

The constructor preflights bounded input/output work and dimensions before SDK
allocation, then re-admits the actual operands under caller limits. A mismatched
program returns host `ErrorKind::DigestMismatch`; this is not an executed
consensus rejection. Resource errors remain host/infrastructure outcomes.
Executed package tests separately exercise actual VM mismatch rejection, strict
failing application and resource exhaustion. No complete spend, signature validation,
chain observation or compiler-preservation proof is supplied by this binding.

## Canonical provenance and integration gate

The retained fixtures pin Chia Network's Apache-2.0 sources at commit
`d122456feeef55708d354a065623037bfa010714`, root Git tree
`89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62`:

- [Exact source](https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/puzzles/p2_puzzle_hash.clsp).
- [Published compiled hex](https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/puzzles/p2_puzzle_hash.clsp.hex).
- [Upstream license](https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/LICENSE).
- [Retained byte identities](../tests/fixtures/p2-puzzle-hash/provenance.json).

The module is 143 bytes. Its tree hash is
`13e29a62b42cd2ef72a79e4bacdc59733ca6310d65af83d349360d36ec622363`;
its blob SHA-256 is
`678bc94e1a0fa439ba21eb11ccfb7d1cea5ac9ee6e59dcf51175f3ab642aca64`.
These pins do not identify an observed deployed coin.

The reference-compiler dev dependency is reflected in the isolated lock. The
following package checks have passed there:

```sh
cargo test -p chialisp-oxide-package --locked --test p2_puzzle_hash
cargo test -p chialisp-oxide-package --locked
cargo clippy -p chialisp-oxide-package --all-targets --locked -- -D warnings
```

Run the required workspace and registry gates in the coordinated integration
phase. Root integration, broader authoring/spend evidence and release remain
pending.
