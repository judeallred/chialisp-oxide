//! Bounded wallet-free committed-instance observations for the unchanged tutorial.
//! Five caller-selected module artifacts are exercised, never trusted by a label.
//! Public disposable fixtures only. This executable never uses a network provider.
use anyhow::{Context, Result, ensure};
use chia_puzzle_types::DeriveSynthetic;
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};
use chialisp_oxide_backend_experiment::{Execution, FailureCode, Outcome};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, Coin, CoinSpend, ConsensusRejection, NETWORK, REJECTION_PROTOCOL,
    SIMULATOR_VERSION, SecretKey, Signature, Simulator, SimulatorError, SpendBundle, VM_VERSION,
    coin_spend, create_coin, program, puzzle_hash, require_consensus_rejection, sign_transaction,
};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr,
    allocator::SExp,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};

const REQUEST_PROTOCOL: &str = "chialisp-oxide-standard-tutorial-ledger-request/v1";
const REPORT_PROTOCOL: &str = "chialisp-oxide-standard-tutorial-ledger/v1";
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_MODULE_HEX_BYTES: usize = 512 * 1024;
const MAX_DECODED_MODULE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FIXTURE_BYTES: usize = 16 * 1024;
const PRIMARY_MAX_COST: u64 = 10_000_000;
const LANES: [&str; 5] = ["rust-off", "rust-on", "chialisp", "rue-off", "rue-on"];
const PUBLIC_ORIGINAL_SEED: [u8; 32] = [7; 32];
const PUBLIC_WRONG_SEED: [u8; 32] = [8; 32];
const DESTINATION: [u8; 32] = [0x42; 32];
const ALTERNATE_DESTINATION: [u8; 32] = [0x43; 32];
const AMOUNT: u64 = 1_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    runs: Vec<Input>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    lane: String,
    module_hex: String,
}

fn load_request(path: &Path) -> Result<Request> {
    let info = path.symlink_metadata()?;
    ensure!(
        info.is_file() && !info.file_type().is_symlink(),
        "regular request file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(info.nlink() == 1, "single-link request file");
    }
    ensure!(info.len() <= MAX_REQUEST_BYTES as u64, "request precharge");
    let source = std::fs::File::open(path)?;
    let mut raw = Vec::new();
    source
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() <= MAX_REQUEST_BYTES && raw.len() as u64 == info.len(),
        "request changed beyond bounded read"
    );
    let request: Request = serde_json::from_slice(&raw)?;
    ensure!(
        request.protocol == REQUEST_PROTOCOL && request.runs.len() == LANES.len(),
        "closed five-build request"
    );
    let mut seen = BTreeSet::new();
    let mut decoded = 0usize;
    for (input, lane) in request.runs.iter().zip(LANES) {
        ensure!(
            input.lane == lane && seen.insert(input.lane.as_str()),
            "exact ordered unique build lanes"
        );
        let n = input.module_hex.len();
        ensure!(
            n > 0 && n <= MAX_MODULE_HEX_BYTES && n.is_multiple_of(2),
            "module predecode bound"
        );
        ensure!(
            input
                .module_hex
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "canonical lowercase module hex"
        );
        ensure!(
            n / 2 <= MAX_DECODED_MODULE_BYTES - decoded,
            "complete decoded module precharge"
        );
        decoded += n / 2;
    }
    Ok(request)
}

// Independent fixed-fixture wire spelling. No unbounded caller-supplied trees.
fn atom(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() < 64);
    if bytes.len() == 1 && bytes[0] < 128 {
        return bytes.to_vec();
    }
    let mut result = vec![0x80 | u8::try_from(bytes.len()).unwrap()];
    result.extend_from_slice(bytes);
    result
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut result = vec![0xff];
    result.extend_from_slice(first);
    result.extend_from_slice(rest);
    result
}
fn list(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .rev()
        .fold(vec![0x80], |tail, value| pair(value, &tail))
}
fn quote(value: &[u8]) -> Vec<u8> {
    pair(&atom(&[1]), value)
}
fn curry(module: &[u8], key: &[u8; 48]) -> Vec<u8> {
    list(&[
        atom(&[2]),
        quote(module),
        list(&[atom(&[4]), quote(&atom(key)), atom(&[1])]),
    ])
}
fn conditions(destination: &[u8], amount: &[u8]) -> Vec<u8> {
    list(&[list(&[atom(&[51]), destination.to_vec(), amount.to_vec()])])
}
fn payment(destination: &[u8; 32], amount: u64) -> Vec<u8> {
    assert!((1..=0x7fff).contains(&amount));
    let bytes = amount.to_be_bytes();
    let first = bytes.iter().position(|b| *b != 0).unwrap();
    let mut encoded = bytes[first..].to_vec();
    if encoded[0] & 0x80 != 0 {
        encoded.insert(0, 0);
    }
    conditions(&atom(destination), &atom(&encoded))
}
fn solution(original: &[u8], delegated: &[u8], inner_solution: &[u8]) -> Vec<u8> {
    let result = list(&[
        original.to_vec(),
        delegated.to_vec(),
        inner_solution.to_vec(),
    ]);
    assert!(result.len() <= MAX_FIXTURE_BYTES);
    result
}
fn hash(bytes: &[u8]) -> Result<[u8; 32]> {
    ensure!(
        bytes.len() <= MAX_DECODED_MODULE_BYTES,
        "bounded canonical program input"
    );
    // This parser rejects trailing bytes; the round trip also rejects alternate
    // atom header spellings without changing exact atom payload bytes.
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, bytes)?;
    ensure!(
        node_to_bytes(&allocator, node)? == bytes,
        "exact canonical serialized program required"
    );
    Ok(puzzle_hash(bytes)?.to_bytes())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn expected_delegated(key: &[u8; 48], delegated: &[u8], output: &[u8]) -> Result<Vec<u8>> {
    Ok(pair(
        &list(&[atom(&[50]), atom(key), atom(&hash(delegated)?)]),
        output,
    ))
}
fn final_message(message_hash: &[u8; 32], coin: &Coin, domain: &[u8; 32]) -> Vec<u8> {
    [
        message_hash.as_slice(),
        coin.coin_id().as_ref(),
        domain.as_slice(),
    ]
    .concat()
}
fn sign_program(secret: &SecretKey, delegated: &[u8], coin: &Coin) -> Result<Signature> {
    Ok(chia_bls::sign(
        secret,
        final_message(
            &hash(delegated)?,
            coin,
            &TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        ),
    ))
}
fn transaction(spends: &[CoinSpend], signature: &Signature) -> Value {
    json!({"signature_hex":hex::encode(signature.to_bytes()),"spends":spends.iter().map(|spend|json!({
        "coin":{"coin_id":hex::encode(spend.coin.coin_id()),"parent_coin_id":hex::encode(spend.coin.parent_coin_info),
        "puzzle_hash":hex::encode(spend.coin.puzzle_hash),"amount":spend.coin.amount},
        "reveal_hex":hex::encode(spend.puzzle_reveal.as_ref()),"solution_hex":hex::encode(spend.solution.as_ref())
    })).collect::<Vec<_>>()})
}
// A caller-supplied module can create a shared output DAG cheaply. Count
// expanded occurrences before serialization so that output is never trusted
// merely because the module itself had a small bounded serialized representation.
fn preflight_output(allocator: &Allocator, output: NodePtr) -> Result<()> {
    let mut pending = vec![(output, 0usize)];
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        ensure!(
            nodes < 4096 && depth <= 128,
            "inconclusive primary output node/depth resource limit"
        );
        nodes += 1;
        match allocator.sexp(node) {
            SExp::Pair(first, rest) => pending.extend([(rest, depth + 1), (first, depth + 1)]),
            SExp::Atom => {
                let length = allocator.atom(node).as_ref().len();
                ensure!(
                    length <= MAX_FIXTURE_BYTES - bytes,
                    "inconclusive primary output byte resource limit"
                );
                bytes += length;
            }
        }
    }
    Ok(())
}
fn primary(reveal: &[u8], solution: &[u8]) -> Result<Execution> {
    ensure!(
        solution.len() <= MAX_FIXTURE_BYTES,
        "bounded fixed fixture solution"
    );
    program(solution)?;
    let mut allocator = Allocator::new();
    let module = node_from_bytes(&mut allocator, reveal)?;
    let environment = node_from_bytes(&mut allocator, solution)?;
    // Same pinned primary VM/protocol as backend::execute; no output codec runs
    // before this lane's stricter expanded output preflight.
    Ok(
        match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            module,
            environment,
            PRIMARY_MAX_COST,
        ) {
            Ok(reduction) => {
                preflight_output(&allocator, reduction.1)?;
                Execution {
                    outcome: Outcome::Accepted {
                        output_hex: hex::encode(node_to_bytes(&allocator, reduction.1)?),
                    },
                    cost: Some(reduction.0),
                    error: None,
                    failure: None,
                }
            }
            Err(error) => {
                let failure = chialisp_oxide_vm::classify_failure(&error);
                Execution {
                    outcome: if failure.is_program_rejection() {
                        Outcome::Rejected
                    } else {
                        Outcome::Inconclusive
                    },
                    cost: None,
                    error: Some(format!("{error:?}")),
                    failure: Some(failure),
                }
            }
        },
    )
}
fn accepted_primary(reveal: &[u8], solution: &[u8], expected: &[u8]) -> Result<Execution> {
    let execution = primary(reveal, solution)?;
    ensure!(
        matches!(&execution.outcome, Outcome::Accepted {output_hex} if output_hex==&hex::encode(expected)),
        "primary exact complete output mismatch"
    );
    Ok(execution)
}
#[derive(Clone, Copy, Debug)]
enum ProgramFailure {
    Shape,
    Point,
    HiddenMismatch,
}
fn rejected_primary(
    lane: &str,
    reveal: &[u8],
    solution: &[u8],
    expected: ProgramFailure,
) -> Result<Execution> {
    ensure!(LANES.contains(&lane), "unknown primary fixture lane {lane}");
    let execution = primary(reveal, solution)?;
    let failure = execution.failure.context("missing primary typed failure")?;
    ensure!(
        matches!(&execution.outcome, Outcome::Rejected) && failure.is_program_rejection(),
        "resource/infrastructure cannot confirm rejection"
    );
    // Exact codes independently observed by the five-build template lane.
    // These differences remain language-specific program outcomes; no generic
    // program error, resource result or infrastructure result is substituted.
    let expected_code = match (lane, expected) {
        ("rust-off" | "rust-on", ProgramFailure::Shape) => FailureCode::Raise,
        ("chialisp" | "rue-off" | "rue-on", ProgramFailure::Shape) => FailureCode::PathIntoAtom,
        ("rue-off" | "rue-on", ProgramFailure::Point) => FailureCode::Raise,
        ("rust-off" | "rust-on" | "chialisp", ProgramFailure::Point) => {
            FailureCode::InvalidAllocArg
        }
        (_, ProgramFailure::HiddenMismatch) => FailureCode::Raise,
        _ => anyhow::bail!("unrecognized primary fixture {lane} {expected:?}"),
    };
    ensure!(
        failure.code() == expected_code,
        "primary fixture {expected:?} lane {lane}: observed {:?}, expected {expected_code:?}",
        failure.code()
    );
    Ok(execution)
}
fn reject(
    simulator: &mut Simulator,
    label: &str,
    spend: CoinSpend,
    signature: Signature,
    primary: Execution,
    expected: ConsensusRejection,
) -> Result<Value> {
    let wire = transaction(std::slice::from_ref(&spend), &signature);
    let before = simulator.clone();
    require_consensus_rejection(
        simulator.new_transaction(SpendBundle::new(vec![spend], signature)),
        expected,
    )?;
    ensure!(
        *simulator == before,
        "full simulator state changed on typed rejection"
    );
    Ok(
        json!({"name":label,"status":"consensus_rejected","code":expected.as_str(),
        "full_simulator_equality":true,"transaction":wire,"primary_vm":primary}),
    )
}
fn reject_runtime(
    simulator: &mut Simulator,
    label: &str,
    spend: CoinSpend,
    primary: Execution,
) -> Result<Value> {
    let wire = transaction(std::slice::from_ref(&spend), &Signature::default());
    let before = simulator.clone();
    ensure!(
        matches!(
            simulator.new_transaction(SpendBundle::new(vec![spend], Signature::default())),
            Err(SimulatorError::Validation(
                chialisp_oxide_simulator::ErrorCode::GeneratorRuntimeError
            ))
        ),
        "expected SDK runtime umbrella"
    );
    ensure!(
        *simulator == before,
        "full simulator state changed on runtime umbrella"
    );
    Ok(
        json!({"name":label,"status":"runtime_umbrella","code":"GeneratorRuntimeError","semantic_rejection_credit":false,
        "full_simulator_equality":true,"transaction":wire,"primary_vm":primary}),
    )
}
fn accept(
    simulator: &mut Simulator,
    label: &str,
    spend: CoinSpend,
    signature: Signature,
    primary: Execution,
    destination: &[u8; 32],
) -> Result<Value> {
    let input = spend.coin;
    let wire = transaction(std::slice::from_ref(&spend), &signature);
    simulator.new_transaction(SpendBundle::new(vec![spend], signature))?;
    let children = simulator.children(input.coin_id());
    ensure!(
        children.len() == 1
            && children[0].coin.parent_coin_info == input.coin_id()
            && children[0].coin.puzzle_hash == (*destination).into()
            && children[0].coin.amount == AMOUNT,
        "exact accepted child differs"
    );
    ensure!(
        simulator
            .coin_state(input.coin_id())
            .context("input state missing")?
            .spent_height
            .is_some(),
        "input must be consumed"
    );
    let child = children[0].coin;
    Ok(
        json!({"name":label,"status":"accepted","transaction":wire,"primary_vm":primary,
        "input_consumed":true,"child":{"coin_id":hex::encode(child.coin_id()),
        "parent_coin_id":hex::encode(child.parent_coin_info),"puzzle_hash":hex::encode(child.puzzle_hash),"amount":child.amount}}),
    )
}

fn run(input: Input) -> Result<Value> {
    let module = hex::decode(&input.module_hex)?;
    // Exact object validation and hash precede curry/VM/coin operations.
    let module_hash = hash(&module)?;
    let owner = SecretKey::from_seed(&PUBLIC_ORIGINAL_SEED);
    let wrong_owner = SecretKey::from_seed(&PUBLIC_WRONG_SEED);
    let hidden_output = payment(&DESTINATION, AMOUNT);
    let hidden_program = quote(&hidden_output);
    let hidden_hash = hash(&hidden_program)?;
    let synthetic = owner.derive_synthetic_hidden(&hidden_hash);
    let key = synthetic.public_key().to_bytes();
    ensure!(
        synthetic.public_key() == owner.public_key().derive_synthetic_hidden(&hidden_hash)
            && synthetic.public_key() != owner.public_key()
            && !synthetic.public_key().is_inf(),
        "independent synthetic fixture derivation"
    );
    let domain = TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes();
    ensure!(
        domain != MAINNET_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        "distinct network domains"
    );
    let reveal = curry(&module, &key);
    let reveal_hash = hash(&reveal)?;
    let mut simulator = Simulator::new();
    // The actual module and synthetic key are committed before minting each coin.
    let coin_a = create_coin(&mut simulator, &reveal, AMOUNT)?;
    let coin_b = create_coin(&mut simulator, &reveal, AMOUNT)?;
    let coin_c = create_coin(&mut simulator, &reveal, AMOUNT)?;
    ensure!(
        coin_a.coin_id() != coin_b.coin_id()
            && coin_a.coin_id() != coin_c.coin_id()
            && coin_b.coin_id() != coin_c.coin_id(),
        "distinct fixture inputs"
    );
    let delegated = quote(&hidden_output);
    let delegated_solution = solution(&atom(&[]), &delegated, &atom(&[]));
    let delegated_expected = expected_delegated(&key, &delegated, &hidden_output)?;
    let spend = coin_spend(coin_a, &reveal, &delegated_solution)?;
    let proper = sign_program(&synthetic, &delegated, &coin_a)?;
    // Reject an arbitrary module's unexpected/expanded output before allowing
    // the SDK signer to execute it. Retain this observation on the first attempt.
    let mut initial_primary = Some(accepted_primary(
        &reveal,
        &delegated_solution,
        &delegated_expected,
    )?);
    ensure!(
        proper
            == sign_transaction(
                std::slice::from_ref(&spend),
                std::slice::from_ref(&synthetic)
            )?,
        "host final message differs from actual SDK signer requirement"
    );
    let mut attempts = Vec::new();
    let mut wrong_coin = coin_a.coin_id().to_bytes();
    wrong_coin[0] ^= 1;
    let wrong_coin_message = [
        hash(&delegated)?.as_slice(),
        wrong_coin.as_slice(),
        domain.as_slice(),
    ]
    .concat();
    let solution_hash = hash(&atom(&[]))?;
    ensure!(
        solution_hash != hash(&delegated)?,
        "distinct solution and program hash witnesses"
    );
    let signature_cases = [
        ("empty_signature", Signature::default()),
        (
            "original_key_signature",
            chia_bls::sign(&owner, final_message(&hash(&delegated)?, &coin_a, &domain)),
        ),
        (
            "solution_hash_instead_of_program_hash",
            chia_bls::sign(&synthetic, final_message(&solution_hash, &coin_a, &domain)),
        ),
        (
            "wrong_coin_id_signature",
            chia_bls::sign(&synthetic, wrong_coin_message),
        ),
        (
            "wrong_network_domain_signature",
            chia_bls::sign(
                &synthetic,
                final_message(
                    &hash(&delegated)?,
                    &coin_a,
                    &MAINNET_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
                ),
            ),
        ),
    ];
    for (label, signature) in signature_cases {
        if label != "empty_signature" {
            ensure!(
                signature != Signature::default(),
                "actual nonempty invalid signature"
            );
        }
        attempts.push(reject(
            &mut simulator,
            label,
            spend.clone(),
            signature,
            match initial_primary.take() {
                Some(observation) => observation,
                None => accepted_primary(&reveal, &delegated_solution, &delegated_expected)?,
            },
            ConsensusRejection::BadAggregateSignature,
        )?);
    }
    for (label, output, code) in [
        (
            "pair_destination",
            conditions(&pair(&atom(&[1]), &atom(&[2])), &atom(&[3, 0xe8])),
            ConsensusRejection::InvalidPuzzleHash,
        ),
        (
            "pair_amount",
            conditions(&atom(&DESTINATION), &pair(&atom(&[1]), &atom(&[2]))),
            ConsensusRejection::InvalidCoinAmount,
        ),
        (
            "overmint_one_mojo",
            payment(&DESTINATION, AMOUNT + 1),
            ConsensusRejection::MintingCoin,
        ),
    ] {
        let delegated = quote(&output);
        let solution = solution(&atom(&[]), &delegated, &atom(&[]));
        let signature = sign_program(&synthetic, &delegated, &coin_a)?;
        let primary = accepted_primary(
            &reveal,
            &solution,
            &expected_delegated(&key, &delegated, &output)?,
        )?;
        attempts.push(reject(
            &mut simulator,
            label,
            coin_spend(coin_a, &reveal, &solution)?,
            signature,
            primary,
            code,
        )?);
    }
    let malformed = atom(&[]);
    attempts.push(reject_runtime(
        &mut simulator,
        "missing_solution_fields",
        coin_spend(coin_a, &reveal, &malformed)?,
        rejected_primary(&input.lane, &reveal, &malformed, ProgramFailure::Shape)?,
    )?);
    let zero_original = solution(&atom(&[0]), &hidden_program, &atom(&[]));
    attempts.push(reject_runtime(
        &mut simulator,
        "nonempty_zero_original_key_selects_hidden_path",
        coin_spend(coin_a, &reveal, &zero_original)?,
        rejected_primary(&input.lane, &reveal, &zero_original, ProgramFailure::Point)?,
    )?);
    attempts.push(accept(
        &mut simulator,
        "signed_delegated_payment",
        spend.clone(),
        proper.clone(),
        accepted_primary(&reveal, &delegated_solution, &delegated_expected)?,
        &DESTINATION,
    )?);
    attempts.push(reject(
        &mut simulator,
        "consumed_input_replay",
        spend,
        proper,
        accepted_primary(&reveal, &delegated_solution, &delegated_expected)?,
        ConsensusRejection::DoubleSpend,
    )?);

    let wrong_original = solution(
        &atom(&wrong_owner.public_key().to_bytes()),
        &hidden_program,
        &atom(&[]),
    );
    ensure!(
        wrong_owner
            .public_key()
            .derive_synthetic_hidden(&hidden_hash)
            != synthetic.public_key(),
        "wrong original opening differs"
    );
    attempts.push(reject_runtime(
        &mut simulator,
        "wrong_original_key_hidden_opening",
        coin_spend(coin_b, &reveal, &wrong_original)?,
        rejected_primary(
            &input.lane,
            &reveal,
            &wrong_original,
            ProgramFailure::HiddenMismatch,
        )?,
    )?);
    let altered_hidden = quote(&payment(&ALTERNATE_DESTINATION, AMOUNT));
    ensure!(
        hash(&altered_hidden)? != hidden_hash
            && owner
                .public_key()
                .derive_synthetic_hidden(&hash(&altered_hidden)?)
                != synthetic.public_key(),
        "changed hidden opening differs"
    );
    let wrong_hidden = solution(
        &atom(&owner.public_key().to_bytes()),
        &altered_hidden,
        &atom(&[]),
    );
    attempts.push(reject_runtime(
        &mut simulator,
        "wrong_hidden_program_opening",
        coin_spend(coin_b, &reveal, &wrong_hidden)?,
        rejected_primary(
            &input.lane,
            &reveal,
            &wrong_hidden,
            ProgramFailure::HiddenMismatch,
        )?,
    )?);
    let hidden_solution = solution(
        &atom(&owner.public_key().to_bytes()),
        &hidden_program,
        &atom(&[]),
    );
    attempts.push(accept(
        &mut simulator,
        "public_hidden_fixed_payment_without_synthetic_signature",
        coin_spend(coin_b, &reveal, &hidden_solution)?,
        Signature::default(),
        accepted_primary(&reveal, &hidden_solution, &hidden_output)?,
        &DESTINATION,
    )?);

    // Signing an unrestricted identity program intentionally leaves its solution free.
    let identity = atom(&[1]);
    let contemplated = solution(&atom(&[]), &identity, &hidden_output);
    let retarget_output = payment(&ALTERNATE_DESTINATION, AMOUNT);
    let retarget = solution(&atom(&[]), &identity, &retarget_output);
    let retarget_spend = coin_spend(coin_c, &reveal, &retarget)?;
    let retarget_signature = sign_program(&synthetic, &identity, &coin_c)?;
    let contemplated_spend = coin_spend(coin_c, &reveal, &contemplated)?;
    let contemplated_primary = accepted_primary(
        &reveal,
        &contemplated,
        &expected_delegated(&key, &identity, &hidden_output)?,
    )?;
    let retarget_primary = accepted_primary(
        &reveal,
        &retarget,
        &expected_delegated(&key, &identity, &retarget_output)?,
    )?;
    ensure!(
        retarget_signature
            == sign_transaction(&[contemplated_spend], std::slice::from_ref(&synthetic))?
            && retarget_signature
                == sign_transaction(
                    std::slice::from_ref(&retarget_spend),
                    std::slice::from_ref(&synthetic)
                )?,
        "delegated solution is not separately signed"
    );
    let mut retarget_record = accept(
        &mut simulator,
        "signed_identity_program_allows_chosen_solution_recipient",
        retarget_spend,
        retarget_signature,
        retarget_primary,
        &ALTERNATE_DESTINATION,
    )?;
    retarget_record["contemplated_primary_vm"] = json!(contemplated_primary);
    retarget_record["contemplated_solution_hex"] = json!(hex::encode(contemplated));
    retarget_record["delegated_program_tree_hash"] = json!(hex::encode(hash(&identity)?));
    attempts.push(retarget_record);
    let accepted = attempts
        .iter()
        .filter(|a| a["status"] == "accepted")
        .count();
    let rejected = attempts
        .iter()
        .filter(|a| a["status"] == "consensus_rejected")
        .count();
    let umbrellas = attempts
        .iter()
        .filter(|a| a["status"] == "runtime_umbrella")
        .count();
    ensure!(
        (attempts.len(), accepted, rejected, umbrellas) == (16, 3, 9, 4),
        "exact dedicated attempt matrix"
    );
    Ok(
        json!({"lane":input.lane,"module_hex":input.module_hex,"module_blob_sha256":digest(&module),
        "module_tree_hash":hex::encode(module_hash),"reveal_hex":hex::encode(&reveal),"instance_tree_hash":hex::encode(reveal_hash),
        "original_public_key":hex::encode(owner.public_key().to_bytes()),"committed_synthetic_public_key":hex::encode(key),
        "hidden_program_hex":hex::encode(&hidden_program),"hidden_program_tree_hash":hex::encode(hidden_hash),
        "hidden_fixture_requires_signature":false,"agg_sig_me_domain":hex::encode(domain),"direct_fixture_funding_coins":3,
        "counts":{"scenario_groups":3,"transaction_attempts":16,"accepted":accepted,"typed_consensus_rejected":rejected,"runtime_umbrellas":umbrellas},
        "attempts":attempts}),
    )
}

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments
        .next()
        .context("usage: standard_tutorial_ledger REQUEST.json")?;
    ensure!(
        arguments.next().is_none(),
        "one explicit request file required"
    );
    let request = load_request(Path::new(&path))?;
    let mut builds = Vec::new();
    for input in request.runs {
        builds.push(run(input)?);
    }
    println!(
        "{}",
        json!({"protocol":REPORT_PROTOCOL,"status":"executed","sdk":SIMULATOR_VERSION,
        "consensus":CONSENSUS_VERSION,"sdk_vm":VM_VERSION,"network":NETWORK,"rejection_protocol":REJECTION_PROTOCOL,
        "primary_vm":"clvmr 0.21.0 MEMPOOL_MODE","primary_failure_protocol":"clvmr-0.21.0/v1","primary_max_cost":PRIMARY_MAX_COST,
        "sdk_max_cost":5_500_000_000_u64,"counts":{"builds":5,"scenario_groups":15,"direct_fixture_funding_coins":15,
        "transaction_attempts":80,"accepted":15,"typed_consensus_rejected":45,"runtime_umbrellas":20},"builds":builds,
        "scope":"Finite five-build committed-instance SDK observations only. Every coin commits its own actual module and chosen synthetic key before direct simulator fixture funding. Public hidden payment fixture needs no signature. Native/template/primaryVM/SDK phases remain distinct. Runtime umbrellas add no semantic rejection credit. Caller artifact admission/source authenticity are separate. No wallet/network/live-chain, canonical Chia module identity, whole-page completion, execution authentication, compiler preservation or production readiness claim."})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_program_gate_preserves_payloads_and_rejects_wire_aliases() {
        assert!(hash(&[1]).is_ok());
        // Canonical serialization of the two-byte payload00,01 remains valid.
        assert!(hash(&[0x82, 0, 1]).is_ok());
        assert!(hash(&[0x81, 1]).is_err());
        assert!(hash(&[1, 0x80]).is_err());
    }

    #[test]
    fn expanded_output_preflight_rejects_small_shared_dag_before_serialization() {
        let mut allocator = Allocator::new();
        let mut tree = NodePtr::NIL;
        for _ in 0..10 {
            tree = allocator.new_pair(tree, tree).unwrap();
        }
        assert!(preflight_output(&allocator, tree).is_ok());
        // Twelve physical pairs can expand to8191 serialized tree occurrences.
        tree = allocator.new_pair(tree, tree).unwrap();
        tree = allocator.new_pair(tree, tree).unwrap();
        assert!(preflight_output(&allocator, tree).is_err());
        let oversized = allocator
            .new_atom(&vec![0x55; MAX_FIXTURE_BYTES + 1])
            .unwrap();
        assert!(preflight_output(&allocator, oversized).is_err());
    }
}
