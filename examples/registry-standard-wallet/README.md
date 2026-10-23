# Retrieved standard module inside an on-chain Rust wrapper

**WIP prototype. Not ready for real funds.** This unpublished ordinary Cargo
library applies the exact standard-wallet module retrieved through an explicit
local registry lock. It demonstrates dependency authoring, not a new wallet or
a canonical standard-module Rust port. The generated helper remains ordinary
Rust/MIR; its name, metadata and package supply no compiler privilege.

`prepare.py` admits the pinned retained Coinset fixture and its complete file
inventory, publishes its exact module bytes into a fresh local store, chooses
the exact envelope and module hash, writes an explicit dependency lock and runs
the production generator. It materializes this Cargo library in a fresh output
directory and rewrites its reviewed relative dependencies to absolute paths.
It makes no network request and does not build anything. Missing and corrupt
dependency controls and an edited-generated-source control fail offline without
changing the dependency lock or the checked generated source.

The authored `standard_wallet` export passes the **complete raw environment**
to the retrieved standard module. The author supplies the standard contract;
the lock does not infer it. Before funding, the SDK harness curries one chosen
48-byte synthetic public key into each actual compiled wrapper. Its remaining
solution is exactly `(ORIGINAL_KEY DELEGATED_PUZZLE SOLUTION)`. This example
chooses nil ORIGINAL_KEY, a quoted CREATE_COIN program and nil delegated
solution. It does not implement the standard hidden-puzzle path.

The Rust wrapper, retrieved canonical module and each key-curried instance have
different identities. The harness funds each build's own committed instance;
substituting a canonical reveal on a wrapper coin would only test a hash
mismatch. The chosen synthetic secret key is public disposable test data, not
the private key of the retained mainnet observation.

After finishing shared host builds and coordinating driver execution:

```sh
python3 examples/registry-standard-wallet/prepare.py \
  --registry-cli target/debug/cargo-chialisp-oxide \
  --output target/registry-standard-wallet
python3 examples/registry-standard-wallet/check.py \
  --registry-cli target/debug/cargo-chialisp-oxide \
  --driver target/debug/chialisp-oxide-driver \
  --output target/registry-standard-wallet --execute
```

Use a fresh output directory for each attempt; failed runs retain their logs.
The prepared package is an ordinary Cargo library at
`target/registry-standard-wallet/project`, with its own manifest and lockfile.
The execution gate first resolves that lockfile offline, then runs normal
`cargo check`, `cargo test --lib` and Clippy against the materialized manifest.
Native tests enable the facade's `vm` feature through a dev-dependency;
the separate CLVM subset check and both extracted builds leave it disabled.
An offline resolution failure means the selected dependencies need caching
before a new coordinated run; it is not a puzzle rejection.

The execution gate runs ordinary native checks/tests and builds the host harness
before extracting both compiler modes with complete schema-2 source receipts.
It checks the exact rewritten materialized manifest, native test source and
host ledger harness before native Cargo, then freezes their inputs through the
last host build and compiler execution. Its native observation count comes from
the actual test summary and exact emitted output witness, not a fixed success
counter. The retained source window includes the vendored Rue backend.
It then executes primary VM observations and actual SDK spends for both
wrappers and the retrieved canonical module as a separately executed reference.
The canonical reference uses the pinned module bytes, not another Rust wrapper
or a newly compiled Chialisp source. Exact wrong-key, empty and wrong-message
signatures reach consensus and fail with
`BadAggregateSignature`, preserving the entire simulator in memory. A replay
must fail with `DoubleSpend`. Accepted spends must create exactly the intended
1,000-mojo child. Malformed solutions have a separate typed primary-VM failure;
the SDK's `GeneratorRuntimeError` is only a runtime umbrella, never semantic
rejection evidence. Neither lane proves compiler preservation or authenticates
previous execution. No live network signing or spending is implemented.

The coordinated local preflight passed native checking, its native output test,
Clippy, the subset check, both compiler modes and the VM/SDK gate. Its finite
observations are separate:

- One native test group checks one exact output from the actual generated
  module, using the retained observation's public key without its private key.
- Six primary-VM observations cover three accepted condition outputs and three
  typed malformed-solution failures: optimized and unoptimized Rust wrappers,
  plus the retrieved canonical reference. These runs do not verify BLS.
- Eighteen actual SDK submissions cover those same three builds: three accepted
  complete spends, twelve exact typed consensus rejections and three runtime
  umbrellas. Each accepted spend creates one 1,000-mojo child at the intended
  destination. Each rejected submission checks full simulator equality in memory.

The SDK lane uses `chia-sdk-test 0.36.0`, `chia-consensus 0.36.1` and
`clvmr 0.16.2` with TESTNET11 constants. This is simulator execution, not a
Testnet11 network deployment. The primary VM uses `clvmr 0.21.0` and the separate
`clvmr-0.21.0/v1` typed failure protocol. Neither the counts nor retained consistency
checks establish authenticated execution, compiler preservation, a general
typed dependency interface or suitability for real funds.

`python3 examples/registry-standard-wallet/check.py --self-test` checks sixteen
constructed source-edit, zero-test, native-output and compiled-transport
corruption controls without running Cargo, a driver, a native policy, a VM or
an SDK submission. Compiled-output parsing uses separate bounded iterative
inspection; it does not increase source-generation limits or compiler budgets.

The raw retained module's upstream source/license/provenance remain under
`crates/chialisp-oxide-package/tests/fixtures/mainnet-standard`; public Coinset
observations are not authenticated chain inclusion or ownership evidence.
