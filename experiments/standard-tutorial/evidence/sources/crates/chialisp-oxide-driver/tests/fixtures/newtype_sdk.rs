#[path = "../../../../sdk/chialisp_oxide.rs"]
pub mod chialisp_oxide;

pub fn puzzle(
    hash: chialisp_oxide::PuzzleHash,
    amount: chialisp_oxide::Amount,
) -> (chialisp_oxide::CreateCoin, chialisp_oxide::AssertMyAmount) {
    assert!(amount.0 <= 1000);
    let original = chialisp_oxide::Amount(amount.0);
    (
        chialisp_oxide::create_coin(hash, amount),
        chialisp_oxide::assert_my_amount(original),
    )
}
