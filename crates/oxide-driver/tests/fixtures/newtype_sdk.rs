#[path = "../../../../sdk/oxide.rs"]
pub mod oxide;

pub fn puzzle(
    hash: oxide::PuzzleHash,
    amount: oxide::Amount,
) -> (oxide::CreateCoin, oxide::AssertMyAmount) {
    assert!(amount.0 <= 1000);
    let original = oxide::Amount(amount.0);
    (
        oxide::create_coin(hash, amount),
        oxide::assert_my_amount(original),
    )
}
