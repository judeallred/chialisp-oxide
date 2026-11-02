use crate::common::*;
use anyhow::Result;
use chialisp_oxide_simulator::{
    Coin, ErrorCode, Signature, Simulator, SpendBundle, coin_spend, puzzle_hash,
};
use serde_json::{Value, json};
fn reveal(module: &[u8], head: &[u8; 32], count: usize) -> Result<Vec<u8>> {
    let mh = puzzle_hash(module)?;
    let terms = encode(|a| {
        let m = a.new_atom(mh.as_ref())?;
        let h = a.new_atom(head)?;
        let n = uint(a, count as u64)?;
        list(a, &[m, h, n])
    })?;
    curry(module, &terms)
}
fn solution(
    recipients: &[[u8; 32]],
    heads: &[[u8; 32]],
    i: usize,
    width: usize,
) -> Result<Vec<u8>> {
    encode(|a| {
        let mut openings = vec![];
        for j in i..i + width {
            let r = a.new_atom(&recipients[j])?;
            let h = a.new_atom(&heads[j + 1])?;
            openings.push(list(a, &[r, h])?);
        }
        let opening = if width == 1 {
            openings[0]
        } else {
            list(a, &openings)?
        };
        list(a, &[opening])
    })
}
pub fn run(module: &[u8], count: usize, width: usize) -> Result<Value> {
    let recipients = (1..=count)
        .map(|i| [i as u8 * 0x11; 32])
        .collect::<Vec<_>>();
    let mut heads = vec![[0; 32]; count + 1];
    heads[count] = sha(&[b"chialisp-oxide/continuation/v1/end:1000"]);
    for i in (0..count).rev() {
        heads[i] = sha(&[
            b"chialisp-oxide/continuation/v1/item:1000",
            &recipients[i],
            &heads[i + 1],
        ]);
    }
    let mut sim = Simulator::new();
    let first = reveal(module, &heads[0], count)?;
    let mut coin = sim.new_coin(puzzle_hash(&first)?, count as u64 * 1000);
    let mut steps = vec![];
    let mut controls = vec![];
    for i in (0..count).step_by(width) {
        let rev = reveal(module, &heads[i], count - i)?;
        let sol = solution(&recipients, &heads, i, width)?;
        let b = SpendBundle::new(vec![coin_spend(coin, &rev, &sol)?], Signature::default());
        if i == 0 {
            let wrong = sim.new_coin(puzzle_hash(&rev)?, coin.amount - 1);
            let bad = SpendBundle::new(vec![coin_spend(wrong, &rev, &sol)?], Signature::default());
            controls.push(reject(
                &mut sim,
                bad,
                ErrorCode::AssertMyAmountFailed,
                "bad_amount",
            )?);
            let mut malformed = b.clone();
            malformed.coin_spends[0].solution = chialisp_oxide_simulator::program(&[0x80])?;
            controls.push(reject(
                &mut sim,
                malformed,
                ErrorCode::GeneratorRuntimeError,
                "malformed_opening",
            )?);
            let extra = quoted(&[(51, vec![[0xee; 32].to_vec(), amount(2)])])?;
            let input = sim.new_coin(puzzle_hash(&extra)?, 1);
            let mut inflated = b.clone();
            inflated
                .coin_spends
                .push(coin_spend(input, &extra, &[0x80])?);
            controls.push(reject(
                &mut sim,
                inflated,
                ErrorCode::MintingCoin,
                "composition_inflation",
            )?);
        }
        let mut expected = vec![];
        for recipient in &recipients[i..i + width] {
            expected.push(Coin::new(coin.coin_id(), (*recipient).into(), 1000));
        }
        let successor = if i + width < count {
            let r = reveal(module, &heads[i + width], count - i - width)?;
            let c = Coin::new(
                coin.coin_id(),
                puzzle_hash(&r)?,
                (count - i - width) as u64 * 1000,
            );
            expected.push(c);
            Some(c)
        } else {
            None
        };
        steps.push(accept(&mut sim, b.clone(), &expected)?);
        controls.push(reject(&mut sim, b, ErrorCode::DoubleSpend, "stale_coin")?);
        if let Some(c) = successor {
            coin = c;
        }
    }
    Ok(
        json!({"count":count,"batch_width":width,"recipient_queue":recipients.iter().map(hex::encode).collect::<Vec<_>>(),"payouts":count,"payout_total":count*1000,"terminal_successor":false,"steps":steps,"whole_chain":totals(&steps),"controls":controls,"inflation_fixture":"quoted composition fixture; not standard wallet integration"}),
    )
}
