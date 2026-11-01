#!/usr/bin/env python3
"""Independent host batch-two recipe, not a compiler, VM or funding admission.

Contract shares batch-one fixed-byte commitments; no batch-one implementation is
imported. A selected module hash is an input to this recipe, not authentication.
"""
import hashlib
import json
from pathlib import Path

PAYOUT = 1000
COUNTS = (2, 4, 6, 8)
ITEM_DOMAIN = b'chialisp-oxide/continuation/v1/item:1000'
END_DOMAIN = b'chialisp-oxide/continuation/v1/end:1000'


def require(value, message):
    if not value: raise ValueError(message)


def fixed(value):
    require(type(value) is bytes and len(value) == 32, 'fixed 32-byte field')
    return value


def sha(value):
    return hashlib.sha256(value).digest()


def terminal():
    return sha(END_DOMAIN)


def item(recipient, next_digest):
    return sha(ITEM_DOMAIN+fixed(recipient)+fixed(next_digest))


def proper(*values):
    result = b''
    for value in reversed(values): result = (value, result)
    return result


def tree_hash(tree):
    if type(tree) is bytes: return sha(b'\x01'+tree)
    require(type(tree) is tuple and len(tree) == 2, 'atom/pair tree')
    return sha(b'\x02'+tree_hash(tree[0])+tree_hash(tree[1]))


def terms(module, head, count):
    require(type(count) is int and count in COUNTS, 'even finite count')
    return proper(fixed(module), fixed(head), bytes([count]))


def instance_hash(module, head, count):
    # Explicit standard one-parameter curry tree with a pre-hashed module leaf.
    parameters = terms(module, head, count)
    quoted_module = sha(b'\x02'+tree_hash(b'\x01')+module)
    environment = proper(b'\x04', (b'\x01', parameters), b'\x01')
    return sha(b'\x02'+tree_hash(b'\x02')+sha(b'\x02'+quoted_module+tree_hash(proper(environment))))


def step(module, head, count, first, middle, second, next_digest):
    terms(module, head, count)
    require(item(first, middle) == head, 'first opening')
    require(middle != terminal(), 'first opening cannot terminate')
    require(item(second, next_digest) == middle, 'second opening')
    require(first != second, 'duplicate output coins within batch')
    require((count == 2) == (next_digest == terminal()), 'terminal/count mismatch')
    result = [[73, PAYOUT*count], [51, first.hex(), PAYOUT], [51, second.hex(), PAYOUT]]
    if count > 2:
        result.append([51, instance_hash(module, next_digest, count-2).hex(), PAYOUT*(count-2)])
    return result


def plan(selected_module, module, recipients):
    require(fixed(selected_module) == fixed(module), 'selected module binding')
    require(type(recipients) is list and len(recipients) in COUNTS, 'complete even queue length')
    for recipient in recipients: fixed(recipient)
    for index in range(0, len(recipients), 2):
        require(recipients[index] != recipients[index+1], 'pair-distinct funding queue')
    heads = [terminal()]
    for recipient in reversed(recipients): heads.insert(0, item(recipient, heads[0]))
    return [dict(index=index, remaining=len(recipients)-index, head=heads[index].hex(),
                 first=recipients[index].hex(), middle=heads[index+1].hex(),
                 second=recipients[index+1].hex(), next=heads[index+2].hex(),
                 instance_hash=instance_hash(module, heads[index], len(recipients)-index).hex(),
                 conditions=step(module, heads[index], len(recipients)-index,
                                 recipients[index], heads[index+1], recipients[index+1], heads[index+2]))
            for index in range(0, len(recipients), 2)]


def fixtures():
    module = bytes.fromhex('aa'*32)
    recipients = [bytes([value])*32 for value in (17, 34, 51, 68)]
    return dict(schema=1, scope='host recipe fixtures only; no compiled/spend/proof evidence',
                fixture_module_hash=module.hex(), terminal=terminal().hex(),
                steps=plan(module, module, recipients))


if __name__ == '__main__':
    expected = json.loads(Path(__file__).with_name('fixtures.json').read_text())
    require(fixtures() == expected, 'fixture mismatch')
    print(json.dumps(expected, indent=2))
