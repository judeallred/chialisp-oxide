---
title: From a Cargo build to a coin spend
description: Learn the separate build, test, funding, signing, submission and observation stages, with a wallet-free local exercise.
---

**WIP prototype. Not ready for real funds or production use.** Begin with the
[coin model](/chialisp-oxide/start/coin-model/) and install the pinned tools in
the [quickstart](/chialisp-oxide/start/quickstart/). No wallet is required below.

## Know what each stage produces

1. **Author:** write a Cargo library and annotate the exported puzzle. Choose its
   recipient, amount, authorization and failure rules explicitly.
2. **Build:** native Rust compilation checks Rust; the additional CLVM subset
   check produces IR and compiled artifacts. A build creates no coin.
3. **Test:** run native tests, actual VM inputs and adversarial complete spends
   in the simulator. Compare independent references where available.
4. **Fund:** a wallet spends an existing coin to create a new coin locked to the
   exact puzzle hash, with the intended amount. This is what deployment means
   for a simple coin puzzle. A protocol such as a singleton has additional launch rules.
5. **Spend:** construct the reveal and solution for that exact coin, obtain any
   required signatures, and submit the bundle. Signing and submission are separate actions.
6. **Observe:** reconcile the input spend and expected children. Distinguish
   preparation, submission, mempool acceptance and confirmation.

Keep the exact compiled program and any curried parameters. A later rebuild
can produce a different puzzle hash; it cannot change the rules of an existing
coin. The original coin's reveal must still match its committed hash.

## First execute the local lifecycle

The companion's fresh payment recipe is a small Cargo library. It curries a
recipient into a payment that consumes exactly 1,000 mojos and creates a
1,000-mojo recipient coin. Its spending solution is nil. It is related to the
[first fixed payment](/chialisp-oxide/tutorials/first-payment/), but is a separate
recipe and compiled module, not a deployment of that laboratory export.

Run from the repository root:

```sh
cargo build --workspace --bins --examples --locked
mkdir -p target/lifecycle-local

target/debug/chialisp-oxide-lifecycle fresh run \
  --cargo "$(rustup which --toolchain nightly-2026-07-14 cargo)" \
  --compiler "$PWD/target/debug/cargo-chialisp-oxide" \
  --destination-hex 2323232323232323232323232323232323232323232323232323232323232323 \
  --mode on --output "$PWD/target/lifecycle-local/check-1" \
  --journal "$PWD/target/lifecycle-local/payment.lifecycle.json" --steps 3
```

The `23…23` recipient is an arbitrary simulator fixture, not a wallet you
control. Use fresh paths if you have already run the example. This command
performs fresh native, compiler, reference, VM and SDK checks, then stops the
simulator workflow after three durable transitions. It has not sent XCH.
Fresh validation can take several minutes, including on resume; the output
directory retains the detailed checks.

Resume by repeating the command with `fresh resume`, changing `check-1` to
`check-2` and removing `--steps 3`. Keep the same journal, recipient and mode.
Inspect the returned stage and child coin records. A completed run reports
`confirmed` in the simulator; it does not establish a network confirmation.

To use the same core in a GUI, replace `fresh run` with `fresh serve`, use another
new output directory, remove `--steps 3`, and open the printed loopback URL.
The [lifecycle guide](/chialisp-oxide/reference/lifecycle/) explains each stage,
recovery and the distinct journal formats.

## Then choose a supported live workflow deliberately

The separate Testnet11 adapter has executed narrow fixed-payment and CAT
issuance trials. It is not a generic deployer for every compiled export.
Its payment workflow funds 1,000 mojos and returns them to the same disposable
wallet, with separately selected funding and spending fees. It requires the
supported public-wallet derivation and a local macOS hidden-terminal signer.
External signer support remains open.

The [live CLI instructions](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/live.md)
cover wallet preconditions and the actual sequence: `plan`, `prepare-funding`,
`submit-funding`, `monitor-funding`, `prepare-payment`, `submit-payment`,
`monitor-payment`. Every invocation repeats fresh checks and retains a new output
directory. Its public outbox stores the intent and history; it stores no key.
Do not fund a fixture address from this lesson or put a seed/key in a command.

A custom coin needs its own reviewed host recipe, solution construction,
signing rules, ledger tests and monitoring. Compiling a Rust function is one
part of that work. The existing tooling does not infer these obligations from
its function signature.

## Check your understanding

Can you identify the funded coin's exact puzzle hash and amount, distinguish
its solution from committed parameters, explain who can spend it, and predict
all children and fees? Can you tell a VM success from a confirmed ledger spend?
If any answer is unclear, return to the corresponding local stage before
choosing a live workflow.
