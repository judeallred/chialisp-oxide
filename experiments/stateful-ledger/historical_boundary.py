"""Read frozen historical boundary packets; never grant current-source credit."""
import importlib.util
import json
import re
from pathlib import Path
import sys
from capture_support import ROOT, read, require, sha,closed,retained_files
from vm_failures import check_execution
FIELDS={'schema','status','scope','compiler','toolchain','tool_files','source_files','dependencies','native_tests','python_tests','subset','rust_modes','reference','sdk_spends','cost_comparison_executed','compiler_budgets_changed','source_authentication','compiler_preservation','files'}

def validate(base):
    packet=base/'evidence';r=json.loads(read(packet/'receipt.json'));snapshot=base/'historical-boundary-source';s=json.loads(read(snapshot/'snapshot.json'))
    closed(r,FIELDS,'historical boundary receipt');closed(s,{'protocol','receipt_sha256','source_files','dependencies','scope'},'historical snapshot')
    require(s['protocol']=='chialisp-oxide-historical-boundary-source/v1' and s['receipt_sha256']==sha(packet/'receipt.json'),'historical receipt identity')
    require(r['schema']==1 and r['status']=='boundary_observed' and s['source_files']==r['source_files'] and s['dependencies']==r['dependencies'],'exact historical source inventory')
    require(r['sdk_spends']==0 and all(r[k] is False for k in ('cost_comparison_executed','compiler_budgets_changed','source_authentication','compiler_preservation')),'historical zero SDK/false claim boundaries')
    require(r['subset']==dict(status='not_completed',diagnostic='CLVM expansion nesting limit') and set(r['rust_modes'])=={'on','off'},'historical complete unsupported modes')
    expected_native=8 if base.name=='continuation-batches' else 7;expected_python=10 if base.name=='continuation-batches' else 9
    require(r['native_tests']==expected_native and r['python_tests']==expected_python,'historical native/host counts')
    require(re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(packet/'native.stdout'))==[(str(expected_native).encode(),b'0',b'0'),(b'0',b'0',b'0')],'historical exact native stdout')
    for mode,value in r['rust_modes'].items():
        closed(value,{'status','diagnostic','ir','ir_sha256'},'historical unsupported mode')
        require(value['status']=='not_emitted' and value['diagnostic']=='CLVM expansion nesting limit' and value['ir']==f'diagnostics/rust-{mode}.ir.json' and sha(packet/value['ir'])==value['ir_sha256'],'historical typed diagnostic IR')
        require(b'CLVM expansion nesting limit' in read(packet/f'rust-{mode}.stderr'),'historical emission failure stdout')
    expected_snapshot={'snapshot.json':sha(snapshot/'snapshot.json')}
    for name,digest in r['source_files'].items():require(sha(snapshot/name)==digest,'historical source bytes '+name);expected_snapshot[name]=digest
    for name,digest in r['dependencies'].items():require(sha(snapshot/'.dependencies'/name)==digest,'historical dependency bytes '+name);expected_snapshot['.dependencies/'+name]=digest
    require(retained_files(snapshot)==dict(sorted(expected_snapshot.items())),'exact physical frozen source snapshot inventory')
    require(retained_files(packet)==r['files'],'exact historical physical file inventory and hashes')
    commands=json.loads(read(packet/'commands.json'));labels=['compiler','fmt','native','clippy','python','subset','rust-on','rust-off','reference']
    require([c['label'] for c in commands]==labels,'exact historical command groups')
    for c in commands:
        closed(c,{'label','argv','expected_exit','status','exit_code'},'historical command')
        expected=1 if c['label'] in ('subset','rust-on','rust-off') else 0
        require(c['status']=='completed' and c['exit_code']==c['expected_exit']==expected,'historical expected command exits')
    saved=sys.modules.pop('oracle',None);saved_paths=sys.path.copy();sys.path.insert(0,str(snapshot))
    try:
        spec=importlib.util.spec_from_file_location('historical_stateful_replay',snapshot/'replay.py');replay=importlib.util.module_from_spec(spec);spec.loader.exec_module(replay);vectors=replay.vectors()
    finally:
        sys.path[:]=saved_paths;sys.modules.pop('oracle',None)
        if saved is not None:sys.modules['oracle']=saved
    require(json.loads(read(packet/'reference-vm/vectors.json'))==vectors,'exact historical vectors')
    observations=json.loads(read(packet/'reference-vm/chialisp.json'));require(len(vectors)==len(observations),'historical complete VM count')
    costs=[]
    for case,actual in zip(vectors,observations):
        require(check_execution(actual)==case['expected'],'retained historical VM output')
        if case['failure'] is not None:require(actual['failure']==case['failure'],'historical typed failure')
        if case['expected']['status']=='accepted':costs.append(dict(label=case['label'],cost=actual['cost']))
    vm=r['reference'];closed(vm,{'schema','scope','cases','programs','status'},'historical VM summary');payload=read(packet/'reference.clvm.hex')
    expected_program=dict(program_hex_sha256=sha(packet/'reference.clvm.hex'),serialized_bytes=len(bytes.fromhex(payload.decode().strip())),accepted_costs=costs)
    require(vm['schema']==1 and vm['status']=='passed' and vm['cases']==len(vectors) and vm['programs']=={'chialisp':expected_program} and json.loads(read(packet/'reference-vm/report.json'))==vm,'historical exact VM program/cost summary')
    return dict(status='historical_boundary_packet_valid',experiment=base.name,source_scope='exact_frozen_historical_snapshot',current_source_freshness=False,current_compiler_boundary_credit=False,current_success_credit=False,execution_credit=False)
