//! Local counter protocol prototype. No live transactions or compiler execution.
mod wire;
use anyhow::{Context, Result, ensure};
use chia_puzzle_types::{
    EveProof, LineageProof, Proof, singleton::LauncherSolution, standard::StandardArgs,
};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ConsensusRejection, ErrorCode, SecretKey, Signature, Simulator,
    SimulatorError, SpendBundle, coin_spend, create_coin, sign_transaction,
};
use serde_json::{Value, json};
use sha2::Digest;
use std::{fs, io::Read, path::Path};
use wire::*;
const PRIMARY_COST: u64 = 250_000_000;

fn read(path: &Path) -> Result<Vec<u8>> {
    let m = fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink() && m.len() <= 16 * 1024 * 1024,
        "bounded regular input"
    );
    let mut b = Vec::new();
    fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut b)?;
    ensure!(b.len() <= 16 * 1024 * 1024, "input grew");
    Ok(b)
}
fn admit_inventory(packet: &Value) -> Result<()> {
    ensure!(
        packet["protocol"] == "chialisp-oxide-action-counter-artifacts/v2",
        "actual raw artifact protocol"
    );
    let rows = packet["artifacts"]
        .as_array()
        .context("complete artifact inventory")?;
    ensure!(rows.len() == 18, "exact18 inventory");
    let mut expected = Vec::new();
    for c in ["original", "increment_no_change", "reset_drop_signature"] {
        for m in ["off", "on"] {
            for e in ["increment", "reset"] {
                expected.push((c, m, e));
            }
        }
    }
    expected.extend([
        ("original", "chialisp", "increment"),
        ("original", "chialisp", "reset"),
    ]);
    for m in ["rue-off", "rue-on"] {
        for e in ["increment", "reset"] {
            expected.push(("original", m, e));
        }
    }
    let mut bytes_left = 16 * LIMIT;
    for (row, (c, m, e)) in rows.iter().zip(expected) {
        ensure!(
            row["id"] == format!("{c}/{m}/{e}")
                && row["control"] == c
                && row["mode"] == m
                && row["export"] == e,
            "exact ordered complete artifact identities"
        );
        let text = row["clvm_hex"].as_str().context("artifact hex")?;
        ensure!(
            !text.is_empty()
                && text.len() <= 2 * LIMIT
                && text.len().is_multiple_of(2)
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "canonical bounded hex before decoding"
        );
        ensure!(
            text.len() / 2 <= bytes_left,
            "complete artifact byte precharge"
        );
        bytes_left -= text.len() / 2;
    }
    let mut work_left = 128 * LIMIT;
    for row in rows {
        let raw = hex::decode(row["clvm_hex"].as_str().unwrap())?;
        let verified = chialisp_oxide_package::artifact::inspect_artifact(
            &raw,
            chialisp_oxide_package::artifact::Encoding::Plain,
            &chialisp_oxide_package::Limits {
                max_blob_bytes: LIMIT,
                max_expanded_bytes: LIMIT,
                max_nodes: 65536,
                max_depth: 512,
                max_atom_bytes: LIMIT,
                max_atom_heap_bytes: 2 * LIMIT,
                max_work: work_left.min(16 * LIMIT),
                ..Default::default()
            },
        )?;
        work_left = work_left
            .checked_sub(verified.stats().work as usize)
            .context("complete modeled work budget")?;
        let recorded = &row["inspection"]["identity"];
        ensure!(
            recorded["encoding"] == "clvm-legacy-plain/v1"
                && recorded["bytes"].as_u64() == Some(raw.len() as u64)
                && recorded["blob_sha256"] == verified.identity().blob_sha256.to_hex()
                && recorded["module_tree_hash"] == verified.identity().module_tree_hash.to_hex(),
            "every retained blob/tree/length identity independently checked"
        );
    }
    Ok(())
}

fn selected(packet: &Value, id: &str) -> Result<Vec<u8>> {
    let rows = packet["artifacts"]
        .as_array()
        .context("artifact inventory")?;
    ensure!(rows.len() == 18, "complete18 artifacts");
    let rows = rows.iter().filter(|r| r["id"] == id).collect::<Vec<_>>();
    ensure!(rows.len() == 1, "unique selected artifact");
    let text = rows[0]["clvm_hex"].as_str().context("artifact hex")?;
    ensure!(text.len() <= 2 * LIMIT, "artifact hex precharge");
    let raw = hex::decode(text)?;
    admit(&raw)?;
    ensure!(
        hex::encode(hash(&raw)?) == rows[0]["inspection"]["identity"]["module_tree_hash"],
        "retained artifact independent treehash"
    );
    Ok(raw)
}
fn primary(reveal: &[u8], solution: &[u8]) -> Result<Value> {
    admit(reveal)?;
    admit(solution)?;
    let mut a = clvmr::Allocator::new_limited(32 * 1024 * 1024);
    let p = clvmr::serde::node_from_bytes(&mut a, reveal)?;
    let s = clvmr::serde::node_from_bytes(&mut a, solution)?;
    Ok(
        match clvmr::run_program(
            &mut a,
            &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
            p,
            s,
            PRIMARY_COST,
        ) {
            Ok(r) => {
                let output = clvmr::serde::node_to_bytes_limit(&a, r.1, LIMIT)?;
                admit(&output)?;
                json!({"outcome":{"status":"accepted","output_hex":hex::encode(output)},"cost":r.0,"failure":null})
            }
            Err(error) => {
                let f = chialisp_oxide_vm::classify_failure(&error);
                ensure!(
                    f.is_program_rejection(),
                    "inconclusive primary resource/infrastructure: {error}"
                );
                json!({"outcome":{"status":"rejected"},"cost":null,"failure":f})
            }
        },
    )
}
fn coin(c: Coin) -> Value {
    json!({"coin_id":hex::encode(c.coin_id()),"parent_coin_id":hex::encode(c.parent_coin_info),"puzzle_hash":hex::encode(c.puzzle_hash),"amount":c.amount})
}
fn submit(
    sim: &mut Simulator,
    attempts: &mut Vec<Value>,
    name: &str,
    spends: Vec<CoinSpend>,
    sig: Signature,
) -> Result<()> {
    let executions = spends
        .iter()
        .map(|s| primary(s.puzzle_reveal.as_ref(), s.solution.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        executions
            .iter()
            .all(|e| e["outcome"]["status"] == "accepted"),
        "positive primary component rejected"
    );
    let tx = json!({"signature_hex":hex::encode(sig.to_bytes()),"spends":spends.iter().map(|s|json!({"coin":coin(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>()});
    eprintln!(
        "ACTION14_PENDING {}",
        json!({"name":name,"transaction":tx,"primary_vm":executions})
    );
    sim.new_transaction(SpendBundle::new(spends, sig))?;
    attempts.push(json!({"name":name,"transaction":tx,"primary_vm":executions,"outcome":{"status":"accepted"}}));
    Ok(())
}
fn child(sim: &Simulator, parent: Coin, reveal: &[u8]) -> Result<Coin> {
    let children = sim.children(parent.coin_id());
    ensure!(children.len() == 1, "exact unique child");
    let c = children[0].coin;
    ensure!(
        c.parent_coin_info == parent.coin_id() && c.puzzle_hash == hash(reveal)? && c.amount == 1,
        "independent actual child commitment"
    );
    ensure!(
        sim.coin_state(parent.coin_id())
            .context("parent state")?
            .spent_height
            .is_some(),
        "actual parent consumed"
    );
    Ok(c)
}
fn reject(
    sim: &mut Simulator,
    attempts: &mut Vec<Value>,
    name: &str,
    spends: Vec<CoinSpend>,
    sig: Signature,
    expected: ConsensusRejection,
) -> Result<()> {
    reject_code(sim, attempts, name, spends, sig, expected.error_code())
}
fn reject_code(
    sim: &mut Simulator,
    attempts: &mut Vec<Value>,
    name: &str,
    spends: Vec<CoinSpend>,
    sig: Signature,
    expected: ErrorCode,
) -> Result<()> {
    let executions = spends
        .iter()
        .map(|s| primary(s.puzzle_reveal.as_ref(), s.solution.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        if expected == ErrorCode::GeneratorRuntimeError {
            executions
                .iter()
                .any(|e| e["outcome"]["status"] == "rejected")
        } else {
            executions
                .iter()
                .all(|e| e["outcome"]["status"] == "accepted")
        },
        "typed consensus vs primary-resolved runtime boundary"
    );
    let tx = json!({"signature_hex":hex::encode(sig.to_bytes()),"spends":spends.iter().map(|s|json!({"coin":coin(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>()});
    let before = sim.clone();
    match sim.new_transaction(SpendBundle::new(spends, sig)) {
        Err(SimulatorError::Validation(code)) if code == expected => {}
        other => anyhow::bail!("expected exact consensus {expected:?}, observed {other:?}"),
    }
    ensure!(
        *sim == before,
        "complete in-memory simulator equality after rejection"
    );
    attempts.push(json!({"name":name,"transaction":tx,"primary_vm":executions,"outcome":{"status":"rejected","stage":if expected==ErrorCode::GeneratorRuntimeError {"runtime_umbrella"} else {"consensus"},"code":format!("{expected:?}"),"complete_state_unchanged":true}}));
    Ok(())
}

fn followups(
    sim: &Simulator,
    policy: &Policy,
    owner: &SecretKey,
    eve: Coin,
    continued: Coin,
) -> Result<Vec<Value>> {
    let proof = Proof::Lineage(LineageProof {
        parent_parent_coin_info: eve.parent_coin_info,
        parent_inner_puzzle_hash: hash(&policy.inner(0)?)?,
        parent_amount: 1,
    });
    let reveal = policy.full(1)?;
    let mut cases = Vec::new();
    for (name, actions, next_state) in [
        ("reset", vec![1], 0),
        ("increment_then_reset", vec![0, 1], 0),
        ("reset_then_increment", vec![1, 0], 1),
        ("reset_twice", vec![1, 1], 0),
    ] {
        let mut branch = sim.clone();
        let solution = policy.full_solution(proof, &actions)?;
        let spend = coin_spend(continued, &reveal, &solution)?;
        let spends = vec![spend];
        let signature = sign_transaction(&spends, std::slice::from_ref(owner))?;
        let mut attempts = Vec::new();
        reject(
            &mut branch,
            &mut attempts,
            "missing_reset_signature",
            spends.clone(),
            Signature::default(),
            ConsensusRejection::BadAggregateSignature,
        )?;
        if actions == [1, 1] {
            let single = coin_spend(continued, &reveal, &policy.full_solution(proof, &[1])?)?;
            let single_signature = sign_transaction(&[single], std::slice::from_ref(owner))?;
            ensure!(
                single_signature != signature,
                "duplicate reset signatures retain multiplicity"
            );
            reject(
                &mut branch,
                &mut attempts,
                "single_signature_for_two_resets",
                spends.clone(),
                single_signature,
                ConsensusRejection::BadAggregateSignature,
            )?;
        }
        submit(
            &mut branch,
            &mut attempts,
            name,
            spends.clone(),
            signature.clone(),
        )?;
        let next = child(&branch, continued, &policy.full(next_state)?)?;
        reject(
            &mut branch,
            &mut attempts,
            "consumed_bundle_replay",
            spends,
            signature,
            ConsensusRejection::DoubleSpend,
        )?;
        let next_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: continued.parent_coin_info,
            parent_inner_puzzle_hash: hash(&policy.inner(1)?)?,
            parent_amount: 1,
        });
        let next_spend = coin_spend(
            next,
            &policy.full(next_state)?,
            &policy.full_solution(next_proof, &[0])?,
        )?;
        submit(
            &mut branch,
            &mut attempts,
            "continue_actual_child",
            vec![next_spend],
            Signature::default(),
        )?;
        let final_child = child(&branch, next, &policy.full(next_state + 1)?)?;
        cases.push(json!({"name":name,"ledger_relation":"alternative clone of the same unspent increment_child","initial_state":1,"actions":actions,"next_state":next_state,"child":coin(next),"continued_state":next_state+1,"continued_child":coin(final_child),"attempts":attempts}));
    }
    Ok(cases)
}
fn wallet(owner: &SecretKey) -> Result<Vec<u8>> {
    let raw = curry(
        &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        &[atom(&owner.public_key().to_bytes())?],
    )?;
    ensure!(
        hash(&raw)?.to_bytes() == StandardArgs::curry_tree_hash(owner.public_key()).to_bytes(),
        "standard exact synthetic-key curry"
    );
    Ok(raw)
}
fn adversarial(
    sim: &Simulator,
    policy: &Policy,
    owner: &SecretKey,
    eve: Coin,
    continued: Coin,
) -> Result<Vec<Value>> {
    let proof = Proof::Lineage(LineageProof {
        parent_parent_coin_info: eve.parent_coin_info,
        parent_inner_puzzle_hash: hash(&policy.inner(0)?)?,
        parent_amount: 1,
    });
    let reveal = policy.full(1)?;
    let mut cases = Vec::new();
    let intent = b"chialisp-oxide/counter/reset:v1!";
    for name in [
        "wrong_key_signature",
        "wrong_message_signature",
        "wrong_coin_signature",
        "wrong_domain_signature",
    ] {
        let actions = [1];
        let solution = policy.full_solution(proof, &actions)?;
        let spends = vec![coin_spend(continued, &reveal, &solution)?];
        let mut message = intent.to_vec();
        if name == "wrong_message_signature" {
            message[0] ^= 1;
        }
        let bound_coin = if name == "wrong_coin_signature" {
            Bytes32::new([0x44; 32])
        } else {
            continued.coin_id()
        };
        let domain = if name == "wrong_domain_signature" {
            Bytes32::new([0; 32])
        } else {
            chia_sdk_types::TESTNET11_CONSTANTS.agg_sig_me_additional_data
        };
        let wrong = SecretKey::from_seed(&[0x15; 32]);
        let key = if name == "wrong_key_signature" {
            &wrong
        } else {
            owner
        };
        let signed = [message.as_slice(), bound_coin.as_ref(), domain.as_ref()].concat();
        let sig = chia_bls::sign(key, &signed);
        ensure!(
            sig != Signature::default(),
            "actual nonempty invalid signature"
        );
        let mut branch = sim.clone();
        let mut attempts = Vec::new();
        reject(
            &mut branch,
            &mut attempts,
            name,
            spends,
            sig,
            ConsensusRejection::BadAggregateSignature,
        )?;
        cases.push(json!({"name":name,"initial_state":1,"actions":actions,"expected_state":null,"child":null,"mutation":{"kind":"signature","signed_message_hex":hex::encode(&signed),"signing_public_key_hex":hex::encode(key.public_key().to_bytes()),"condition_message_hex":hex::encode(message),"bound_coin_id":hex::encode(bound_coin),"additional_data_hex":hex::encode(domain)},"attempts":attempts}));
    }
    let original = policy.full_solution(proof, &[0])?;
    let inner_prefix = [true, true, false];
    let inner_path = |suffix: &[bool]| {
        let mut p = inner_prefix.to_vec();
        p.extend(suffix);
        p
    };
    type WireMutation = (&'static str, Vec<bool>, Vec<u8>, Option<ErrorCode>);
    let variants: Vec<WireMutation> = vec![
        (
            "wrong_membership_sibling",
            inner_path(&[true, false, false, true, true, false]),
            atom(&[0x45; 32])?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "missing_membership_sibling",
            inner_path(&[true, false, false, true, true]),
            atom(&[])?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "first_scanned_nil_proof",
            inner_path(&[true, false, false, true]),
            atom(&[])?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "missing_action_solution",
            inner_path(&[true, true, false]),
            atom(&[])?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "wrong_three_field_lineage",
            vec![false, false],
            atom(&[0x46; 32])?,
            Some(ErrorCode::AssertMyParentIdFailed),
        ),
        (
            "wrong_two_field_eve_proof",
            vec![false],
            list(&[atom(&[0x47; 32])?, integer(1)?])?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "claimed_odd_amount_three",
            vec![true, false],
            integer(3)?,
            Some(ErrorCode::AssertMyAmountFailed),
        ),
        (
            "claimed_even_amount_two",
            vec![true, false],
            integer(2)?,
            Some(ErrorCode::GeneratorRuntimeError),
        ),
        (
            "proof_path_high_bit_alias",
            inner_path(&[true, false, false, true, false]),
            integer(2)?,
            None,
        ),
        (
            "unused_extra_puzzle",
            inner_path(&[false, true]),
            list(&[integer(1)?])?,
            None,
        ),
        (
            "unused_improper_solution_tail",
            inner_path(&[true, true, false, true]),
            integer(1)?,
            None,
        ),
        (
            "ignored_finalizer_pair",
            inner_path(&[true, true, true]),
            pair(&atom(&[0xaa])?, &atom(&[0xbb])?)?,
            None,
        ),
    ];
    for (name, path, replacement, expected) in variants {
        let solution = rewrite(&original, &path, &replacement)?;
        let spends = vec![coin_spend(continued, &reveal, &solution)?];
        let mut branch = sim.clone();
        let mut attempts = Vec::new();
        let c = if let Some(code) = expected {
            reject_code(
                &mut branch,
                &mut attempts,
                name,
                spends,
                Signature::default(),
                code,
            )?;
            None
        } else {
            submit(
                &mut branch,
                &mut attempts,
                name,
                spends,
                Signature::default(),
            )?;
            Some(child(&branch, continued, &policy.full(2)?)?)
        };
        cases.push(json!({"name":name,"initial_state":1,"actions":[0],"expected_state":c.map(|_|2),"child":c.map(coin),"mutation":{"kind":"replace_physical_path","path":path,"replacement_hex":hex::encode(replacement)},"attempts":attempts}));
    }
    for (name, actions, state) in [
        ("repeat_increment", vec![0, 0], 3),
        ("repeat_increment_three", vec![0, 0, 0], 4),
    ] {
        let solution = policy.full_solution(proof, &actions)?;
        let spends = vec![coin_spend(continued, &reveal, &solution)?];
        let mut branch = sim.clone();
        let mut attempts = Vec::new();
        submit(
            &mut branch,
            &mut attempts,
            name,
            spends,
            Signature::default(),
        )?;
        let c = child(&branch, continued, &policy.full(state)?)?;
        cases.push(json!({"name":name,"initial_state":1,"actions":actions,"expected_state":state,"child":coin(c),"mutation":{"kind":"none"},"attempts":attempts}));
    }
    Ok(cases)
}
fn funding_solution(destination: Bytes32, announcement: Bytes32) -> Result<Vec<u8>> {
    let conditions = list(&[
        list(&[integer(51)?, atom(destination.as_ref())?, integer(1)?])?,
        list(&[integer(61)?, atom(announcement.as_ref())?])?,
    ])?;
    list(&[atom(&[])?, pair(&integer(1)?, &conditions)?, atom(&[])?])
}
fn minimal(packet: &Value, mode: &str, owner: &SecretKey) -> Result<Value> {
    let mut sim = Simulator::new();
    let wallet = wallet(owner)?;
    let genesis = create_coin(&mut sim, &wallet, 1)?;
    let launcher = Coin::new(
        genesis.coin_id(),
        Bytes32::new(chia_puzzles::SINGLETON_LAUNCHER_HASH),
        1,
    );
    let policy = Policy::new(
        selected(packet, &format!("original/{mode}/increment"))?,
        selected(packet, &format!("original/{mode}/reset"))?,
        &owner.public_key().to_bytes(),
        launcher.coin_id(),
    )?;
    let eve_reveal = policy.full(0)?;
    let launch_solution = list(&[atom(hash(&eve_reveal)?.as_ref())?, integer(1)?, atom(&[])?])?;
    let mut a = allocator();
    ensure!(
        launch_solution
            == encode(
                &mut a,
                &LauncherSolution {
                    singleton_puzzle_hash: hash(&eve_reveal)?,
                    amount: 1,
                    key_value_list: ()
                }
            )?,
        "independent launcher solution wire"
    );
    let announce = sha(&[
        launcher.coin_id().as_ref(),
        hash(&launch_solution)?.as_ref(),
    ]);
    let spends = vec![
        coin_spend(
            genesis,
            &wallet,
            &funding_solution(launcher.puzzle_hash, announce)?,
        )?,
        coin_spend(
            launcher,
            &chia_puzzles::SINGLETON_LAUNCHER,
            &launch_solution,
        )?,
    ];
    let sig = sign_transaction(&spends, std::slice::from_ref(owner))?;
    let mut attempts = Vec::new();
    submit(
        &mut sim,
        &mut attempts,
        "fund_and_launch_singleton",
        spends,
        sig,
    )?;
    let eve = child(&sim, launcher, &eve_reveal)?;
    let solution = policy.full_solution(
        Proof::Eve(EveProof {
            parent_parent_coin_info: genesis.coin_id(),
            parent_amount: 1,
        }),
        &[0],
    )?;
    let outer_solution = policy.solution(&[0])?;
    let inner = policy.inner(0)?;
    let outer = primary(&inner, &outer_solution)?;
    ensure!(
        outer["outcome"]["status"] == "accepted",
        "outer increment accepted"
    );
    let next_inner = policy.inner(1)?;
    let expected = list(&[list(&[
        integer(51)?,
        atom(hash(&next_inner)?.as_ref())?,
        integer(1)?,
        list(&[atom(launcher.coin_id().as_ref())?])?,
    ])?])?;
    ensure!(
        outer["outcome"]["output_hex"] == hex::encode(&expected),
        "independent finalizer exact new state/hash/conditions"
    );
    let spends = vec![coin_spend(eve, &eve_reveal, &solution)?];
    submit(
        &mut sim,
        &mut attempts,
        "eve_increment",
        spends,
        Signature::default(),
    )?;
    let continued = child(&sim, eve, &policy.full(1)?)?;
    let lifecycle_cases = followups(&sim, &policy, owner, eve, continued)?;
    let adversarial_cases = adversarial(&sim, &policy, owner, eve, continued)?;
    Ok(
        json!({"mode":mode,"owner_hex":hex::encode(owner.public_key().to_bytes()),"merkle_root":hex::encode(policy.root),"genesis":coin(genesis),"launcher":coin(launcher),"eve":coin(eve),"increment_child":coin(continued),"outer_components":[{"name":"increment_finalizer","reveal_hex":hex::encode(inner),"solution_hex":hex::encode(outer_solution),"execution":outer}],"attempts":attempts,"lifecycle_cases":lifecycle_cases,"adversarial_cases":adversarial_cases}),
    )
}
fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    ensure!(args.len() == 2, "artifact packet argument");
    let raw = read(Path::new(&args[1]))?;
    let packet: Value = serde_json::from_slice(&raw)?;
    admit_inventory(&packet)?;
    let owner = SecretKey::from_seed(&[0x14; 32]);
    let mut runs = Vec::new();
    for mode in ["off", "on", "chialisp", "rue-off", "rue-on"] {
        runs.push(minimal(&packet, mode, &owner)?);
    }
    let mut source_control_runs = Vec::new();
    for control in ["increment_no_change", "reset_drop_signature"] {
        for mode in ["off", "on"] {
            source_control_runs.push(source_control(&packet, mode, control, &owner)?);
        }
    }
    println!(
        "{}",
        json!({"protocol":"chialisp-oxide-action14-prototype/v2","scope":"Local canonical host / original counter components in five builds; no complete issue14, slots, corpus or live credit.","artifact_packet_sha256":hex::encode(sha2::Sha256::digest(&raw)),"primary_vm":{"version":"clvmr 0.21.0","flags":clvmr::chia_dialect::MEMPOOL_MODE.bits(),"max_cost":PRIMARY_COST},"sdk":{"simulator":"chia-sdk-test 0.36.0","consensus":"chia-consensus 0.36.1","vm":"clvmr 0.16.2","network":"TESTNET11","max_transaction_cost":5_500_000_000u64},"runs":runs,"source_control_runs":source_control_runs})
    );
    Ok(())
}

fn source_control(packet: &Value, mode: &str, control: &str, owner: &SecretKey) -> Result<Value> {
    let mut sim = Simulator::new();
    let wallet = wallet(owner)?;
    let genesis = create_coin(&mut sim, &wallet, 1)?;
    let launcher = Coin::new(
        genesis.coin_id(),
        Bytes32::new(chia_puzzles::SINGLETON_LAUNCHER_HASH),
        1,
    );
    let identity = control == "increment_no_change";
    let increment_id = if identity {
        format!("{control}/{mode}/increment")
    } else {
        "original/chialisp/increment".to_owned()
    };
    let reset_id = if identity {
        "original/chialisp/reset".to_owned()
    } else {
        format!("{control}/{mode}/reset")
    };
    let policy = Policy::new(
        selected(packet, &increment_id)?,
        selected(packet, &reset_id)?,
        &owner.public_key().to_bytes(),
        launcher.coin_id(),
    )?;
    let initial_state = 5;
    let eve_reveal = policy.full(initial_state)?;
    let launch_solution = list(&[atom(hash(&eve_reveal)?.as_ref())?, integer(1)?, atom(&[])?])?;
    let announce = sha(&[
        launcher.coin_id().as_ref(),
        hash(&launch_solution)?.as_ref(),
    ]);
    let spends = vec![
        coin_spend(
            genesis,
            &wallet,
            &funding_solution(launcher.puzzle_hash, announce)?,
        )?,
        coin_spend(
            launcher,
            &chia_puzzles::SINGLETON_LAUNCHER,
            &launch_solution,
        )?,
    ];
    let sig = sign_transaction(&spends, std::slice::from_ref(owner))?;
    let mut attempts = Vec::new();
    submit(
        &mut sim,
        &mut attempts,
        "fund_and_launch_singleton",
        spends,
        sig,
    )?;
    let eve = child(&sim, launcher, &eve_reveal)?;
    let action = if identity { 0 } else { 1 };
    let observed_state = if identity { 5 } else { 0 };
    let solution = policy.full_solution(
        Proof::Eve(EveProof {
            parent_parent_coin_info: genesis.coin_id(),
            parent_amount: 1,
        }),
        &[action],
    )?;
    let spends = vec![coin_spend(eve, &eve_reveal, &solution)?];
    submit(
        &mut sim,
        &mut attempts,
        "mutant_unsigned_transition",
        spends,
        Signature::default(),
    )?;
    let c = child(&sim, eve, &policy.full(observed_state)?)?;
    Ok(
        json!({"control":control,"mode":mode,"companion_mode":"chialisp","owner_hex":hex::encode(owner.public_key().to_bytes()),"merkle_root":hex::encode(policy.root),"genesis":coin(genesis),"launcher":coin(launcher),"eve":coin(eve),"initial_state":initial_state,"actions":[action],"observed_state":observed_state,"policy_increment_artifact_id":increment_id,"policy_reset_artifact_id":reset_id,"original_policy_expected_state":if identity {6} else {0},"original_policy_requires_signature":!identity,"observed_requires_signature":false,"attempts":attempts,"child":coin(c)}),
    )
}
