//! Complete local registry/slot lifecycle with exact primary output and SDK rejection checks.
use super::wire::*;
use super::{ACTIONS, PRIMARY_COST, RegistryOrdinaryCounts, fixtures};
use anyhow::{Context, Result, bail, ensure};
use chia_puzzle_types::{EveProof, LineageProof, Proof};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, Simulator, SimulatorError,
    SpendBundle, coin_spend, create_coin, sign_single_requirement_with_wrong_key, sign_transaction,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct LifecycleJournal(pub(crate) Value);

pub(super) fn primary(reveal: &[u8], solution: &[u8]) -> Result<Value> {
    admit(reveal)?;
    admit(solution)?;
    let mut a = clvmr::Allocator::new_limited(32 * LIMIT);
    let p = clvmr::serde::node_from_bytes(&mut a, reveal)?;
    let s = clvmr::serde::node_from_bytes(&mut a, solution)?;
    match clvmr::run_program(
        &mut a,
        &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
        p,
        s,
        PRIMARY_COST,
    ) {
        Ok(r) => {
            let out = clvmr::serde::node_to_bytes_limit(&a, r.1, LIMIT)?;
            admit(&out)?;
            Ok(
                json!({"outcome":{"status":"accepted","output_hex":hex::encode(out)},"cost":r.0,"failure":null}),
            )
        }
        Err(error) => {
            let failure = chialisp_oxide_vm::classify_failure(&error);
            ensure!(
                failure.is_program_rejection(),
                "inconclusive primary resource/infrastructure failure: {error}"
            );
            Ok(json!({"outcome":{"status":"rejected"},"cost":null,"failure":failure}))
        }
    }
}
pub(super) fn coin(c: Coin) -> Value {
    json!({"coin_id":hex::encode(c.coin_id()),"parent_coin_id":hex::encode(c.parent_coin_info),"puzzle_hash":hex::encode(c.puzzle_hash),"amount":c.amount})
}
pub(super) fn state(s: State) -> Value {
    json!({"initialized":s.initialized,"alpha_hash":hex::encode(s.alpha),"beta_hash":hex::encode(s.beta)})
}
#[derive(Clone, Copy)]
pub(super) struct Controller {
    pub(super) coin: Coin,
    pub(super) state: State,
    pub(super) parent: Option<(Coin, State)>,
}
#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub(super) coin: Coin,
    pub(super) creator: Coin,
    pub(super) creator_state: State,
    pub(super) entry: Bytes32,
}
pub(super) fn slot_record(slot: Slot) -> Value {
    json!({"coin":coin(slot.coin),"creator_controller":coin(slot.creator),"creator_state":state(slot.creator_state),"entry_hash":hex::encode(slot.entry)})
}
pub(super) struct Ledger {
    pub(super) sim: Simulator,
    pub(super) attempts: Vec<Value>,
}
impl Ledger {
    pub(super) fn submit(
        &mut self,
        name: &str,
        spends: Vec<CoinSpend>,
        signature: Signature,
        outputs: Vec<Vec<u8>>,
        expected: Option<ErrorCode>,
    ) -> Result<()> {
        ensure!(
            outputs.len() == spends.len(),
            "complete expected primary output inventory"
        );
        let executions = spends
            .iter()
            .map(|s| primary(s.puzzle_reveal.as_ref(), s.solution.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        for (execution, output) in executions.iter().zip(outputs) {
            ensure!(
                execution["outcome"]["status"] == "accepted"
                    && execution["outcome"]["output_hex"] == hex::encode(output),
                "independent exact component output for {name}"
            );
        }
        let tx = json!({"signature_hex":hex::encode(signature.to_bytes()),"spends":spends.iter().map(|s|json!({"coin":coin(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>()});
        let outcome = if let Some(code) = expected {
            let before = self.sim.clone();
            match self
                .sim
                .new_transaction(SpendBundle::new(spends, signature))
            {
                Err(SimulatorError::Validation(actual)) if actual == code => (),
                other => bail!("expected exact {code:?} for {name}, got {other:?}"),
            };
            ensure!(
                self.sim == before,
                "full in-memory simulator equality after {name}"
            );
            json!({"status":"rejected","stage":"consensus","code":format!("{code:?}"),"complete_state_unchanged":true})
        } else {
            self.sim
                .new_transaction(SpendBundle::new(spends, signature))?;
            json!({"status":"accepted"})
        };
        self.attempts
            .push(json!({"name":name,"transaction":tx,"primary_vm":executions,"outcome":outcome}));
        Ok(())
    }
    pub(super) fn unspent(&self, c: Coin) -> Result<()> {
        ensure!(
            self.sim
                .coin_state(c.coin_id())
                .context("actual coin state")?
                .spent_height
                .is_none(),
            "required actual unspent coin"
        );
        Ok(())
    }
    pub(super) fn spent(&self, c: Coin) -> Result<()> {
        ensure!(
            self.sim
                .coin_state(c.coin_id())
                .context("actual coin state")?
                .spent_height
                .is_some(),
            "required actual spent coin"
        );
        Ok(())
    }
    pub(super) fn children(&self, parent: Coin, expected: &[(Vec<u8>, u64)]) -> Result<Vec<Coin>> {
        self.spent(parent)?;
        let children = self.sim.children(parent.coin_id());
        ensure!(
            children.len() == expected.len(),
            "exact total created child count"
        );
        let mut result = Vec::new();
        for (reveal, amount) in expected {
            let ph = hash(reveal)?;
            let matches = children
                .iter()
                .filter(|c| c.coin.puzzle_hash == ph && c.coin.amount == *amount)
                .collect::<Vec<_>>();
            ensure!(matches.len() == 1, "unique child reveal/amount");
            let c = matches[0].coin;
            ensure!(
                c.parent_coin_info == parent.coin_id(),
                "actual child parent"
            );
            result.push(c);
        }
        Ok(result)
    }
}
pub(super) fn controller_spend(
    policy: &Policy,
    c: Controller,
    action: usize,
    old: &[u8; 32],
    new: &[u8; 32],
    genesis: Coin,
) -> Result<CoinSpend> {
    let proof = if let Some((parent, parent_state)) = c.parent {
        Proof::Lineage(LineageProof {
            parent_parent_coin_info: parent.parent_coin_info,
            parent_inner_puzzle_hash: hash(&policy.inner(parent_state)?)?,
            parent_amount: parent.amount,
        })
    } else {
        Proof::Eve(EveProof {
            parent_parent_coin_info: genesis.coin_id(),
            parent_amount: 1,
        })
    };
    coin_spend(
        c.coin,
        &policy.full(c.state)?,
        &policy.full_solution(proof, action, old, new)?,
    )
}
pub(super) fn slot_spend(policy: &Policy, slot: Slot, current: Controller) -> Result<CoinSpend> {
    ensure!(
        slot.creator.puzzle_hash == hash(&policy.full(slot.creator_state)?)?
            && slot.creator.amount == 1,
        "actual creator controller commitment"
    );
    coin_spend(
        slot.coin,
        &policy.slot(slot.entry)?,
        &policy.slot_solution(slot.creator, slot.creator_state, current.state)?,
    )
}
pub(super) fn slot_output(slot: Slot, current: Controller) -> Result<Vec<u8>> {
    list(&[
        list(&[integer(71)?, atom(slot.creator.coin_id().as_ref())?])?,
        list(&[
            integer(67)?,
            integer(18)?,
            atom(&[])?,
            atom(current.coin.puzzle_hash.as_ref())?,
        ])?,
    ])
}
pub(super) fn create_slot_condition(policy: &Policy, entry: Bytes32) -> Result<Vec<u8>> {
    list(&[
        integer(51)?,
        atom(hash(&policy.slot(entry)?)?.as_ref())?,
        integer(0)?,
        list(&[atom(policy.slot_first_hash.as_ref())?])?,
    ])
}
pub(super) fn controller_conditions(
    policy: &Policy,
    current: Controller,
    next: State,
    action: usize,
    old: &[u8; 32],
    new: &[u8; 32],
    owner: &SecretKey,
) -> Result<Vec<Vec<u8>>> {
    let mut conditions = vec![
        list(&[integer(73)?, integer(1)?])?,
        list(&[integer(71)?, atom(current.coin.parent_coin_info.as_ref())?])?,
        list(&[
            integer(51)?,
            atom(hash(&policy.full(next)?)?.as_ref())?,
            integer(1)?,
            list(&[atom(policy.launcher.as_ref())?])?,
        ])?,
    ];
    if action == 0 {
        conditions.push(create_slot_condition(policy, next.alpha)?);
        conditions.push(create_slot_condition(policy, next.beta)?);
    } else {
        let side = action - 1;
        let old_entry = entry_hash(side, old);
        let new_entry = entry_hash(side, new);
        conditions.push(list(&[
            integer(66)?,
            integer(18)?,
            atom(&[])?,
            atom(hash(&policy.slot(old_entry)?)?.as_ref())?,
        ])?);
        conditions.push(create_slot_condition(policy, new_entry)?);
        let domain = sha(&[if side == 0 {
            b"chialisp-oxide/registry/v1/set-alpha"
        } else {
            b"chialisp-oxide/registry/v1/set-beta"
        }]);
        let message = sha(&[domain.as_ref(), old, new]);
        conditions.push(list(&[
            integer(50)?,
            atom(&owner.public_key().to_bytes())?,
            atom(message.as_ref())?,
        ])?);
    }
    Ok(conditions)
}
pub(super) fn controller_output(
    policy: &Policy,
    current: Controller,
    next: State,
    action: usize,
    old: &[u8; 32],
    new: &[u8; 32],
    owner: &SecretKey,
) -> Result<Vec<u8>> {
    list(&controller_conditions(
        policy, current, next, action, old, new, owner,
    )?)
}
pub(super) struct Update<'a> {
    pub(super) action: usize,
    pub(super) old: &'a [u8; 32],
    pub(super) new: &'a [u8; 32],
    pub(super) name: &'static str,
}
pub(super) fn accepted_update(
    ledger: &mut Ledger,
    policy: &Policy,
    current: Controller,
    old_slot: Slot,
    update: Update<'_>,
    owner: &SecretKey,
    genesis: Coin,
) -> Result<(Controller, Slot)> {
    let Update {
        action,
        old,
        new,
        name,
    } = update;
    ledger.unspent(current.coin)?;
    ledger.unspent(old_slot.coin)?;
    let next = current.state.set(action - 1, new);
    let cs = controller_spend(policy, current, action, old, new, genesis)?;
    let ss = slot_spend(policy, old_slot, current)?;
    let spends = vec![cs, ss];
    let sig = sign_transaction(&spends, std::slice::from_ref(owner))?;
    ledger.submit(
        name,
        spends,
        sig,
        vec![
            controller_output(policy, current, next, action, old, new, owner)?,
            slot_output(old_slot, current)?,
        ],
        None,
    )?;
    let entry = entry_hash(action - 1, new);
    let children = ledger.children(
        current.coin,
        &[(policy.full(next)?, 1), (policy.slot(entry)?, 0)],
    )?;
    ensure!(
        ledger.sim.children(old_slot.coin.coin_id()).is_empty(),
        "consumed slot creates no children itself"
    );
    ledger.spent(old_slot.coin)?;
    Ok((
        Controller {
            coin: children[0],
            state: next,
            parent: Some((current.coin, current.state)),
        },
        Slot {
            coin: children[1],
            creator: current.coin,
            creator_state: current.state,
            entry,
        },
    ))
}
pub(super) struct Start {
    pub(super) ledger: Ledger,
    pub(super) policy: Policy,
    pub(super) genesis: Coin,
    pub(super) launcher: Coin,
    pub(super) eve: Controller,
    pub(super) c1: Controller,
    pub(super) a0: Slot,
    pub(super) b0: Slot,
}
pub(super) fn start(templates: [&[u8]; 3], owner: &SecretKey) -> Result<Start> {
    let mut ledger = Ledger {
        sim: Simulator::new(),
        attempts: Vec::new(),
    };
    let wallet = curry(
        &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        &[atom(&owner.public_key().to_bytes())?],
    )?;
    let genesis = create_coin(&mut ledger.sim, &wallet, 1)?;
    let launcher = Coin::new(
        genesis.coin_id(),
        Bytes32::new(chia_puzzles::SINGLETON_LAUNCHER_HASH),
        1,
    );
    let policy = Policy::new(
        templates,
        &owner.public_key().to_bytes(),
        launcher.coin_id(),
    )?;
    let initial = State::initial();
    let eve_reveal = policy.full(initial)?;
    let launch_solution = list(&[atom(hash(&eve_reveal)?.as_ref())?, integer(1)?, atom(&[])?])?;
    let announcement = sha(&[
        launcher.coin_id().as_ref(),
        hash(&launch_solution)?.as_ref(),
    ]);
    let fund_conditions = list(&[
        list(&[
            integer(51)?,
            atom(launcher.puzzle_hash.as_ref())?,
            integer(1)?,
        ])?,
        list(&[integer(61)?, atom(announcement.as_ref())?])?,
    ])?;
    let delegated = pair(&integer(1)?, &fund_conditions)?;
    let funding_solution = list(&[atom(&[])?, delegated.clone(), atom(&[])?])?;
    let spends = vec![
        coin_spend(genesis, &wallet, &funding_solution)?,
        coin_spend(
            launcher,
            &chia_puzzles::SINGLETON_LAUNCHER,
            &launch_solution,
        )?,
    ];
    let sig = sign_transaction(&spends, std::slice::from_ref(owner))?;
    let fund_output = pair(
        &list(&[
            integer(50)?,
            atom(&owner.public_key().to_bytes())?,
            atom(hash(&delegated)?.as_ref())?,
        ])?,
        &fund_conditions,
    )?;
    let launcher_output = list(&[
        list(&[
            integer(51)?,
            atom(hash(&eve_reveal)?.as_ref())?,
            integer(1)?,
        ])?,
        list(&[integer(60)?, atom(hash(&launch_solution)?.as_ref())?])?,
    ])?;
    ledger.submit(
        "fund_and_launch",
        spends,
        sig,
        vec![fund_output, launcher_output],
        None,
    )?;
    let eve_coin = ledger.children(launcher, &[(eve_reveal, 1)])?[0];
    let eve = Controller {
        coin: eve_coin,
        state: initial,
        parent: None,
    };
    let initialized = State {
        initialized: true,
        ..initial
    };
    let init = controller_spend(&policy, eve, 0, &[0; 32], &[0; 32], genesis)?;
    ledger.submit(
        "initialize",
        vec![init],
        Signature::default(),
        vec![controller_output(
            &policy,
            eve,
            initialized,
            0,
            &[0; 32],
            &[0; 32],
            owner,
        )?],
        None,
    )?;
    let children = ledger.children(
        eve.coin,
        &[
            (policy.full(initialized)?, 1),
            (policy.slot(initialized.alpha)?, 0),
            (policy.slot(initialized.beta)?, 0),
        ],
    )?;
    let c1 = Controller {
        coin: children[0],
        state: initialized,
        parent: Some((eve.coin, eve.state)),
    };
    let a0 = Slot {
        coin: children[1],
        creator: eve.coin,
        creator_state: eve.state,
        entry: initialized.alpha,
    };
    let b0 = Slot {
        coin: children[2],
        creator: eve.coin,
        creator_state: eve.state,
        entry: initialized.beta,
    };
    Ok(Start {
        ledger,
        policy,
        genesis,
        launcher,
        eve,
        c1,
        a0,
        b0,
    })
}
pub(super) fn run(
    artifacts: &[(String, String, Vec<u8>)],
    mode: &str,
    owner: &SecretKey,
) -> Result<(LifecycleJournal, RegistryOrdinaryCounts)> {
    let mut templates = Vec::new();
    for action in ACTIONS {
        let matches = artifacts
            .iter()
            .filter(|r| r.0 == action && r.1 == mode)
            .collect::<Vec<_>>();
        ensure!(matches.len() == 1, "unique selected source artifact");
        templates.push(matches[0].2.as_slice());
    }
    let Start {
        mut ledger,
        policy,
        genesis,
        launcher,
        eve,
        c1,
        a0,
        b0,
    } = start([templates[0], templates[1], templates[2]], owner)?;
    let alpha_next = c1.state.set(0, &[0x11; 32]);
    let alpha_cs = controller_spend(&policy, c1, 1, &[0; 32], &[0x11; 32], genesis)?;
    let alpha_ss = slot_spend(&policy, a0, c1)?;
    let valid_pair = vec![alpha_cs.clone(), alpha_ss.clone()];
    let valid_outputs = vec![
        controller_output(&policy, c1, alpha_next, 1, &[0; 32], &[0x11; 32], owner)?,
        slot_output(a0, c1)?,
    ];
    ledger.submit(
        "missing_owner_signature",
        valid_pair.clone(),
        Signature::default(),
        valid_outputs.clone(),
        Some(ErrorCode::BadAggregateSignature),
    )?;
    let wrong_key = SecretKey::from_seed(&fixtures::WRONG_OWNER_SEED);
    let wrong_signature = sign_single_requirement_with_wrong_key(&valid_pair, &wrong_key)?;
    ensure!(
        wrong_signature != Signature::default(),
        "actual nonempty wrong-key signature"
    );
    ledger.submit(
        "nonempty_wrong_owner_signature",
        valid_pair,
        wrong_signature,
        valid_outputs,
        Some(ErrorCode::BadAggregateSignature),
    )?;
    let sig = sign_transaction(std::slice::from_ref(&alpha_cs), std::slice::from_ref(owner))?;
    ledger.submit(
        "controller_without_slot",
        vec![alpha_cs],
        sig,
        vec![controller_output(
            &policy,
            c1,
            alpha_next,
            1,
            &[0; 32],
            &[0x11; 32],
            owner,
        )?],
        Some(ErrorCode::MessageNotSentOrReceived),
    )?;
    ledger.submit(
        "slot_without_controller",
        vec![alpha_ss],
        Signature::default(),
        vec![slot_output(a0, c1)?],
        Some(ErrorCode::MessageNotSentOrReceived),
    )?;
    ledger.unspent(c1.coin)?;
    ledger.unspent(a0.coin)?;
    let (c2, a1) = accepted_update(
        &mut ledger,
        &policy,
        c1,
        a0,
        Update {
            action: 1,
            old: &[0; 32],
            new: &[0x11; 32],
            name: "set_alpha",
        },
        owner,
        genesis,
    )?;
    ensure!(
        b0.creator.coin_id() == eve.coin.coin_id() && b0.creator.coin_id() != c2.coin.coin_id(),
        "beta retains its older actual Eve creator"
    );
    ledger.unspent(b0.coin)?;
    let (c3, b1) = accepted_update(
        &mut ledger,
        &policy,
        c2,
        b0,
        Update {
            action: 2,
            old: &[0; 32],
            new: &[0x22; 32],
            name: "set_beta_with_older_creator",
        },
        owner,
        genesis,
    )?;
    let (c4, a2) = accepted_update(
        &mut ledger,
        &policy,
        c3,
        a1,
        Update {
            action: 1,
            old: &[0x11; 32],
            new: &[0x11; 32],
            name: "alpha_noop",
        },
        owner,
        genesis,
    )?;
    ensure!(
        a1.coin.puzzle_hash == a2.coin.puzzle_hash
            && a1.coin.coin_id() != a2.coin.coin_id()
            && c3.coin.puzzle_hash == c4.coin.puzzle_hash
            && c3.coin.coin_id() != c4.coin.coin_id(),
        "no-op same puzzle hashes but distinct actual coin IDs"
    );
    ledger.unspent(c4.coin)?;
    ledger.unspent(a2.coin)?;
    ledger.spent(a1.coin)?;
    let fresh = controller_spend(&policy, c4, 1, &[0x11; 32], &[0x11; 32], genesis)?;
    let old = slot_spend(&policy, a1, c4)?;
    let live = slot_spend(&policy, a2, c4)?;
    let signature = sign_transaction(std::slice::from_ref(&fresh), std::slice::from_ref(owner))?;
    ensure!(
        signature == sign_transaction(&[fresh.clone(), old.clone()], std::slice::from_ref(owner))?
            && signature
                == sign_transaction(&[fresh.clone(), live.clone()], std::slice::from_ref(owner))?,
        "same correctly signed fresh C4 controller, both slot primaries add no signature"
    );
    let fresh_output =
        controller_output(&policy, c4, c4.state, 1, &[0x11; 32], &[0x11; 32], owner)?;
    ledger.submit(
        "consumed_slot_with_fresh_controller",
        vec![fresh.clone(), old],
        signature.clone(),
        vec![fresh_output.clone(), slot_output(a1, c4)?],
        Some(ErrorCode::DoubleSpend),
    )?;
    ledger.unspent(c4.coin)?;
    ledger.unspent(a2.coin)?;
    ledger.spent(a1.coin)?;
    ledger.submit(
        "live_slot_same_fresh_controller",
        vec![fresh, live],
        signature,
        vec![fresh_output, slot_output(a2, c4)?],
        None,
    )?;
    let children = ledger.children(
        c4.coin,
        &[(policy.full(c4.state)?, 1), (policy.slot(a2.entry)?, 0)],
    )?;
    let c5 = Controller {
        coin: children[0],
        state: c4.state,
        parent: Some((c4.coin, c4.state)),
    };
    let a3 = Slot {
        coin: children[1],
        creator: c4.coin,
        creator_state: c4.state,
        entry: a2.entry,
    };
    ledger.spent(a2.coin)?;
    ledger.unspent(b1.coin)?;
    ledger.unspent(c5.coin)?;
    ledger.unspent(a3.coin)?;
    let accepted = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["status"] == "accepted")
        .count();
    let primary_count: usize = ledger
        .attempts
        .iter()
        .map(|a| a["primary_vm"].as_array().unwrap().len())
        .sum();
    ensure!(
        ledger.attempts.len() == 11 && accepted == 6 && primary_count == 19,
        "exact minimal registry/slot attempt inventory"
    );
    let counts = RegistryOrdinaryCounts {
        scenario_runs: 1,
        sdk_transaction_attempts: ledger.attempts.len(),
        accepted,
        typed_consensus_rejections: ledger.attempts.len() - accepted,
        component_primary_observations: primary_count,
    };
    Ok((
        LifecycleJournal(
            json!({"mode":mode,"owner_hex":hex::encode(owner.public_key().to_bytes()),"nonce":{"u64_hex":"0x434f5852454731","canonical_atom_hex":"434f5852454731","decimal_string":"18946064195602225"},"merkle_root":hex::encode(policy.root),"curried_actions_hex":policy.actions.iter().map(hex::encode).collect::<Vec<_>>(),"slot_first_curry_hex":hex::encode(&policy.slot_first),"slot_first_curry_hash":hex::encode(policy.slot_first_hash),"genesis":coin(genesis),"launcher":coin(launcher),"controller_progress":[{"name":"eve","coin":coin(eve.coin),"state":state(eve.state)},{"name":"c1","coin":coin(c1.coin),"state":state(c1.state)},{"name":"c2","coin":coin(c2.coin),"state":state(c2.state)},{"name":"c3","coin":coin(c3.coin),"state":state(c3.state)},{"name":"c4","coin":coin(c4.coin),"state":state(c4.state)},{"name":"c5","coin":coin(c5.coin),"state":state(c5.state)}],"slots":{"alpha0":slot_record(a0),"beta0":slot_record(b0),"alpha1":slot_record(a1),"beta1":slot_record(b1),"alpha2":slot_record(a2),"alpha3":slot_record(a3)},"attempts":ledger.attempts,"sdk_transaction_attempts":11,"accepted":accepted,"typed_rejected":5,"component_primary_vm_runs":primary_count,"scope":if mode.starts_with("rust-") {"Actual retained Rust frontend artifacts in a local SDK lifecycle. Full simulator equality after every rejected submission and exact primary outputs. No new frontend execution by this producer, BLS proof, live wallet, general security or whole issue14 completion."} else {"Reference-only local SDK prototype. Full simulator equality after every rejected actual submission; primary outputs checked independently. No Rust frontend, BLS proof, live wallet, general uniqueness/security proof or whole issue14 completion."}}),
        ),
        counts,
    ))
}
