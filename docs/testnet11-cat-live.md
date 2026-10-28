# Closed Testnet11 CAT issuance

**WIP prototype, not ready for use.** This local workflow issues one CAT to the
supplied test wallet. It supports one fixed genesis input, one Eve coin and one
normal post-Eve coin. Recipient and XCH change stay with that wallet. Transfer,
burn, multi-coin rings and cross-wallet recovery are separate work.

The CLI operations, in order, are `live cat-plan`, `cat-prepare-funding`,
`cat-submit-funding`, `cat-monitor-funding`, `cat-prepare-issuance`,
`cat-submit-issuance` and `cat-monitor-issuance`. `cat-status` rechecks the saved
intent. These operations take explicit `--network testnet11`, installed dated
nightly `--cargo`, `--compiler`, public `--wallet-puzzle-hash`,
`--master-public-key`, `--mode on|off`, a new `--output` directory for each
invocation and one shared `--outbox FILE.cat.live.json`. Funding and issuance
fees use `--fee-mojos` and `--issuance-fee-mojos`; both default to 10,000 mojos
and each is capped at 1,000,000. The saved intent fixes both fees.

Every command performs fresh native, complete Cargo-bundle, VM and SDK checks
before obtaining source authority. Reports cannot supply that authority.
Both Rust modes admit and recompile all 40 declared exports before selection.
The durable v2 source identity includes complete sorted IR/blob/tree inventories,
native inputs, installed tool fingerprints and independently checked outcomes;
output paths and launcher snapshots remain report data outside that identity.
Two actual runs in different output directories must agree before live use.

`cat-plan` creates an empty outbox without contacting the provider. Preparation
selects and durably saves the genesis input before reading a credential. The
hidden controlling-terminal reader accepts only the matching disposable
Testnet11 master secret; there is no browser field, argument, environment,
file or standard-input credential option. Saved signed bytes are rechecked
against the fresh closed intent, including actual BLS verification and complete
bundle cost, before submission. Funding must confirm before issuance preparation.

The append-only outbox records `PushStarted` durably before HTTP submission.
After interruption or an unknown response, reconcile the exact input spends
and expected children. Retry only the existing signed bundle. A fresh process
recognizing confirmation performs no further push. A changed saved revision
requires reloading before network work or a signer callback. The storage model
assumes cooperating writers and does not authenticate a hostile host.

## Executed local trial, October 28, 2026

The optimized Rust TAIL issued 1,000 raw CAT units to the supplied wallet on
Testnet11. Its module hash remains
`548b9ad1cb31d0ac0f623c99971ce2289abce58113a04576e94ded125f0aee69`,
which differs from the canonical Chialisp TAIL. This is a Rust port, not an
unchanged canonical module.

- Asset ID: `671235619e409cb184ec4384b4bdccf5b19feda50f3b5ed0698bfe80d2090469`.
- Funding bundle: `d595f463dcc3f8c98eb8ad6d853783eea9482f858cdab669f92392f9262f4a94`, input spent at height 4,887,473.
- Issuance bundle: `c010d374f3d1f8cf74aa30d373fb62555cbd2b57f4f01d2fb090c20f55330d9f`, both inputs spent at height 4,887,492.
- Normal CAT coin: `901e3ec8ce6497e382aaf53489eef3de0d3bc8e7287864cb6e4fa467b8940bce`.

Both submissions returned matching mempool acceptance. Point observations then
matched exact input spends, expected children and stable bracketing tips.
Funding cost was 10,179,584 with 445 charged generator bytes; issuance cost was
52,203,163 with 3,613 charged bytes. Fees were 10,000 mojos per bundle, 20,000
total. The principal remains in the CAT coin. Existing 10-million policy and
25-million standard-wallet limits are unchanged; the separately reviewed closed
CAT limit is 100 million. These measured costs are not a general fee quote.

Sage 0.13.1 discovered the exact asset automatically in the existing test wallet.
Its token details showed “Unknown CAT”, 1 CAT, 1 spendable and one owned coin
matching the normal coin above. Sage uses three display decimals, so its 1 CAT
corresponds to 1,000 raw units. No Sage issuance or send action was performed.
A separate fresh CLI process resumed issuance, recognized confirmation and kept
the total at two `PushStarted` events and two distinct bundles.

The machine-local receipt is `target/cat-first-live-a01/receipt.json` in the CAT
live worktree. It inventories public outbox, command, source-check and provider
records. Eleven actual fresh source checks ran: two initial checks and nine
lifecycle invocations. Each repeats the same 37 native observations, 111 primary
VM observations and 57 SDK attempts; repetition adds no distinct scenario
coverage. The SDK case set has 24 accepted submissions, 21 exact typed consensus
rejections and 12 runtime umbrellas. Resource failures remain inconclusive.

This trial establishes finite provider-consistency observations and actual Sage
discovery. It does not establish authenticated inclusion/finality, cross-wallet
spend acceptance, arbitrary CAT support, compiler preservation or production
readiness. Full workspace publication checks and integration remain separate
from these local trial results.
