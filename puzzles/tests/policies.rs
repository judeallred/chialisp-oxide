//! Ordinary native tests of the same exports built by `cargo oxide`.
//! Signature and timelock enforcement require the separate complete-spend suite.
use oxide_puzzles::{
    boolean_gate, bounded_counter, capped_payment, fixed_payment, recovery_payment,
    signature_payment, split_payment, timelocked_payment, two_signature_payment,
};
use oxide_sdk::{Amount, AssertMyAmount, CreateCoin, Message32};

fn assert_payment(assertion: AssertMyAmount, payment: CreateCoin, expected: u64) {
    // Independent expected wire values deliberately use literal opcodes here.
    assert_eq!((assertion.0, assertion.1.mojos()), (73, 1_000));
    assert_eq!(payment.0, 51);
    assert_eq!(payment.1.0, [0x42; 32]);
    assert_eq!(payment.2.mojos(), expected);
}

#[test]
fn fixed_and_capped_payments_return_the_expected_conditions() {
    let (assertion, payment) = fixed_payment::puzzle();
    assert_payment(assertion, payment, 1_000);
    for amount in [0, 1, 500, 999, 1_000] {
        let (assertion, payment) = capped_payment::puzzle(Amount::from_mojos(amount));
        assert_payment(assertion, payment, amount);
    }
    for amount in [1_001, u64::MAX] {
        assert!(
            std::panic::catch_unwind(|| capped_payment::puzzle(Amount::from_mojos(amount)))
                .is_err()
        );
    }
}

#[test]
fn split_conserves_the_coin_amount_at_each_boundary() {
    for amount in [0, 1, 500, 999, 1_000] {
        let (assertion, first, second) = split_payment::puzzle(Amount::from_mojos(amount));
        assert_payment(assertion, first, amount);
        assert_eq!(second.0, 51);
        assert_eq!(second.1.0, [0x43; 32]);
        assert_eq!(second.2.mojos(), 1_000 - amount);
        assert_eq!(first.2.mojos() + second.2.mojos(), 1_000);
    }
    for amount in [1_001, u64::MAX] {
        assert!(
            std::panic::catch_unwind(|| split_payment::puzzle(Amount::from_mojos(amount))).is_err()
        );
    }
}

#[test]
fn counter_acceptance_has_an_exact_upper_bound() {
    assert_eq!(bounded_counter::puzzle(0), 1);
    assert_eq!(bounded_counter::puzzle(999), 1_000);
    for counter in [1_000, 1_001, u64::MAX] {
        assert!(std::panic::catch_unwind(|| bounded_counter::puzzle(counter)).is_err());
    }
}

#[test]
fn boolean_gate_is_a_caller_supplied_predicate() {
    let (payment,) = boolean_gate::puzzle(true);
    assert_eq!(payment.0, 51);
    assert_eq!(payment.1.0, [0x42; 32]);
    assert_eq!(payment.2.mojos(), 1_000);
    assert!(std::panic::catch_unwind(|| boolean_gate::puzzle(false)).is_err());
}

#[test]
fn timelock_and_recovery_emit_absolute_times_and_distinct_signers() {
    let (timelock, assertion, payment) = timelocked_payment::puzzle();
    assert_eq!((timelock.0, timelock.1.unix_seconds()), (81, 100));
    assert_payment(assertion, payment, 1_000);
    let (owner, owner_time, assertion, payment) =
        recovery_payment::puzzle(false, Message32([7; 32]));
    assert_payment(assertion, payment, 1_000);
    let (recovery, recovery_time, assertion, payment) =
        recovery_payment::puzzle(true, Message32([7; 32]));
    assert_payment(assertion, payment, 1_000);
    assert_eq!((owner.0, recovery.0), (50, 50));
    assert_ne!(owner.1.0, recovery.1.0);
    assert_eq!((owner.2.0, recovery.2.0), ([7; 32], [7; 32]));
    assert_eq!((owner_time.0, owner_time.1.unix_seconds()), (81, 0));
    assert_eq!((recovery_time.0, recovery_time.1.unix_seconds()), (81, 100));
}

#[test]
fn signature_conditions_preserve_both_signers_and_the_message() {
    let (single, assertion, payment) = signature_payment::puzzle(Message32([9; 32]));
    assert_payment(assertion, payment, 1_000);
    let (first, second, assertion, payment) = two_signature_payment::puzzle(Message32([9; 32]));
    assert_payment(assertion, payment, 1_000);
    assert_eq!((single.0, first.0, second.0), (50, 50, 50));
    assert_eq!(single.1.0, first.1.0);
    assert_ne!(first.1.0, second.1.0);
    assert_eq!(
        (single.2.0, first.2.0, second.2.0),
        ([9; 32], [9; 32], [9; 32])
    );
}
