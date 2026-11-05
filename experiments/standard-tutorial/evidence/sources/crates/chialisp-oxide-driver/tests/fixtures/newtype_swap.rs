#[path = "../../../../sdk/chialisp_oxide.rs"]
mod chialisp_oxide;
pub fn puzzle(
    coin: chialisp_oxide::CoinId,
    amount: chialisp_oxide::Amount,
) -> chialisp_oxide::CreateCoin {
    chialisp_oxide::create_coin(coin, amount)
}
