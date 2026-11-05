//! Actual protocol admission for an opaque, freshly checked Cargo material.
//! Existing 15-attempt corpus controls remain a separately scoped lane. The new
//! closed plans additionally execute their own four-step funding/mint/transfer/
//! return chain with real fixture BLS and the fixed100M CAT host ceiling.
use super::*;
use anyhow::{Result as AnyResult, ensure};
use chia_bls::{SecretKey, master_to_wallet_unhardened};
use chia_puzzle_types::DeriveSynthetic;
use chialisp_oxide_simulator::{Simulator, sign_transaction};
use serde::Serialize;
use serde_json::{Value, json};

// Public deterministic fixture data, never wallet credentials or live inputs.
const OWNER_SEED: [u8; 32] = [0x39; 32];
const RECIPIENT_SEED: [u8; 32] = [0x58; 32];
const BASELINE_OWNER_SEED: [u8; 32] = [0x63; 32];
const BASELINE_RECIPIENT_SEED: [u8; 32] = [0x64; 32];

#[derive(Serialize)]
pub(crate) struct CatSdkCheckReport {
    protocol: &'static str,
    source_identity: String,
    scope: &'static str,
    primary_vm_limit: u64,
    closed_cat_total_limit: u64,
    lanes: Vec<CatSdkLaneCheck>,
    sdk_transaction_attempts: usize,
    accepted: usize,
    specific_consensus_rejections: usize,
    runtime_umbrella_results: usize,
}
#[derive(Serialize)]
struct CatSdkLaneCheck {
    lane: &'static str,
    module_tree_hash: String,
    corpus_baseline: Value,
    corpus_baseline_scope: &'static str,
    closed_mint_intent: Value,
    closed_plan_attempts: Vec<Value>,
    closed_plan_scope: &'static str,
}

/// No bytes, report or caller assertions can construct source authority through
/// this API. The material's private constructor belongs solely to the actual
/// Cargo/native/source coordinator. Candidate capabilities never escape this
/// function; only one selected Rust authority returns after all three protocol
/// lanes and the final source/tool consistency check have succeeded.
pub(crate) fn certify_fresh_source(
    material: crate::fresh::cat::CheckedCatSourceMaterial,
    selected: CatMode,
) -> AnyResult<(CatTailAuthority, CatSdkCheckReport)> {
    material.require_current()?;
    let mut selected_authority = None;
    let mut lanes = Vec::new();
    for (label, mode) in [
        ("rust_off", Some(CatMode::RustOff)),
        ("rust_on", Some(CatMode::RustOn)),
        ("chialisp_reference", None),
    ] {
        let candidate = CatTailAuthority::checked_candidate(&material, mode)?;
        lanes.push(check_candidate(&candidate, label)?);
        if mode == Some(selected) {
            selected_authority = Some(candidate);
        }
    }
    material.require_current()?;
    let selected_authority =
        selected_authority.ok_or_else(|| anyhow::anyhow!("selected Rust CAT authority missing"))?;
    ensure!(
        selected_authority.mode() == Some(selected),
        "reference probe cannot become selected Rust authority"
    );
    let report = CatSdkCheckReport {
        protocol: "chialisp-oxide-fresh-cat-sdk-admission/v1",
        source_identity: hex::encode(material.source_identity()),
        scope: "Actual fixture SDK execution using the just-checked module bytes; no real key, network or Sage activity. Existing corpus baseline and new closed-plan lanes are separate. Runtime umbrella codes are not specific semantic rejection or resource-absence evidence; primary observations remain independently labeled. No compiler-preservation or execution-authentication claim.",
        primary_vm_limit: super::super::wallet::MAX_POLICY_VM_COST,
        closed_cat_total_limit: MAX_CAT_BUNDLE_COST,
        lanes,
        sdk_transaction_attempts: 57,
        accepted: 24,
        specific_consensus_rejections: 21,
        runtime_umbrella_results: 12,
    };
    Ok((selected_authority, report))
}
fn fixture_identity(seed: [u8; 32]) -> AnyResult<(WalletIdentity, SecretKey)> {
    let master = SecretKey::from_seed(&seed);
    let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    let hash = chia_puzzle_types::standard::StandardArgs::curry_tree_hash(synthetic.public_key())
        .to_bytes();
    Ok((
        WalletIdentity::from_master_public_key(master.public_key().to_bytes(), hash)?,
        synthetic,
    ))
}
fn sign_closed(plan: &CatSpendPlan, key: &SecretKey) -> AnyResult<VerifiedCatBundle> {
    ensure!(
        plan.required_bls().len() == 2,
        "closed fixture requires exactly two BLS conditions"
    );
    let mut signature = Signature::default();
    for requirement in plan.required_bls() {
        ensure!(
            requirement.public_key == key.public_key(),
            "closed fixture public signing role mismatch"
        );
        signature += &chia_bls::sign(key, requirement.message());
    }
    Ok(plan.verify_returned_signature(&signature)?)
}
struct FixtureTransaction<'a> {
    bundle: &'a SpendBundle,
    bytes: &'a [u8],
    transaction_id: [u8; 32],
    costs: Value,
    intent: Value,
}
fn accepted_fixture(
    simulator: &mut Simulator,
    name: &'static str,
    verified: FixtureTransaction<'_>,
    rows: &mut Vec<Value>,
) -> AnyResult<()> {
    ensure!(
        verified.bytes.len() <= MAX_CAT_BUNDLE_BYTES,
        "closed fixture bundle bound"
    );
    ensure!(
        verified.bundle.to_bytes()? == verified.bytes,
        "stored verified bundle encoding differs"
    );
    ensure!(
        verified.bundle.name().to_bytes() == verified.transaction_id,
        "stored verified transaction ID differs"
    );
    simulator.new_transaction(verified.bundle.clone())?;
    rows.push(json!({"name":name,"status":"accepted","transaction_id":hex::encode(verified.transaction_id),
        "bundle_hex":hex::encode(verified.bytes),"costs":verified.costs,"intent":verified.intent,
        "scope":"actual SDK fixture admission, not chain submission"}));
    Ok(())
}
fn coin_intent(coin: Coin) -> Value {
    json!({"coin_id":hex::encode(coin.coin_id()),
        "parent_coin_info":hex::encode(coin.parent_coin_info),
        "puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount})
}
fn normal_intent(coin: &NormalCatCoin) -> Value {
    let lineage = coin.lineage();
    json!({"coin":coin_intent(coin.coin()),"parent":coin_intent(coin.parent_coin()),
        "asset_id":hex::encode(coin.asset_id()),"inner_puzzle_hash":hex::encode(coin.inner_puzzle_hash()),
        "lineage":{"parent_parent_coin_info":hex::encode(lineage.parent_parent_coin_info),
        "parent_inner_puzzle_hash":hex::encode(lineage.parent_inner_puzzle_hash),"parent_amount":lineage.parent_amount}})
}
fn plan_intent(plan: &CatSpendPlan) -> Value {
    json!({"unsigned_bundle_hex":hex::encode(plan.unsigned_bundle_bytes()),
        "input_coins":plan.input_coins().into_iter().map(coin_intent).collect::<Vec<_>>(),
        "expected_cat":normal_intent(plan.expected_cat()),"asset_id":hex::encode(plan.asset_id()),
        "fee":plan.fee(),"fee_change":coin_intent(plan.fee_change()),"unsigned_preflight":costs(plan.costs())})
}
fn costs(costs: CatCosts) -> Value {
    json!({"total":costs.total,"execution":costs.execution,"conditions":costs.conditions,
    "charged_generator_bytes":costs.charged_generator_bytes,"cost_per_byte":costs.cost_per_byte,
    "max_cost":costs.max_cost,"height":costs.height,"bls_verified":costs.bls_verified,
    "byte_charge_scope":"plain generator minus2quote bytes; no signature bytes or backreference claim"})
}
fn check_candidate(
    authority: &CatTailAuthority,
    label: &'static str,
) -> AnyResult<CatSdkLaneCheck> {
    // This PUBLIC helper executes, rather than imports, the mature immutable
    // 15-attempt protocol lane. Its SDK5.5B simulator ceiling remains separate
    // from the explicit10M primary-VM bound supplied here and the new100M
    // closed-plan total preflight below.
    let baseline = chialisp_oxide_corpus::cat_genesis_tail::lifecycle::run_lifecycle(
        authority.module_bytes(),
        &SecretKey::from_seed(&BASELINE_OWNER_SEED),
        &SecretKey::from_seed(&BASELINE_RECIPIENT_SEED),
        super::super::wallet::MAX_POLICY_VM_COST,
    )?
    .into_json();
    let attempts = baseline["attempts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing executed corpus attempts"))?;
    ensure!(attempts.len() == 15, "exact corpus baseline attempt count");
    let mut counts = (0, 0, 0);
    for row in attempts {
        match row["outcome"]["status"].as_str() {
            Some("accepted") => counts.0 += 1,
            Some("consensus_rejected") => {
                ensure!(
                    row["outcome"]["full_simulator_equality"] == true,
                    "corpus negative full state equality"
                );
                counts.1 += 1
            }
            Some("runtime_umbrella") => {
                ensure!(
                    row["outcome"]["full_simulator_equality"] == true,
                    "corpus umbrella full state equality"
                );
                counts.2 += 1
            }
            _ => anyhow::bail!("unknown executed corpus outcome"),
        }
    }
    ensure!(counts == (4, 7, 4), "fixed corpus outcome groups changed");
    let (owner, key) = fixture_identity(OWNER_SEED)?;
    let (recipient, recipient_key) = fixture_identity(RECIPIENT_SEED)?;
    let network = Testnet11Network::from_provider_labels("testnet11", "txch")?;
    let genesis = Coin::new(
        [0xa3; 32].into(),
        owner.wallet_puzzle_hash().into(),
        1_000_000_000_000,
    );
    let mint = CatMintPlan::new(
        authority,
        owner,
        genesis,
        owner.wallet_puzzle_hash(),
        CatFees::new(DEFAULT_CAT_FEE, DEFAULT_CAT_FEE)?,
        0,
        &network,
    )?;
    let closed_mint_intent = json!({"genesis":coin_intent(genesis),
        "funded_eve":coin_intent(mint.eve_coin()),"asset_id":hex::encode(mint.asset_id()),
        "tail_reveal_hex":hex::encode(mint.tail_reveal()),
        "expected_post_eve_cat":normal_intent(mint.expected_post_eve_cat()),
        "funding_fee":DEFAULT_CAT_FEE,"issuance_fee":DEFAULT_CAT_FEE});
    let funding = mint.funding_plan();
    let mut simulator = Simulator::new();
    simulator.insert_coin(genesis);
    let own_unsigned = SpendBundle::from_bytes(funding.unsigned_bundle_bytes())?;
    let signature = sign_transaction(&own_unsigned.coin_spends, std::slice::from_ref(&key))?;
    let signed = SpendBundle::new(own_unsigned.coin_spends, signature).to_bytes()?;
    let funded = funding.verify_signed_bundle(&signed)?;
    let mut rows = Vec::new();
    accepted_fixture(
        &mut simulator,
        "fund_exact_eve",
        FixtureTransaction {
            bundle: funded.bundle(),
            bytes: funded.bundle_bytes(),
            transaction_id: funded.transaction_id(),
            costs: json!({"total":funded.cost(),"max_cost":super::super::wallet::MAX_WALLET_BUNDLE_COST,"scope":"standardwallet preflight; separate25M ceiling"}),
            intent: json!({"input":coin_intent(genesis),"expected_eve":coin_intent(mint.eve_coin()),"fee":DEFAULT_CAT_FEE}),
        },
        &mut rows,
    )?;
    ensure!(
        simulator
            .children(genesis.coin_id())
            .iter()
            .any(|r| r.coin == mint.eve_coin()),
        "exact funded Eve absent"
    );
    let issued = sign_closed(mint.issuance_plan(), &key)?;
    accepted_fixture(
        &mut simulator,
        "issue_normal_cat_with_recipient_hint",
        FixtureTransaction {
            bundle: issued.bundle(),
            bytes: issued.bundle_bytes(),
            transaction_id: issued.transaction_id(),
            costs: costs(issued.costs()),
            intent: plan_intent(mint.issuance_plan()),
        },
        &mut rows,
    )?;
    ensure!(
        simulator
            .children(mint.eve_coin().coin_id())
            .iter()
            .any(|r| r.coin == issued.normal_cat().coin()),
        "exact normal CAT child absent"
    );
    let transfer = CatTransferPlan::new(
        authority,
        issued.normal_cat(),
        owner,
        recipient.wallet_puzzle_hash(),
        issued.fee_change(),
        DEFAULT_CAT_FEE,
        0,
        &network,
    )?;
    let sent = sign_closed(transfer.spend_plan(), &key)?;
    ensure!(
        normal_intent(sent.normal_cat()) == normal_intent(transfer.expected_cat()),
        "exact planned transfer child differs"
    );
    accepted_fixture(
        &mut simulator,
        "transfer_whole1000",
        FixtureTransaction {
            bundle: sent.bundle(),
            bytes: sent.bundle_bytes(),
            transaction_id: sent.transaction_id(),
            costs: costs(sent.costs()),
            intent: plan_intent(transfer.spend_plan()),
        },
        &mut rows,
    )?;
    ensure!(
        sent.asset_id() == issued.asset_id()
            && sent.normal_cat().inner_puzzle_hash() == recipient.wallet_puzzle_hash(),
        "transferred CAT identity changed"
    );
    let recipient_fee = Coin::new(
        [0xa4; 32].into(),
        recipient.wallet_puzzle_hash().into(),
        1_000_000,
    );
    simulator.insert_coin(recipient_fee);
    let back = CatTransferPlan::new(
        authority,
        sent.normal_cat(),
        recipient,
        owner.wallet_puzzle_hash(),
        recipient_fee,
        DEFAULT_CAT_FEE,
        0,
        &network,
    )?;
    let returned = sign_closed(back.spend_plan(), &recipient_key)?;
    ensure!(
        normal_intent(returned.normal_cat()) == normal_intent(back.expected_cat()),
        "exact planned return child differs"
    );
    accepted_fixture(
        &mut simulator,
        "return_whole1000",
        FixtureTransaction {
            bundle: returned.bundle(),
            bytes: returned.bundle_bytes(),
            transaction_id: returned.transaction_id(),
            costs: costs(returned.costs()),
            intent: plan_intent(back.spend_plan()),
        },
        &mut rows,
    )?;
    ensure!(
        returned.asset_id() == issued.asset_id()
            && returned.normal_cat().inner_puzzle_hash() == owner.wallet_puzzle_hash(),
        "returned CAT identity changed"
    );
    ensure!(rows.len() == 4, "closed plan attempt count");
    Ok(CatSdkLaneCheck {
        lane: label,
        module_tree_hash: hex::encode(authority.module_hash()),
        corpus_baseline: baseline,
        corpus_baseline_scope: "15 actual SDK attempts using PUBLIC existing corpus lifecycle helper; primaryVM10M, SDK simulator5.5B separate from newclosed100M; no fixture history rewritten",
        closed_mint_intent,
        closed_plan_attempts: rows,
        closed_plan_scope: "four actual closed funding/mint/transfer/return SDK attempts; synthetic wallet path, exact recipient first hints, samewallet feechange, complete BLS/consensus100M CAT preflight",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executed_canonical_probe_keeps_baseline_and_closed_lanes_separate() {
        let authority =
            CatTailAuthority::fixture_reference(&chia_puzzles::GENESIS_BY_COIN_ID).unwrap();
        let checked = check_candidate(&authority, "canonical_fixture_only").unwrap();
        assert_eq!(
            checked.corpus_baseline["attempts"]
                .as_array()
                .unwrap()
                .len(),
            15
        );
        assert_eq!(checked.closed_plan_attempts.len(), 4);
        assert!(
            checked
                .closed_plan_attempts
                .iter()
                .all(|row| row["status"] == "accepted")
        );
        // No actual Cargo material or fresh production authority is constructed
        // by this fixture check, even though the SDK calls genuinely execute.
        assert!(authority.source_identity().is_none());
    }
}
