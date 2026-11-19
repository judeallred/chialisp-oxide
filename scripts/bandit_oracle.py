"""Independent expected outcomes for the one-armed-bandit port.

The seed commitment, outcome byte, paytable and payouts are computed here with
Python hashlib and the raw-tree codec, without reading the Rust, Chialisp or Rue
sources. This finite oracle checks recorded vectors; it does not authenticate
execution, prove consensus behavior, or show the game is fair beyond its stated
assumptions (SHA-256, an unpredictable house seed that is never reused, and a
player salt chosen only after the commitment is published).
"""
import hashlib

import ordered_payout_oracle as shapes
import raw_tree

BET = 1000
PULL_TOTAL = 8000
RESOLVE_WINDOW = 10
JACKPOT_BELOW, PAIR_BELOW = 16, 64
JACKPOT_MULTIPLIER, PAIR_MULTIPLIER = 7, 2
SEED_DOMAIN = b"chialisp-oxide/bandit/v1/seed:00"
SPIN_DOMAIN = b"chialisp-oxide/bandit/v1/spin:00"
ASSERT_MY_AMOUNT, ASSERT_HEIGHT_RELATIVE, ASSERT_BEFORE_HEIGHT_RELATIVE, CREATE_COIN = 73, 82, 86, 51


def sha(*parts):
    return hashlib.sha256(b"".join(parts)).digest()


def commitment(seed):
    return sha(SEED_DOMAIN, seed)


def spin(seed, salt):
    return sha(SPIN_DOMAIN, seed, salt)[31]


def payout(outcome):
    if outcome < JACKPOT_BELOW:
        return BET * JACKPOT_MULTIPLIER
    if outcome < PAIR_BELOW:
        return BET * PAIR_MULTIPLIER
    return 0


def expected_return_numerator():
    """Exact total payout over all 256 outcome bytes, in units of BET."""
    return sum(payout(outcome) for outcome in range(256)) // BET


def relation(environment):
    fields, fixed, proper, integer = shapes.fields, shapes.fixed, shapes.proper, shapes.integer
    arguments = fields(environment, 3)
    if arguments is None:
        return {"status": "rejected"}
    terms, resolve, seed = arguments
    terms = fields(terms, 4)
    if terms is None or resolve not in (b"", b"\1") or not all(fixed(v, 32) for v in (*terms, seed)):
        return {"status": "rejected"}
    house, committed, player, salt = terms
    conditions = [proper(integer(ASSERT_MY_AMOUNT), integer(PULL_TOTAL))]
    if resolve:
        if commitment(seed) != committed:
            return {"status": "rejected"}
        won = payout(spin(seed, salt))
        conditions += [proper(integer(ASSERT_BEFORE_HEIGHT_RELATIVE), integer(RESOLVE_WINDOW)),
                       proper(integer(CREATE_COIN), house, integer(PULL_TOTAL - won))]
        if won:
            conditions.append(proper(integer(CREATE_COIN), player, integer(won)))
    else:
        conditions += [proper(integer(ASSERT_HEIGHT_RELATIVE), integer(RESOLVE_WINDOW)),
                       proper(integer(CREATE_COIN), player, integer(PULL_TOTAL))]
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def expected(solution):
    return relation(raw_tree.parse(solution))
