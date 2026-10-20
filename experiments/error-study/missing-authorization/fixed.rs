use chialisp_oxide::{AggSigMe, Amount, CreateCoin, Message32, PublicKey, PuzzleHash, agg_sig_me, create_coin};

const SIGNED_ABOVE: Amount = Amount::from_mojos(100);
const STUDY_MESSAGE: Message32 = Message32([0x77; 32]);

pub fn puzzle(public_key: PublicKey, recipient: PuzzleHash, amount: Amount) -> (CreateCoin, Option<AggSigMe>) {
    let signature = if amount.is_at_most(SIGNED_ABOVE) {
        None
    } else {
        Some(agg_sig_me(public_key, STUDY_MESSAGE))
    };
    (create_coin(recipient, amount), signature)
}

#[test]
fn small_payments_need_no_signature() {
    for mojos in [7, 100] {
        let (_, signature) = puzzle(PublicKey([0x11; 48]), PuzzleHash([0x42; 32]), Amount::from_mojos(mojos));
        assert!(signature.is_none());
    }
}

#[test]
fn large_payment_requires_a_signature() {
    let (payment, signature) = puzzle(PublicKey([0x11; 48]), PuzzleHash([0x42; 32]), Amount::from_mojos(101));
    assert_eq!(payment.2.mojos(), 101);
    let signature = signature.expect("signature condition");
    assert_eq!((signature.0, signature.1.0, signature.2.0), (50, [0x11; 48], [0x77; 32]));
}
