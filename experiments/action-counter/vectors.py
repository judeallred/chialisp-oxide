#!/usr/bin/env python3
"""Independent whole-environment specification; no compiler, VM or SDK run.

Tuple trees are pairs, bytes are exact atoms. Bound codec work before extending
outputs. Expected acceptance is an independently specified finite action ABI.
"""
import sys
sys.dont_write_bytecode = True
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

HERE=Path(__file__).resolve().parent
MAX_COUNTER=(1<<64)-1
RESET_INTENT=b"chialisp-oxide/counter/reset:v1!"
# Public fixture data, not a secret or an assertion of signing capability.
OWNER=bytes.fromhex("8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5")
assert len(RESET_INTENT)==32 and len(OWNER)==48


def proper(values):
    result=b""
    for value in reversed(values):result=(value,result)
    return result


def atom_integer(n):
    assert 0<=n<1<<72
    return b"" if n==0 else n.to_bytes((n.bit_length()+8)//8,"big")


def encode(tree):
    pending=[(tree,1)];output=bytearray();nodes=0
    while pending:
        value,depth=pending.pop();nodes+=1
        assert nodes<=8192 and depth<=128
        if type(value) is tuple:
            assert len(value)==2 and len(output)<65536
            output.append(255);pending.extend(((value[1],depth+1),(value[0],depth+1)))
        else:
            assert type(value) is bytes and len(value)<=8192
            n=len(value)
            if n==1 and value[0]<128:prefix=b""
            elif n<64:prefix=bytes([128+n])
            elif n<8192:prefix=bytes([192+(n>>8),n&255])
            else:prefix=b"\xe0"+n.to_bytes(2,"big")
            assert len(output)+len(prefix)+n<=65536
            output.extend(prefix);output.extend(value)
    return bytes(output)


def text(tree):
    encode(tree)
    return "("+text(tree[0])+" . "+text(tree[1])+")" if type(tree) is tuple else "0x"+tree.hex() if tree else "()"


def fields(tree):
    result=[]
    while type(tree) is tuple:
        if len(result)>=3:return None
        result.append(tree[0]);tree=tree[1]
    return result if tree==b"" else None


def decode_count(atom):
    if type(atom) is not bytes or len(atom)>9 or atom and atom[0]&128:return None
    count=int.from_bytes(atom,"big")
    return count if count<=MAX_COUNTER and atom_integer(count)==atom else None


def expected(export,environment,control="original"):
    values=fields(environment)
    if values is None or len(values)!=(1 if export=="increment" else 2):return {"status":"rejected","stage":"abi"}
    if export=="reset":
        owner,truth=values
        if type(owner) is not bytes or len(owner)!=48:return {"status":"rejected","stage":"abi"}
    else:truth=values[0]
    if type(truth) is not tuple:return {"status":"rejected","stage":"abi"}
    ephemeral,count_atom=truth;count=decode_count(count_atom)
    if count is None:return {"status":"rejected","stage":"abi"}
    if export=="increment":
        if count==MAX_COUNTER and control!="increment_no_change":return {"status":"rejected","stage":"policy"}
        result=((ephemeral,atom_integer(count+1 if control!="increment_no_change" else count)),b"")
    else:
        conditions=b"" if control=="reset_drop_signature" else proper([proper([atom_integer(50),owner,RESET_INTENT])])
        result=((ephemeral,b""),conditions)
    return {"status":"accepted","output_hex":encode(result).hex()}


def cases():
    rows=[]
    def add(export,name,environment):
        rows.append({"export":export,"name":name,"environment":text(environment),"environment_hex":encode(environment).hex(),"expected":expected(export,environment)})
    ephemeral=[("nil",b""),("zero-byte",b"\x00"),("improper-pair",(b"a",b"tail")),("nested-list",proper([b"",b"\x00"]))]
    counts=[0,1,127,128,1<<63,MAX_COUNTER-1,MAX_COUNTER]
    for export in ("increment","reset"):
        def env(truth):return proper([truth] if export=="increment" else [OWNER,truth])
        for e_name,e in ephemeral:
            for count in counts:add(export,e_name+"-count-"+str(count),env((e,atom_integer(count))))
        invalid=[("zero-byte",b"\x00"),("padded-zero",b"\x00\x00"),("padded-one",b"\x00\x01"),("negative",b"\xff"),("u64-plus-one",atom_integer(1<<64)),("nine-byte-high",b"\x01"+b"\x00"*8),("wide-padded",b"\x00"*32),("pair",(b"",b"")),("list",proper([b"\x01"]))]
        for label,count in invalid:add(export,"invalid-count-"+label,env((b"",count)))
        for label,truth in [("nil",b""),("atom",b"x"),("proper-two-list",proper([b"",b"\x01"]))]:add(export,"invalid-truth-"+label,env(truth))
        good=(b"",b"\x01");args=[good] if export=="increment" else [OWNER,good]
        for label,raw in [("nil",b""),("atom",b"x"),("missing",proper(args[:-1])),("extra",proper(args+[b""])),("improper-tail",(args[0],b"bad-tail"))]:add(export,"invalid-environment-"+label,raw)
        add(export,"opaque-1024-byte-ephemeral",env((b"\x00"*1024,b"\x01")))
    for width in [0,1,47,49,96]:add("reset","invalid-owner-width-"+str(width),proper([b"\x77"*width,(b"",b"\x01")]))
    add("reset","invalid-owner-pair",proper([(OWNER,b""),(b"",b"\x01")]))
    add("reset","owner-zero48-is-width-valid-not-curve-proof",proper([b"\x00"*48,(b"",b"\x01")]))
    assert len({(r['export'],r['name']) for r in rows})==len(rows)
    return {"protocol":"chialisp-oxide-action-counter-vector-spec/v1","status":"specified_not_executed","scope":"Independent finite whole-environment acceptance and exact outputs only; expected stages distinguish typed ABI from body overflow. No observed typed VM failure, BLS validation, action-layer or complete SDK spend credit.","cases":rows}


def check_provenance():
    sys.path.insert(0,str(HERE.parents[1]/"scripts"))
    from action_counter_provenance import check
    return check()


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--write',action='store_true');args=parser.parse_args()
    value=cases();payload=json.dumps(value,indent=2)+'\n'
    if args.write:(HERE/'vectors.json').write_text(payload)
    else:assert (HERE/'vectors.json').read_text()==payload,'stale independent vector specification'
    check_provenance()
    print('PASS retained-source identities and',len(value['cases']),'independent vector specifications; no native/compiler/VM/SDK execution')
if __name__=='__main__':main()
