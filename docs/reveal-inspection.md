# Inspect reveals and bind standard modules

**WIP prototype. Not ready for production or real funds.** This guide covers
local reveal inspection and one reviewed host binding. It does not establish
chain inclusion, ownership, a valid signature or a complete coin spend.

Start with the [offline package registry](https://judeallred.github.io/chialisp-oxide/reference/package-registry/)
to build, publish and lock exact artifacts. A puzzle hash alone cannot disclose
its program or recover nominal types. An inspector needs the reveal bytes;
a reviewed binding also needs an exact known module and a specific schema.

## Inspect an existing reveal

```sh
target/debug/cargo-chialisp-oxide registry inspect --clvm-hex-file reveal.hex --expected-puzzle-hash FULL_PUZZLE_HASH --json
```

Both flags take explicit inputs. The hash is optional; when supplied, it must
match the complete reveal's computed tree hash. The command performs bounded
canonical plain-CLVM admission and structural inspection without running the
puzzle, Cargo, a compiler, a wallet or a network client. Input files may have
edge ASCII whitespace and an optional `0x` prefix. Backreference and serde-2026
encodings remain unsupported.

The report separates these identities and outcomes:

- `reveal`: the exact serialized blob identity and full decoded tree hash.
- `form`: `recognized_raw_module`, `canonical_curry` or `unclassified`.
- `module`: an extracted module's exact bytes and independent identity.
- `arguments`: exact argument trees in curry order, preserving atom padding,
  nested pairs and improper tails. There are at most 128 arguments.
- `module_recognized` and `known_module`: an explicit reviewed catalog match.
- `standard_arguments`: `decoded`, `invalid`, `not_curried` or `not_applicable`.

Canonical curry syntax has exact apply, quote and cons atoms, proper outer
forms and the environment tail `1`. A semantically similar expression can be
unclassified. That is a limit of structural recognition, not a claim that the
program rejects, is unsafe or cannot be written in Rust.

The current catalog recognizes the exact published
`p2_delegated_puzzle_or_hidden_puzzle` module. Its single argument is a
compressed synthetic public key. Decoding checks the actual BLS point through
the pinned parser; accepting any 48 bytes would be insufficient. The report
also identifies the identity point, which that historical parser accepts.
The role label does not prove synthetic derivation, a default hidden puzzle
or knowledge of a private key. See the
[standard transaction terminology](https://chialisp.com/standard-transactions/).

## Find packages for an extracted module

Add an explicit store to inspect a reveal and then look up its module:

```sh
target/debug/cargo-chialisp-oxide registry inspect --clvm-hex-file reveal.hex --expected-puzzle-hash FULL_PUZZLE_HASH --registry .local-puzzles --json
```

`module_lookup` is separate from the inspection report. The full reveal hash
is checked first; lookup uses only the extracted module hash and checks the
complete bounded store. With no extracted or recognized module it returns null and opens
no registry. No default store is discovered.

Ordinary `registry lookup --module-hash HASH` continues to find packages whose
complete artifact tree has that hash. A module package is not returned as
though it contained a different curried reveal. Package object IDs, serialized
blob digests, module hashes and full instance hashes retain separate meanings.

## Reproduce a real observed standard puzzle

The repository retains exact public Coinset HTTP responses for the spent coin
`ca3fe5c90b9466ad958964c035c38ecbd4b596df70e5e31341e33cee0b640421`,
reported at spend height **6318254**. Its reveal is **291 bytes** with full
puzzle hash
`545e9487959a1b2f6c50b5567dc63193eba648d8007896ec52bcde427f310499`.
The extracted canonical module is **227 bytes**, with module hash
`e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52`.

From the checkout, extract the retained reveal transport and inspect it:

```sh
python3 - <<'PY'
import json
from pathlib import Path
fixture = Path("crates/chialisp-oxide-package/tests/fixtures/mainnet-standard")
spend = json.loads((fixture / "standard-coin-spend.body").read_bytes())
output = Path("target/observed-standard.hex")
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(spend["coin_solution"]["puzzle_reveal"] + "\n")
PY
target/debug/cargo-chialisp-oxide registry inspect --clvm-hex-file target/observed-standard.hex --expected-puzzle-hash 545e9487959a1b2f6c50b5567dc63193eba648d8007896ec52bcde427f310499 --json
```

The fixture includes raw response/request receipts, byte digests, exact coin
fields, pinned upstream source/artifact identities and its source license.
Tests recompute coin/reveal relations, decode the actual reveal and solution
with the pinned Wallet SDK types, and recurry to exactly the observed bytes.
The three-field solution has a nil original-key field, a delegated program
and its solution. These are local relation and codec checks over a server
observation. Mutually consistent HTTP responses do not authenticate a chain,
header, inclusion proof, confirmation, signature or complete spend bundle.

## Use the reviewed standard binding from Rust

The host package exposes an explicit `StandardModule` binding. Construct it
from a package resolved through the exact dependency lock. It requires the
catalog's exact canonical module bytes, blob digest and tree hash; an arbitrary
package ABI or a suggestive alias cannot choose this schema.

```rust
use chialisp_oxide_package::{
    Limits, Result, VerifiedPackage,
    artifact::VerifiedArtifact,
    standard::{StandardInstance, StandardModule, SyntheticPublicKey},
};

fn delegated_instance(
    standard_package: &VerifiedPackage,
    inner_package: &VerifiedPackage,
    synthetic_key: SyntheticPublicKey,
    inner_solution: &VerifiedArtifact,
) -> Result<(StandardInstance, VerifiedArtifact)> {
    let limits = Limits::default();
    let module = StandardModule::from_package(standard_package, &limits)?;
    let instance = module.curry(&synthetic_key, &limits)?;
    let solution = instance.delegated_solution(
        inner_package.artifact(), inner_solution, &limits,
    )?;
    Ok((instance, solution))
}
```

The curry encoder uses the actual retrieved module. The delegated solution
uses the actual retrieved inner program. The implementation does not switch
to an SDK embedded module after doing a lookup. Changing the synthetic key
changes the full instance hash while preserving the canonical module identity.

`SyntheticPublicKey::from_bytes` validates the compressed point and rejects
the identity point for new instances. Its name records the caller's intended
role; the constructor does not derive a synthetic key or prove ownership.
The delegated builder chooses a nil original-key field. The canonical module's
hidden branch is retained, but a typed hidden-branch builder is not provided.

These types describe the **outer standard schema**. The inner program and its
solution remain opaque verified trees; no inner ABI is inferred. Constructing
a delegated solution does not sign it or validate the resulting conditions.
These host APIs are also not newly admitted calls inside annotated on-chain
Rust functions. General typed registry dependencies in the frontend remain
separate work.

## Exercise retrieved Rust inner programs

The `locked-standard` example resolves two aliases from a portable lock: the
canonical standard module and an inner program. It curries the former, builds
the delegated solution and executes the resulting bytes in the host VM. Its
arguments are the lock, registry, two aliases, synthetic-key hex and inner
solution hex. Missing or tampered dependencies fail before VM execution.

After compiling the tools and extracting paired corpus bundles as described
in the registry guide, run the dedicated replay with new output:

```sh
cargo build --locked -p chialisp-oxide-package -p cargo-chialisp-oxide --bins --examples
python3 scripts/check-registry-standard.py --cli target/debug/cargo-chialisp-oxide --consumer target/debug/examples/locked-standard --bundle-on target/registry-source-on --bundle-off target/registry-source-off --output target/standard-replay
```

It seeds the exact published standard module and the actual Rust capped-payment
artifacts, then checks the standard signature condition and expected payment
conditions. It keeps intended typed VM failures separate from host admission
failures and checks key/module substitution and dependency tampering. The
example bounds solution transport, aggregate construction and VM resources.
Resource failures remain inconclusive. Producing `AGG_SIG_ME` in a VM result
does not verify its required aggregate signature or admit a simulator spend.

The binding uses reviewed `chia-puzzle-types` 0.36.1 schemas and the SDK's
0.16.2 tree codecs. Artifact admission and explicit example VM execution use
the project's separate 0.21.0 boundary. All are pinned dependencies. This
interop check does not prove universal codec equivalence, compiler preservation
or production safety. The registry guide also covers the explicit
[`p2_puzzle_hash` host binding](https://judeallred.github.io/chialisp-oxide/reference/package-registry/#construct-a-reviewed-p2_puzzle_hash-instance-on-the-host)
and a separate [on-chain Cargo wrapper with SDK spends](https://judeallred.github.io/chialisp-oxide/reference/package-registry/#spend-through-a-retrieved-standard-module).
Broader bindings and the remaining package-manager work stay tracked in issue #21.
