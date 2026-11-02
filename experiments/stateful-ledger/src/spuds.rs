use crate::{common::*, fixture_keys};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, ErrorCode, SecretKey, Signature, Simulator, SpendBundle, coin_spend, puzzle_hash,
    sign_single_requirement_with_wrong_key, sign_transaction,
};
use serde_json::{Value, json};
const TAKE: &[u8] = b"chialisp-oxide/spuds/take:v1";
const CLAIM: &[u8] = b"chialisp-oxide/spuds/claim:v1";
#[derive(Clone)]
struct Game {
    coin: Coin,
    round: u8,
    key: SecretKey,
    payout: [u8; 32],
}
fn reveal(module: &[u8], key: &SecretKey, payout: &[u8; 32], round: u8) -> Result<Vec<u8>> {
    let mh = puzzle_hash(module)?;
    let pk = key.public_key().to_bytes();
    let terms = encode(|a| {
        let m = a.new_atom(mh.as_ref())?;
        let k = a.new_atom(&pk)?;
        let p = a.new_atom(payout)?;
        let r = uint(a, round.into())?;
        list(a, &[m, k, p, r])
    })?;
    curry(module, &terms)
}
fn solution(take: bool, key: &SecretKey, payout: &[u8; 32]) -> Result<Vec<u8>> {
    let pk = key.public_key().to_bytes();
    encode(|a| {
        let t = uint(a, u64::from(take))?;
        let k = a.new_atom(&pk)?;
        let p = a.new_atom(payout)?;
        list(a, &[t, k, p])
    })
}
fn game(sim: &mut Simulator, module: &[u8]) -> Result<Game> {
    let key = fixture_keys::holder();
    let payout = [0xa1; 32];
    let rev = reveal(module, &key, &payout, 1)?;
    Ok(Game {
        coin: sim.new_coin(puzzle_hash(&rev)?, 1000),
        round: 1,
        key,
        payout,
    })
}
fn topup(
    sim: &mut Simulator,
    game: &Game,
    message: [u8; 32],
    input: u64,
    change: u64,
) -> Result<(chialisp_oxide_simulator::CoinSpend, Vec<Coin>)> {
    let ann = sha(&[game.coin.coin_id().as_ref(), &message]);
    let mut conditions = vec![(61, vec![ann.to_vec()])];
    if change > 0 {
        conditions.push((51, vec![[0xcc; 32].to_vec(), amount(change)]));
    }
    let rev = quoted(&conditions)?;
    let coin = sim.new_coin(puzzle_hash(&rev)?, input);
    let output = if change > 0 {
        vec![Coin::new(coin.coin_id(), [0xcc; 32].into(), change)]
    } else {
        vec![]
    };
    Ok((coin_spend(coin, &rev, &[0x80])?, output))
}
fn take(
    sim: &mut Simulator,
    module: &[u8],
    g: &Game,
    key: SecretKey,
    payout: [u8; 32],
    topup_amount: u64,
) -> Result<(SpendBundle, Game, Vec<Coin>)> {
    let rev = reveal(module, &g.key, &g.payout, g.round)?;
    let next_reveal = reveal(module, &key, &payout, g.round + 1)?;
    let next = Game {
        coin: Coin::new(
            g.coin.coin_id(),
            puzzle_hash(&next_reveal)?,
            g.coin.amount + 1000,
        ),
        round: g.round + 1,
        key,
        payout,
    };
    let message = sha(&[TAKE, next.coin.puzzle_hash.as_ref()]);
    let change = topup_amount.saturating_sub(1000);
    let (funding, mut outputs) = topup(sim, g, message, topup_amount, change)?;
    let spends = vec![
        coin_spend(g.coin, &rev, &solution(true, &next.key, &payout)?)?,
        funding,
    ];
    exact_conditions(
        &rev,
        spends[0].solution.as_ref(),
        &[
            (
                50,
                vec![next.key.public_key().to_bytes().to_vec(), message.to_vec()],
            ),
            (73, vec![amount(g.coin.amount)]),
            (82, vec![amount(1)]),
            (86, vec![amount(5)]),
            (
                51,
                vec![
                    next.coin.puzzle_hash.as_ref().to_vec(),
                    amount(next.coin.amount),
                ],
            ),
            (60, vec![message.to_vec()]),
        ],
    )?;
    let sig = sign_transaction(&spends, std::slice::from_ref(&next.key))
        .map_err(|e| anyhow::anyhow!("sign take: {e:?}"))?;
    outputs.push(next.coin);
    Ok((SpendBundle::new(spends, sig), next, outputs))
}
fn claim(module: &[u8], g: &Game) -> Result<SpendBundle> {
    let rev = reveal(module, &g.key, &g.payout, g.round)?;
    let spends = vec![coin_spend(
        g.coin,
        &rev,
        &solution(false, &g.key, &g.payout)?,
    )?];
    let message = sha(&[CLAIM]);
    exact_conditions(
        &rev,
        spends[0].solution.as_ref(),
        &[
            (
                50,
                vec![g.key.public_key().to_bytes().to_vec(), message.to_vec()],
            ),
            (73, vec![amount(g.coin.amount)]),
            (82, vec![amount(1)]),
            (82, vec![amount(5)]),
            (51, vec![g.payout.to_vec(), amount(g.coin.amount)]),
            (60, vec![message.to_vec()]),
        ],
    )?;
    let sig = sign_transaction(&spends, std::slice::from_ref(&g.key))
        .map_err(|e| anyhow::anyhow!("sign claim: {e:?}"))?;
    Ok(SpendBundle::new(spends, sig))
}
fn age(sim: &Simulator, g: &Game) -> u32 {
    sim.height()
        - sim
            .coin_state(g.coin.coin_id())
            .unwrap()
            .created_height
            .unwrap()
}
fn advance(sim: &mut Simulator, g: &Game, target: u32) {
    while age(sim, g) < target {
        sim.create_block();
    }
}
pub fn run(module: &[u8]) -> Result<Value> {
    let mut sim = Simulator::new();
    let mut g = game(&mut sim, module)?;
    let mut controls = vec![];
    let mut steps = vec![];
    let capped_reveal = reveal(module, &g.key, &g.payout, 8)?;
    let capped = sim.new_coin(puzzle_hash(&capped_reveal)?, 8000);
    let capped_bundle = SpendBundle::new(
        vec![coin_spend(
            capped,
            &capped_reveal,
            &solution(true, &fixture_keys::next(), &[0xa2; 32])?,
        )?],
        Signature::default(),
    );
    controls.push(reject(
        &mut sim,
        capped_bundle,
        ErrorCode::GeneratorRuntimeError,
        "take_at_maximum_round_runtime_umbrella",
    )?);

    // A game created and consumed within this bundle has age zero, even at a later global height.
    let game_reveal = reveal(module, &g.key, &g.payout, g.round)?;
    let launch_reveal = quoted(&[(51, vec![g.coin.puzzle_hash.as_ref().to_vec(), amount(1000)])])?;
    let launch = sim.new_coin(puzzle_hash(&launch_reveal)?, 1000);
    let mut ephemeral = g.clone();
    ephemeral.coin = Coin::new(launch.coin_id(), puzzle_hash(&game_reveal)?, 1000);
    let (mut ephemeral_bundle, _, _) = take(
        &mut sim,
        module,
        &ephemeral,
        fixture_keys::next(),
        [0xa2; 32],
        2000,
    )?;
    ephemeral_bundle
        .coin_spends
        .insert(0, coin_spend(launch, &launch_reveal, &[0x80])?);
    controls.push(reject(
        &mut sim,
        ephemeral_bundle,
        ErrorCode::EphemeralRelativeCondition,
        "ephemeral_take_age_zero",
    )?);

    let (early, _, _) = take(&mut sim, module, &g, fixture_keys::next(), [0xa2; 32], 2000)?;
    controls.push(reject(
        &mut sim,
        early,
        ErrorCode::AssertHeightRelativeFailed,
        "take_at_age_zero",
    )?);
    advance(&mut sim, &g, 1);
    let (valid, next, outputs) =
        take(&mut sim, module, &g, fixture_keys::next(), [0xa2; 32], 2000)?;
    let mut wrong = valid.clone();
    wrong.aggregated_signature =
        sign_single_requirement_with_wrong_key(&wrong.coin_spends, &fixture_keys::wrong())?;
    ensure!(
        wrong.aggregated_signature != Signature::default(),
        "nonempty wrong signature"
    );
    controls.push(reject(
        &mut sim,
        wrong,
        ErrorCode::BadAggregateSignature,
        "wrong_key_actual_bundle",
    )?);
    let (mut transplant, _, _) =
        take(&mut sim, module, &g, fixture_keys::next(), [0xa3; 32], 2000)?;
    transplant.aggregated_signature = valid.aggregated_signature.clone();
    controls.push(reject(
        &mut sim,
        transplant,
        ErrorCode::BadAggregateSignature,
        "transplanted_payout_old_signature",
    )?);
    let (under, _, _) = take(&mut sim, module, &g, fixture_keys::next(), [0xa2; 32], 999)?;
    controls.push(reject(
        &mut sim,
        under,
        ErrorCode::MintingCoin,
        "under_topup",
    )?);
    steps.push(accept(&mut sim, valid.clone(), &outputs)?);
    controls.push(reject(
        &mut sim,
        valid,
        ErrorCode::DoubleSpend,
        "stale_taken_coin",
    )?);
    g = next;
    ensure!(age(&sim, &g) == 1, "actual successor age reset");
    // The previous holder's deadline is irrelevant: the new birth controls the timer.
    advance(&mut sim, &g, 4);
    controls.push(reject(
        &mut sim,
        claim(module, &g)?,
        ErrorCode::AssertHeightRelativeFailed,
        "claim_at_age_four",
    )?);
    let (valid, next, outputs) = take(
        &mut sim,
        module,
        &g,
        fixture_keys::holder(),
        [0xa4; 32],
        2000,
    )?;
    steps.push(accept(&mut sim, valid, &outputs)?);
    g = next;
    ensure!(age(&sim, &g) == 1, "second successor age reset");
    controls.push(reject(
        &mut sim,
        claim(module, &g)?,
        ErrorCode::AssertHeightRelativeFailed,
        "claim_after_take_timer_reset",
    )?);
    advance(&mut sim, &g, 5);
    let (expired, _, _) = take(&mut sim, module, &g, fixture_keys::next(), [0xa5; 32], 2000)?;
    controls.push(reject(
        &mut sim,
        expired,
        ErrorCode::AssertBeforeHeightRelativeFailed,
        "take_at_age_five",
    )?);
    let mut claim_topup = claim(module, &g)?;
    let future = reveal(module, &fixture_keys::next(), &[0xa5; 32], g.round + 1)?;
    let msg = sha(&[TAKE, puzzle_hash(&future)?.as_ref()]);
    let (funding, _) = topup(&mut sim, &g, msg, 2000, 1000)?;
    claim_topup.coin_spends.push(funding);
    controls.push(reject(
        &mut sim,
        claim_topup,
        ErrorCode::AssertCoinAnnouncementFailed,
        "claim_with_take_protected_topup",
    )?);
    let output = Coin::new(g.coin.coin_id(), g.payout.into(), g.coin.amount);
    steps.push(accept(&mut sim, claim(module, &g)?, &[output])?);
    Ok(
        json!({"topup_fixture":"fixed quoted conditions: exact take announcement and fixed own change; not standard wallet integration","fixture_public_keys":{"holder":hex::encode(fixture_keys::holder().public_key().to_bytes()),"next":hex::encode(fixture_keys::next().public_key().to_bytes()),"wrong":hex::encode(fixture_keys::wrong().public_key().to_bytes())},"take_ages":[1,4],"claim_age":5,"timer_reset_checked":true,"exact_six_condition_VM_outputs_checked":true,"steps":steps,"whole_chain":totals(&steps),"controls":controls,"claim_message":hex::encode(sha(&[CLAIM]))}),
    )
}
