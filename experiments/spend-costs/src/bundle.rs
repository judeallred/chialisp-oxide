use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, Signature, SpendBundle, program, puzzle_hash,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub signature_hex: String,
    pub spends: Vec<Spend>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spend {
    pub coin: CoinRecord,
    pub reveal_hex: String,
    pub solution_hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoinRecord {
    pub id: String,
    pub parent_id: String,
    pub puzzle_hash: String,
    pub amount: u64,
}

fn bytes32(value: &str) -> Result<Bytes32> {
    Ok(Bytes32::new(
        hex::decode(value)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("expected 32 bytes"))?,
    ))
}

impl Transaction {
    pub fn decode(&self) -> Result<SpendBundle> {
        ensure!(!self.spends.is_empty(), "empty measurement bundle");
        let spends = self
            .spends
            .iter()
            .map(|spend| {
                let coin = Coin::new(
                    bytes32(&spend.coin.parent_id)?,
                    bytes32(&spend.coin.puzzle_hash)?,
                    spend.coin.amount,
                );
                ensure!(
                    hex::encode(coin.coin_id()) == spend.coin.id,
                    "changed coin ID"
                );
                let reveal = hex::decode(&spend.reveal_hex)?;
                ensure!(
                    puzzle_hash(&reveal)? == coin.puzzle_hash,
                    "wrong reveal hash"
                );
                Ok(CoinSpend::new(
                    coin,
                    program(&reveal)?,
                    program(&hex::decode(&spend.solution_hex)?)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let signature: [u8; 96] = hex::decode(&self.signature_hex)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("expected 96-byte signature"))?;
        Ok(SpendBundle::new(
            spends,
            Signature::from_bytes(&signature).context("invalid BLS encoding")?,
        ))
    }
}
