"""Bounded compiled-artifact codec; separate from the narrow source fixtures.

The retained Chialisp fixture decoder remains unchanged. This consumer profile
allows 1MiB/65,536nodes/512depth and charges at most16MiB of modeled codec work
before building a decoded tree. These are host transport limits, not policy or
compiler guards, execution authentication, or exact Python heap measurements.
"""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"scripts"))
from registry_codegen_evidence import inspect_compiled_artifact, require
MAX_BYTES=1024*1024
MAX_NODES=65536
MAX_DEPTH=512
MAX_WORK=16*1024*1024


def inspect(raw):
    record=inspect_compiled_artifact(raw,max_bytes=MAX_BYTES,max_nodes=MAX_NODES,max_depth=MAX_DEPTH)
    stats=record["stats"]
    charge=stats["nodes"]*192+stats["atom_payload_bytes"]+len(raw)
    require(charge<=MAX_WORK,"compiled decoded allocation/work charge exceeded")
    record["decoded_work_profile"]={"charged_bytes":charge,"maximum":MAX_WORK,"node_charge":192,"scope":"Conservative abstract allocation/work units; not Python heap measurement."}
    return record


def identity(raw):
    return inspect(raw)["identity"]


def decode(raw):
    # Complete canonical plain preflight precedes all decoded-node allocation.
    inspect(raw)
    cursor=0;stack=[]
    while True:
        head=raw[cursor];cursor+=1
        if head==255:
            stack.append(None);continue
        if head<128:value=bytes([head])
        elif head==128:value=b""
        else:
            width=0
            while head & (128>>width):width+=1
            length=head & ((1<<(7-width))-1)
            for byte in raw[cursor:cursor+width-1]:length=length*256+byte
            cursor+=width-1
            value=raw[cursor:cursor+length];cursor+=length
        while stack and stack[-1] is not None:value=(stack.pop(),value)
        if stack:stack[-1]=value
        else:return value


def encode(tree):
    pending=[(tree,1)];output=bytearray();nodes=0;work=0
    while pending:
        value,depth=pending.pop();nodes+=1;work+=192
        require(nodes<=MAX_NODES and depth<=MAX_DEPTH and work<=MAX_WORK,"compiled serialization shape/work budget")
        if type(value) is tuple:
            require(len(value)==2 and nodes+len(pending)+2<=MAX_NODES and depth<MAX_DEPTH,"compiled pair budget before children")
            require(len(output)<MAX_BYTES,"compiled pair byte budget before allocation")
            output.append(255);pending.extend(((value[1],depth+1),(value[0],depth+1)))
        else:
            require(type(value) is bytes and len(value)<=MAX_BYTES,"compiled atom byte bound")
            work+=len(value);require(work<=MAX_WORK,"compiled payload work bound")
            length=len(value)
            if length==1 and value[0]<128:prefix=b""
            else:
                width=max(1,(length.bit_length()+7)//7)
                prefix=bytes([((255<<(8-width))&255)|(length>>(8*(width-1)))])+length.to_bytes(width,"big")[1:]
            require(len(output)+len(prefix)+length<=MAX_BYTES,"compiled output bound before allocation")
            output.extend(prefix);output.extend(value)
        require(len(output)<=MAX_BYTES,"compiled pair output byte bound")
    raw=bytes(output);inspect(raw);return raw


def self_test(artifacts):
    for artifact in artifacts:
        raw=bytes.fromhex(artifact["clvm_hex"])
        require(encode(decode(raw))==raw,"actual extracted canonical plain round trip")
    controls=[b"\x80\x80",b"\x81\x01",b"\xff"*512+b"\x80"*513,b"\xff\x80"*32768+b"\x80",b"\x80"*(MAX_BYTES+1)]
    for raw in controls:
        try:decode(raw)
        except ValueError:continue
        raise ValueError("compiled codec corruption accepted")
    # Observe the append itself with a tiny profile: rejecting after appending
    # byte MAX+1 would fail this control even if the final exception is correct.
    original_limit=MAX_BYTES;seen=[]
    class ObservedBytearray(bytearray):
        def append(self,value):
            seen.append(len(self));super().append(value)
    globals()["MAX_BYTES"]=1
    globals()["bytearray"]=ObservedBytearray
    try:
        try:encode(((b"",b""),b""))
        except ValueError:pass
        else:raise ValueError("compiled pair precharge control accepted")
        require(seen==[0],"pair byte cap must reject before oversized append")
    finally:
        globals()["MAX_BYTES"]=original_limit
        globals().pop("bytearray",None)
    return len(controls)+1
