#!/usr/bin/env python3
"""Independent host recipe. Condition generation is not consensus execution."""
import hashlib
import json
from pathlib import Path

MINIMUM_RAISE = 1000
MAXIMUM_ROUND = 8
MINIMUM_AGE = 1
TAKE_WINDOW = 5
TAKE_DOMAIN = b'chialisp-oxide/spuds/take:v1'
CLAIM_DOMAIN = b'chialisp-oxide/spuds/claim:v1'


def require(value, message):
    if not value: raise ValueError(message)


def fixed(value, length):
    require(type(value) is bytes and len(value) == length, 'fixed atom bytes')
    return value


def sha(value):
    return hashlib.sha256(value).digest()


def proper(*values):
    result = b''
    for value in reversed(values): result = (value, result)
    return result


def tree_hash(tree):
    if type(tree) is bytes: return sha(b'\x01'+tree)
    require(type(tree) is tuple and len(tree) == 2, 'atom or pair')
    return sha(b'\x02'+tree_hash(tree[0])+tree_hash(tree[1]))


def terms(module, key, payout, round):
    require(type(round) is int and 1 <= round <= MAXIMUM_ROUND, 'finite round 1..8')
    return proper(fixed(module,32), fixed(key,48), fixed(payout,32), bytes([round]))


def instance_hash(module, key, payout, round):
    parameter = terms(module,key,payout,round)
    # Construct the real standard curry shape, using the module's established
    # tree hash as the only pre-hashed leaf. No SDK/Rust hash helper is imported.
    quoted_module = sha(b'\x02'+tree_hash(b'\x01')+module)
    environment = proper(b'\x04', (b'\x01',parameter), b'\x01')
    return sha(b'\x02'+tree_hash(b'\x02')+sha(b'\x02'+quoted_module+tree_hash(proper(environment))))


def step(module, holder_key, holder_payout, round, take, next_key, next_payout):
    terms(module,holder_key,holder_payout,round)
    require(type(take) is bool, 'exact bool')
    fixed(next_key,48); fixed(next_payout,32)
    amount = round*MINIMUM_RAISE
    if take:
        require(round < MAXIMUM_ROUND, 'maximum pot reached')
        output_hash = instance_hash(module,next_key,next_payout,round+1)
        message = sha(TAKE_DOMAIN+output_hash)
        key, boundary, output_amount = next_key, 86, amount+MINIMUM_RAISE
    else:
        output_hash, output_amount = holder_payout, amount
        message = sha(CLAIM_DOMAIN)
        key, boundary = holder_key, 82
    return [[50,key.hex(),message.hex()], [73,amount], [82,MINIMUM_AGE],
            [boundary,TAKE_WINDOW], [51,output_hash.hex(),output_amount], [60,message.hex()]]


def top_up_announcement_id(game_coin_id, message):
    return sha(fixed(game_coin_id,32)+fixed(message,32))


def fixture_keys():
    # Public teaching keys copied from puzzles/fixture_keys.rs, not custody keys.
    return (bytes.fromhex('8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5'),
            bytes.fromhex('b47fa1e15162a5b1f9444d1aae57d131a1d5c98bade8f8e93b8ebfe1649b95c55787b8731d2f1a0638b3e1d23ada48e8'))


def fixtures():
    first,second = fixture_keys()
    module, payout, next_payout = b'\xaa'*32,b'\x11'*32,b'\x22'*32
    return dict(schema=1,scope='independent host fixtures; no compiled, signature, age or ledger execution credit',
                fixture_module_hash=module.hex(), holder_key=first.hex(), next_key=second.hex(),
                holder_payout=payout.hex(), next_payout=next_payout.hex(), round=1,
                initial_instance_hash=instance_hash(module,first,payout,1).hex(),
                take=step(module,first,payout,1,True,second,next_payout),
                claim=step(module,first,payout,1,False,second,next_payout))


if __name__ == '__main__':
    actual=fixtures()
    require(json.loads(Path(__file__).with_name('fixtures.json').read_text()) == actual,'fixture mismatch')
    print(json.dumps(actual,indent=2))
