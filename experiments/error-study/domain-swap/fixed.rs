use chialisp_oxide::{Amount, CoinId, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(recipient: PuzzleHash, _coin_id: CoinId, amount: Amount) -> (CreateCoin,) {
    (create_coin(recipient, amount),)
}

#[test]
fn destination_is_the_recipient_even_when_coin_id_has_the_same_width() {
    let (condition,) = puzzle(
        PuzzleHash([0x42; 32]),
        CoinId([0x99; 32]),
        Amount::from_mojos(7),
    );
    assert_eq!(condition.0, 51);
    assert_eq!(condition.1.0, [0x42; 32]);
    assert_eq!(condition.2.mojos(), 7);
}
