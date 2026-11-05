//! Local SDK observations for condition-omission variants of our own teaching policies.
//! Callers first admit complete original/control Cargo bundles and source recipes.
use super::{REFERENCES, fixtures, lifecycle, wire::*};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{SecretKey, sign_transaction};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryControlSide {
    Alpha,
    Beta,
}
impl RegistryControlSide {
    fn index(self) -> usize {
        match self {
            Self::Alpha => 0,
            Self::Beta => 1,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Beta => "beta",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryControlKind {
    DropSend,
    DropCreate,
}
impl RegistryControlKind {
    fn name(self) -> &'static str {
        match self {
            Self::DropSend => "drop_send",
            Self::DropCreate => "drop_create",
        }
    }
}

#[derive(Clone, Copy)]
pub struct RegistryControlBuild<'a> {
    pub side: RegistryControlSide,
    pub kind: RegistryControlKind,
    pub backend: &'a str,
    pub optimize_lir: bool,
    pub entry: &'a str,
    pub module: &'a [u8],
}
#[derive(Clone, Copy)]
pub struct RegistryControlReferences<'a> {
    pub initialize: &'a [u8],
    pub set_alpha: &'a [u8],
    pub set_beta: &'a [u8],
}
impl<'a> RegistryControlReferences<'a> {
    fn modules(self) -> [&'a [u8]; 3] {
        [self.initialize, self.set_alpha, self.set_beta]
    }
}
#[derive(Debug, Serialize)]
pub struct RegistryControlJournal {
    pub side: RegistryControlSide,
    pub kind: RegistryControlKind,
    pub backend: String,
    pub optimize_lir: bool,
    pub entry: String,
    pub artifact: Value,
    pub setup: Vec<Value>,
    pub mutation: Value,
    pub policy: Value,
    pub postconditions: Value,
}
#[derive(Debug, Serialize)]
pub struct RegistrySourceControlRecord {
    pub schema_version: u32,
    pub protocol: &'static str,
    pub reference_artifacts: Vec<Value>,
    pub groups: Vec<RegistryControlJournal>,
    pub counts: Value,
    pub scope: &'static str,
}

fn artifact(module: &[u8]) -> Result<Value> {
    Ok(
        json!({"bytes":module.len(),"blob_sha256":hex::encode(Sha256::digest(module)),"module_tree_hash":hex::encode(hash(module)?)}),
    )
}

/// Supply exactly alpha/drop-send off/on, alpha/drop-create off/on,
/// beta/drop-send off/on, beta/drop-create off/on. No destination paths enter here.
pub fn validate_source_controls<'a>(
    builds: &[RegistryControlBuild<'a>],
    references: RegistryControlReferences<'a>,
) -> Result<RegistrySourceControlRecord> {
    ensure!(
        builds.len() == 8,
        "registry control inventory needs exactly eight builds"
    );
    let order = [
        (RegistryControlSide::Alpha, RegistryControlKind::DropSend),
        (RegistryControlSide::Alpha, RegistryControlKind::DropCreate),
        (RegistryControlSide::Beta, RegistryControlKind::DropSend),
        (RegistryControlSide::Beta, RegistryControlKind::DropCreate),
    ];
    let mut remaining = 9 * LIMIT;
    for (index, build) in builds.iter().enumerate() {
        let (side, kind) = order[index / 2];
        let entry = format!(
            "chialisp_oxide_registry_controls::{}_{}::puzzle",
            side.name(),
            kind.name()
        );
        ensure!(
            build.side == side
                && build.kind == kind
                && build.backend == "rust_mir"
                && build.optimize_lir == (index % 2 == 1)
                && build.entry == entry,
            "exact ordered registry control identity/profile"
        );
    }
    // Charge every module before inspecting, copying or selecting any module.
    for module in builds.iter().map(|b| b.module).chain(references.modules()) {
        ensure!(
            !module.is_empty() && module.len() <= LIMIT && module.len() <= remaining,
            "registry control complete module inventory precharge"
        );
        remaining -= module.len();
    }
    for module in builds.iter().map(|b| b.module).chain(references.modules()) {
        admit(module)?;
    }
    let mut reference_artifacts = Vec::new();
    for (source, module) in REFERENCES.iter().zip(references.modules()) {
        let compiled = chialisp_oxide_reference::compile(source)?;
        ensure!(
            compiled == module,
            "registry controls bind independently compiled Chialisp partners"
        );
        reference_artifacts.push(json!({"artifact":artifact(module)?,"source_sha256":hex::encode(Sha256::digest(source.as_bytes()))}));
    }
    let owner = SecretKey::from_seed(&fixtures::OWNER_SEED);
    ensure!(
        hex::encode(owner.public_key().to_bytes()) == fixtures::OWNER_PUBLIC_KEY_HEX,
        "deterministic public simulator owner"
    );
    let mut groups = Vec::new();
    for build in builds {
        groups.push(run(*build, references, &owner)?);
    }
    let setup_attempts = groups.iter().map(|g| g.setup.len()).sum::<usize>();
    let setup_primary = groups
        .iter()
        .flat_map(|g| &g.setup)
        .map(|r| r["primary_vm"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    let mutation_primary = groups
        .iter()
        .map(|g| g.mutation["primary_vm"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    ensure!(
        setup_attempts == 16 && setup_primary == 24 && mutation_primary == 12,
        "actual setup and mutation observation counts"
    );
    Ok(RegistrySourceControlRecord {
        schema_version: 1,
        protocol: "chialisp-oxide-registry-source-controls/v1",
        reference_artifacts,
        counts: json!({"groups":groups.len(),"setup_sdk_attempts":setup_attempts,"setup_primary_observations":setup_primary,"mutation_sdk_attempts":groups.len(),"mutation_primary_observations":mutation_primary,"accepted_sdk_attempts":setup_attempts+groups.len()}),
        groups,
        scope: "Local own-root SDK observations of deliberately omitted returned conditions in the original teaching actions. All partner modules are independently compiled Chialisp. Native observations and template VM replays belong to the caller's separate capture. Two-condition mutant outputs intentionally differ from the original output ABI; inputs and their source recipes are admitted separately. Repeats add no coverage. No live funds, BLS/security proof, authenticated execution, compiler preservation or general registry claim.",
    })
}

fn run<'a>(
    build: RegistryControlBuild<'a>,
    references: RegistryControlReferences<'a>,
    owner: &SecretKey,
) -> Result<RegistryControlJournal> {
    let side = build.side.index();
    let mut templates = references.modules();
    templates[side + 1] = build.module;
    // start constructs this selected action set/root before the first funding submission.
    let lifecycle::Start {
        mut ledger,
        policy,
        genesis,
        launcher,
        eve,
        c1,
        a0,
        b0,
    } = lifecycle::start(templates, owner)?;
    let old_slot = if side == 0 { a0 } else { b0 };
    let peer_slot = if side == 0 { b0 } else { a0 };
    const OLD_VALUE: [u8; 32] = [0; 32];
    const NEW_VALUE: [u8; 32] = [0x11; 32];
    let next = c1.state.set(side, &NEW_VALUE);
    let controller =
        lifecycle::controller_spend(&policy, c1, side + 1, &OLD_VALUE, &NEW_VALUE, genesis)?;
    let mut expected = lifecycle::controller_conditions(
        &policy,
        c1,
        next,
        side + 1,
        &OLD_VALUE,
        &NEW_VALUE,
        owner,
    )?;
    ensure!(
        expected.len() == 6,
        "original controller plus three action conditions"
    );
    let omitted = expected.remove(if build.kind == RegistryControlKind::DropSend {
        3
    } else {
        4
    });
    let mut outputs = vec![list(&expected)?];
    let mut spends = vec![controller];
    if build.kind == RegistryControlKind::DropCreate {
        spends.push(lifecycle::slot_spend(&policy, old_slot, c1)?);
        outputs.push(lifecycle::slot_output(old_slot, c1)?);
    }
    let signature = sign_transaction(&spends, std::slice::from_ref(owner))?;
    ledger.submit(
        "selected_condition_omitted",
        spends,
        signature,
        outputs,
        None,
    )?;
    ensure!(
        ledger.attempts.len() == 3,
        "funding, initialization, selected update only"
    );
    if build.kind == RegistryControlKind::DropSend {
        ledger.unspent(old_slot.coin)?;
    } else {
        ledger.spent(old_slot.coin)?;
    }
    ledger.unspent(peer_slot.coin)?;
    let replacement = policy.slot(entry_hash(side, &NEW_VALUE))?;
    let mut expected_children = vec![(policy.full(next)?, 1)];
    if build.kind == RegistryControlKind::DropSend {
        expected_children.push((replacement.clone(), 0));
    }
    let children = ledger.children(c1.coin, &expected_children)?;
    let replacement_hash = hash(&replacement)?;
    let actual_replacements = ledger
        .sim
        .children(c1.coin.coin_id())
        .iter()
        .filter(|child| child.coin.puzzle_hash == replacement_hash && child.coin.amount == 0)
        .count();
    ensure!(
        actual_replacements == usize::from(build.kind == RegistryControlKind::DropSend),
        "actual replacement-slot child query"
    );
    let old_spent = ledger
        .sim
        .coin_state(old_slot.coin.coin_id())
        .ok_or_else(|| anyhow::anyhow!("actual old-slot state missing"))?
        .spent_height
        .is_some();
    let peer_unspent = ledger
        .sim
        .coin_state(peer_slot.coin.coin_id())
        .ok_or_else(|| anyhow::anyhow!("actual peer-slot state missing"))?
        .spent_height
        .is_none();
    let mutation = ledger
        .attempts
        .pop()
        .ok_or_else(|| anyhow::anyhow!("mutation observation missing"))?;
    Ok(RegistryControlJournal {
        side: build.side,
        kind: build.kind,
        backend: build.backend.to_owned(),
        optimize_lir: build.optimize_lir,
        entry: build.entry.to_owned(),
        artifact: artifact(build.module)?,
        setup: ledger.attempts,
        mutation,
        policy: json!({"merkle_root":hex::encode(policy.root),"curried_actions_hex":policy.actions.iter().map(hex::encode).collect::<Vec<_>>(),"genesis":lifecycle::coin(genesis),"launcher":lifecycle::coin(launcher),"eve":lifecycle::coin(eve.coin),"controller_before":lifecycle::coin(c1.coin),"state_before":lifecycle::state(c1.state),"state_after":lifecycle::state(next),"old_slot":lifecycle::slot_record(old_slot),"peer_slot":lifecycle::slot_record(peer_slot),"omitted_original_condition_hex":hex::encode(omitted)}),
        postconditions: json!({"old_slot_spent":old_spent,"peer_slot_unspent":peer_unspent,"replacement_puzzle_hash":hex::encode(replacement_hash),"replacement_amount":0,"replacement_child_count":actual_replacements,"controller_children":children.into_iter().map(lifecycle::coin).collect::<Vec<_>>(),"old_slot_state_query":"Simulator::coin_state(...).spent_height","replacement_query":"Simulator::children(controller_before.coin_id) filtered by exact replacement puzzle hash and amount"}),
    })
}
