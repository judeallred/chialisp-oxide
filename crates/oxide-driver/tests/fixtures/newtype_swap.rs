#[path = "../../../../sdk/oxide.rs"]
mod oxide;
pub fn puzzle(coin: oxide::CoinId, amount: oxide::Amount) -> oxide::CreateCoin {
    oxide::create_coin(coin, amount)
}
