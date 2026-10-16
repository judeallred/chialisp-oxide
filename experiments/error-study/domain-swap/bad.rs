use chialisp_oxide::{Amount, CoinId, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(_recipient: PuzzleHash, coin_id: CoinId, amount: Amount) -> (CreateCoin,) {
    (create_coin(coin_id, amount),)
}
