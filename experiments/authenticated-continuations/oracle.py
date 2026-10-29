#!/usr/bin/env python3
"""Independent host recipe. No compiler/SDK imports and no execution credit.

A fixed-byte queue commitment is distinct from a CLVM tree hash. The curry
oracle constructs the actual standard curry tree before recursively hashing it.
"""
import hashlib
import json
from pathlib import Path

PAYOUT = 1000
MAX_ITEMS = 8
ITEM_DOMAIN = b"chialisp-oxide/continuation/v1/item:1000"
END_DOMAIN = b"chialisp-oxide/continuation/v1/end:1000"


def sha(data):
    return hashlib.sha256(data).digest()


def atom(value):
    if not isinstance(value, bytes):
        raise ValueError("atom bytes required")
    return value


def proper(*values):
    result = b""
    for value in reversed(values):
        result = (value, result)
    return result


def tree_hash(tree):
    if isinstance(tree, bytes):
        return sha(b"\x01" + tree)
    if isinstance(tree, tuple) and len(tree) == 2:
        return sha(b"\x02" + tree_hash(tree[0]) + tree_hash(tree[1]))
    raise ValueError("expected atom or pair")


def integer(value):
    if type(value) is not int or value < 0:
        raise ValueError("unsigned integer required")
    if value == 0:
        return b""
    data = value.to_bytes((value.bit_length() + 7) // 8, "big")
    return b"\0" + data if data[0] & 128 else data


def terminal():
    return sha(END_DOMAIN)


def item(recipient, next_digest):
    if len(recipient) != 32 or len(next_digest) != 32:
        raise ValueError("fixed 32-byte fields required")
    return sha(ITEM_DOMAIN + recipient + next_digest)


def instance_hash(module, head, remaining):
    if len(module) != 32 or len(head) != 32 or not 1 <= remaining <= MAX_ITEMS:
        raise ValueError("bad terms")
    # Represent the unknown module by its already-established tree hash. Hash
    # every other node of the explicitly constructed standard curry tree.
    terms = proper(module, head, integer(remaining))
    quoted_module_hash = sha(b"\x02" + tree_hash(b"\x01") + module)
    environment = proper(b"\x04", (b"\x01", terms), b"\x01")
    return sha(b"\x02" + tree_hash(b"\x02") + sha(
        b"\x02" + quoted_module_hash + tree_hash(proper(environment))))


def step(module, head, remaining, recipient, next_digest):
    if len(module) != 32 or len(head) != 32 or not 1 <= remaining <= MAX_ITEMS:
        raise ValueError("bad terms")
    if item(recipient, next_digest) != head:
        raise ValueError("opening")
    if (remaining == 1) != (next_digest == terminal()):
        raise ValueError("terminal/count mismatch")
    conditions = [[73, PAYOUT * remaining], [51, recipient.hex(), PAYOUT]]
    if remaining > 1:
        conditions.append([51, instance_hash(module, next_digest, remaining - 1).hex(),
                           PAYOUT * (remaining - 1)])
    return conditions


def funding_hash(selected_module, module, head, remaining):
    if selected_module != module:
        raise ValueError("module binding")
    return instance_hash(module, head, remaining)


def fixtures():
    module = bytes.fromhex("aa" * 32)  # oracle fixture, NOT a compiled artifact
    recipients = [bytes([x]) * 32 for x in (0x11, 0x22, 0x33)]
    heads = [terminal()]
    for recipient in reversed(recipients):
        heads.insert(0, item(recipient, heads[0]))
    return {
        "schema": 1,
        "scope": "independent host recipe fixtures; no compiled/spend/proof evidence",
        "fixture_module_hash": module.hex(),
        "terminal": terminal().hex(),
        "steps": [dict(remaining=3-i, recipient=r.hex(), head=heads[i].hex(),
                       next=heads[i+1].hex(), instance_hash=instance_hash(module, heads[i], 3-i).hex(),
                       conditions=step(module, heads[i], 3-i, r, heads[i+1]))
                  for i, r in enumerate(recipients)],
    }


if __name__ == "__main__":
    expected = json.loads(Path(__file__).with_name("fixtures.json").read_text())
    actual = fixtures()
    if actual != expected:
        raise SystemExit("oracle fixture mismatch")
    print(json.dumps(actual, indent=2))
