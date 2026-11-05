//! Closed batch-one continuation recipe shared by CLI and loopback GUI.
//! Provider assertions are not chain authentication, finality or inclusion proofs.
use crate::{CoinRecord, Digest, digest};
use anyhow::{Context, Result, ensure};
use chia_bls::Signature;
use chia_consensus::{
    flags::MEMPOOL_MODE,
    solution_generator::calculate_generator_length,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
};
use chia_protocol::{CoinSpend, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chia_traits::Streamable;
use clvm_utils::tree_hash;
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

mod journal;
mod observation;
pub use journal::*;
mod store;
pub use store::ContinuationStore;
mod session;
pub use session::{ContinuationNetwork, ContinuationSession};
pub(super) mod cli;

pub const MAX_CONTINUATION_COST: u64 = 5_500_000_000;
pub const PAYOUT: u64 = 1000;
const MAX_MODULE: usize = 100_000;
const MAX_BUNDLE: usize = 128 * 1024;
const PROTOCOL: &str = "chialisp-oxide-continuation-live/v1";
const ITEM: &[u8] = b"chialisp-oxide/continuation/v1/item:1000";
const END: &[u8] = b"chialisp-oxide/continuation/v1/end:1000";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationMode {
    Off,
    On,
}
#[derive(Debug)]
pub enum ContinuationError {
    Inconclusive,
    Conflict,
    AlreadySubmitted,
    RevisionConflict,
    Consensus(chia_consensus::validation_error::ErrorCode),
}
impl std::fmt::Display for ContinuationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Self::Consensus(code) = self {
            return write!(f, "continuation SDK preflight {code:?}");
        }
        write!(
            f,
            "continuation {}",
            match self {
                Self::Inconclusive => "provider snapshot is inconclusive",
                Self::Conflict => "state conflicts with the exact recipe",
                Self::AlreadySubmitted => "push already started; reconcile without retry",
                Self::RevisionConflict => "journal revision changed; reload",
                Self::Consensus(_) => "typed SDK consensus rejection",
            }
        )
    }
}
impl std::error::Error for ContinuationError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueStep {
    pub head: Digest,
    pub next: Digest,
    pub recipient: Digest,
    pub puzzle_hash: Digest,
    pub amount: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPlan {
    pub mode: ContinuationMode,
    pub source_identity: Digest,
    pub module_hash: Digest,
    pub module_sha256: Digest,
    pub recipients: Vec<Digest>,
    pub steps: Vec<QueueStep>,
}

/// Only a fresh actual-process coordinator can create this production capability.
/// Persisted plans/modules/receipts cannot be deserialized into it.
pub struct ContinuationRecipe {
    material: crate::fresh::continuation::CheckedContinuationMaterial,
    plan: ContinuationPlan,
}
impl ContinuationRecipe {
    pub(crate) fn checked(
        material: crate::fresh::continuation::CheckedContinuationMaterial,
        mode: ContinuationMode,
        recipients: Vec<Digest>,
    ) -> Result<Self> {
        material.require_current()?;
        let plan = build_plan(material.module(mode), mode, material.identity(), recipients)?;
        // Admit every supported initial count against the selected module before
        // any funding or signing capability is exposed. These are actual SDK
        // executions, not the retained coordinator report as authority.
        for count in 1..=8 {
            let queue = (0..count)
                .map(|i| plan.recipients[i % plan.recipients.len()])
                .collect();
            let trial = build_plan(material.module(mode), mode, material.identity(), queue)?;
            let coin = CoinRecord {
                parent: [0x19; 32],
                puzzle_hash: trial.steps[0].puzzle_hash,
                amount: trial.steps[0].amount,
            };
            prepare(material.module(mode), &trial, 0, coin, 0)?;
        }
        for (index, step) in plan.steps.iter().enumerate() {
            let coin = CoinRecord {
                parent: [0x29; 32],
                puzzle_hash: step.puzzle_hash,
                amount: step.amount,
            };
            prepare(material.module(mode), &plan, index, coin, 0)?;
        }
        Ok(Self { material, plan })
    }
    pub fn plan(&self) -> &ContinuationPlan {
        &self.plan
    }
    pub fn require_current(&self) -> Result<()> {
        self.material.require_current()
    }
    pub fn funding_puzzle_hash(&self) -> Digest {
        self.plan.steps[0].puzzle_hash
    }
    pub fn funding_amount(&self) -> u64 {
        self.plan.steps[0].amount
    }
    pub fn module(&self) -> &[u8] {
        self.material.module(self.plan.mode)
    }
    pub fn prepare(
        &self,
        index: usize,
        coin: CoinRecord,
        height: u32,
    ) -> Result<ContinuationPrepared> {
        self.require_current()?;
        prepare(self.module(), &self.plan, index, coin, height)
    }
}
fn sha(parts: &[&[u8]]) -> Digest {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}
fn list(a: &mut Allocator, items: &[NodePtr]) -> Result<NodePtr> {
    let mut tail = NodePtr::NIL;
    for item in items.iter().rev() {
        tail = a.new_pair(*item, tail)?;
    }
    Ok(tail)
}
fn reveal(module: &[u8], head: Digest, count: u8) -> Result<Vec<u8>> {
    let mut a = Allocator::new_limited(32 * 1024 * 1024);
    let m = node_from_bytes(&mut a, module)?;
    let mh = tree_hash(&a, m).to_bytes();
    let hash = a.new_atom(&mh)?;
    let head = a.new_atom(&head)?;
    let n = a.new_atom(&[count])?;
    let terms = list(&mut a, &[hash, head, n])?;
    let one = a.new_atom(&[1])?;
    let two = a.new_atom(&[2])?;
    let four = a.new_atom(&[4])?;
    let qm = a.new_pair(one, m)?;
    let qt = a.new_pair(one, terms)?;
    let env = list(&mut a, &[four, qt, one])?;
    let root = list(&mut a, &[two, qm, env])?;
    Ok(node_to_bytes(&a, root)?)
}
fn opening(recipient: Digest, next: Digest) -> Result<Vec<u8>> {
    let mut a = Allocator::new_limited(1024 * 1024);
    let r = a.new_atom(&recipient)?;
    let n = a.new_atom(&next)?;
    let o = list(&mut a, &[r, n])?;
    let root = list(&mut a, &[o])?;
    Ok(node_to_bytes(&a, root)?)
}
fn hash_program(bytes: &[u8]) -> Result<Digest> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_BUNDLE,
        "bounded canonical program"
    );
    let mut a = Allocator::new_limited(32 * 1024 * 1024);
    let p = node_from_bytes(&mut a, bytes)?;
    ensure!(
        node_to_bytes(&a, p)? == bytes,
        "canonical serialized program"
    );
    Ok(tree_hash(&a, p).to_bytes())
}
fn build_plan(
    module: &[u8],
    mode: ContinuationMode,
    source_identity: Digest,
    recipients: Vec<Digest>,
) -> Result<ContinuationPlan> {
    ensure!(
        !module.is_empty() && module.len() <= MAX_MODULE && (1..=8).contains(&recipients.len()),
        "closed module/queue bounds"
    );
    let module_hash = hash_program(module)?;
    let mut heads = vec![[0; 32]; recipients.len() + 1];
    heads[recipients.len()] = sha(&[END]);
    for i in (0..recipients.len()).rev() {
        heads[i] = sha(&[ITEM, &recipients[i], &heads[i + 1]]);
    }
    let mut steps = Vec::new();
    for i in 0..recipients.len() {
        let count = u8::try_from(recipients.len() - i)?;
        let program = reveal(module, heads[i], count)?;
        steps.push(QueueStep {
            head: heads[i],
            next: heads[i + 1],
            recipient: recipients[i],
            puzzle_hash: hash_program(&program)?,
            amount: PAYOUT * u64::from(count),
        });
    }
    Ok(ContinuationPlan {
        mode,
        source_identity,
        module_hash,
        module_sha256: digest(module),
        recipients,
        steps,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPrepared {
    pub index: usize,
    pub coin: CoinRecord,
    pub children: Vec<CoinRecord>,
    pub bundle: Vec<u8>,
    pub bundle_id: Digest,
    pub total_cost: u64,
    pub execution_cost: u64,
    pub condition_cost: u64,
    pub charged_generator_bytes: u64,
    pub previous_transaction_height: u32,
}
impl ContinuationPrepared {
    pub(super) fn spend_bundle(&self) -> Result<SpendBundle> {
        ensure!(self.bundle.len() <= MAX_BUNDLE, "bounded own bundle");
        let value = crate::live::decode_bundle(&self.bundle)?;
        ensure!(
            value.coin_spends.len() == 1
                && value.aggregated_signature == Signature::default()
                && value.name().to_bytes() == self.bundle_id,
            "one exact permissionless bundle"
        );
        Ok(value)
    }
}
fn prepare(
    module: &[u8],
    plan: &ContinuationPlan,
    index: usize,
    coin: CoinRecord,
    height: u32,
) -> Result<ContinuationPrepared> {
    ensure!(
        height < u32::MAX,
        "unsupported future continuation protocol"
    );
    let step = plan.steps.get(index).context("queue complete")?;
    ensure!(
        coin.puzzle_hash == step.puzzle_hash && coin.amount == step.amount,
        "exact continuation input"
    );
    let count = u8::try_from(plan.steps.len() - index)?;
    let spend = CoinSpend::new(
        coin.coin(),
        Program::from(reveal(module, step.head, count)?),
        Program::from(opening(step.recipient, step.next)?),
    );
    let bundle = SpendBundle::new(vec![spend], Signature::default());
    let mut children = vec![CoinRecord {
        parent: coin.id(),
        puzzle_hash: step.recipient,
        amount: PAYOUT,
    }];
    if let Some(next) = plan.steps.get(index + 1) {
        children.push(CoinRecord {
            parent: coin.id(),
            puzzle_hash: next.puzzle_hash,
            amount: next.amount,
        });
    }
    let flags = get_flags_for_height_and_constants(height, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
    let (conditions, _) =
        validate_clvm_and_signature(&bundle, MAX_CONTINUATION_COST, &TESTNET11_CONSTANTS, flags)
            .map_err(|e| ContinuationError::Consensus(e.1))?;
    let mut expected = children
        .iter()
        .map(|c| (c.puzzle_hash, c.amount, true))
        .collect::<Vec<_>>();
    expected.sort();
    ensure!(
        conditions.spends.len() == 1
            && conditions.removal_amount == u128::from(coin.amount)
            && conditions.addition_amount == u128::from(coin.amount)
            && conditions.reserve_fee == 0
            && conditions.agg_sig_unsafe.is_empty(),
        "exact complete zero-fee continuation effects"
    );
    let mut actual = conditions.spends[0]
        .create_coin
        .iter()
        .map(|(h, n, hint)| ((*h).to_bytes(), *n, hint.is_none()))
        .collect::<Vec<_>>();
    actual.sort();
    ensure!(actual == expected, "complete exact payout/successor set");
    let bytes = bundle.to_bytes()?;
    ensure!(bytes.len() <= MAX_BUNDLE, "continuation bundle bytes");
    Ok(ContinuationPrepared {
        index,
        coin,
        children,
        bundle: bytes,
        bundle_id: bundle.name().to_bytes(),
        total_cost: conditions.cost,
        execution_cost: conditions.execution_cost,
        condition_cost: conditions.condition_cost,
        charged_generator_bytes: u64::try_from(
            calculate_generator_length(&bundle.coin_spends)
                .checked_sub(2)
                .context("quoted generator length")?,
        )?,
        previous_transaction_height: height,
    })
}

#[cfg(test)]
mod tests;
