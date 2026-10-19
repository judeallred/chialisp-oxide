#!/usr/bin/env python3
"""Explicit separate prepare/native/capture phases; never implicit execution.

Before capture, the caller must finish all host builds, obtain the reviewed
LAST-host quiet receipt and coordinate target ownership. No receipt authenticates
execution or grants permission. This script never creates that quiet receipt.
"""
from pathlib import Path
import argparse
import copy
import json
import os
import shutil
import subprocess
import sys
import tomllib

import p2dc_source_controls as c


def environment(target=None):
    value=dict(os.environ)
    for name in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','RUST_MIN_STACK','RUSTC_BOOTSTRAP','CARGO_MAKEFLAGS','MAKEFLAGS','MFLAGS'):value.pop(name,None)
    value.update(CARGO_NET_OFFLINE='true',RUSTUP_AUTO_INSTALL='0',RUSTUP_TOOLCHAIN=toolchain(),CARGO_BUILD_JOBS='2',PYTHONDONTWRITEBYTECODE='1')
    if target is not None:value.update(CARGO_TARGET_DIR=str(target),CHIALISP_OXIDE_DRIVER=str(target/'debug/chialisp-oxide-driver'))
    return value


def toolchain():return tomllib.loads(c.read(c.ROOT/'rust-toolchain.toml').decode())['toolchain']['channel']
def write(path,value):path.write_text(json.dumps(value,indent=2)+'\n')


def command(argv,*,env,stdin=None):
    result=subprocess.run(list(map(str,argv)),cwd=c.ROOT,env=env,input=stdin,capture_output=True,timeout=180)
    c.require(type(result.returncode) is int and result.returncode==0 and not result.stderr,'independent tool query/formatter failed or inconclusive')
    return result.stdout


def prepare(package,receipt):
    c.require(not package.exists() and not receipt.exists(),'NEW prepared package/receipt required')
    window=c.source_window();files,raw,witnesses=c.package_files(c.ROOT);env=environment()
    formatter=Path(command(['rustup','which','--toolchain',toolchain(),'rustfmt'],env=env).decode().strip()).resolve(strict=True)
    pin=c.sha(formatter,2*1024*1024*1024);argv=[str(formatter),'--edition','2024','--emit','stdout','--config','skip_children=true'];relations=[]
    baseline=c.read(c.ROOT/f'puzzles/{c.vectors.SLUG}/puzzle.rs').decode()
    for name in sorted(c.NAMES):
        original=(baseline if name=='baseline' else raw[name]).encode();actual=command(argv,env=env,stdin=original)
        c.require(actual==files[f'src/{name}.rs'],'actual rustfmt output differs from closed reviewed layout')
        relations.append(dict(path=f'src/{name}.rs',raw_sha256=c.digest(original),formatted_sha256=c.digest(actual)))
    version=command([str(formatter),'--version'],env=env).decode().strip()
    c.require(c.sha(formatter,2*1024*1024*1024)==pin and c.source_window()==window,'source/formatter changed during prepare')
    package.mkdir(parents=True)
    for path,payload in files.items():destination=package/path;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(payload)
    value=dict(schema_version=1,protocol='chialisp-oxide-p2dc-controls-prepare/v1',status='eight_formatted_inputs_pinned_before_first_Cargo',source_root=str(c.ROOT),package_root=str(package),toolchain=toolchain(),source_window=window,
        prepared=dict(formatter=dict(path=str(formatter),sha256=pin,version=version,argv=argv,relations=relations),source_files=[dict(path=p,sha256=c.digest(files[p])) for p in sorted(files)],witnesses=witnesses))
    write(receipt,value)
    return value


def native(package,prepared_path,target,output):
    c.require(not output.exists(),'NEW native receipt/log directory required');output.mkdir(parents=True)
    prepared=c.load(prepared_path);c.closed(prepared,['schema_version','protocol','status','source_root','package_root','toolchain','source_window','prepared'],'prepare receipt')
    c.require(type(prepared['schema_version']) is int and prepared['schema_version']==1 and prepared['protocol']=='chialisp-oxide-p2dc-controls-prepare/v1'
        and prepared['toolchain']==toolchain() and prepared['status']=='eight_formatted_inputs_pinned_before_first_Cargo' and prepared['source_root']==str(c.ROOT) and prepared['package_root']==str(package) and prepared['source_window']==c.source_window(),'prepared source identity')
    c.prepared_metadata(prepared['prepared'],package,locked=False)
    expected,_,_=c.package_files(c.ROOT);fixed={p:c.digest(payload) for p,payload in expected.items()};env=environment(target)
    def quiet():
        c.require(c.source_window()==prepared['source_window'] and all(c.sha(package/p)==h for p,h in fixed.items()),'source changed during native Cargo phase')
    commands=[];native_log=None
    for index,(flags,tail) in enumerate([(['generate-lockfile','--offline'],[]),(['fmt'],['--all','--check']),(['check'],['--locked','--offline']),(['test'],['--locked','--offline','--','--nocapture']),(['clippy'],['--locked','--offline','--all-targets','--','-D','warnings'])]):
        quiet();argv=['rustup','run',toolchain(),'cargo',*flags,'--manifest-path',str(package/'Cargo.toml'),*tail];log=output/f'{index}.log'
        with log.open('wb') as stream:result=subprocess.run(argv,cwd=c.ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=1800)
        commands.append(dict(argv=argv,exit_code=result.returncode,log_sha256=c.sha(log)))
        c.require(type(result.returncode) is int and result.returncode==0,'native Cargo/format phase failed; no source rejection or test credit')
        if index==0:fixed['Cargo.lock']=c.sha(package/'Cargo.lock')
        if index==3:native_log=log
        quiet()
    snapshot=c.private_snapshot(package);metadata=copy.deepcopy(prepared['prepared']);metadata['source_files']=snapshot['sources']
    c.native_rows(c.read(native_log).decode(),metadata['witnesses']);shutil.copy2(native_log,output/'native.log')
    value=dict(schema_version=1,protocol='chialisp-oxide-p2dc-controls-native/v1',status='native_phase_passed_before_LAST_host_build',source_root=str(c.ROOT),package_root=str(package),toolchain=toolchain(),source_window=prepared['source_window'],prepared=metadata,
        native=dict(argv=commands[3]['argv'],exit_code=0,native_test_groups=1,native_policy_observations=3,log='native.log',log_sha256=c.sha(native_log)),commands=commands)
    write(output/'native-receipt.json',value);return value


def capture(package,native_path,quiet_path,target,output):
    c.require(not output.exists(),'NEW capture output required');receipt=c.load(native_path);quiet=c.load(quiet_path);env=environment(target)
    c.closed(quiet,['schema_version','protocol','status','source_root','package_root','compiler','toolchain','source_window','tools','native_receipt_sha256','scope'],'last host quiet window')
    c.closed(receipt,['schema_version','protocol','status','source_root','package_root','toolchain','source_window','prepared','native','commands'],'native phase receipt')
    c.require(type(quiet['schema_version']) is int and quiet['schema_version']==1 and quiet['protocol']=='chialisp-oxide-p2dc-controls-quiet-window/v1'
        and type(quiet['scope']) is str and quiet['scope'] and type(receipt['schema_version']) is int and receipt['schema_version']==1
        and receipt['protocol']=='chialisp-oxide-p2dc-controls-native/v1' and receipt['source_root']==str(c.ROOT) and receipt['package_root']==str(package)
        and receipt['toolchain']==toolchain(),'closed pre-execution quiet/native identity')
    compiler=command(['rustup','run',toolchain(),'rustc','-vV'],env=env).decode();c.compiler_host(compiler)
    c.require(command([target/'debug/chialisp-oxide-driver','--compiler-version'],env=env).decode().strip()==compiler.splitlines()[0],'independent driver/compiler version differs')
    tools={name:dict(path=str(target/'debug'/name),sha256=c.sha(target/'debug'/name,2*1024*1024*1024)) for name in c.TOOLS}
    c.require(quiet.get('status')=='LAST_host_build_terminal_tools_quiet' and quiet.get('source_root')==str(c.ROOT) and quiet.get('package_root')==str(package)
        and quiet.get('compiler')==compiler and quiet.get('toolchain')==toolchain() and quiet.get('tools')==tools
        and quiet.get('source_window')==c.source_window() and quiet.get('native_receipt_sha256')==c.sha(native_path),'separately reviewed LAST-host quiet source/tool/native receipt required')
    c.require(receipt['status']=='native_phase_passed_before_LAST_host_build' and receipt['source_window']==quiet['source_window'],'native source window differs from LAST host')
    c.prepared_metadata(receipt['prepared'],package,locked=True)
    private=c.private_snapshot(package);c.require(private['sources']==receipt['prepared']['source_files'],'native receipt/current nine-file identity')
    c.require(receipt['native']['native_test_groups']==1 and type(receipt['native']['native_test_groups']) is int
        and receipt['native']['native_policy_observations']==3 and type(receipt['native']['native_policy_observations']) is int
        and c.sha(native_path.parent/'native.log')==receipt['native']['log_sha256'],'native group/count/log linkage before capture')
    c.native_rows(c.read(native_path.parent/'native.log').decode(),c.inputs()[3])
    fixed={str(package/r['path']):r['sha256'] for r in private['sources']};fixed.update({str(native_path):c.sha(native_path),str(quiet_path):c.sha(quiet_path),str(native_path.parent/'native.log'):c.sha(native_path.parent/'native.log')})
    def unchanged():
        c.require(c.source_window()==quiet['source_window'] and all(c.sha(Path(p))==h for p,h in fixed.items()) and all(c.sha(Path(v['path']),2*1024*1024*1024)==v['sha256'] for v in tools.values()),'current source/package/tool/receipt changed during capture')
    unchanged();output.mkdir(parents=True);bundles={};runs=[]
    for path in c.FILES+['Cargo.lock']:
        destination=output/'package'/path;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(package/path,destination)
    shutil.copy2(native_path,output/'native-receipt.json');shutil.copy2(native_path.parent/'native.log',output/'native.log');shutil.copy2(quiet_path,output/'last-host-window.json')
    fixed.update({str(path):c.sha(path) for path in [output/'native-receipt.json',output/'native.log',output/'last-host-window.json',*(output/'package'/p for p in c.FILES+['Cargo.lock'])]})
    commands=[];(output/'logs').mkdir()
    def execute(role,argv,timeout=1800):
        unchanged();index=len(commands);stdout=output/f'logs/{index:02}.stdout';stderr=output/f'logs/{index:02}.stderr'
        with stdout.open('wb') as out,stderr.open('wb') as err:
            result=subprocess.run(list(map(str,argv)),cwd=c.ROOT,env=env,stdout=out,stderr=err,timeout=timeout)
        commands.append(dict(role=role,argv=list(map(str,argv)),exit_code=result.returncode,stdout=stdout.relative_to(output).as_posix(),stdout_sha256=c.sha(stdout),stderr=stderr.relative_to(output).as_posix(),stderr_sha256=c.sha(stderr)))
        unchanged()
        c.require(type(result.returncode) is int and result.returncode==0 and (role.startswith('export-') or not c.read(stderr,allow_empty=True)), 'capture command failed/inconclusive; no policy rejection credit')
        fixed[str(stdout)]=c.sha(stdout);fixed[str(stderr)]=c.sha(stderr)
        return c.read(stdout,allow_empty=True)
    for mode in ('off','on'):
        directory=output/f'fresh-{mode}';execute(f'export-{mode}',[target/'debug/cargo-chialisp-oxide','build',*c.options(package),'--output-dir',directory]+(['--no-optimize'] if mode=='off' else []))
        checked=c.fresh_bundle(directory,package,root=c.ROOT,compiler=compiler,driver_sha256=tools['chialisp-oxide-driver']['sha256'],toolchain=toolchain(),optimize=mode=='on')
        retained=output/'bundles'/mode;retained.mkdir(parents=True);shutil.copy2(checked.manifest_path,retained/'manifest.json');recompiled=[]
        fixed[str(retained/'manifest.json')]=c.sha(retained/'manifest.json')
        by_name={}
        for entry in checked.manifest['entries']:
            for key in ('ir','clvm'):
                shutil.copy2(checked.manifest_path.parent/entry[key],retained/entry[key]);fixed[str(retained/entry[key])]=c.sha(retained/entry[key])
            ir=retained/entry['ir'];c.raw_abi(c.load(ir),entry['entry'])
            compiled=json.loads(execute(f'recompile-{mode}-{entry["entry"].split("::")[-2]}',[target/'debug/chialisp-oxide-verifier',ir,'--artifacts']).decode(),object_pairs_hook=c.cargo_authoring.unique_object)
            chosen=next(a for a in compiled if a['optimize'] is (mode=='on'))
            c.require(all(chosen[k]==entry['artifact'][k] for k in ('bytes','tree_hash','clvm_hex')),'actual ordinary validated IR recompile differs')
            recompiled.append(dict(entry=entry['entry'],ir_sha256=c.sha(ir),artifacts=compiled));by_name[entry['entry'].split('::')[-2]]=entry
        bundles[mode]=dict(manifest=f'bundles/{mode}/manifest.json',manifest_sha256=c.sha(retained/'manifest.json'),recompiled=recompiled)
        for spec in receipt['prepared']['witnesses']:
            for name in ('baseline',spec['name']):
                entry=by_name[name];solution=output/f'{mode}-{name}-{spec["name"]}-solution.json';write(solution,[spec['solution']]);fixed[str(solution)]=c.sha(solution)
                result=json.loads(execute(f'vm-{mode}-{name}-{spec["name"]}',[target/'debug/chialisp-oxide','run-batch',retained/entry['clvm'],'--solutions',solution,'--max-cost','1000000']).decode())
                c.require(type(result) is list and len(result)==1,'one actual VM witness required')
                expected=spec['baseline'] if name=='baseline' else spec['mutant'];c.require(c.check_execution(result[0])==expected,'actual retained witness differs from independent expectation')
                if expected['status']=='rejected':c.require(result[0].get('failure')==dict(kind='program',code='raise'),'source assertion needs typed Raise, not resource/infrastructure failure')
                runs.append(dict(mode=mode,name=name,witness=spec['witness'],solution=spec['solution'],artifact_sha256=c.digest(bytes.fromhex(entry['artifact']['clvm_hex'])),execution=result[0]))
    context=dict(source_root=str(c.ROOT),package_root=str(package),capture_root=str(output),compiler=compiler,toolchain=toolchain(),source_window=quiet['source_window'],tools=tools,last_host_window=dict(path=str(quiet_path),sha256=c.sha(quiet_path)))
    files=set('package/'+p for p in c.FILES+['Cargo.lock'])|{'native.log','native-receipt.json','last-host-window.json'}
    for row in commands:files.update((row['stdout'],row['stderr']))
    files.update(p.relative_to(output).as_posix() for p in output.glob('*-solution.json'))
    for mode,value in bundles.items():
        files.add(value['manifest']);manifest=c.load(output/value['manifest']);files.update(f'bundles/{mode}/'+r[k] for r in manifest['entries'] for k in ('ir','clvm'))
    report=dict(schema_version=1,protocol=c.PROTOCOL,status='actual_source_controls_captured',scope=c.SCOPE,context=context,prepared=receipt['prepared'],native=receipt['native'],bundles=bundles,runs=runs,commands=commands,files=[dict(path=p,sha256=c.sha(output/p)) for p in sorted(files)])
    unchanged();c.validate_recorded(report,output);write(output/'report.json',report);return report


def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='phase',required=True)
    p=sub.add_parser('prepare');p.add_argument('--package',type=Path,required=True);p.add_argument('--receipt',type=Path,required=True)
    p=sub.add_parser('native');p.add_argument('--package',type=Path,required=True);p.add_argument('--prepared',type=Path,required=True);p.add_argument('--target',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    p=sub.add_parser('capture');p.add_argument('--package',type=Path,required=True);p.add_argument('--native-receipt',type=Path,required=True);p.add_argument('--last-host-window',type=Path,required=True);p.add_argument('--target',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=parser.parse_args();package=a.package.resolve()
    try:
        if a.phase=='prepare':result=prepare(package,a.receipt.resolve())
        elif a.phase=='native':result=native(package,a.prepared.resolve(),a.target.resolve(),a.output.resolve())
        else:result=capture(package,a.native_receipt.resolve(),a.last_host_window.resolve(),a.target.resolve(),a.output.resolve())
    except BaseException as error:
        if hasattr(a,'output') and a.output.exists():
            path=a.output/'incomplete-failure.json'
            if not path.exists():write(path,dict(status='incomplete_failure_no_success_credit',phase=a.phase,error_type=type(error).__name__,message=str(error),scope='No automaticretry/reuse. For timeouts inspect owned children/latewrites before retry. No resource/infrastructure failure is policy rejection. Existing logs retained.'))
        raise
    print(json.dumps({'status':result['status'],'scope':__doc__},indent=2))


if __name__=='__main__':main()
