#!/usr/bin/env python3
"""Current-host CI execution, separate from the immutable macOS capture context.

Build the four host tools before invoking this script. Native Cargo work uses
private targets; this script never rebuilds a driver while it executes one.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import sys
import admission as a
import bounded_process
import check_retained
import check_ledger

ROOT=Path(__file__).resolve().parents[2]
BASE=Path(__file__).resolve().parent
PUBLICATION_FILES={
    'experiments/authenticated-continuations/ci.py',
    'experiments/authenticated-continuations/README.md','docs/continuations.md',
    '.github/workflows/check.yml','www/package.json','www/astro.config.mjs',
    'www/src/integrations/shared-guide-headings.mjs',
    'www/src/content/docs/reference/continuations.mdx',
    'www/src/components/ContinuationEvidence.astro','www/src/lib/continuations.mjs',
    'www/scripts/test-continuations.mjs','www/scripts/check-site.mjs',
}
TOOLS=(*sorted(check_retained.TOOLS),'chialisp-oxide-verifier')


def digest(path):
    value=hashlib.sha256()
    with path.open('rb') as stream:
        while chunk:=stream.read(65536):value.update(chunk)
    return value.hexdigest()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--tools',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--expected-host',required=True)
    args=parser.parse_args();args.tools=args.tools.resolve(strict=True);args.output=args.output.resolve()
    args.output.mkdir(parents=True,exist_ok=False)
    files=PUBLICATION_FILES|check_ledger.CODE_FILES|check_retained.DEPENDENCIES|{
        'experiments/authenticated-continuations/'+n for n in check_retained.SOURCE_FILES}|{'Cargo.toml','Cargo.lock','rust-toolchain.toml'}
    snapshot=lambda:{name:digest(a.cargo_bundle._contained(ROOT,name)) for name in sorted(files)}
    before=snapshot();tools={name:digest(args.tools/name) for name in TOOLS}
    env=os.environ.copy()
    for key in ('RUSTC_BOOTSTRAP','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','RUSTC'):
        env.pop(key,None)
    env.update(RUSTUP_TOOLCHAIN=a.TOOLCHAIN,RUSTUP_AUTO_INSTALL='0',CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='2')
    commands=[]
    def command(label,argv,*,target=None,timeout=180):
        row=dict(label=label,argv=list(map(str,argv)),timeout_seconds=timeout,status='starting')
        commands.append(row);(args.output/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        settings=env.copy()
        if target:settings['CARGO_TARGET_DIR']=str(target)
        try:result=bounded_process.run(argv,env=settings,cwd=ROOT,timeout=timeout)
        except Exception as error:
            row.update(status='inconclusive_transport_failure',error=type(error).__name__)
            (args.output/f'{label}.stdout').write_bytes(getattr(error,'stdout',b'') or b'')
            (args.output/f'{label}.stderr').write_bytes(getattr(error,'stderr',b'') or b'')
            (args.output/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');raise
        (args.output/f'{label}.stdout').write_bytes(result.stdout);(args.output/f'{label}.stderr').write_bytes(result.stderr)
        row.update(status='completed',exit_code=result.returncode)
        (args.output/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        a.require(result.returncode==0,f'{label}: command failure is inconclusive; inspect retained logs')
        return result.stdout
    compiler=command('compiler-version',['rustup','run',a.TOOLCHAIN,'rustc','-vV']).decode()
    hosts=[x.removeprefix('host: ') for x in compiler.splitlines() if x.startswith('host: ')]
    a.require(hosts==[args.expected_host],'actual independent compiler host must match this runner')
    commit=command('git-commit',['git','rev-parse','HEAD']).decode().strip()
    status=command('git-status',['git','status','--porcelain']).decode()
    if os.environ.get('GITHUB_ACTIONS')=='true':
        a.require(not status.strip() and os.environ.get('GITHUB_SHA')==commit,'CI requires the clean exact checkout commit')
    check_retained.check(BASE);check_ledger.check(BASE)
    historical=json.loads((BASE/'evidence/receipt.json').read_text())
    native=args.output/'native-target';ledger_target=args.output/'ledger-target'
    for label,manifest,target in [('prototype',BASE/'Cargo.toml',native),('ledger',BASE/'ledger/Cargo.toml',ledger_target)]:
        for action,extra in [('fmt',['--check']),('test',['--all-targets','--locked']),('clippy',['--all-targets','--locked','--','-D','warnings'])]:
            command(label+'-'+action,['cargo',action,'--manifest-path',manifest,*extra],target=target,timeout=900)
    native_log=(args.output/'prototype-test.stdout').read_text()
    native_results=[tuple(map(int,row)) for row in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',native_log)]
    a.require(native_results and sum(row[0] for row in native_results)==7 and all(row[1:]==(0,0) for row in native_results),'seven executed native tests, no failures or ignored cases')
    command('python',[sys.executable,'-B','-m','unittest','discover','-s',BASE,'-p','test_*.py'])
    python_log=(args.output/'python.stderr').read_text()
    a.require(re.findall(r'Ran (\d+) tests?',python_log)==['28'] and python_log.rstrip().endswith('OK'),'28 executed Python checks without skips')
    command('ledger-build',['cargo','build','--manifest-path',BASE/'ledger/Cargo.toml','--locked'],target=ledger_target,timeout=900)
    runner=ledger_target/'debug/chialisp-oxide-continuation-ledger'
    command('retained-vm-ir',[sys.executable,'-B',BASE/'check_retained.py','--replay-cli',args.tools/'chialisp-oxide',
        '--verifier',args.tools/'chialisp-oxide-verifier','--output',args.output/'retained-vm'])
    command('retained-sdk-ir',[sys.executable,'-B',BASE/'check_ledger.py','--runner',runner,
        '--verifier',args.tools/'chialisp-oxide-verifier','--output',args.output/'retained-sdk'])
    # Fresh extraction is admitted using this host's independently queried full
    # rustc identity and actual current driver hash, never the macOS tool hashes.
    fresh={};compiler_target=args.output/'source-target'
    for mode in ('on','off'):
        output=args.output/('fresh-'+mode)
        options=['--manifest-path',str(BASE/'Cargo.toml'),'--package',a.PACKAGE,'--locked']
        argv=[args.tools/'cargo-chialisp-oxide','build',*options,'--output-dir',output]
        if mode=='off':argv+=['--no-optimize']
        package_before=a.source_snapshot(BASE)
        command('fresh-'+mode,argv,target=compiler_target,timeout=900)
        a.require(a.source_snapshot(BASE)==package_before,'package source changed during actual fresh extraction')
        loaded=a.fresh(output,BASE,mode,compiler=compiler,driver_sha256=tools['chialisp-oxide-driver'],cargo_options=options)
        entry=loaded.manifest['entries'][0]
        replay=a.recompile(loaded.manifest_path.parent,mode,args.tools/'chialisp-oxide-verifier')
        fresh[mode]=dict(manifest=str(loaded.manifest_path),program=str(loaded.manifest_path.parent/entry['clvm']),recompiled=replay)
    reference=args.output/'fresh-reference.clvm.hex'
    command('fresh-reference',['cargo','run','--manifest-path',BASE/'Cargo.toml','--locked','--example','compile_reference','--',reference],target=native,timeout=900)
    command('fresh-vm',[sys.executable,'-B',BASE/'replay.py','--cli',args.tools/'chialisp-oxide',
        '--rust-on',fresh['on']['program'],'--rust-off',fresh['off']['program'],'--reference',reference,'--output',args.output/'fresh-vm'])
    programs=[dict(name='rust_'+mode,hex_path=fresh[mode]['program']) for mode in ('on','off')]
    programs.append(dict(name='chialisp',hex_path=str(reference)))
    request=args.output/'fresh-sdk-input.json';request.write_text(json.dumps(dict(programs=programs),indent=2)+'\n')
    command('fresh-sdk',[runner,request,args.output/'fresh-sdk'])
    a.require((args.output/'fresh-sdk/report.json').read_bytes()==(BASE/'ledger/evidence/base-report.json').read_bytes(),
        'fresh actual-source SDK outcomes/costs differ; do not rewrite the historical capture to hide this')
    command('fresh-source-controls',[sys.executable,'-B',BASE/'source_controls.py','--tools',args.tools,
        '--work',args.output/'fresh-source-controls','--target',args.output/'mutation-target','--ledger',runner],timeout=1200)
    a.require((args.output/'fresh-source-controls/ledger/report.json').read_bytes()==(BASE/'ledger/evidence/controls-report.json').read_bytes(),
        'fresh actual-source control SDK outcomes differ from independently expected retained observations')
    a.require(snapshot()==before and {name:digest(args.tools/name) for name in TOOLS}==tools,'source/tool window changed during CI execution')
    report=dict(schema=1,status='passed',commit=commit,working_tree_dirty=bool(status.strip()),scope='Current-host finite execution; distinct from historical capture, binary authentication and compiler preservation',
        current_execution=dict(compiler=compiler,expected_host=args.expected_host,tools=tools,runner_sha256=digest(runner)),
        historical_capture=dict(compiler=historical['compiler'],tools=historical['tool_files'],verifier_sha256=historical['verifier_sha256']),
        current_source_window=before,fresh=fresh,counts=dict(native_tests=7,python_tests=28,retained_primary_vm=108,
        base_sdk_attempts=45,source_control_native=8,source_control_primary_vm=16,source_control_sdk_attempts=16),
        repetition_scope='Fresh and retained reruns repeat the fixed case sets; they add no distinct coverage')
    (args.output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print('PASS current-host continuation native/compiler/VM/SDK/publication source window; historical capture remains separate')

if __name__=='__main__':main()
