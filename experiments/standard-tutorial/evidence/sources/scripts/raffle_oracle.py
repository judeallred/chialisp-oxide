"""Independent expected outcomes for the raffle port.

Commitments, the draw seed, the winner index and the refund queue's curried
puzzle hash are computed here with Python hashlib, the raw-tree codec and the
independent ordered-payout oracle, without reading the raffle's Rust, Chialisp or
Rue sources. This finite oracle checks recorded vectors; it does not authenticate
execution, prove consensus behavior, or show the raffle is fair (SHA-256 is an
assumption, and the last salt holder can learn the outcome and force a refund).
"""
import hashlib

import ordered_payout_oracle as queue
import raw_tree

POT = 4000
REFUND_AGE = 10
COMMIT_DOMAIN = b"chialisp-oxide/raffle/v1/commit"
DRAW_DOMAIN = b"chialisp-oxide/raffle/v1/draw"
REFUND_INTENT = b"chialisp-oxide/raffle/v1/refund!"
ASSERT_MY_AMOUNT, ASSERT_HEIGHT_RELATIVE, ASSERT_BEFORE_HEIGHT_RELATIVE = 73, 82, 86
CREATE_COIN, CREATE_COIN_ANNOUNCEMENT = 51, 60


def sha(*parts):
    return hashlib.sha256(b"".join(parts)).digest()


def commitment(salt):
    return sha(COMMIT_DOMAIN, salt)


def seed(salts):
    return sha(DRAW_DOMAIN, sha(salts[0], salts[1]), sha(salts[2], salts[3]))


def winner_index(salts):
    return seed(salts)[31] % 4


def refund_queue_hash(ordered_module, payouts):
    digests = [queue.end()]
    for payout in reversed(payouts):
        digests.append(queue.item(payout, digests[-1]))
    return queue.instance(ordered_module, digests[-1])


def relation(environment):
    fixed, proper, integer = queue.fixed, queue.proper, queue.integer
    arguments = queue.fields(environment, 3)
    if arguments is None:
        return {"status": "rejected"}
    terms, draw, salts = arguments
    terms, salts = queue.fields(terms, 5), queue.fields(salts, 4)
    if terms is None or salts is None or draw not in (b"", b"\1"):
        return {"status": "rejected"}
    ordered_module, entries = terms[0], [queue.fields(entry, 2) for entry in terms[1:]]
    if not fixed(ordered_module, 32) or any(entry is None for entry in entries):
        return {"status": "rejected"}
    if not all(fixed(value, 32) for entry in entries for value in entry) or not all(fixed(s, 32) for s in salts):
        return {"status": "rejected"}
    payouts = [entry[0] for entry in entries]
    if draw:
        if any(commitment(salt) != entry[1] for salt, entry in zip(salts, entries)):
            return {"status": "rejected"}
        drawn = seed(salts)
        age, destination, message = (ASSERT_BEFORE_HEIGHT_RELATIVE, payouts[drawn[31] % 4], drawn)
    else:
        age, destination, message = (ASSERT_HEIGHT_RELATIVE, refund_queue_hash(ordered_module, payouts), REFUND_INTENT)
    conditions = [proper(integer(ASSERT_MY_AMOUNT), integer(POT)),
                  proper(integer(age), integer(REFUND_AGE)),
                  proper(integer(CREATE_COIN), destination, integer(POT)),
                  proper(integer(CREATE_COIN_ANNOUNCEMENT), message)]
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def expected(solution):
    return relation(raw_tree.parse(solution))
