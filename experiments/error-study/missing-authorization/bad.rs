use chialisp_oxide::{AggSigMe, Amount, CreateCoin, PublicKey, PuzzleHash, create_coin};

pub fn puzzle(_public_key: PublicKey, recipient: PuzzleHash, amount: Amount) -> (CreateCoin, Option<AggSigMe>) {
    (create_coin(recipient, amount), None)
}

#[test]
fn small_payments_pass() {
    for mojos in [7, 100] {
        let (payment, signature) = puzzle(PublicKey([0x11; 48]), PuzzleHash([0x42; 32]), Amount::from_mojos(mojos));
        assert_eq!((payment.0, payment.2.mojos()), (51, mojos));
        assert!(signature.is_none());
    }
}

#[test]
fn large_payment_requires_a_signature() {
    let (_, signature) = puzzle(PublicKey([0x11; 48]), PuzzleHash([0x42; 32]), Amount::from_mojos(101));
    assert!(signature.is_some());
}
