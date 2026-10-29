#!/usr/bin/env python3
"""Closed offline experiment packet; actual SDK replay is an explicit option."""
import argparse
import hashlib
import bounded_process
import json
from pathlib import Path
import subprocess
import admission as a
import check_retained
import source_controls as s
import ledger_checks as witnesses
import vm_witnesses

CODE_FILES={'experiments/authenticated-continuations/ledger/Cargo.toml',
    'experiments/authenticated-continuations/ledger/Cargo.lock',
    'experiments/authenticated-continuations/ledger/src/main.rs',
    'experiments/authenticated-continuations/ledger/src/controls.rs',
    'experiments/authenticated-continuations/source_controls.py',
    'experiments/authenticated-continuations/check_ledger.py',
    'experiments/authenticated-continuations/ledger_checks.py',
    'experiments/authenticated-continuations/test_ledger.py',
    'experiments/spend-costs/src/measurement.rs'}


def check(base):
    base=base.resolve();root=base.parents[1];packet=base/'ledger/evidence'
    check_retained.check(base)
    reader=a.cargo_bundle._Reader()
    load=lambda n:reader.json(a.cargo_bundle._contained(packet,n))[0]
    r=load('receipt.json')
    a.require(r['schema']==1 and set(r['source_files'])==CODE_FILES,'closed ledger code inputs')
    for directory,records in ((root,r['source_files']),(packet,r['files'])):
        for name,sha in records.items():
            a.require(hashlib.sha256(reader.bytes(a.cargo_bundle._contained(directory,name))).hexdigest()==sha,f'stale ledger input {name}')
    a.require({p.relative_to(packet).as_posix() for p in packet.rglob('*') if p.is_file()}==set(r['files'])|{'receipt.json'},'closed ledger packet inventory')
    a.require(hashlib.sha256((base/'evidence/receipt.json').read_bytes()).hexdigest()==r['spike_receipt_sha256'],'original spike relation')
    spike=json.loads((base/'evidence/receipt.json').read_text())
    a.require(r['compiler']==spike['compiler'] and r['tools']==spike['tool_files'],'same independent compiler/tools')
    source=load('source-controls.json')
    vm_witnesses.controls(source)
    a.require(source['compiler']==r['compiler'] and source['tools']==r['tools'],'fresh source-control tool relation')
    keys=[(x['kind'],x['role'],x['mode']) for x in source['records']]
    expected={(k,role,m) for k in s.KINDS for role in ('original','mutant') for m in ('on','off')}
    a.require(len(keys)==len(expected) and set(keys)==expected,'complete exact original/mutant/mode inventory')
    control_modules={}
    for row in source['records']:
        kind,role,mode=row['kind'],row['role'],row['mode'];mutated=role=='mutant'
        relative=f'controls/{kind}/{role}'
        package=packet/relative
        body=s.edited((base/'src/lib.rs').read_text(),kind if mutated else 'original').encode()
        a.require(reader.bytes(a.cargo_bundle._contained(package,'src/lib.rs'))==body,'exact actual-source mutation recipe')
        a.require(hashlib.sha256(body).hexdigest()==row['source_sha256'],'selected mutated source hash')
        a.require(reader.bytes(a.cargo_bundle._contained(package,'examples/native_case.rs'))==s.native_example(kind,mutated).encode(),'actual native helper binding')
        native=json.loads(reader.bytes(a.cargo_bundle._contained(package,'native.stdout')))
        a.require(native==s.expected(kind,mutated),'actual native output witness')
        a.require(row['manifest']==relative+f'/{mode}/manifest.json','exact bundle location')
        captured=r['origin_work']+f'/{kind}-{role}'
        manifest=a.admit(package/mode,mode,compiler=r['compiler'],driver_sha256=r['tools']['chialisp-oxide-driver'],
            sources={n:reader.bytes(a.cargo_bundle._contained(package,n)) for n in a.SOURCE_NAMES},
            cargo_options=['--manifest-path',captured+'/Cargo.toml','--package',a.PACKAGE,'--locked'],drop_amount=mutated and kind=='drop_amount')
        a.require(manifest['workspace_root']==captured,'retained origin relation without host reads')
        a.require(row['program']==relative+f'/{mode}/'+manifest['entries'][0]['clvm'],'selected program relation')
        artifact=manifest['entries'][0]['artifact']
        control_modules[(kind,role,mode)]=(bytes.fromhex(artifact['tree_hash']),artifact['bytes'])
        a.check_recompiled(package/mode,mode,row['recompiled'],source['verifier_sha256'])
    a.require(source['counts']==dict(native_observations=8,primary_vm_observations=16,sdk_attempts=16),'separate actual-source counts')
    base_report=load('base-report.json');controls=load('controls-report.json')
    a.require(base_report['counts']==dict(scenario_runs=3,accepted_submissions=12,typed_rejected_submissions=21,
        runtime_umbrella_submissions=12,total_sdk_attempts=45,successor_coins=9),'base ledger counts')
    a.require(controls['counts']==dict(selected_control_builds=8,accepted_submissions=10,typed_rejected_submissions=2,
        runtime_umbrella_submissions=4,total_sdk_attempts=16),'actual-source ledger counts')
    modules={}
    for mode in ('on','off'):
        manifest=json.loads((base/'evidence'/f'rust-{mode}'/'manifest.json').read_text());artifact=manifest['entries'][0]['artifact']
        modules['rust_'+mode]=(bytes.fromhex(artifact['tree_hash']),artifact['bytes'])
    reference=bytes.fromhex((base/'evidence/reference.clvm.hex').read_text().strip())
    modules['chialisp']=(a.raw_tree.tree_hash(a.raw_tree.deserialize(reference)),len(reference))
    witnesses.base(base_report,modules)
    witnesses.controls(controls,control_modules)
    for name,sha in r['source_files'].items():
        a.require(hashlib.sha256(reader.bytes(a.cargo_bundle._contained(root,name))).hexdigest()==sha,'ledger inputs changed during reading')
    return source


def main():
    base=Path(__file__).resolve().parent
    source=check(base)
    parser=argparse.ArgumentParser();parser.add_argument('--runner',type=Path);parser.add_argument('--output',type=Path);parser.add_argument('--verifier',type=Path)
    args=parser.parse_args();a.require(bool(args.runner)==bool(args.output),'runner requires new output')
    a.require(not args.verifier or args.runner,'verifier requires actual SDK replay output')
    if args.runner:
        args.output.mkdir(parents=True,exist_ok=False)
        if args.verifier:
            recompiled=[a.recompile(base/'evidence'/f'rust-{m}',m,args.verifier) for m in ('on','off')]
            recompiled += [a.recompile(base/'ledger/evidence'/x['manifest'].removesuffix('/manifest.json'),x['mode'],args.verifier) for x in source['records']]
            (args.output/'ir-recompile.json').write_text(json.dumps(recompiled,indent=2)+'\n')
        programs=[dict(name=f'rust_{m}',hex_path=str(next((base/'evidence'/f'rust-{m}').glob('*.clvm.hex')))) for m in ('on','off')]
        programs.append(dict(name='chialisp',hex_path=str(base/'evidence/reference.clvm.hex')))
        inputs=args.output/'base-input.json';inputs.write_text(json.dumps(dict(programs=programs)))
        bounded_process.run([str(args.runner.resolve()),str(inputs),str(args.output/'base')],check=True,timeout=180)
        a.require((args.output/'base/report.json').read_bytes()==(base/'ledger/evidence/base-report.json').read_bytes(),'actual base SDK replay differs')
        controls=[]
        for kind in s.KINDS:
            for mode in ('on','off'):
                get=lambda role:next(x for x in source['records'] if (x['kind'],x['mode'],x['role'])==(kind,mode,role))
                controls.append(dict(kind=kind,mode=mode,original_path=str(base/'ledger/evidence'/get('original')['program']),
                    mutant_path=str(base/'ledger/evidence'/get('mutant')['program'])))
        inputs=args.output/'controls-input.json';inputs.write_text(json.dumps(dict(controls=controls)))
        bounded_process.run([str(args.runner.resolve()),'--controls',str(inputs),str(args.output/'controls')],check=True,timeout=180)
        a.require((args.output/'controls/report.json').read_bytes()==(base/'ledger/evidence/controls-report.json').read_bytes(),'actual source-control SDK replay differs')
    print('complete retained ledger consistency passed; reader alone adds no SDK execution credit')

if __name__=='__main__':main()
