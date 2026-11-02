"""Shared reproducible capture infrastructure; importing runs no external tools.

Source/tool freshness and complete Cargo receipts are consistency checks, not
execution authentication, source authenticity or compiler preservation.
"""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import os
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
sys.path.append(str(ROOT/'experiments/authenticated-continuations'))
import bounded_process
import cargo_bundle
from source_walk import source_files

TOOLCHAIN='nightly-2026-07-14'
TOOLS=('cargo-chialisp-oxide','chialisp-oxide-driver','chialisp-oxide','chialisp-oxide-verifier')
PROTOCOL='chialisp-oxide-stateful-success/v1'

def require(ok,message):
    if not ok:raise ValueError(message)

COMMON_RECEIPT={'source_origin','toolchain','compiler','tools','compiler_sources','tool_source_origin','tool_sources','critical_compiler_sources_equal','source_authentication','execution_authentication','compiler_preservation','files'}

def closed(value,fields,label):
    require(type(value) is dict and set(value)==set(fields),label+': exact closed fields required')

def retained_files(base):
    result={};stack=[(base,0)];directories=entries=total=0
    while stack:
        directory,depth=stack.pop();directories+=1
        require(directories<=4096 and depth<=64,'retained directory/depth budget')
        children=[]
        with os.scandir(directory) as stream:
            for entry in stream:
                entries+=1;require(entries<=16384 and len(children)<4096,'retained bounded directory entries')
                children.append(entry)
        for entry in sorted(children,key=lambda e:e.name):
            if entry.name in ('native-target','compiler-target','sdk-target') and (entry.is_dir(follow_symlinks=False) or entry.is_symlink()):continue
            require(not entry.is_symlink(),'retained symlink')
            path=Path(entry.path)
            if entry.is_dir(follow_symlinks=False):
                require(len(stack)+directories<4096,'retained directory work budget');stack.append((path,depth+1));continue
            require(entry.is_file(follow_symlinks=False),'retained regular file required')
            if directory==base and entry.name=='receipt.json':continue
            size=entry.stat(follow_symlinks=False).st_size;total+=size
            require(size<=64*1024*1024 and total<=64*1024*1024 and len(result)<4096,'retained file/aggregate budget')
            result[path.relative_to(base).as_posix()]=sha(path)
    return dict(sorted(result.items()))

def validate_inventory(report,base):
    files=report['files'];require(type(files) is dict and 1<=len(files)<=4096,'bounded complete retained file inventory')
    # Walk only this caller-selected packet, never declared external source paths.
    require(files==retained_files(base),'exact retained file inventory and hashes')
    for name in files:
        rel=PurePosixPath(name);require(not rel.is_absolute() and rel.as_posix()==name and not any(x in ('.','..') for x in rel.parts),'normalized confined retained inventory')
        path=base/name;require(path.resolve().is_relative_to(base) and not any(p.is_symlink() for p in [path,*path.parents] if p.is_relative_to(base)),'retained symlink or path escape')


def read(path,limit=64*1024*1024):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size<=limit,'bounded regular input')
    with path.open('rb') as stream:payload=stream.read(limit+1)
    require(len(payload)<=limit,'bounded input grew')
    return payload

def sha(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size<=2*1024*1024*1024,'bounded regular hash input')
    result=hashlib.sha256();consumed=0
    with path.open('rb') as stream:
        while chunk:=stream.read(65536):
            consumed+=len(chunk);require(consumed<=2*1024*1024*1024,'hash input grew beyond budget');result.update(chunk)
    return result.hexdigest()

def write(path,value):path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')

def environment():
    env=os.environ.copy()
    for key in list(env):
        if key in ('RUSTC_BOOTSTRAP','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','RUSTC') or key.startswith('RUST_BIGDECIMAL_'):env.pop(key)
    env.update(RUSTUP_TOOLCHAIN=TOOLCHAIN,RUSTUP_AUTO_INSTALL='0',CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='2')
    return env

SOURCE_READER_CONTROLS={'omitted_source','extra_source','old_source_hash'}

def validate_reader_controls(actual,names):
    require(actual=={name:'rejected' for name in names},'exact executed reader corruption-control inventory')

def validate_commands(base,expected):
    rows=json.loads(read(base/'commands.json'));require(type(rows) is list and [r['label'] for r in rows]==expected,'exact successful command group inventory/order')
    for row in rows:
        closed(row,{'label','argv','exit_code','status'},'command record');require(row['status']=='completed' and row['exit_code']==0 and type(row['argv']) is list and 1<=len(row['argv'])<=64 and all(type(x) is str and len(x)<=8192 for x in row['argv']),'successful bounded command')

def validate_vm_summary(report,base,vectors,paths):
    closed(report,{'schema','scope','cases','programs','mode_scope','status'},'primary VM summary')
    require(report['scope']=='finite primary VM observations, no SDK spends/authentication/proof' and report['mode_scope']==('reference_only' if set(paths)=={'chialisp'} else 'three_mode'),'VM summary scope')
    require(report['schema']==1 and report['status']=='passed' and report['cases']==len(vectors) and set(report['programs'])==set(paths),'exact VM summary coverage')
    for mode,path in paths.items():
        observations=json.loads(read(base/(mode+'.json')));require(len(observations)==len(vectors),'complete retained VM observations')
        from vm_failures import check_execution
        costs=[]
        for case,out in zip(vectors,observations):
            require(check_execution(out)==case['expected'],'independent exact VM output')
            failure=case.get('failure_by_mode',{}).get(mode,case['failure'])
            if failure is not None:require(out['failure']==failure,'exact typed VM failure')
            if case['expected']['status']=='accepted':costs.append(dict(label=case['label'],cost=out['cost']))
        payload=read(path)
        require(report['programs'][mode]==dict(program_hex_sha256=hashlib.sha256(payload).hexdigest(),serialized_bytes=len(bytes.fromhex(payload.decode().strip())),accepted_costs=costs),'VM selected module identity/exact accepted cost summary')


def discover_tool_source_root(tools):
    tools=tools.resolve(strict=True)
    for candidate in [tools,*list(tools.parents)[:12]]:
        if (candidate/'Cargo.toml').is_file() and (candidate/'rust-toolchain.toml').is_file() and all((candidate/p).is_dir() for p in ('crates/chialisp-oxide-driver','crates/chialisp-oxide-ir','crates/cargo-chialisp-oxide')):
            import tomllib
            require('workspace' in tomllib.loads(read(candidate/'Cargo.toml').decode()),'compiler build source must be a Cargo workspace')
            return candidate
    raise ValueError('no bounded compiler workspace ancestor of supplied tools')


class Capture:
    def __init__(self,out,tools):
        self.out=out.resolve();self.tools=tools.resolve(strict=True)
        self.out.mkdir(parents=True,exist_ok=False)
        self.commands=[];self.env=environment()
        self.tool_hashes={name:sha(self.tools/name) for name in TOOLS}
        self.tool_source_root=discover_tool_source_root(self.tools)
        self.tool_sources=compiler_snapshot(self.tool_source_root)
        self.current_compiler_sources=compiler_snapshot(ROOT)
        require(self.tool_sources==self.current_compiler_sources,'frozen complete compiler implementation differs from current compiler')
        self.compiler=self.command('compiler',['rustup','run',TOOLCHAIN,'rustc','-vV']).stdout.decode()
    def command(self,label,argv,*,target=None,cwd=ROOT,timeout=900):
        env=self.env.copy()
        if target:env['CARGO_TARGET_DIR']=str(target)
        row=dict(label=label,argv=list(map(str,argv)));self.commands.append(row)
        write(self.out/'commands.json',self.commands)
        try:r=bounded_process.run(argv,cwd=cwd,env=env,timeout=timeout,stdout_limit=16*1024*1024,stderr_limit=4*1024*1024)
        except Exception as e:
            row.update(status='inconclusive_transport',error=type(e).__name__);write(self.out/'commands.json',self.commands);raise
        (self.out/(label+'.stdout')).write_bytes(r.stdout);(self.out/(label+'.stderr')).write_bytes(r.stderr)
        row.update(exit_code=r.returncode,status='completed');write(self.out/'commands.json',self.commands)
        require(r.returncode==0,label+': command failed; no success credit')
        return r
    def build(self,project,package,output,label):
        project=project.resolve();entry=package.replace('-','_')+'::step'
        def snapshot():
            names=['Cargo.toml','Cargo.lock']+[p.relative_to(project).as_posix() for p in sorted((project/'src').rglob('*.rs'))]
            return dict(mode='cargo_library',package=package,manifest='Cargo.toml',library='src/lib.rs',exports={'step':dict(entry=entry,source='src/lib.rs')},sources=[dict(path=p,sha256=sha(project/p)) for p in names])
        authoring=snapshot();options=['--manifest-path',str(project/'Cargo.toml'),'--package',package,'--locked']
        self.command(label+'-check',[self.tools/'cargo-chialisp-oxide','check',*options],target=self.out/'compiler-target')
        modes={}
        for mode in ('on','off'):
            dest=output/('rust-'+mode);argv=[self.tools/'cargo-chialisp-oxide','build',*options,'--output-dir',dest]
            if mode=='off':argv.append('--no-optimize')
            self.command(label+'-'+mode,argv,target=self.out/'compiler-target')
            loaded=cargo_bundle.load_declared_export(dest,root=project,authoring=authoring,selected_entry=entry,compiler=self.compiler,driver_sha256=self.tool_hashes['chialisp-oxide-driver'],toolchain=TOOLCHAIN,current_authoring=snapshot,optimize=mode=='on',cargo_options=options)
            row=loaded.manifest['entries'][0];path=loaded.manifest_path.parent/row['clvm']
            modes['rust_'+mode]=path
        require(snapshot()==authoring,'source changed in capture')
        return modes
    def finish(self,report):
        require({name:sha(self.tools/name) for name in TOOLS}==self.tool_hashes,'tools changed during capture')
        require(compiler_snapshot(ROOT)==self.current_compiler_sources and compiler_snapshot(self.tool_source_root)==self.tool_sources,'compiler/tool-source window changed during capture')
        report.update(source_origin=str(ROOT),toolchain=TOOLCHAIN,compiler=self.compiler,tools=self.tool_hashes,compiler_sources=self.current_compiler_sources,tool_source_origin=str(self.tool_source_root),tool_sources=self.tool_sources,critical_compiler_sources_equal=True,source_authentication=False,execution_authentication=False,compiler_preservation=False)
        report['files']=retained_files(self.out)
        write(self.out/'receipt.json',report)
        return report

def fingerprints(root,labels):
    labels=sorted(set(labels));require(len(labels)<=4096,'source fingerprint inventory budget')
    remaining=64*1024*1024;result={}
    for label in labels:
        path=root/label;require(path.is_file() and not path.is_symlink(),'regular declared source')
        size=path.stat().st_size;require(size<=min(16*1024*1024,remaining),'source fingerprint byte budget before hashing');remaining-=size;result[label]=sha(path)
    return result


def compiler_snapshot(root):
    directories=['crates/chialisp-oxide-ir','crates/chialisp-oxide-driver','crates/chialisp-oxide-cli','crates/cargo-chialisp-oxide','crates/chialisp-oxide-macros','crates/chialisp-oxide','crates/chialisp-oxide-vm','crates/chialisp-oxide-package','crates/chialisp-oxide-reference','crates/chialisp-oxide-rue','crates/chialisp-oxide-verifier','experiments/backend','vendor']
    names=['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo/config.toml']+source_files(root,directories,names=('Cargo.toml','Cargo.lock'))
    return fingerprints(root,names)


def validate_source_window(captured,expected):
    require(type(captured) is dict and captured==expected,'exact complete current source inventory and hashes required')

def validate_compiler_window(report):
    expected=compiler_snapshot(ROOT)
    require(report['source_authentication'] is False and report['execution_authentication'] is False and report['compiler_preservation'] is False and report['toolchain']==TOOLCHAIN and type(report['compiler']) is str and report['compiler'].startswith('rustc '),'exact compiler claim boundaries')
    require(report['compiler_sources']==report['tool_sources']==expected and report['critical_compiler_sources_equal'] is True,'exact complete current/compiler-tool source inventory and hashes')
    require(set(report['tools'])==set(TOOLS) and all(type(v) is str and len(v)==64 and all(c in '0123456789abcdef' for c in v) for v in report['tools'].values()),'closed tool hash inventory')

def source_corruption_controls(captured,expected):
    require(captured==expected,'positive source window control')
    results={}
    for kind in ('omitted_source','extra_source','old_source_hash'):
        bad=dict(captured);name=next(iter(bad))
        if kind=='omitted_source':del bad[name]
        elif kind=='extra_source':bad['invented-extra-source.rs']='00'*32
        else:bad[name]='00'*32 if bad[name]!='00'*32 else '11'*32
        try:validate_source_window(bad,expected)
        except ValueError:results[kind]='rejected'
        else:raise ValueError('source corruption accepted: '+kind)
    return results


def source_window(base):
    relative=base.relative_to(ROOT).as_posix()
    paths=source_files(ROOT,[relative],suffixes=('.py','.rs','.toml','.lock','.clsp','.json'),recursive=False)
    paths+=source_files(ROOT,[relative+'/src',relative+'/examples',relative+'/historical-boundary-source'],suffixes=('.py','.rs','.toml','.lock','.clsp','.json'))
    paths+=source_files(ROOT,['experiments/stateful-ledger'],suffixes=('.py','.toml','.lock'),recursive=False)
    paths+=source_files(ROOT,['experiments/stateful-ledger/src'])
    paths+=['experiments/spend-costs/src/measurement.rs','crates/chialisp-oxide-simulator/src/lib.rs','crates/chialisp-oxide-simulator/Cargo.toml','crates/chialisp-oxide-simulator/rejection-protocol.json','scripts/cargo_bundle.py','scripts/cargo_authoring.py','scripts/source_walk.py','scripts/cargo_source_receipt.py','scripts/raw_tree.py','scripts/vm_failures.py','crates/chialisp-oxide-vm/failure-protocol.json','experiments/authenticated-continuations/bounded_process.py']
    names=[ROOT/p for p in paths]
    return fingerprints(ROOT,[p.relative_to(ROOT).as_posix() for p in names]+list(compiler_snapshot(ROOT)))

def capture_success(base,args):
    before=source_window(base);c=Capture(args.output,args.tools);package='chialisp-oxide-'+base.name
    target=ROOT/'target/stateful-native-private';manifest=base/'Cargo.toml'
    c.command('fmt',['cargo','fmt','--manifest-path',manifest,'--','--check'],target=target)
    native=c.command('native',['cargo','test','--manifest-path',manifest,'--locked'],target=target)
    expected=8 if base.name=='continuation-batches' else 7
    require(f'test result: ok. {expected} passed; 0 failed; 0 ignored;'.encode() in native.stdout,'complete native test group')
    c.command('clippy',['cargo','clippy','--manifest-path',manifest,'--all-targets','--locked','--','-D','warnings'],target=target)
    c.command('python',[sys.executable,'-B','-m','unittest','discover','-s',base,'-p','test_*.py'])
    modes=c.build(base,package,c.out,'source')
    reference=c.out/'reference.clvm.hex'
    c.command('reference',['cargo','run','--manifest-path',manifest,'--locked','--example','compile_reference','--',reference],target=target)
    c.command('vm',[sys.executable,'-B',base/'replay.py','--cli',c.tools/'chialisp-oxide','--rust-on',modes['rust_on'],'--rust-off',modes['rust_off'],'--reference',reference,'--output',c.out/'vm'])
    require(source_window(base)==before,'complete declared source window changed')
    return c.finish(dict(protocol=PROTOCOL,status='successful_source_capture',experiment=base.name,source_files=before,native_tests=expected,vm=json.loads(read(c.out/'vm/report.json')),sdk_spends=0,reader_controls=source_corruption_controls(before,source_window(base)),scope='native Rust checks, fresh complete two-mode Cargo bundles, independent Chialisp and exact typed primary VM observations; no SDK or live credit'))

def validate_success(path):
    base=path.resolve().parent;report=json.loads(read(path))
    closed(report,COMMON_RECEIPT|{'protocol','status','experiment','source_files','native_tests','vm','sdk_spends','reader_controls','scope'},'successful capture receipt');validate_inventory(report,base)
    require(report.get('protocol')==PROTOCOL and report.get('status')=='successful_source_capture','success protocol/status')
    require(report.get('compiler_preservation') is False and report.get('sdk_spends')==0,'claim boundary')
    expected_native=8 if report['experiment']=='continuation-batches' else 7
    import re
    require(report['native_tests']==expected_native and re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(base/'native.stdout'))==[(str(expected_native).encode(),b'0',b'0'),(b'0',b'0',b'0')],'exact native test group and retained command stdout')
    validate_commands(base,['compiler','fmt','native','clippy','python','source-check','source-on','source-off','reference','vm'])
    validate_reader_controls(report['reader_controls'],SOURCE_READER_CONTROLS)
    require(report['vm']['status']=='passed' and report['vm']['mode_scope']=='three_mode' and set(report['vm']['programs'])=={'chialisp','rust_on','rust_off'},'three-mode primary VM gate')
    files=report['files'];require(type(files) is dict and 1<=len(files)<=4096,'bounded file inventory')
    for label,digest in files.items():
        p=PurePosixPath(label)
        require(not p.is_absolute() and p.as_posix()==label and not any(x in ('.','..') for x in p.parts),'confined retained path')
        file=base.joinpath(*p.parts)
        require(file.resolve().is_relative_to(base) and not file.is_symlink() and sha(file)==digest,'retained byte identity')
    require(json.loads(read(base/'vm/report.json'))==report['vm'],'retained VM report identity')
    require(report['experiment'] in ('continuation-batches','spuds'),'closed successful experiment identity')
    validate_source_window(report['source_files'],source_window(ROOT/'experiments'/report['experiment']))
    validate_compiler_window(report)
    experiment=ROOT/'experiments'/report['experiment']
    import importlib.util
    saved=sys.modules.pop('oracle',None);saved_paths=sys.path.copy();sys.path.insert(0,str(experiment))
    try:
        spec=importlib.util.spec_from_file_location('current_stateful_replay',experiment/'replay.py');replay=importlib.util.module_from_spec(spec);spec.loader.exec_module(replay);vectors=replay.vectors()
    finally:
        sys.path[:]=saved_paths;sys.modules.pop('oracle',None)
        if saved is not None:sys.modules['oracle']=saved
    require(json.loads(read(base/'vm/vectors.json'))==vectors,'current independent vectors')
    from vm_failures import check_execution
    for mode in ('rust_on','rust_off','chialisp'):
        actual=json.loads(read(base/'vm'/(mode+'.json')));require(len(actual)==len(vectors),'complete retained VM observations')
        for case,row in zip(vectors,actual):
            require(check_execution(row)==case['expected'],'retained exact VM semantics')
            failure=case.get('failure_by_mode',{}).get(mode,case['failure'])
            if failure is not None:require(row['failure']==failure,'retained exact typed VM failure')
    selected={'chialisp':base/'reference.clvm.hex'}
    for mode in ('on','off'):
        manifest=admit_retained_bundle(base/('rust-'+mode),report,'chialisp-oxide-'+report['experiment'],mode=='on',experiment)
        output=base/('rust-'+mode);pointer=json.loads(read(output/'latest.json'));selected['rust_'+mode]=(output/pointer['manifest']).parent/manifest['entries'][0]['clvm']
    validate_vm_summary(report['vm'],base/'vm',vectors,selected)
    return dict(status='retained_success_packet_valid',files=len(files),execution_credit=False)

def admit_retained_bundle(output,report,package,optimize,source_root):
    """Admit the entire retained schema2 IR/blob/source inventory, without executing it.

    Captured absolute paths remain declarations so retained packets can move.
    Current declared source bytes are checked separately against source_files.
    """
    output=output.resolve(strict=True);source_root=source_root.resolve(strict=True)
    pointer=json.loads(read(output/'latest.json'));require(set(pointer)=={'bundle','manifest'},'closed bundle pointer')
    rel=cargo_bundle._relative(pointer['bundle']);require(len(rel.parts)==1 and pointer['manifest']==str(rel/'manifest.json'),'bundle pointer identity')
    root=output/str(rel);manifest=json.loads(read(root/'manifest.json'));require(manifest.get('schema_version')==2,'complete source-aware schema2 bundle required')
    require(manifest['package']==package and manifest['compiler']==report['compiler'] and manifest['driver_sha256']==report['tools']['chialisp-oxide-driver'] and manifest['toolchain']==report['toolchain'],'retained compiler/driver/package identity')
    require(len(manifest['entries'])==1 and manifest['entries'][0]['entry']==package.replace('-','_')+'::step','exact complete entry set')
    reader=cargo_bundle._Reader();declared=cargo_bundle._preflight_ir_declarations(manifest,reader,optimize);paths=set();details={}
    for row in manifest['entries']:
        loaded=cargo_bundle._artifact_details(row,root,reader,paths,optimize,2,ir_declaration=declared[row['entry']]);details[row['entry']]=(row['ir'],loaded[3],loaded[2])
    attachments=cargo_bundle.validate_receipt(manifest,root,details,reader,paths)
    library=manifest['source_receipt']['unit']['library_source'];attached={f.label:f for f in attachments}
    require(library in attached and attached[library].contents==read(source_root/'src/lib.rs'),'bundle selected compiler-read library differs from exact declared source')
    for label,file in attached.items():
        if label.startswith('package/'):
            relative=cargo_bundle._relative(label.removeprefix('package/'));path=source_root/str(relative)
            require(path.resolve().is_relative_to(source_root.resolve()) and file.contents==read(path),'attached package source differs from actual declared source')
    return manifest


def main(base):
    parser=argparse.ArgumentParser(description='Successful three-mode capture, or read-only retained-packet consistency check.')
    parser.add_argument('--tools',type=Path);parser.add_argument('--output',type=Path);parser.add_argument('--check',type=Path)
    args=parser.parse_args()
    if args.check:
        require(not args.tools and not args.output,'reader does not execute tools');result=validate_success(args.check)
    else:
        require(args.tools and args.output,'capture requires tools and fresh output');result=capture_success(base,args)
    print(json.dumps(result,indent=2))
