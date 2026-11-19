//! Original teaching game (#20): one pull of a one-armed bandit.
//! WIP prototype; not ready for real funds. Testnet teaching only.

use chialisp_oxide::{
    Amount, AssertHeightRelative, AssertMyAmount, BlockAge, Cons, CreateCoin, Message32,
    PuzzleHash, assert_before_height_relative, assert_height_relative, assert_my_amount,
    create_coin, sha256_parts,
};

/// The player's fixed stake for one pull.
pub const BET: Amount = Amount::from_mojos(1_000);
/// Bet plus the house reserve. It exceeds the largest payout, so the house is
/// never bankrupted mid-pull and always receives a nonzero remainder.
pub const PULL_TOTAL: Amount = Amount::from_mojos(8_000);
/// The house must resolve before this age; afterwards the player takes it all.
pub const RESOLVE_WINDOW: BlockAge = BlockAge::from_blocks(10);
/// Outcome bytes below this pay `JACKPOT_MULTIPLIER` (16 of 256).
pub const JACKPOT_BELOW: u8 = 16;
/// Outcome bytes below this (and not a jackpot) pay `PAIR_MULTIPLIER` (48 of 256).
pub const PAIR_BELOW: u8 = 64;
pub const JACKPOT_MULTIPLIER: u64 = 7;
pub const PAIR_MULTIPLIER: u64 = 2;
const SEED_DOMAIN: [u8; 32] = *b"chialisp-oxide/bandit/v1/seed:00";
const SPIN_DOMAIN: [u8; 32] = *b"chialisp-oxide/bandit/v1/spin:00";

/// Curried as ONE proper list: (house payout, house seed commitment, player
/// payout, player salt). The house publishes its commitment first; the player
/// then chooses the salt. The house must never reuse a seed.
pub type Terms = (PuzzleHash, Message32, PuzzleHash, Message32);
/// The house's remainder (resolve) or the player's whole claim (timeout), then
/// the player's winnings only when a resolved pull pays.
pub type Conditions =
    Cons<AssertMyAmount, Cons<AssertHeightRelative, Cons<CreateCoin, Option<CreateCoin>>>>;

/// `sha256(SEED_DOMAIN || seed)`, published by the house before any salt exists.
pub fn seed_commitment(seed: Message32) -> Message32 {
    Message32(sha256_parts(&SEED_DOMAIN, &seed.0, &[]))
}

/// The outcome byte for a revealed seed and the player's salt.
pub fn spin(seed: Message32, salt: Message32) -> u8 {
    sha256_parts(&SPIN_DOMAIN, &seed.0, &salt.0)[31]
}

/// The player's winnings for one outcome byte, from the published paytable.
pub fn payout(outcome: u8) -> Amount {
    if outcome < JACKPOT_BELOW {
        Amount::from_mojos(BET.mojos() * JACKPOT_MULTIPLIER)
    } else if outcome < PAIR_BELOW {
        Amount::from_mojos(BET.mojos() * PAIR_MULTIPLIER)
    } else {
        Amount::from_mojos(0)
    }
}

/// Resolve (`resolve` true): before the window closes, the revealed seed must
/// open the commitment; the player receives the paytable amount and the house
/// the remainder. Timeout (`resolve` false): at or after the window, the player
/// receives the whole pull, so a withholding house forfeits its reserve. The
/// seed is still decoded on timeout, then ignored.
#[chialisp_oxide::puzzle]
pub fn puzzle(terms: Terms, resolve: bool, seed: Message32) -> Conditions {
    let (house, commitment, player, salt) = terms;
    let (age, first, winnings) = if resolve {
        assert!(seed_commitment(seed).0 == commitment.0);
        let won = payout(spin(seed, salt));
        let remainder = Amount::from_mojos(PULL_TOTAL.mojos() - won.mojos());
        let winnings = if won.mojos() == 0 {
            None
        } else {
            Some(create_coin(player, won))
        };
        (
            assert_before_height_relative(RESOLVE_WINDOW),
            create_coin(house, remainder),
            winnings,
        )
    } else {
        (
            assert_height_relative(RESOLVE_WINDOW),
            create_coin(player, PULL_TOTAL),
            None,
        )
    };
    Cons {
        first: assert_my_amount(PULL_TOTAL),
        rest: Cons {
            first: age,
            rest: Cons {
                first,
                rest: winnings,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact expected return over every outcome byte; not sampling.
    #[test]
    fn published_return_to_player_is_exactly_208_of_256() {
        let returned: u64 = (0..=255u8).map(|outcome| payout(outcome).mojos()).sum();
        assert_eq!(returned, BET.mojos() * (16 * 7 + 48 * 2));
        assert_eq!(16 * 7 + 48 * 2, 208);
        // The largest payout always leaves the house a nonzero remainder.
        assert!(BET.mojos() * JACKPOT_MULTIPLIER < PULL_TOTAL.mojos());
    }
}
