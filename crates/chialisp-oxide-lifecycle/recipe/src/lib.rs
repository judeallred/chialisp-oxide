//! Original WIP permissionless teaching payment. Never use with real funds.
use chialisp_oxide::{
    Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin,
};

pub const COIN_AMOUNT: Amount = Amount::from_mojos(1000);
pub type Conditions = (AssertMyAmount, CreateCoin);

/// Commit this exact destination by curry before funding the resulting reveal.
/// The spending solution is nil. Anyone can submit the committed full payment.
#[chialisp_oxide::puzzle]
pub fn pay(destination: PuzzleHash) -> Conditions {
    (
        assert_my_amount(COIN_AMOUNT),
        create_coin(destination, COIN_AMOUNT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_complete_conditions_for_distinct_destinations() {
        for destination in [[0; 32], [0x42; 32], [0xff; 32]] {
            let (assertion, output) = pay(PuzzleHash(destination));
            assert_eq!((assertion.0, assertion.1.mojos()), (73, 1000));
            assert_eq!(
                (output.0, output.1.0, output.2.mojos()),
                (51, destination, 1000)
            );
        }
    }
}
