#!/usr/bin/env python3
"""Complete retained-source/bundle consistency; no execution authentication."""
import argparse
import hashlib
import bounded_process
import json
from pathlib import Path
import subprocess
import sys
import admission as a
import vm_witnesses

SOURCE_FILES={'Cargo.toml','Cargo.lock','src/lib.rs','reference.clsp','oracle.py','fixtures.json',
              'test_oracle.py','test_retained.py','test_bounded_process.py','bounded_process.py','vm_witnesses.py','replay.py','examples/compile_reference.rs','admission.py','check_retained.py'}
DEPENDENCIES={'scripts/cargo_bundle.py','scripts/cargo_source_receipt.py','scripts/cargo_authoring.py','scripts/raw_tree.py','scripts/vm_failures.py','scripts/source_walk.py','crates/chialisp-oxide-vm/failure-protocol.json'}
TOOLS={'cargo-chialisp-oxide','chialisp-oxide-driver','chialisp-oxide'}


def check(base):
    base=base.resolve()
    packet=base/'evidence'
    reader=a.cargo_bundle._Reader()
    receipt=reader.json(a.cargo_bundle._contained(packet,'receipt.json'))[0]
    a.require(receipt.get('schema')==1 and set(receipt['source_files'])==SOURCE_FILES,'closed expected current source set')
    a.require(set(receipt['reader_dependencies'])==DEPENDENCIES and set(receipt['tool_files'])==TOOLS,'closed reader/tool set')
    for root,records in ((base,receipt['source_files']),(base.parents[1],receipt['reader_dependencies']),(packet,receipt['files'])):
        for name,digest in records.items():
            path=a.cargo_bundle._contained(root.resolve(),name)
            payload=reader.bytes(path)
            a.require(hashlib.sha256(payload).hexdigest()==digest,f'stale retained file {name}')
    actual={str(p.relative_to(packet)) for p in packet.rglob('*') if p.is_file()}
    a.require(actual==set(receipt['files'])|{'receipt.json'},'retained file inventory mismatch')
    a.require({p.name for p in packet.iterdir() if p.is_file()}=={'receipt.json','chialisp.json','reference.clvm.hex','report.json','rust_off.json','rust_on.json','vectors.json','ir-recompile.json'},'closed complete top-level VM packet files')
    recompiles=json.loads((packet/'ir-recompile.json').read_text())
    a.require(len(recompiles)==2 and {r['mode'] for r in recompiles}=={'on','off'},'complete retained IR recompile modes')
    options=['--manifest-path','experiments/authenticated-continuations/Cargo.toml','--package',a.PACKAGE,'--locked']
    for mode in ('on','off'):
        a.admit(packet/f'rust-{mode}',mode,compiler=receipt['compiler'],driver_sha256=receipt['tool_files']['chialisp-oxide-driver'],
            sources={n:(base/n).read_bytes() for n in a.SOURCE_NAMES},cargo_options=options)
        a.check_recompiled(packet/f'rust-{mode}',mode,next(r for r in recompiles if r['mode']==mode),receipt['verifier_sha256'])
    results={m:a.cargo_bundle.decode_json(reader.bytes(a.cargo_bundle._contained(packet,m+'.json'))) for m in ('rust_on','rust_off','chialisp')}
    program_hex={m:reader.bytes(next((packet/('rust-'+m.removeprefix('rust_'))).glob('*.clvm.hex'))) for m in ('rust_on','rust_off')}
    program_hex['chialisp']=reader.bytes(a.cargo_bundle._contained(packet,'reference.clvm.hex'))
    vm_witnesses.base(a.cargo_bundle.decode_json(reader.bytes(a.cargo_bundle._contained(packet,'vectors.json'))),
        reader.json(a.cargo_bundle._contained(packet,'report.json'))[0],results,program_hex)
    for root,records in ((base,receipt['source_files']),(base.parents[1],receipt['reader_dependencies'])):
        a.require({n:hashlib.sha256(reader.bytes(a.cargo_bundle._contained(root.resolve(),n))).hexdigest() for n in records}==records,'sources changed during retained admission')
    return receipt


def main():
    base=Path(__file__).resolve().parent
    check(base)
    parser=argparse.ArgumentParser()
    parser.add_argument('--replay-cli',type=Path)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--verifier',type=Path)
    args=parser.parse_args()
    a.require(bool(args.replay_cli)==bool(args.output),'replay requires CLI and new output')
    a.require(not args.verifier or args.replay_cli,'verifier requires actual replay output')
    if args.replay_cli:
        packet=base/'evidence'
        paths={m:next((packet/f'rust-{m}').glob('*.clvm.hex')) for m in ('on','off')}
        bounded_process.run([sys.executable,'-B',str(base/'replay.py'),'--cli',str(args.replay_cli),
            '--rust-on',str(paths['on']),'--rust-off',str(paths['off']),'--reference',str(packet/'reference.clvm.hex'),
            '--output',str(args.output)],check=True,timeout=60)
        a.require(json.loads((args.output/'report.json').read_text())==json.loads((packet/'report.json').read_text()),'retained replay differs')
        if args.verifier:
            recompiled=[a.recompile(packet/f'rust-{m}',m,args.verifier) for m in ('on','off')]
            (args.output/'ir-recompile.json').write_text(json.dumps(recompiled,indent=2)+'\n')
    print('complete retained source/bundle consistency passed; reader alone adds no execution credit')

if __name__=='__main__':main()
