// Permissionless fixture: any caller may select an amount within the cap.
#[path = "../../sdk/oxide.rs"]
pub mod oxide;

pub fn puzzle(amount: oxide::Amount) -> (oxide::AssertMyAmount, oxide::CreateCoin) {
    assert!(amount.0 <= 1000);
    (
        oxide::assert_my_amount(oxide::Amount(1000)),
        oxide::create_coin(oxide::PuzzleHash([0x42; 32]), amount),
    )
}
