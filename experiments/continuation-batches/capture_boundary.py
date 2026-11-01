#!/usr/bin/env python3
"""Reproduce the current source/compiler boundary; never claim a Rust port pass.

Native Rust checks are distinct from compiler admission and CLVM emission. This
expects the unchanged compiler's expansion-depth failure in both modes and then
executes only independently compiled Chialisp. Tool/source freshness is not
binary authentication or compiler preservation. No SDK or network commands.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

import replay
from replay import bounded_process

BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[1]
SOURCE_FILES={'Cargo.toml','Cargo.lock','src/lib.rs','reference.clsp','oracle.py','test_oracle.py',
              'fixtures.json','examples/compile_reference.rs','replay.py','test_replay.py','capture_boundary.py'}
DEPENDENCIES={'experiments/authenticated-continuations/bounded_process.py','scripts/raw_tree.py','scripts/vm_failures.py',
              'crates/chialisp-oxide-vm/failure-protocol.json'}
TOOLS={'cargo-chialisp-oxide','chialisp-oxide-driver','chialisp-oxide','chialisp-oxide-verifier'}
TOOLCHAIN='nightly-2026-07-14'


def require(ok,message):
    if not ok:raise ValueError(message)


def digest(path):
    result=hashlib.sha256()
    with path.open('rb') as stream:
        while chunk:=stream.read(65536):result.update(chunk)
    return result.hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tools',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();tools=args.tools.resolve(strict=True);out=args.output.resolve()
    out.mkdir(parents=True,exist_ok=False)
    before={name:digest(BASE/name) for name in sorted(SOURCE_FILES)}
    dependencies={name:digest(ROOT/name) for name in sorted(DEPENDENCIES)}
    tool_hashes={name:digest(tools/name) for name in sorted(TOOLS)}
    env=os.environ.copy()
    for name in ('RUSTC_BOOTSTRAP','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER',
                 'RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','RUSTC'):
        env.pop(name,None)
    for name in list(env):
        if name.startswith('RUST_BIGDECIMAL_'):env.pop(name)
    env.update(RUSTUP_TOOLCHAIN=TOOLCHAIN,RUSTUP_AUTO_INSTALL='0',CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='2')
    commands=[]
    def command(label,argv,*,target=None,expected=0,timeout=300):
        settings=env.copy()
        if target:settings['CARGO_TARGET_DIR']=str(target)
        row=dict(label=label,argv=list(map(str,argv)),expected_exit=expected,status='starting')
        commands.append(row);(out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        try:result=bounded_process.run(argv,env=settings,cwd=ROOT,timeout=timeout,stdout_limit=4*1024*1024,stderr_limit=2*1024*1024)
        except Exception as error:
            row.update(status='inconclusive_transport_failure',error=type(error).__name__)
            (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');raise
        (out/(label+'.stdout')).write_bytes(result.stdout);(out/(label+'.stderr')).write_bytes(result.stderr)
        row.update(status='completed',exit_code=result.returncode)
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        require(result.returncode==expected,label+': unexpected command result, no emission credit')
        return result
    compiler=command('compiler',['rustup','run',TOOLCHAIN,'rustc','-vV']).stdout.decode()
    manifest=BASE/'Cargo.toml';native=out/'native-target';source_target=out/'compiler-target'
    command('fmt',['cargo','fmt','--manifest-path',manifest,'--','--check'],target=native)
    native_run=command('native',['cargo','test','--manifest-path',manifest,'--locked'],target=native,timeout=900)
    native_counts=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',native_run.stdout)
    require(native_counts==[(b'8',b'0',b'0'),(b'0',b'0',b'0')],'eight native tests plus zero doctests')
    command('clippy',['cargo','clippy','--manifest-path',manifest,'--all-targets','--locked','--','-D','warnings'],target=native,timeout=900)
    py=command('python',[sys.executable,'-B','-m','unittest','discover','-s',BASE,'-p','test_*.py'])
    require(re.findall(rb'Ran (\d+) tests?',py.stderr)==[b'10'] and py.stderr.rstrip().endswith(b'OK'),'ten host/reader tests')
    options=['--manifest-path',str(manifest),'--package','chialisp-oxide-continuation-batches','--locked']
    subset=command('subset',[tools/'cargo-chialisp-oxide','check',*options],target=source_target,expected=1,timeout=900)
    require(b'CLVM expansion nesting limit' in subset.stderr, 'public subset command reaches the same backend boundary')
    modes={};diagnostics=out/'diagnostics';diagnostics.mkdir()
    for mode in ('on','off'):
        runs=source_target/'chialisp-oxide/runs'
        prior=set(runs.glob('*/*.ir.json'))
        argv=[tools/'cargo-chialisp-oxide','build',*options,'--output-dir',out/('rust-'+mode)]
        if mode=='off':argv.append('--no-optimize')
        result=command('rust-'+mode,argv,target=source_target,expected=1,timeout=900)
        require(b'CLVM expansion nesting limit' in result.stderr,'exact observed backend diagnostic')
        fresh=set(runs.glob('*/*.ir.json'))-prior
        require(len(fresh)==1,'one newly extracted actual-source IR')
        path=fresh.pop();payload=path.read_bytes();require(len(payload)<=2*1024*1024,'bounded diagnostic IR')
        ir=json.loads(payload)
        require(ir['ir_version']==6 and ir['environment']=='strict' and ir['program']['name']=='chialisp_oxide_continuation_batches::step','exact actual-source diagnostic entry')
        retained=diagnostics/('rust-'+mode+'.ir.json');retained.write_bytes(payload)
        modes[mode]=dict(status='not_emitted',diagnostic='CLVM expansion nesting limit',ir=retained.relative_to(out).as_posix(),ir_sha256=hashlib.sha256(payload).hexdigest())
    reference=out/'reference.clvm.hex'
    command('reference',['cargo','run','--manifest-path',manifest,'--locked','--example','compile_reference','--',reference],target=native,timeout=900)
    vm=replay.execute(tools/'chialisp-oxide',dict(chialisp=reference),out/'reference-vm')
    require({name:digest(BASE/name) for name in before}==before and {name:digest(ROOT/name) for name in dependencies}==dependencies
            and {name:digest(tools/name) for name in tool_hashes}==tool_hashes,'complete declared source/tool window changed')
    report=dict(schema=1,status='boundary_observed',scope='native Rust success; public compiler check/build unsupported at unchanged expansion-depth budget; Chialisp-only finite VM lane',
                compiler=compiler,toolchain=TOOLCHAIN,tool_files=tool_hashes,source_files=before,dependencies=dependencies,
                native_tests=8,python_tests=10,subset=dict(status='not_completed',diagnostic='CLVM expansion nesting limit'),rust_modes=modes,reference=vm,
                sdk_spends=0,cost_comparison_executed=False,compiler_budgets_changed=False,
                source_authentication=False,compiler_preservation=False)
    retained_files={p.relative_to(out).as_posix():digest(p) for p in out.rglob('*') if p.is_file()
                    and not any(part in ('native-target','compiler-target') for part in p.relative_to(out).parts)}
    report['files']=retained_files
    (out/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Observed compiler boundary in public check/both builds; native pass; Chialisp-only VM pass')


if __name__=='__main__':main()
