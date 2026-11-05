//! Finite canonical singleton composition; not a Rust port of the top layer.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, Simulator, SimulatorError,
    SpendBundle, coin_spend, program, puzzle_hash, sign_transaction,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const PROTOCOL: &str = "chia-consensus-0.36.1/p2-singleton-v1";

#[path = "p2_singleton/codec.rs"]
mod codec;
#[path = "p2_singleton/modules.rs"]
mod modules;
use modules::Modules;

fn digest(parts: &[&[u8]]) -> Bytes32 {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
    }
    Bytes32::new(hash.finalize().into())
}

fn coin_json(coin: Coin) -> Value {
    json!({"id":hex::encode(coin.coin_id()),"parent_id":hex::encode(coin.parent_coin_info),"puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount})
}

fn bundle_json(bundle: &SpendBundle) -> Value {
    json!({"signature_hex":hex::encode(bundle.aggregated_signature.to_bytes()),"spends":bundle.coin_spends.iter().map(|s|json!({"coin":coin_json(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>()})
}

fn signed(spends: Vec<CoinSpend>, key: &SecretKey) -> Result<SpendBundle> {
    let signature = sign_transaction(&spends, std::slice::from_ref(key))?;
    ensure!(
        signature != Signature::default(),
        "signed policy produced no signature"
    );
    Ok(SpendBundle::new(spends, signature))
}

fn reject(
    simulator: &mut Simulator,
    name: &str,
    bundle: SpendBundle,
    expected: ErrorCode,
) -> Result<Value> {
    let before = simulator.clone();
    let transaction = bundle_json(&bundle);
    let result = simulator.new_transaction(bundle);
    let Err(SimulatorError::Validation(observed)) = result else {
        anyhow::bail!("{name}: expected consensus {expected:?}, got {result:?}")
    };
    ensure!(
        observed == expected,
        "{name}: expected consensus {expected:?}, got {observed:?}"
    );
    ensure!(
        *simulator == before,
        "{name}: rejected transaction mutated complete simulator state"
    );
    Ok(
        json!({"name":name,"stage":"actual_consensus_submission","expected":format!("{expected:?}"),"observed":format!("{observed:?}"),"full_simulator_equality":true,"transaction":transaction,"consensus":{"stage":"consensus","protocol":PROTOCOL,"status":"rejected","code":format!("{observed:?}"),"semantic_rejection_credit":true}}),
    )
}

#[derive(Clone)]
struct Joint {
    p2_coin: Coin,
    p2_reveal: Vec<u8>,
    inner_hash: Bytes32,
    my_id: Bytes32,
    puzzle_message: Option<Bytes32>,
    coin_binding: Option<Bytes32>,
    recipient: Bytes32,
    output_amount: u64,
    claimed_amount: u64,
}

struct Fixture {
    simulator: Simulator,
    key: SecretKey,
    funding: Coin,
    launcher: Coin,
    eve: Coin,
    p2_coin: Coin,
    p2_reveal: Vec<u8>,
    singleton_reveal: Vec<u8>,
    inner_hash: Bytes32,
    genesis: SpendBundle,
}

impl Fixture {
    fn new(modules: &Modules, p2_module: &[u8]) -> Result<Self> {
        let mut simulator = Simulator::new();
        let owner = simulator.bls(3);
        let standard = codec::curry(&modules.standard, &[codec::atom(&owner.pk.to_bytes())?])?;
        let inner_hash = puzzle_hash(&standard)?;
        ensure!(
            inner_hash == owner.puzzle_hash,
            "hand-built StandardLayer curry differs from SDK hash"
        );
        let launcher = Coin::new(owner.coin.coin_id(), modules.launcher_hash, 1);
        let structure = codec::assemble(&format!(
            "(0x{} . (0x{} . 0x{}))",
            hex::encode(modules.singleton_hash),
            hex::encode(launcher.coin_id()),
            hex::encode(modules.launcher_hash)
        ))?;
        let singleton_reveal = codec::curry(&modules.singleton, &[structure, standard.clone()])?;
        let singleton_hash = puzzle_hash(&singleton_reveal)?;
        let p2_reveal = codec::curry(
            p2_module,
            &[
                codec::atom(modules.singleton_hash.as_ref())?,
                codec::atom(launcher.coin_id().as_ref())?,
                codec::atom(modules.launcher_hash.as_ref())?,
            ],
        )?;
        let p2_hash = puzzle_hash(&p2_reveal)?;
        let p2_coin = Coin::new(owner.coin.coin_id(), p2_hash, 2);
        let eve = Coin::new(launcher.coin_id(), singleton_hash, 1);
        let launch_solution =
            codec::assemble(&format!("(0x{} 1 ())", hex::encode(singleton_hash)))?;
        let launch_commitment = chialisp_oxide_simulator::puzzle_hash(&launch_solution)?;
        let announcement = digest(&[launcher.coin_id().as_ref(), launch_commitment.as_ref()]);
        let conditions = format!(
            "((51 0x{} 1) (51 0x{} 2) (61 0x{}))",
            hex::encode(modules.launcher_hash),
            hex::encode(p2_hash),
            hex::encode(announcement)
        );
        let genesis = signed(
            vec![
                coin_spend(
                    owner.coin,
                    &standard,
                    &codec::standard_solution(&conditions)?,
                )?,
                coin_spend(launcher, &modules.launcher, &launch_solution)?,
            ],
            &owner.sk,
        )?;
        simulator.new_transaction(genesis.clone())?;
        ensure!(
            simulator
                .coin_state(eve.coin_id())
                .is_some_and(|s| s.coin == eve && s.spent_height.is_none()),
            "canonical launcher did not create exact Eve"
        );
        ensure!(
            simulator
                .coin_state(p2_coin.coin_id())
                .is_some_and(|s| s.coin == p2_coin && s.spent_height.is_none()),
            "funding did not create exact 2-mojo p2 coin"
        );
        ensure!(
            simulator.children(launcher.coin_id()).len() == 1,
            "launcher child count changed"
        );
        Ok(Self {
            simulator,
            key: owner.sk,
            funding: owner.coin,
            launcher,
            eve,
            p2_coin,
            p2_reveal,
            singleton_reveal,
            inner_hash,
            genesis,
        })
    }

    fn request(&self) -> Joint {
        Joint {
            p2_coin: self.p2_coin,
            p2_reveal: self.p2_reveal.clone(),
            inner_hash: self.inner_hash,
            my_id: self.p2_coin.coin_id(),
            puzzle_message: Some(self.p2_coin.coin_id()),
            coin_binding: Some(digest(&[self.p2_coin.coin_id().as_ref(), b"$"])),
            recipient: self.inner_hash,
            output_amount: 3,
            claimed_amount: 1,
        }
    }

    fn singleton_spend(
        &self,
        coin: Coin,
        lineage: &str,
        amount: u64,
        conditions: &str,
    ) -> Result<CoinSpend> {
        let inner = codec::standard_solution(conditions)?;
        let mut allocator = clvmr::Allocator::new();
        let inner = clvmr::serde::node_from_bytes(&mut allocator, &inner)?;
        let inner_text = rue_clvm::disassemble_value(&allocator, inner);
        let solution = codec::assemble(&format!("({lineage} {amount} {inner_text})"))?;
        coin_spend(coin, &self.singleton_reveal, &solution)
    }

    fn spends(&self, request: &Joint) -> Result<Vec<CoinSpend>> {
        let mut conditions = vec![format!(
            "(51 0x{} {})",
            hex::encode(request.recipient),
            request.output_amount
        )];
        if let Some(message) = request.puzzle_message {
            conditions.push(format!("(62 0x{})", hex::encode(message)));
        }
        if let Some(binding) = request.coin_binding {
            conditions.push(format!("(61 0x{})", hex::encode(binding)));
        }
        let conditions = format!("({})", conditions.join(" "));
        let lineage = format!("(0x{} 1)", hex::encode(self.funding.coin_id()));
        let singleton =
            self.singleton_spend(self.eve, &lineage, request.claimed_amount, &conditions)?;
        let p2_solution = codec::assemble(&format!(
            "(0x{} 0x{})",
            hex::encode(request.inner_hash),
            hex::encode(request.my_id)
        ))?;
        // Construct directly so a deliberately wrong reveal reaches consensus.
        let p2 = CoinSpend::new(
            request.p2_coin,
            program(&request.p2_reveal)?,
            program(&p2_solution)?,
        );
        Ok(vec![singleton, p2])
    }
}

fn negative_cases(
    fixture: &Fixture,
    modules: &Modules,
    p2_module: &[u8],
    alternate_module: &[u8],
) -> Result<Vec<Value>> {
    let mut results = Vec::new();
    let original = fixture.request();
    let valid = signed(fixture.spends(&original)?, &fixture.key)?;
    let mut simulator = fixture.simulator.clone();
    results.push(reject(
        &mut simulator,
        "missing singleton participant",
        SpendBundle::new(vec![valid.coin_spends[1].clone()], Signature::default()),
        ErrorCode::AssertPuzzleAnnouncementFailed,
    )?);
    let mut request = original.clone();
    request.output_amount = 1;
    let singleton = fixture.spends(&request)?.remove(0);
    results.push(reject(
        &mut simulator,
        "missing p2 participant with independently balanced singleton",
        signed(vec![singleton], &fixture.key)?,
        ErrorCode::AssertCoinAnnouncementFailed,
    )?);
    let mut request = original.clone();
    request.inner_hash = Bytes32::new([0x91; 32]);
    results.push(reject(
        &mut simulator,
        "wrong inner-puzzle hash",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertPuzzleAnnouncementFailed,
    )?);
    let mut request = original.clone();
    request.my_id = Bytes32::new([0x92; 32]);
    request.puzzle_message = Some(request.my_id);
    results.push(reject(
        &mut simulator,
        "wrong my_id with announcement otherwise matched",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertMyCoinIdFailed,
    )?);
    let other = simulator.new_coin(fixture.p2_coin.puzzle_hash, 2);
    let mut request = original.clone();
    request.my_id = other.coin_id();
    request.puzzle_message = Some(other.coin_id());
    results.push(reject(
        &mut simulator,
        "another p2 coin ID cannot identify this input",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertMyCoinIdFailed,
    )?);
    let mut request = original.clone();
    request.puzzle_message = Some(Bytes32::new([0x93; 32]));
    results.push(reject(
        &mut simulator,
        "wrong singleton puzzle-announcement message",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertPuzzleAnnouncementFailed,
    )?);
    let mut request = original.clone();
    request.coin_binding = Some(digest(&[fixture.p2_coin.coin_id().as_ref(), b"x"]));
    results.push(reject(
        &mut simulator,
        "wrong dollar announcement binding",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertCoinAnnouncementFailed,
    )?);
    let mut request = original.clone();
    request.coin_binding = Some(digest(&[other.coin_id().as_ref(), b"$"]));
    results.push(reject(
        &mut simulator,
        "wrong p2 issuer in reciprocal coin announcement",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertCoinAnnouncementFailed,
    )?);
    for (name, mod_hash, launcher_id, launcher_hash) in [
        (
            "wrong singleton module identity",
            Bytes32::new([0x94; 32]),
            fixture.launcher.coin_id(),
            modules.launcher_hash,
        ),
        (
            "wrong launcher identity",
            modules.singleton_hash,
            Bytes32::new([0x95; 32]),
            modules.launcher_hash,
        ),
        (
            "wrong launcher puzzle hash",
            modules.singleton_hash,
            fixture.launcher.coin_id(),
            Bytes32::new([0x96; 32]),
        ),
    ] {
        let reveal = codec::curry(
            p2_module,
            &[
                codec::atom(mod_hash.as_ref())?,
                codec::atom(launcher_id.as_ref())?,
                codec::atom(launcher_hash.as_ref())?,
            ],
        )?;
        let coin = simulator.new_coin(puzzle_hash(&reveal)?, 2);
        let mut request = original.clone();
        request.p2_reveal = reveal;
        request.p2_coin = coin;
        request.my_id = coin.coin_id();
        request.puzzle_message = Some(coin.coin_id());
        request.coin_binding = Some(digest(&[coin.coin_id().as_ref(), b"$"]));
        results.push(reject(
            &mut simulator,
            name,
            signed(fixture.spends(&request)?, &fixture.key)?,
            ErrorCode::AssertPuzzleAnnouncementFailed,
        )?);
    }
    let mut request = original.clone();
    request.puzzle_message = None;
    let extra_reveal = chialisp_oxide_reference::compile(&format!(
        "(mod () (list (list 62 0x{})))",
        hex::encode(fixture.p2_coin.coin_id())
    ))?;
    let extra = simulator.new_coin(puzzle_hash(&extra_reveal)?, 1);
    let mut spends = fixture.spends(&request)?;
    spends.push(coin_spend(extra, &extra_reveal, &[0x80])?);
    results.push(reject(
        &mut simulator,
        "another puzzle cannot issue the singleton announcement",
        signed(spends, &fixture.key)?,
        ErrorCode::AssertPuzzleAnnouncementFailed,
    )?);
    results.push(reject(
        &mut simulator,
        "omitted signature submitted to consensus",
        SpendBundle::new(valid.coin_spends.clone(), Signature::default()),
        ErrorCode::BadAggregateSignature,
    )?);
    ensure!(
        fixture.genesis.aggregated_signature != Signature::default(),
        "control signature is empty"
    );
    results.push(reject(
        &mut simulator,
        "nonempty genesis signature cannot authorize singleton spend",
        SpendBundle::new(
            valid.coin_spends.clone(),
            fixture.genesis.aggregated_signature.clone(),
        ),
        ErrorCode::BadAggregateSignature,
    )?);
    let mut request = original.clone();
    request.recipient = Bytes32::new([0x97; 32]);
    results.push(reject(
        &mut simulator,
        "signed continuation recipient cannot be changed",
        SpendBundle::new(
            fixture.spends(&request)?,
            valid.aggregated_signature.clone(),
        ),
        ErrorCode::BadAggregateSignature,
    )?);
    let mut request = original.clone();
    request.claimed_amount = 3;
    results.push(reject(
        &mut simulator,
        "singleton asserted input amount must match",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::AssertMyAmountFailed,
    )?);
    let alternate = codec::curry(
        alternate_module,
        &[
            codec::atom(modules.singleton_hash.as_ref())?,
            codec::atom(fixture.launcher.coin_id().as_ref())?,
            codec::atom(modules.launcher_hash.as_ref())?,
        ],
    )?;
    ensure!(
        puzzle_hash(&alternate)? != fixture.p2_coin.puzzle_hash,
        "wrong reveal control is identical"
    );
    let mut request = original;
    request.p2_reveal = alternate;
    results.push(reject(
        &mut simulator,
        "different-language reveal submitted to consensus",
        signed(fixture.spends(&request)?, &fixture.key)?,
        ErrorCode::WrongPuzzleHash,
    )?);
    Ok(results)
}

fn lifecycle(fixture: &Fixture) -> Result<Value> {
    let mut simulator = fixture.simulator.clone();
    let joint = signed(fixture.spends(&fixture.request())?, &fixture.key)?;
    simulator.new_transaction(joint.clone())?;
    let continuation = Coin::new(fixture.eve.coin_id(), fixture.eve.puzzle_hash, 3);
    let state = simulator
        .coin_state(continuation.coin_id())
        .context("missing exact 3-mojo continuation")?;
    ensure!(
        state.coin == continuation && state.spent_height.is_none(),
        "incorrect continuation state"
    );
    ensure!(
        simulator.children(fixture.eve.coin_id()).len() == 1
            && simulator.children(fixture.p2_coin.coin_id()).is_empty(),
        "joint output cardinality changed"
    );
    for coin in [fixture.eve, fixture.p2_coin] {
        ensure!(
            simulator
                .coin_state(coin.coin_id())
                .is_some_and(|s| s.spent_height.is_some()),
            "joint input not spent"
        );
    }
    let replay = reject(
        &mut simulator,
        "joint-spend replay",
        joint.clone(),
        ErrorCode::DoubleSpend,
    )?;
    let conditions = format!("((51 0x{} 3))", hex::encode(fixture.inner_hash));
    let stale = format!("(0x{} 1)", hex::encode(fixture.funding.coin_id()));
    let bad = fixture.singleton_spend(continuation, &stale, 3, &conditions)?;
    let stale_proof = reject(
        &mut simulator,
        "Eve proof cannot spend the next singleton",
        signed(vec![bad], &fixture.key)?,
        ErrorCode::AssertMyParentIdFailed,
    )?;
    let lineage = format!(
        "(0x{} 0x{} 1)",
        hex::encode(fixture.launcher.coin_id()),
        hex::encode(fixture.inner_hash)
    );
    let next = fixture.singleton_spend(continuation, &lineage, 3, &conditions)?;
    let next_bundle = signed(vec![next], &fixture.key)?;
    simulator.new_transaction(next_bundle.clone())?;
    let child = Coin::new(continuation.coin_id(), continuation.puzzle_hash, 3);
    ensure!(
        simulator
            .coin_state(child.coin_id())
            .is_some_and(|s| s.coin == child && s.spent_height.is_none()),
        "lineage spend did not create exact next singleton"
    );
    ensure!(
        simulator.children(continuation.coin_id()).len() == 1,
        "lineage child cardinality changed"
    );
    Ok(
        json!({"name":"canonical Eve joint funding and next lineage spend","funding":coin_json(fixture.funding),"launcher":coin_json(fixture.launcher),"eve":coin_json(fixture.eve),"p2":coin_json(fixture.p2_coin),"continuation":coin_json(continuation),"next_singleton":coin_json(child),"genesis":bundle_json(&fixture.genesis),"joint":bundle_json(&joint),"lineage":bundle_json(&next_bundle),"rejections":[replay,stale_proof],"scope":"Canonical top layer and signed StandardLayer are host counterparts, not Rust ports. One real Eve proof, a 1+2→3 composition, and one subsequent lineage proof are executed."}),
    )
}

pub(super) fn validate(programs: &[(&str, bool, Vec<u8>)]) -> Result<Value> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let artifacts = programs
        .iter()
        .map(|(backend, optimize, code)| {
            Ok(serde_json::to_value(
                chialisp_oxide_backend_experiment::artifact(backend, *optimize, code)?,
            )?)
        })
        .collect::<Result<Vec<Value>>>()?;
    validate_artifacts(&root, &artifacts)
}

pub(super) fn validate_artifacts(root: &Path, artifacts: &[Value]) -> Result<Value> {
    let protocol: Value = serde_json::from_str(include_str!(
        "../../chialisp-oxide-simulator/p2-singleton-rejection-protocol.json"
    ))?;
    ensure!(
        protocol["protocol"] == PROTOCOL,
        "wrong dedicated rejection protocol"
    );
    let modules = Modules::load(root)?;
    ensure!(artifacts.len() == 5, "expected five independent builds");
    let expected = [
        ("rust_mir", false),
        ("rust_mir", true),
        ("chialisp", true),
        ("rue_source", false),
        ("rue_source", true),
    ];
    for (artifact, (backend, optimize)) in artifacts.iter().zip(expected) {
        ensure!(
            artifact["backend"] == backend && artifact["optimize_lir"] == optimize,
            "unexpected protocol artifact backend or mode"
        );
    }
    let mut observations = Vec::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        let module = hex::decode(artifact["clvm_hex"].as_str().context("compiled hex")?)?;
        let alternate = hex::decode(
            artifacts[if index == 2 { 1 } else { 2 }]["clvm_hex"]
                .as_str()
                .unwrap(),
        )?;
        let fixture = Fixture::new(&modules, &module)?;
        let negatives = negative_cases(&fixture, &modules, &module, &alternate)?;
        let mut lifecycle = lifecycle(&fixture)?;
        lifecycle["receipts"] = json!(["genesis", "joint", "lineage"].iter().map(|name| {
            json!({"name":name,"stage":"actual_consensus_submission","status":"accepted","bundle_field":name})
        }).collect::<Vec<_>>());
        ensure!(
            negatives.len() == 17
                && lifecycle["rejections"]
                    .as_array()
                    .is_some_and(|r| r.len() == 2),
            "protocol attempt coverage changed"
        );
        println!(
            "{}/{}: {} exact consensus negatives plus Eve/lineage lifecycle",
            artifact["backend"],
            artifact["optimize_lir"],
            negatives.len() + 2
        );
        observations.push(json!({"backend":artifact["backend"],"optimize_lir":artifact["optimize_lir"],"module_tree_hash":artifact["tree_hash"],"curried_p2_reveal_hex":hex::encode(&fixture.p2_reveal),"curried_p2_hash":hex::encode(puzzle_hash(&fixture.p2_reveal)?),"canonical_singleton_reveal_hex":hex::encode(&fixture.singleton_reveal),"inner_hash":hex::encode(fixture.inner_hash),"negatives":negatives,"lifecycle":lifecycle}));
    }
    let report = json!({"schema_version":1,"protocol":"chialisp-oxide-p2-singleton/v1","status":"validated","rejection_protocol":PROTOCOL,"scope":"Finite five-build pay-to-singleton composition with independently reproduced canonical launcher/top-layer/StandardLayer counterparts. Exact typed consensus rejection and complete in-memory Simulator equality; nondeterministic Debug text is omitted from retained evidence, which is not a state serialization protocol.","simulator":"chia-sdk-test 0.36.0","consensus":"chia-consensus 0.36.1","ledger_VM":"clvmr 0.16.2","network":"TESTNET11","modules":modules.provenance,"observations":observations});
    Ok(report)
}
