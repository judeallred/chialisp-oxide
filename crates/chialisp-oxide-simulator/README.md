# Puzzle spend validation

This crate accepts serialized CLVM from any compiler and constructs actual coin
spends for the pure Rust `chia-sdk-test` ledger simulator. Returning a plausible
condition list is insufficient: the simulator checks coin existence, puzzle hash
commitments, signatures, amounts, announcements/messages, timelocks, and double
spends before updating its coin set.

```rust
use chialisp_oxide_simulator::{Simulator, Signature, SpendBundle, create_coin, coin_spend};

# fn example(compiled_puzzle: &[u8], serialized_solution: &[u8]) -> anyhow::Result<()> {
let mut simulator = Simulator::new();
let coin = create_coin(&mut simulator, compiled_puzzle, 42)?;
let spend = coin_spend(coin, compiled_puzzle, serialized_solution)?;
simulator.new_transaction(SpendBundle::new(vec![spend], Signature::default()))?;
assert!(simulator.coin_state(coin.coin_id()).unwrap().spent_height.is_some());
# Ok(())
# }
```

For a signed puzzle, use `simulator.spend_coins(vec![spend], &[secret_key])` or
submit an explicit `SpendBundle` with its aggregate signature. Test missing keys,
wrong keys, and signature reuse against a different coin as well as success.
For negative signature tests, submit an explicit bundle through
`new_transaction` with a missing, partial or incorrect aggregate signature and
require `Validation(BadAggregateSignature)`. The convenience signer's
`MissingKey` error is SDK preflight and does not establish ledger rejection.

Compose dependent spends in one bundle and require an invalid member to leave
the complete simulator unchanged.

Run `cargo test -p chialisp-oxide-simulator`. Executable regressions cover a fixed payment,
wrong `ASSERT_MY_AMOUNT`, early/mature `ASSERT_SECONDS_ABSOLUTE`, missing and
replayed `AGG_SIG_ME`, value creation, matched/unmatched announcements, bound
message conditions, double spending, malformed serialization, and state rollback.
These are validation-infrastructure fixtures, not migrated CHIP implementations.

Recorded scenarios carry ordered `rejections: [{stage: "consensus", code: "..."}]`
observations. Positive scenarios use an empty list. The shared
`rejection-protocol.json` closes the allowed code set and binds each scenario
name to its expected sequence: recovery checks both the time lock and a wrong
key. It also declares each coin fixture's complete ordered scenario list, so
dropping the same group from all three languages cannot preserve validation.
Codes are policy-specific: the boolean gate's 999-coin test requires
`MintingCoin`, because it creates 1,000 without `ASSERT_MY_AMOUNT`; the other
amount-asserting fixtures require `AssertMyAmountFailed`. The boolean gate's
separate surplus-input scenario accepts a 1,001 input, creates its 1,000 child
and leaves 1 as an implicit fee. Unknown codes, VM/resource failures, SDK preflight failures and unrelated
consensus errors do not satisfy these expectations. A scenario can submit
several bundles, so scenario-run counts are not transaction counts.


The announcement example exercises legacy consensus behavior. The current
[conditions reference](https://chialisp.com/conditions/) recommends
`SEND_MESSAGE` / `RECEIVE_MESSAGE` for new compositions. The message regression
uses mode 63 to bind both complete coin IDs; the counterpart ID is supplied in
the solution to avoid hardcoding mutually dependent coin IDs in puzzle hashes.

The simulator is pinned to `chia-sdk-test 0.36.0`
([source revision](https://github.com/xch-dev/chia-wallet-sdk/tree/e9a08f6e54ea5132f7da06327836a3b386a944b1/crates/chia-sdk-test)),
`chia-consensus 0.36.1`, `clvmr 0.16.2`, and its `TESTNET11` constants. This differs
from the primary differential executor's `clvmr 0.21.0`. Its
`chia-consensus-0.36.1/v1` rejection protocol is distinct from the primary VM's
`clvmr-0.21.0/v1` failure protocol. Record these versions
separately in reports. These tests establish behavior under the selected
simulator rules; they do not establish mainnet admission at arbitrary fork
heights, economic safety, or a general absence of vulnerabilities.
