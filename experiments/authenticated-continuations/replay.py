#!/usr/bin/env python3
"""Execute retained programs through the actual CLI, with independent outputs.

Explicit local paths are required; this never builds, signs, funds or submits.
A non-program failure is inconclusive and fails the gate.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import oracle as o


def encode(tree):
    if isinstance(tree, tuple):
        return b"\xff" + encode(tree[0]) + encode(tree[1])
    if not tree:
        return b"\x80"
    if len(tree) == 1 and tree[0] <= 127:
        return tree
    if len(tree) < 64:
        return bytes([128 + len(tree)]) + tree
    raise ValueError("only small fixture atoms")


def condition_tree(conditions):
    return o.proper(*(o.proper(*(o.integer(v) if type(v) is int else bytes.fromhex(v)
                                for v in condition)) for condition in conditions))


def vectors():
    f = o.fixtures()
    module = f["fixture_module_hash"]
    rows = []
    def add(label, solution, conditions=None):
        rows.append(dict(label=label, solution=solution, expected=(
            dict(status="accepted", output_hex=encode(condition_tree(conditions)).hex())
            if conditions is not None else
            dict(status="rejected", failure=dict(kind="program", code="raise")))))
    def env(s, count=None, recipient=None, next_digest=None, head=None, m=None):
        return f"((0x{m or module} 0x{head or s['head']} {count or s['remaining']}) (0x{recipient or s['recipient']} 0x{next_digest or s['next']}))"
    for i, s in enumerate(f["steps"]):
        add(f"queue step {i+1}", env(s), s["conditions"])
    a,b,c = f["steps"]
    for label, kwargs in [
        ("skip first item", dict(recipient=b["recipient"],next_digest=b["next"])),
        ("reorder recipient", dict(recipient=b["recipient"])),
        ("inject recipient", dict(recipient="44"*32)),
        ("wrong next digest", dict(next_digest=c["head"])),
        ("wrong committed head", dict(head="55"*32)),
        ("early terminal count", dict(count="1")),
        ("zero count", dict(count="()")),
        ("count above finite bound", dict(count="9")),
        ("count u8 maximum", dict(count="255")),
        ("count oversized u8", dict(count="256")),
        ("negative count", dict(count="-1")),
        ("padded count", dict(count="0x0003")),
        ("pair count", dict(count="(3)")),
        ("short module", dict(m="aa"*31)),
        ("long module", dict(m="aa"*33)),
        ("short head", dict(head="55"*31)),
        ("long recipient", dict(recipient="11"*33)),
        ("short next digest", dict(next_digest="55"*31)),
    ]:
        add(label,env(a,**kwargs))
    add("excess terminal continuation",env(c,count="2"))
    for label, solution in [
        ("nil environment", "()"),
        ("atom environment", "1"),
        ("missing opening", f"((0x{module} 0x{a['head']} 3))"),
        ("extra argument", env(a)[:-1]+" ())"),
        ("improper environment", env(a)[:-1]+" . 1)"),
        ("nil terms",f"(() (0x{a['recipient']} 0x{a['next']}))"),
        ("extra terms field",env(a).replace(" 3)"," 3 4)")),
        ("improper terms",env(a).replace(" 3)"," 3 . 4)")),
        ("pair module",env(a).replace(f"0x{module}","(1)")),
        ("nil opening",f"((0x{module} 0x{a['head']} 3) ())"),
        ("extra opening field",env(a)[:-2]+" 1))"),
        ("improper opening",env(a)[:-2]+" . 1))"),
        ("pair recipient",env(a).replace(f"0x{a['recipient']}","(1)")),
    ]:
        add(label,solution)
    # Unoptimized strict ABI emits a nil equality on each final tail. A pair
    # supplied as that tail produces typed InvalidOpArg before policy code;
    # optimized Rust and the explicit reference shape guards raise instead.
    for row in rows:
        if row["label"] in {"extra argument", "extra terms field", "extra opening field"}:
            row["failure_by_mode"] = {"rust_off": dict(kind="program", code="invalid_op_arg")}
    # An arbitrary changed module is permitted by template semantics. Initial
    # funding, rather than this template, must reject a mismatched selection.
    changed = "bb"*32
    out = o.step(bytes.fromhex(changed),bytes.fromhex(a["head"]),3,
                 bytes.fromhex(a["recipient"]),bytes.fromhex(a["next"]))
    add("changed module changes successor; funding preflight required",env(a,m=changed),out)
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--rust-on", required=True, type=Path)
    parser.add_argument("--rust-off", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    rows = vectors()
    (args.output/"vectors.json").write_text(json.dumps(rows,indent=2)+"\n")
    solutions=args.output/"solutions.json"
    solutions.write_text(json.dumps([r["solution"] for r in rows]))
    report=dict(schema=1,scope="finite template primary-VM executions; no SDK spends, build authentication or proof", cases=len(rows),programs={})
    for name,path in [("rust_on",args.rust_on),("rust_off",args.rust_off),("chialisp",args.reference)]:
        raw=path.read_bytes()
        completed=subprocess.run([str(args.cli.resolve()),"run-batch",str(path.resolve()),"--solutions",str(solutions.resolve()),"--max-cost","10000000"],capture_output=True,check=True)
        (args.output/f"{name}.json").write_bytes(completed.stdout)
        actual=json.loads(completed.stdout)
        if len(actual)!=len(rows): raise SystemExit("wrong result count")
        costs=[]
        for case,result in zip(rows,actual):
            expected=case["expected"]
            if result["outcome"]!= {k:v for k,v in expected.items() if k!="failure"}:
                raise SystemExit(f"{name}: {case['label']}: outcome {result}")
            failure = case.get("failure_by_mode", {}).get(name, expected.get("failure"))
            if "failure" in expected and result.get("failure")!=failure:
                raise SystemExit(f"{name}: {case['label']}: typed failure {result}")
            if result["outcome"]["status"]=="accepted":
                costs.append(dict(label=case["label"],cost=result["cost"]))
        report["programs"][name]=dict(program_hex_sha256=hashlib.sha256(raw).hexdigest(),serialized_bytes=len(bytes.fromhex(raw.decode().strip())),accepted_costs=costs)
    report["status"]="passed"
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report,indent=2))

if __name__=="__main__": main()
