//! Finite SDK owner-instance fixtures, including intentional raw-template risks.
//! Adapted mechanically from the reviewed standalone12-scenario harness.
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, Simulator, SimulatorError,
    SpendBundle, coin_spend, create_coin, program, puzzle_hash, sign_transaction,
};
use serde_json::{Value, json};

const AMOUNT: u64 = 1_000;
const RECIPIENT: [u8; 32] = [0x42; 32];
const RETARGET: [u8; 32] = [0x43; 32];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// This deliberately small independent fixture serializer accepts short atoms.
// It is not the project's production encoder or a general untrusted-input API.
fn atom(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() < 0x40);
    if bytes.len() == 1 && bytes[0] <= 0x7f {
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

fn list(items: &[Vec<u8>]) -> Vec<u8> {
    items
        .iter()
        .rev()
        .fold(atom(&[]), |rest, first| pair(first, &rest))
}

fn number(value: u64) -> Vec<u8> {
    if value == 0 {
        return atom(&[]);
    }
    let raw = value.to_be_bytes();
    let start = raw.iter().position(|b| *b != 0).unwrap();
    let mut bytes = Vec::new();
    if raw[start] >= 0x80 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&raw[start..]);
    atom(&bytes)
}

fn curry_key(module: &[u8], key: &SecretKey) -> Vec<u8> {
    let quoted_module = pair(&number(1), module);
    let quoted_key = pair(&number(1), &atom(&key.public_key().to_bytes()));
    let environment = list(&[number(4), quoted_key, number(1)]);
    list(&[number(2), quoted_module, environment])
}

fn conditions(destination: &[u8; 32], amount: u64) -> Vec<u8> {
    list(&[list(&[number(51), atom(destination), number(amount)])])
}

fn solution(conditions: &[u8], ignored_tail: &[u8]) -> Vec<u8> {
    pair(conditions, ignored_tail)
}

fn signed(spend: CoinSpend, key: &SecretKey) -> SpendBundle {
    let signature =
        sign_transaction(std::slice::from_ref(&spend), std::slice::from_ref(key)).unwrap();
    assert_ne!(
        signature,
        Signature::default(),
        "fixture must contain an actual signature"
    );
    SpendBundle::new(vec![spend], signature)
}

fn coin_json(coin: Coin) -> Value {
    json!({"id":hex(coin.coin_id().as_ref()),"parent_id":hex(coin.parent_coin_info.as_ref()),
        "puzzle_hash":hex(coin.puzzle_hash.as_ref()),"amount":coin.amount})
}

fn transaction(bundle: &SpendBundle) -> Value {
    json!({"signature_hex":hex(&bundle.aggregated_signature.to_bytes()),
        "spends":bundle.coin_spends.iter().map(|s|json!({"coin":coin_json(s.coin),
            "reveal_hex":hex(s.puzzle_reveal.as_ref()),"solution_hex":hex(s.solution.as_ref())})).collect::<Vec<_>>()})
}

fn reject(
    simulator: &mut Simulator,
    name: &str,
    bundle: SpendBundle,
    expected: ErrorCode,
) -> Value {
    let before = simulator.clone();
    let submitted = transaction(&bundle);
    let result = simulator.new_transaction(bundle);
    let Err(SimulatorError::Validation(actual)) = result else {
        panic!("{name}: expected exact consensus {expected:?}, got {result:?}")
    };
    assert_eq!(actual, expected, "{name}: wrong consensus rejection");
    assert_eq!(
        *simulator, before,
        "{name}: full simulator state changed after rejection"
    );
    json!({"label":name,"status":"consensus_rejected","code":format!("{actual:?}"),
        "full_simulator_equality":true,"transaction":submitted})
}

fn accept(
    simulator: &mut Simulator,
    name: &str,
    bundle: SpendBundle,
    destination: [u8; 32],
    scope: &str,
) -> Value {
    assert_eq!(bundle.coin_spends.len(), 1);
    let coin = bundle.coin_spends[0].coin;
    let submitted = transaction(&bundle);
    let before = simulator.coin_state(coin.coin_id()).unwrap();
    assert!(before.spent_height.is_none());
    assert!(simulator.children(coin.coin_id()).is_empty());
    simulator.new_transaction(bundle).unwrap();
    assert!(
        simulator
            .coin_state(coin.coin_id())
            .unwrap()
            .spent_height
            .is_some()
    );
    let children = simulator.children(coin.coin_id());
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].coin.amount, AMOUNT);
    assert_eq!(children[0].coin.parent_coin_info, coin.coin_id());
    assert_eq!(children[0].coin.puzzle_hash, Bytes32::new(destination));
    json!({"label":name,"status":"accepted","scope":scope,"transaction":submitted,
        "spent_input_observed":true,"child":coin_json(children[0].coin)})
}

pub(super) fn validate(
    backend: &str,
    optimize: bool,
    module: &[u8],
    alternate_backend: &str,
    alternate_optimize: bool,
    alternate_module: &[u8],
) -> Value {
    let owner = SecretKey::from_seed(&[0x17; 32]);
    let attacker = SecretKey::from_seed(&[0x18; 32]);
    assert_ne!(owner.public_key(), attacker.public_key());
    let reveal = curry_key(module, &owner);
    let attacker_reveal = curry_key(module, &attacker);
    assert_ne!(
        puzzle_hash(&reveal).unwrap(),
        puzzle_hash(&attacker_reveal).unwrap()
    );
    let ordinary_conditions = conditions(&RECIPIENT, AMOUNT);
    let ordinary_solution = solution(&ordinary_conditions, &atom(&[]));
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, &reveal, AMOUNT).unwrap();
    let spend = coin_spend(coin, &reveal, &ordinary_solution).unwrap();
    let complete = signed(spend.clone(), &owner);
    let mut cross_build_simulator = simulator.clone();
    let cross_complete = complete.clone();
    let mut observations = Vec::new();
    observations.push(reject(
        &mut simulator,
        "missing_signature",
        SpendBundle::new(vec![spend.clone()], Signature::default()),
        ErrorCode::BadAggregateSignature,
    ));

    // Signing is deliberately separate from consensus submission. This raw
    // alternate reveal asks the SDK signer for a real attacker-key signature
    // on the SAME input coin and conditions message, then supplies that nonempty
    // signature to the owner's actual reveal. No MissingKey preflight is credited.
    let wrong_key_source = CoinSpend::new(
        coin,
        program(&attacker_reveal).unwrap(),
        program(&ordinary_solution).unwrap(),
    );
    let wrong_key_signature = signed(wrong_key_source.clone(), &attacker).aggregated_signature;
    assert_ne!(wrong_key_signature, complete.aggregated_signature);
    let mut wrong_key = reject(
        &mut simulator,
        "wrong_nonempty_key_signature",
        SpendBundle::new(vec![spend.clone()], wrong_key_signature),
        ErrorCode::BadAggregateSignature,
    );
    wrong_key["signature_source"] = transaction(&signed(wrong_key_source, &attacker));
    wrong_key["signature_source_scope"] = json!(
        "Signer-only alternate reveal on the same coin; not a separately admitted transaction."
    );
    observations.push(wrong_key);

    let changed_solution = solution(&conditions(&RETARGET, AMOUNT), &atom(&[]));
    let retarget = coin_spend(coin, &reveal, &changed_solution).unwrap();
    observations.push(reject(
        &mut simulator,
        "changed_conditions_reuse_old_signature",
        SpendBundle::new(vec![retarget], complete.aggregated_signature.clone()),
        ErrorCode::BadAggregateSignature,
    ));

    let mut other_simulator = simulator.clone();
    let other_coin = create_coin(&mut other_simulator, &reveal, AMOUNT).unwrap();
    assert_ne!(coin.coin_id(), other_coin.coin_id());
    let other_spend = coin_spend(other_coin, &reveal, &ordinary_solution).unwrap();
    observations.push(reject(
        &mut other_simulator,
        "other_coin_reuse_signature",
        SpendBundle::new(vec![other_spend], complete.aggregated_signature.clone()),
        ErrorCode::BadAggregateSignature,
    ));

    let overspend_solution = solution(&conditions(&RECIPIENT, AMOUNT + 1), &atom(&[]));
    let overspend = coin_spend(coin, &reveal, &overspend_solution).unwrap();
    observations.push(reject(
        &mut simulator,
        "correct_signature_cannot_mint_value",
        signed(overspend, &owner),
        ErrorCode::MintingCoin,
    ));

    let substituted_reveal = CoinSpend::new(
        coin,
        program(&attacker_reveal).unwrap(),
        program(&ordinary_solution).unwrap(),
    );
    observations.push(reject(
        &mut simulator,
        "owner_key_curry_cannot_be_replaced",
        signed(substituted_reveal, &attacker),
        ErrorCode::WrongPuzzleHash,
    ));

    let mut ignored_tail_simulator = simulator.clone();
    let ignored_tail = pair(&atom(&[0x00]), &atom(&[0x81]));
    let ignored_solution = solution(&ordinary_conditions, &ignored_tail);
    assert_ne!(ordinary_solution, ignored_solution);
    let ignored_spend = coin_spend(coin, &reveal, &ignored_solution).unwrap();
    assert_eq!(
        signed(ignored_spend.clone(), &owner).aggregated_signature,
        complete.aggregated_signature,
        "ignored outer tail must not alter the committed conditions message"
    );
    observations.push(accept(
        &mut ignored_tail_simulator,
        "ignored_improper_tail_preserves_signature",
        SpendBundle::new(vec![ignored_spend], complete.aggregated_signature.clone()),
        RECIPIENT,
        "Curried owner key; arbitrary ignored tail is preserved.",
    ));
    observations.push(accept(
        &mut simulator,
        "owner_signature_exact_payout",
        complete.clone(),
        RECIPIENT,
        "Curried owner key; finite selected-build consensus observation.",
    ));
    observations.push(reject(
        &mut simulator,
        "spent_coin_cannot_replay",
        complete,
        ErrorCode::DoubleSpend,
    ));

    // Policy-control counterexample: the original module expects its public key
    // from the environment. Funding the UNCURRIED module leaves key selection
    // to the spender. This is intentionally observed success, not a security test
    // that the compiler or consensus is expected to reject.
    let mut raw = Simulator::new();
    let raw_coin = create_coin(&mut raw, module, AMOUNT).unwrap();
    let mut raw_owner = raw.clone();
    let attack_conditions = conditions(&RETARGET, AMOUNT);
    let raw_solution = pair(
        &atom(&attacker.public_key().to_bytes()),
        &solution(&attack_conditions, &atom(&[])),
    );
    let raw_spend = coin_spend(raw_coin, module, &raw_solution).unwrap();
    observations.push(accept(&mut raw, "unsafe_uncurried_caller_selects_attacker_key", signed(raw_spend, &attacker), RETARGET,
        "Intentional missing-policy counterexample: funding raw module does not commit an owner key; not a compiler-prevented error."));
    let owner_raw_solution = pair(&atom(&owner.public_key().to_bytes()), &ordinary_solution);
    let owner_raw_spend = coin_spend(raw_coin, module, &owner_raw_solution).unwrap();
    observations.push(accept(&mut raw_owner, "unsafe_uncurried_alternative_owner_key", signed(owner_raw_spend, &owner), RECIPIENT,
        "Alternative spend of the identical raw coin in a cloned state; not a second spend in one ledger."));

    // Matching policy results do not make separately compiled module identities
    // interchangeable. Signatures commit to this SAME coin/key/message, while
    // consensus independently verifies the supplied reveal's puzzle hash.
    let alternate_reveal = curry_key(alternate_module, &owner);
    assert_ne!(
        puzzle_hash(&reveal).unwrap(),
        puzzle_hash(&alternate_reveal).unwrap()
    );
    let alternate_spend = CoinSpend::new(
        coin,
        program(&alternate_reveal).unwrap(),
        program(&ordinary_solution).unwrap(),
    );
    let alternate_signature = signed(alternate_spend.clone(), &owner).aggregated_signature;
    assert_eq!(alternate_signature, cross_complete.aggregated_signature);
    let mut cross_build = reject(
        &mut cross_build_simulator,
        "different_language_reveal_cannot_spend_input",
        SpendBundle::new(vec![alternate_spend], alternate_signature),
        ErrorCode::WrongPuzzleHash,
    );
    cross_build["alternate_build"] = json!({"backend":alternate_backend,
        "optimize_lir":alternate_optimize,"module_tree_hash":hex(puzzle_hash(alternate_module).unwrap().as_ref()),
        "owner_curried_reveal_hex":hex(&alternate_reveal),
        "owner_instance_hash":hex(puzzle_hash(&alternate_reveal).unwrap().as_ref())});
    cross_build["signature_matches_selected_reveal"] = json!(true);
    cross_build["selected_reveal_signature_source"] = transaction(&cross_complete);
    observations.push(cross_build);

    let accepted = observations
        .iter()
        .filter(|v| v["status"] == "accepted")
        .count();
    let rejected = observations
        .iter()
        .filter(|v| v["status"] == "consensus_rejected")
        .count();
    assert_eq!((observations.len(), accepted, rejected), (12, 4, 8));
    json!({"backend":backend,"optimize_lir":optimize,"status":"validated",
        "module_tree_hash":hex(puzzle_hash(module).unwrap().as_ref()),"module_hex":hex(module),
        "owner_public_key":hex(&owner.public_key().to_bytes()),"attacker_public_key":hex(&attacker.public_key().to_bytes()),
        "owner_curried_reveal_hex":hex(&reveal),"owner_instance_hash":hex(puzzle_hash(&reveal).unwrap().as_ref()),
        "transaction_attempts":12,"accepted":accepted,"typed_rejected_with_full_rollback":rejected,
        "observations":observations,
        "scope":"Actual TESTNET11 submissions for this compiled module and its own owner-key-curried instance. Full simulator Eq is asserted in memory after each exact typed rejection; the recorded Boolean is not a state proof or authentication. The two uncurried key-selection successes are intentional unsafe policy controls. Finite fixtures do not prove exhaustive authorization, BLS or compiler preservation."})
}
