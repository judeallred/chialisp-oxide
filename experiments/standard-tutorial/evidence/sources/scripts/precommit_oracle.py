"""Independent expected outcomes for the precommit-claim and precommit-registry ports.

Trees, canonical integers and every hash are computed here with Python hashlib
and the raw-tree codec, without reading any of the three puzzle sources. The
relation is a finite oracle for recorded vectors; it does not authenticate
execution, verify BLS signatures, prove ledger behavior or show that any
commitment scheme is secure.
"""
import hashlib

import raw_tree

PRICE = 1000
SLOT = 1
MINIMUM_AGE = 2
REFUND_AGE = 8
TREASURY = b"\x7e" * 32
NAME = b"alice"
DOMAIN = b"chialisp-oxide/precommit/v1/"


def sha(*parts):
    return hashlib.sha256(b"".join(parts)).digest()


def proper(*items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def fields(value, count):
    result = []
    for _ in range(count):
        if type(value) is not tuple or len(value) != 2:
            return None
        result.append(value[0])
        value = value[1]
    return result if value == b"" else None


def integer(value):
    if value == 0:
        return b""
    width = (value.bit_length() + 8) // 8
    encoded = value.to_bytes(width, "big", signed=True)
    while len(encoded) > 1 and ((encoded[0] == 0 and encoded[1] < 128)
                               or (encoded[0] == 255 and encoded[1] >= 128)):
        encoded = encoded[1:]
    return encoded


def fixed(value, width):
    return type(value) is bytes and len(value) == width


def atom_hash(value):
    return sha(b"\1", value)


def pair_hash(left, right):
    return sha(b"\2", left, right)


def curried_puzzle_hash(module_hash, argument_hash):
    """Tree hash of (a (q . M) (c (q . A) 1)), spelled out from its structure."""
    nil = atom_hash(b"")
    quoted_argument = pair_hash(atom_hash(b"\1"), argument_hash)
    environment = pair_hash(atom_hash(b"\4"), pair_hash(quoted_argument, pair_hash(atom_hash(b"\1"), nil)))
    quoted_module = pair_hash(atom_hash(b"\1"), module_hash)
    return pair_hash(atom_hash(b"\2"), pair_hash(quoted_module, pair_hash(environment, nil)))


def commitment(registry, refund_to, owner_key, owner_destination, salt):
    owner = sha(DOMAIN + b"owner", owner_key, owner_destination)
    opened = sha(DOMAIN + b"opening", owner, salt)
    target = sha(DOMAIN + b"target", registry, NAME)
    return sha(DOMAIN + b"commit", sha(target, refund_to, opened))


def terms_hash(registry, committed, refund_to):
    nil = atom_hash(b"")
    return pair_hash(atom_hash(registry), pair_hash(atom_hash(committed), pair_hash(atom_hash(refund_to), nil)))


def precommit_coin_id(parent, module_hash, registry, committed, refund_to):
    puzzle_hash = curried_puzzle_hash(module_hash, terms_hash(registry, committed, refund_to))
    return sha(parent, puzzle_hash, integer(PRICE))


def accepted(*conditions):
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def claim_relation(environment):
    arguments = fields(environment, 3)
    if arguments is None:
        return {"status": "rejected"}
    terms, claim, me = arguments
    terms = fields(terms, 3)
    if terms is None or claim not in (b"", b"\1") or not fixed(me, 32):
        return {"status": "rejected"}
    registry, committed, refund_to = terms
    if not all(fixed(value, 32) for value in terms):
        return {"status": "rejected"}
    conditions = [proper(integer(70), me), proper(integer(73), integer(PRICE))]
    if claim:
        accept = sha(DOMAIN + b"accept", committed, me)
        conditions += [proper(integer(82), integer(MINIMUM_AGE)),
                       proper(integer(51), TREASURY, integer(PRICE)),
                       proper(integer(60), sha(DOMAIN + b"ready", committed, me)),
                       proper(integer(61), sha(registry, accept))]
    else:
        conditions += [proper(integer(82), integer(REFUND_AGE)),
                       proper(integer(51), refund_to, integer(PRICE)),
                       proper(integer(60), sha(DOMAIN + b"refunded", committed, me))]
    return accepted(*conditions)


def registry_relation(environment):
    arguments = fields(environment, 5)
    if arguments is None:
        return {"status": "rejected"}
    module_hash, me, parent, refund_to, opening = arguments
    opening = fields(opening, 3)
    if opening is None or not all(fixed(value, 32) for value in (module_hash, me, parent, refund_to)):
        return {"status": "rejected"}
    owner_key, owner_destination, salt = opening
    if not (fixed(owner_key, 48) and fixed(owner_destination, 32) and fixed(salt, 32)):
        return {"status": "rejected"}
    committed = commitment(me, refund_to, owner_key, owner_destination, salt)
    precommit = precommit_coin_id(parent, module_hash, me, committed, refund_to)
    return accepted(
        proper(integer(70), me),
        proper(integer(73), integer(SLOT)),
        proper(integer(61), sha(precommit, sha(DOMAIN + b"ready", committed, precommit))),
        proper(integer(60), sha(DOMAIN + b"accept", committed, precommit)),
        proper(integer(50), owner_key, sha(DOMAIN + b"authorize", committed)),
        proper(integer(51), owner_destination, integer(SLOT)),
    )


def expected(slug, solution):
    environment = raw_tree.parse(solution)
    return {"precommit-claim": claim_relation, "precommit-registry": registry_relation}[slug](environment)
