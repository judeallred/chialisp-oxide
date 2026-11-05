//! Precommit/registry complete-spend observations.
//!
//! Paired scenarios compose each selected build with the other side's Chialisp
//! reference. A separate Chialisp-only naive baseline repeats for each build. The
//! host computes the commitment, curried puzzle hashes and coin IDs with its own
//! SHA-256 spelling, not the puzzle sources' recipe. Finite ledger scenarios do
//! not prove fairness, censorship resistance, salt entropy or the absence of
//! other front-running strategies.
use super::{Scenario, rejection};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, CoinSpend, ConsensusRejection, SecretKey, Signature, Simulator, SpendBundle, coin_spend,
    puzzle_hash, sign_transaction,
};
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use sha2::{Digest, Sha256};

const PRICE: u64 = 1_000;
const SLOT: u64 = 1;
const MINIMUM_AGE: u32 = 2;
const REFUND_AGE: u32 = 8;
const TREASURY: [u8; 32] = [0x7e; 32];
const OWNER_DESTINATION: [u8; 32] = [0x42; 32];
const REFUND_TO: [u8; 32] = [0x43; 32];
const ATTACKER_DESTINATION: [u8; 32] = [0x44; 32];
const SALT: [u8; 32] = [0x5a; 32];
const NAME: &[u8] = b"alice";
const DOMAIN: &[u8] = b"chialisp-oxide/precommit/v1/";

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn domain(name: &str) -> Vec<u8> {
    [DOMAIN, name.as_bytes()].concat()
}

fn commitment(
    registry: &[u8; 32],
    owner_key: &[u8; 48],
    destination: &[u8; 32],
    salt: &[u8; 32],
) -> [u8; 32] {
    let owner = sha(&[&domain("owner"), owner_key, destination]);
    let opened = sha(&[&domain("opening"), &owner, salt]);
    let target = sha(&[&domain("target"), registry, NAME]);
    sha(&[&domain("commit"), &sha(&[&target, &REFUND_TO, &opened])])
}

fn bytes(a: &mut Allocator, node: NodePtr) -> Result<Vec<u8>> {
    Ok(node_to_bytes(a, node)?)
}

fn atoms(items: &[&[u8]]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let mut list = NodePtr::NIL;
    for item in items.iter().rev() {
        let atom = a.new_atom(item)?;
        list = a.new_pair(atom, list)?;
    }
    bytes(&mut a, list)
}

fn atom(value: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let node = a.new_atom(value)?;
    bytes(&mut a, node)
}

struct Modules {
    precommit: Vec<u8>,
    registry: Vec<u8>,
}

struct Instance {
    precommit_reveal: Vec<u8>,
    registry_reveal: Vec<u8>,
    registry: Coin,
    precommit: Coin,
}

impl Modules {
    fn new(slug: &str, code: &[u8]) -> Result<Self> {
        let precommit_reference = chialisp_oxide_reference::compile(include_str!(
            "../../../../puzzles/precommit-claim/reference.clsp"
        ))?;
        let registry_reference = chialisp_oxide_reference::compile(include_str!(
            "../../../../puzzles/precommit-registry/reference.clsp"
        ))?;
        Ok(match slug {
            "precommit-claim" => Self {
                precommit: code.to_vec(),
                registry: registry_reference,
            },
            "precommit-registry" => Self {
                precommit: precommit_reference,
                registry: code.to_vec(),
            },
            other => anyhow::bail!("unknown precommit ledger slug {other}"),
        })
    }

    /// Fund a fresh registry slot and one precommit for the given opening.
    fn launch(
        &self,
        simulator: &mut Simulator,
        owner_key: &[u8; 48],
        destination: &[u8; 32],
    ) -> Result<Instance> {
        let module_hash = puzzle_hash(&self.precommit)?;
        let registry_reveal =
            super::super::curried::standard_form(&self.registry, &atom(module_hash.as_ref())?)?;
        let registry = simulator.new_coin(puzzle_hash(&registry_reveal)?, SLOT);
        let committed = commitment(
            &registry.coin_id().to_bytes(),
            owner_key,
            destination,
            &SALT,
        );
        let precommit_reveal = super::super::curried::standard_form(
            &self.precommit,
            &atoms(&[registry.coin_id().as_ref(), &committed, &REFUND_TO])?,
        )?;
        let precommit = simulator.new_coin(puzzle_hash(&precommit_reveal)?, PRICE);
        Ok(Instance {
            precommit_reveal,
            registry_reveal,
            registry,
            precommit,
        })
    }
}

impl Instance {
    fn claim(&self) -> Result<CoinSpend> {
        let me = self.precommit.coin_id();
        let solution = format!("(1 0x{})", hex::encode(me));
        coin_spend(
            self.precommit,
            &self.precommit_reveal,
            &super::solution(&solution)?,
        )
    }

    fn refund(&self) -> Result<CoinSpend> {
        let me = self.precommit.coin_id();
        let solution = format!("(() 0x{})", hex::encode(me));
        coin_spend(
            self.precommit,
            &self.precommit_reveal,
            &super::solution(&solution)?,
        )
    }

    fn register(
        &self,
        owner_key: &[u8; 48],
        destination: &[u8; 32],
        salt: &[u8; 32],
    ) -> Result<CoinSpend> {
        let solution = format!(
            "(0x{} 0x{} 0x{} (0x{} 0x{} 0x{}))",
            hex::encode(self.registry.coin_id()),
            hex::encode(self.precommit.parent_coin_info),
            hex::encode(REFUND_TO),
            hex::encode(owner_key),
            hex::encode(destination),
            hex::encode(salt),
        );
        coin_spend(
            self.registry,
            &self.registry_reveal,
            &super::solution(&solution)?,
        )
    }
}

fn scenario(name: &str, rejections: Vec<super::Rejection>) -> Scenario {
    Scenario {
        name: name.into(),
        passed: true,
        scope: "ledger".into(),
        rejections,
    }
}

/// Submit an invalid bundle and require both the exact code and full rollback.
fn rejected(
    simulator: &mut Simulator,
    spends: Vec<CoinSpend>,
    keys: &[SecretKey],
    expected: ConsensusRejection,
) -> Result<super::Rejection> {
    let before = simulator.clone();
    let observed = rejection(simulator.spend_coins(spends, keys), expected)?;
    ensure!(
        *simulator == before,
        "rejected precommit bundle changed the ledger"
    );
    Ok(observed)
}

fn age(simulator: &Simulator, coin: &Coin) -> Result<u32> {
    let created = simulator
        .coin_state(coin.coin_id())
        .and_then(|state| state.created_height)
        .ok_or_else(|| anyhow::anyhow!("precommit coin has no creation height"))?;
    Ok(simulator.height() - created)
}

fn advance_to_age(simulator: &mut Simulator, coin: &Coin, target: u32) -> Result<()> {
    while age(simulator, coin)? < target {
        simulator.create_block();
    }
    ensure!(age(simulator, coin)? == target, "precommit age overshot");
    Ok(())
}

/// Explicit simulator ordering control: no mempool, fee auction or RBF is run.
fn naive_attacker_first(owner: &SecretKey, attacker: &SecretKey) -> Result<Scenario> {
    let code = chialisp_oxide_reference::compile(include_str!(
        "../../../../experiments/precommit-controls/naive.clsp"
    ))?;
    let mut simulator = Simulator::new();
    let slot = simulator.new_coin(puzzle_hash(&code)?, SLOT);
    let spend = |key: &SecretKey, destination: &[u8; 32]| -> Result<CoinSpend> {
        coin_spend(
            slot,
            &code,
            &atoms(&[NAME, &key.public_key().to_bytes(), destination])?,
        )
    };
    // Both independently signed submissions name the same public registration
    // intent. The attacker substitutes its own key/destination and is submitted
    // first; its signature is genuine, rather than a stolen owner's signature.
    let honest = spend(owner, &OWNER_DESTINATION)?;
    let copied = spend(attacker, &ATTACKER_DESTINATION)?;
    simulator.spend_coins(vec![copied], std::slice::from_ref(attacker))?;
    let children = simulator.children(slot.coin_id());
    ensure!(
        children.len() == 1
            && children[0].coin.puzzle_hash.to_bytes() == ATTACKER_DESTINATION
            && children[0].coin.amount == SLOT,
        "naive attacker-first registration did not pay the attacker"
    );
    let lost = rejected(
        &mut simulator,
        vec![honest],
        std::slice::from_ref(owner),
        ConsensusRejection::DoubleSpend,
    )?;
    Ok(scenario(
        "naive one-step copied intent wins when attacker is submitted first (simulator ordering only)",
        vec![lost],
    ))
}

/// One forbidden complete-spend attempt against a freshly funded source
/// control. The caller retains the actual Cargo artifact identities and the
/// independently compiled counterpart's source/artifact identity. Supplying
/// bytes here never establishes their provenance or compiler preservation.
pub fn validate_source_control(
    claim_code: &[u8],
    registry_code: &[u8],
    control: &str,
) -> Result<Vec<Scenario>> {
    let (minimum_age, expected) = match control {
        "minimum_age_original" => (true, Some(ConsensusRejection::AssertHeightRelativeFailed)),
        "minimum_age_removed" => (true, None),
        "owner_signature_original" => (false, Some(ConsensusRejection::BadAggregateSignature)),
        "owner_signature_removed" => (false, None),
        other => anyhow::bail!("unknown precommit source control {other}"),
    };
    let modules = Modules {
        precommit: claim_code.to_vec(),
        registry: registry_code.to_vec(),
    };
    let owner = SecretKey::from_seed(&[0x17; 32]);
    let mut simulator = Simulator::new();
    let instance = modules.launch(
        &mut simulator,
        &owner.public_key().to_bytes(),
        &OWNER_DESTINATION,
    )?;
    advance_to_age(
        &mut simulator,
        &instance.precommit,
        if minimum_age {
            MINIMUM_AGE - 1
        } else {
            MINIMUM_AGE
        },
    )?;
    let spends = vec![
        instance.register(&owner.public_key().to_bytes(), &OWNER_DESTINATION, &SALT)?,
        instance.claim()?,
    ];
    // The unsigned witness reaches consensus with an actual empty aggregate;
    // the signer is never allowed to reject it before submission.
    let signature = if minimum_age {
        sign_transaction(&spends, std::slice::from_ref(&owner))?
    } else {
        Signature::default()
    };
    let before = simulator.clone();
    let result = simulator.new_transaction(SpendBundle::new(spends, signature));
    let rejections = if let Some(expected) = expected {
        let observed = rejection(result, expected)?;
        ensure!(
            simulator == before,
            "source-control rejection changed ledger"
        );
        vec![observed]
    } else {
        result?;
        let mut children = simulator
            .children(instance.registry.coin_id())
            .into_iter()
            .chain(simulator.children(instance.precommit.coin_id()))
            .map(|state| (state.coin.puzzle_hash.to_bytes(), state.coin.amount))
            .collect::<Vec<_>>();
        children.sort();
        ensure!(
            children == vec![(OWNER_DESTINATION, SLOT), (TREASURY, PRICE)],
            "source mutant did not create exactly the owner receipt and treasury payment"
        );
        for coin in [instance.registry, instance.precommit] {
            ensure!(
                simulator
                    .coin_state(coin.coin_id())
                    .is_some_and(|state| state.spent_height.is_some()),
                "source mutant did not consume both protocol inputs"
            );
        }
        vec![]
    };
    Ok(vec![scenario(
        &format!("source control {control}: one complete-spend submission"),
        rejections,
    )])
}

pub fn validate(slug: &str, code: &[u8]) -> Result<Vec<Scenario>> {
    let modules = Modules::new(slug, code)?;
    let owner = SecretKey::from_seed(&[0x17; 32]);
    let owner_key = owner.public_key().to_bytes();
    let attacker = SecretKey::from_seed(&[0x29; 32]);
    let attacker_key = attacker.public_key().to_bytes();
    let mut reports = vec![];
    reports.push(naive_attacker_first(&owner, &attacker)?);

    let mut simulator = Simulator::new();
    let honest = modules.launch(&mut simulator, &owner_key, &OWNER_DESTINATION)?;
    let bundle = || -> Result<Vec<CoinSpend>> {
        Ok(vec![
            honest.register(&owner_key, &OWNER_DESTINATION, &SALT)?,
            honest.claim()?,
        ])
    };

    // One block short of the minimum age, the complete honest bundle fails.
    advance_to_age(&mut simulator, &honest.precommit, MINIMUM_AGE - 1)?;
    let immature = rejected(
        &mut simulator,
        bundle()?,
        std::slice::from_ref(&owner),
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    reports.push(scenario(
        "claim one block before minimum age rejected",
        vec![immature],
    ));

    advance_to_age(&mut simulator, &honest.precommit, MINIMUM_AGE)?;
    // After learning the public opening, an attacker can commit its own key and
    // destination to this same slot, but its newly funded coin is not mature.
    // The registry counterpart is still independently compiled Chialisp when
    // the tested artifact is the Rust/Rue claim module (and vice versa).
    let copied_commitment = commitment(
        &honest.registry.coin_id().to_bytes(),
        &attacker_key,
        &ATTACKER_DESTINATION,
        &SALT,
    );
    let copied_reveal = super::super::curried::standard_form(
        &modules.precommit,
        &atoms(&[
            honest.registry.coin_id().as_ref(),
            &copied_commitment,
            &REFUND_TO,
        ])?,
    )?;
    let copied = Instance {
        precommit: simulator.new_coin(puzzle_hash(&copied_reveal)?, PRICE),
        precommit_reveal: copied_reveal,
        registry: honest.registry,
        registry_reveal: honest.registry_reveal.clone(),
    };
    let copied_early = rejected(
        &mut simulator,
        vec![
            copied.register(&attacker_key, &ATTACKER_DESTINATION, &SALT)?,
            copied.claim()?,
        ],
        std::slice::from_ref(&attacker),
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    reports.push(scenario(
        "copied intent with a fresh genuine precommit cannot win immediately (simulator ordering only)",
        vec![copied_early],
    ));
    // Each half alone lacks the other's announcement.
    let registry_alone = rejected(
        &mut simulator,
        vec![honest.register(&owner_key, &OWNER_DESTINATION, &SALT)?],
        std::slice::from_ref(&owner),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    let precommit_alone = rejected(
        &mut simulator,
        vec![honest.claim()?],
        &[],
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    reports.push(scenario(
        "registry and precommit each require the other",
        vec![registry_alone, precommit_alone],
    ));

    // A copier who saw the opening cannot redirect the receipt or change the key:
    // either change alters the commitment, so no genuine precommit matches.
    let redirected = rejected(
        &mut simulator,
        vec![
            honest.register(&owner_key, &ATTACKER_DESTINATION, &SALT)?,
            honest.claim()?,
        ],
        std::slice::from_ref(&owner),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    let rekeyed = rejected(
        &mut simulator,
        vec![
            honest.register(&attacker_key, &OWNER_DESTINATION, &SALT)?,
            honest.claim()?,
        ],
        std::slice::from_ref(&attacker),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    let wrong_salt = rejected(
        &mut simulator,
        vec![
            honest.register(&owner_key, &OWNER_DESTINATION, &[0x5b; 32])?,
            honest.claim()?,
        ],
        std::slice::from_ref(&owner),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    reports.push(scenario(
        "changed destination, key or salt matches no precommit",
        vec![redirected, rekeyed, wrong_salt],
    ));

    // The opening alone does not authorize: the owner key must sign.
    let before = simulator.clone();
    let unsigned = rejection(
        simulator.new_transaction(SpendBundle::new(bundle()?, Signature::default())),
        ConsensusRejection::BadAggregateSignature,
    )?;
    ensure!(
        simulator == before,
        "unsigned precommit bundle changed the ledger"
    );
    reports.push(scenario("owner signature required", vec![unsigned]));

    let early_refund = rejected(
        &mut simulator,
        vec![honest.refund()?],
        &[],
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    reports.push(scenario(
        "refund before refund age rejected",
        vec![early_refund],
    ));

    simulator.spend_coins(bundle()?, std::slice::from_ref(&owner))?;
    let mut children = simulator
        .children(honest.registry.coin_id())
        .into_iter()
        .chain(simulator.children(honest.precommit.coin_id()))
        .map(|state| (state.coin.puzzle_hash.to_bytes(), state.coin.amount))
        .collect::<Vec<_>>();
    children.sort();
    ensure!(
        children == vec![(OWNER_DESTINATION, SLOT), (TREASURY, PRICE)],
        "registration must create exactly the owner receipt and treasury payment"
    );
    reports.push(scenario(
        "mature signed claim pays treasury and owner receipt",
        vec![],
    ));

    // The slot is consumed: a second mature precommit for it cannot register.
    let second = {
        let precommit_reveal = honest.precommit_reveal.clone();
        let precommit = simulator.new_coin(puzzle_hash(&precommit_reveal)?, PRICE);
        Instance {
            precommit_reveal,
            registry_reveal: honest.registry_reveal.clone(),
            registry: honest.registry,
            precommit,
        }
    };
    advance_to_age(&mut simulator, &second.precommit, MINIMUM_AGE)?;
    let reused = rejected(
        &mut simulator,
        vec![
            second.register(&owner_key, &OWNER_DESTINATION, &SALT)?,
            second.claim()?,
        ],
        std::slice::from_ref(&owner),
        ConsensusRejection::DoubleSpend,
    )?;
    reports.push(scenario(
        "spent registry slot cannot register again",
        vec![reused],
    ));

    // Even after its refund deadline, a completed claim cannot refund the same
    // precommit a second time. Waiting avoids confusing a time failure with
    // the intended consumed-coin rejection.
    advance_to_age(&mut simulator, &honest.precommit, REFUND_AGE)?;
    let completed_refund = rejected(
        &mut simulator,
        vec![honest.refund()?],
        &[],
        ConsensusRejection::DoubleSpend,
    )?;
    reports.push(scenario(
        "refund after successful completion rejected even at refund age",
        vec![completed_refund],
    ));

    // An abandoned precommit returns its full amount to the refund destination.
    let mut simulator = Simulator::new();
    let abandoned = modules.launch(&mut simulator, &owner_key, &OWNER_DESTINATION)?;
    advance_to_age(&mut simulator, &abandoned.precommit, REFUND_AGE - 1)?;
    let early = rejected(
        &mut simulator,
        vec![abandoned.refund()?],
        &[],
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    advance_to_age(&mut simulator, &abandoned.precommit, REFUND_AGE)?;
    simulator.spend_coins(vec![abandoned.refund()?], &[])?;
    let refunded = simulator
        .children(abandoned.precommit.coin_id())
        .into_iter()
        .map(|state| (state.coin.puzzle_hash.to_bytes(), state.coin.amount))
        .collect::<Vec<_>>();
    ensure!(
        refunded == vec![(REFUND_TO, PRICE)],
        "refund must return the full amount to the committed destination"
    );
    ensure!(
        simulator
            .coin_state(abandoned.registry.coin_id())
            .is_some_and(|state| state.spent_height.is_none()),
        "refund must leave the registry slot unspent"
    );
    reports.push(scenario(
        "permissionless refund only at refund age",
        vec![early],
    ));
    Ok(reports)
}
