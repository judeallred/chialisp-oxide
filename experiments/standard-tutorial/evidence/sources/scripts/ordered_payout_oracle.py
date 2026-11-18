"""Independent expected outcomes for the ordered-payout continuation port.

Queue digests, canonical integers, tree hashes and the standard curry hash are
computed here with Python hashlib and the raw-tree codec, without reading the
Rust, Chialisp or Rue sources. This finite oracle checks recorded vectors; it
does not authenticate execution, prove consensus behavior, or show that the
queue commitment is secure (SHA-256 collision and preimage resistance are
assumptions).
"""
import hashlib

import raw_tree

PAYOUT = 1000
U64_MAX = (1 << 64) - 1
DOMAIN = b"chialisp-oxide/ordered-payout/v1/1000:"
CREATE_COIN = 51
ASSERT_MY_AMOUNT = 73


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
    """Canonical signed CLVM integer atom."""
    if value == 0:
        return b""
    width = (value.bit_length() + 8) // 8
    encoded = value.to_bytes(width, "big", signed=True)
    while len(encoded) > 1 and ((encoded[0] == 0 and encoded[1] < 128)
                               or (encoded[0] == 255 and encoded[1] >= 128)):
        encoded = encoded[1:]
    return encoded


def canonical_u64(atom):
    """Strict typed u64 decoding: canonical, nonnegative and in range, else None."""
    if type(atom) is not bytes or len(atom) > 9:
        return None
    value = int.from_bytes(atom, "big", signed=True) if atom else 0
    if value < 0 or value > U64_MAX or integer(value) != atom:
        return None
    return value


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


def end():
    return sha(DOMAIN, b"end")


def item(recipient, next_digest):
    return sha(DOMAIN, recipient, next_digest)


def queue(recipients):
    """Every digest from the head to the terminal, in queue order."""
    digests = [end()]
    for recipient in reversed(recipients):
        digests.append(item(recipient, digests[-1]))
    return list(reversed(digests))


def instance(module, head):
    terms = pair_hash(atom_hash(module), pair_hash(atom_hash(head), atom_hash(b"")))
    return curried_puzzle_hash(module, terms)


def relation(environment):
    arguments = fields(environment, 4)
    if arguments is None:
        return {"status": "rejected"}
    terms, recipient, next_digest, amount_atom = arguments
    terms = fields(terms, 2)
    if terms is None:
        return {"status": "rejected"}
    module, head = terms
    amount = canonical_u64(amount_atom)
    if not all(fixed(value, 32) for value in (module, head, recipient, next_digest)) or amount is None:
        return {"status": "rejected"}
    if item(recipient, next_digest) != head:
        return {"status": "rejected"}
    conditions = [proper(integer(ASSERT_MY_AMOUNT), integer(amount)),
                  proper(integer(CREATE_COIN), recipient, integer(PAYOUT))]
    if next_digest == end():
        if amount != PAYOUT:
            return {"status": "rejected"}
    else:
        if amount < 2 * PAYOUT:
            return {"status": "rejected"}
        conditions.append(proper(integer(CREATE_COIN), instance(module, next_digest), integer(amount - PAYOUT)))
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def expected(solution):
    return relation(raw_tree.parse(solution))
