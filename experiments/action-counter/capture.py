#!/usr/bin/env python3
"""Explicit staged counter feasibility capture. Source preparation is not execution.

prepare/native/extract/replay are deliberately separate. Never rebuild the shared
compiler implicitly. Extraction/replay require the parent's coordinated lane.
"""
import argparse
import json
import os
from pathlib import Path
import re
import sys
import tomllib
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import cargo_bundle
import cargo_authoring
from registry_codegen_evidence import decode
from source_walk import source_files, WalkLimits
import vectors
from compiled_codec import inspect
from capture_support import command, digest, identity, read, require, retained_identities, write, recorded_command, same_json

TOOLCHAIN = 'nightly-2026-07-14'
PACKAGE = 'chialisp-oxide-action-counter'
CRATE = PACKAGE.replace('-', '_')
CONTROLS = ('original', 'increment_no_change', 'reset_drop_signature')
EXPORTS = ('increment', 'reset')
FILES = ('Cargo.toml', 'Cargo.lock', 'src/lib.rs', 'tests/native.rs', 'vectors.json',
         'raw-vm.rs', 'reference/increment.clsp', 'reference/reset.clsp')
MUTANT_SOURCES = ('src/counter_types.rs', 'src/counter_increment.rs', 'src/counter_reset.rs')


def source_names(root, directories, *, declared_cache=()):
    """Prune generated Python caches, preserving any explicitly pinned blob.

    The adapter still charges skipped directory entries and rejects symlinks;
    canonical Git-declared cache paths, if any, retain ordinary strict scanning.
    """
    root = Path(root)
    declared = set(declared_cache)
    scanned = 0
    class Scan:
        def __init__(self,directory):
            self.stream=os.scandir(directory)
        def __enter__(self):return self
        def __exit__(self,*_):self.stream.close()
        def __iter__(self):
            nonlocal scanned
            local_count=0
            for entry in self.stream:
                local_count += 1
                scanned += 1
                require(local_count <= WalkLimits().directory_entries and scanned <= WalkLimits().entries,
                        'source discovery budget includes skipped cache entries')
                relative = Path(entry.path).relative_to(root).as_posix()
                if entry.name == '__pycache__' and entry.is_dir(follow_symlinks=False):
                    if not any(name.startswith(relative+'/') for name in declared):
                        continue
                if entry.name.endswith(('.pyc','.pyo')) and entry.is_file(follow_symlinks=False) and relative not in declared:
                    continue
                yield entry
    return source_files(root, directories, suffixes=None, scandir=Scan)


def frozen_inputs():
    record=json.loads(read(HERE/'capture-inputs.json'))
    require(record['status']=='source_only_harness_unexecuted','source preparation status')
    names=source_names(ROOT,['experiments/action-counter'])
    names=[name for name in names if name!='experiments/action-counter/capture-inputs.json']
    actual=[]
    for name in names:
        payload=read(ROOT/name)
        actual.append({'path':str(Path(name).relative_to('experiments/action-counter')),'bytes':len(payload),'sha256':digest(payload)})
    require(same_json(actual,record['files']),'frozen owned source inventory changed; explicitly review and refreeze before capture')


def request_recipe(packet):
    return {'max_cost':50_000_000,
            'modules':[{k:a[k] for k in ('id','export','clvm_hex')} for a in packet['artifacts']],
            'vectors':vectors.cases()['cases']}


def retained_paths(args):
    paths=source_files(args.output,['projects',*('bundle-'+c+'-'+m for c in CONTROLS for m in ('off','on'))],suffixes=None)
    entries=0
    for path in args.output.iterdir():
        require(entries<4096,'bounded top-level retained inventory')
        entries+=1
        # These outputs are produced after the input window has been frozen.
        if path.name.startswith(('raw-attempt1.','raw-attempt2.')):
            continue
        if path.name.endswith(('.stdout','.stderr','.command.json')):
            paths.append(path.name)
    paths += ['native-window-before.json','native-window-after.json','prepared.json','native.json',
              'artifacts.json','request.json','extract-window-before.json','extract-window-after.json']
    return sorted(set(paths))


def native_target(args, control):
    require(control in CONTROLS, 'closed native control target')
    return args.native_target/control


def runner_path(args):
    return native_target(args,'original')/'debug/examples/raw-vm'


def protocol_path(args):
    return native_target(args,'original')/'debug/examples/protocol'


def command_roles(args, *, all_phases, source_root=None):
    source_root = ROOT if source_root is None else Path(source_root)
    cargo=['rustup','run',TOOLCHAIN,'cargo']
    result={'pinned-rustc-path':(['rustup','which','--toolchain',TOOLCHAIN,'rustc'],args.native_target,None)}
    for c in CONTROLS:
        label=c.replace('_','-');manifest=project(args.output,c)/'Cargo.toml'
        for prefix,argv in [
            ('prepare-lock',['update','--workspace','--manifest-path',manifest,'--offline']),
            ('fmt',['fmt','--manifest-path',manifest,'--check']),
            ('clean',['clean','--manifest-path',manifest,'--package',PACKAGE,'--locked']),
            ('check',['check','--manifest-path',manifest,'--all-targets','--locked']),
            ('native',['test','--manifest-path',manifest,'--test','native','--locked','--','--nocapture']),
            ('clippy',['clippy','--manifest-path',manifest,'--all-targets','--locked','--','-D','warnings'])]:
            result[prefix+'-'+label]=(cargo+argv,native_target(args,c),None)
    result['last-host-build']=(cargo+['build','--manifest-path',project(args.output,'original')/'Cargo.toml','--example','raw-vm','--locked'],native_target(args,'original'),None)
    result['last-protocol-build']=(cargo+['build','--manifest-path',source_root/'experiments/action-counter/Cargo.toml','--example','protocol','--locked'],native_target(args,'original'),None)
    if all_phases:
        result['compiler-version']=([args.rustc,'-vV'],args.compiler_target,None)
        result['driver-version']=([args.driver,'--compiler-version'],args.compiler_target,None)
        for c in CONTROLS:
            label=c.replace('_','-');manifest=(source_root/'puzzles/Cargo.toml') if c=='original' else project(args.output,c)/'Cargo.toml'
            result['subset-'+label]=([args.cargo_cli,'check','--manifest-path',manifest,'--locked'],args.compiler_target,args.driver)
            for mode in ('off','on'):
                argv=[args.cargo_cli,'build','--manifest-path',manifest,'--locked','--output-dir',args.output/('bundle-'+c+'-'+mode)]
                if mode=='off':argv.append('--no-optimize')
                result['extract-'+label+'-'+mode]=(argv,args.compiler_target,args.driver)
        runner=runner_path(args)
        result['compile-reference']=([runner,'compile-reference',project(args.output,'original')/'reference'],native_target(args,'original'),None)
        result['compile-rue']=([runner,'compile-rue',source_root/'puzzles'],native_target(args,'original'),None)
        for label in ('raw-attempt1','raw-attempt2'):
            result[label]=([runner,'replay',args.output/'request.json'],native_target(args,'original'),None)
    return result


def check_commands(args, *, all_phases):
    for label,(argv,target,driver) in command_roles(args,all_phases=all_phases).items():
        recorded_command(args.output,label,argv,target=target,rustc=args.rustc,driver=driver)
    require(Path(read(args.output/'pinned-rustc-path.stdout').decode().strip()).resolve()==args.rustc,
            'retained rustup compiler path matches explicit pinned RUSTC')


def spec(control):
    value = vectors.cases()
    require(json.loads(read(HERE / 'vectors.json')) == value, 'current independent vector specification')
    for row in value['cases']:
        row['expected'] = vectors.expected(row['export'], decode(bytes.fromhex(row['environment_hex'])), control)
    return value


def source(control):
    recipes = json.loads(read(HERE / 'mutations.json'))['controls']
    require([r['name'] for r in recipes] == list(CONTROLS), 'exact recipes')
    files = {name: read(ROOT/name).decode() for name in ('puzzles/counter_types.rs', 'puzzles/counter-increment/puzzle.rs', 'puzzles/counter-reset/puzzle.rs')}
    for edit in recipes[CONTROLS.index(control)]['edits']:
        name = edit['source']
        require(name in files and files[name].count(edit['before']) == 1, 'unique actual-source edit')
        files[name] = files[name].replace(edit['before'], edit['after'])
    mapping = {'puzzles/counter_types.rs':'src/counter_types.rs', 'puzzles/counter-increment/puzzle.rs':'src/counter_increment.rs', 'puzzles/counter-reset/puzzle.rs':'src/counter_reset.rs'}
    return {mapping[name]: text.encode() for name,text in files.items()}

def project_inputs(control, source_root=None):
    source_root = ROOT if source_root is None else Path(source_root)
    manifest = read(HERE / 'Cargo.toml').decode()
    manifest = re.sub(r'"(../../(?:crates/[^"\n]+|vendor/rue-lir|puzzles))"', lambda m: json.dumps(str(source_root/(HERE/m.group(1)).resolve().relative_to(ROOT))), manifest)
    manifest = manifest.replace('[[example]]\nname = "protocol"\npath = "examples/protocol/main.rs"\n\n', '')
    if control == 'original':
        library = read(HERE/'src/lib.rs')
        policy_sources = {}
    else:
        manifest = re.sub(r'^chialisp-oxide-puzzles = .+\n', '', manifest, flags=re.M)
        library = b'pub mod counter_increment;\npub mod counter_reset;\npub mod counter_types;\npub use counter_increment::puzzle as increment;\npub use counter_reset::puzzle as reset;\npub use counter_types::{IncrementOutput, RESET_INTENT, ResetOutput, StateTruth};\n'
        policy_sources = source(control)
    return {'Cargo.toml': manifest.encode(), 'src/lib.rs': library, **policy_sources,
            'tests/native.rs': read(HERE/'control-native.rs'),
            'vectors.json': (json.dumps(spec(control), indent=2)+'\n').encode(),
            'raw-vm.rs': read(HERE/'raw-vm.rs'),
            **{'reference/'+e+'.clsp': read(ROOT/'puzzles'/('counter-'+e)/'reference.clsp') for e in EXPORTS}}

def lock_admission(raw):
    reviewed = tomllib.loads(read(ROOT/'Cargo.lock').decode())['package']
    known = {(p['name'], p['version'], p['source']):p.get('checksum') for p in reviewed if 'source' in p}
    local = {(p['name'],p['version']) for p in reviewed if 'source' not in p}
    seen = set()
    for package in tomllib.loads(raw.decode())['package']:
        if 'source' in package:
            key = (package['name'], package['version'], package['source'])
            require(key not in seen and key in known and package.get('checksum') == known[key], 'reviewed root external lock identity')
            seen.add(key)
        else:
            require((package['name'],package['version']) in local or (package['name'],package['version'])==(PACKAGE,'0.0.0'), 'reviewed exact local package identities')


def project(output, control):
    require(control in CONTROLS, 'closed control name')
    return output/'projects'/control


def materialized(output):
    return [identity(project(output,c)/name) for c in CONTROLS for name in (*FILES, *(MUTANT_SOURCES if c!='original' else ()))]


def source_window(root):
    # Includes compiler build.rs, facade and vendor Rue, all evidence readers and
    # the owned experiment. Broad closure is deliberate; no stale helper cache.
    names = source_files(root, ['crates','vendor/rue-lir','puzzles','scripts'],
                         suffixes=('.rs','.toml','.json','.md','.py','.clsp','.clib','.rue','.hex','.lock'), names=('LICENSE',))
    names += ['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo/config.toml']
    if root == ROOT:
        names += source_names(root, ['experiments/action-counter'])
        for base in ('corpus/upstream/slot-machine/6edc94856306d0f77e065c6c99aacf1055e51e2f','corpus/upstream/chia-wallet-sdk/e9a08f6e54ea5132f7da06327836a3b386a944b1'):
            inventory = json.loads(read(root/base/'source-inventory.json'))
            declared = [base+'/'+row['path'] for row in inventory['files']]
            names += source_names(root,[base],declared_cache=declared)
    return retained_identities(root, sorted(set(names)), maximum=128*1024*1024)


def quiet(args, tools=()):
    return {'source_root':str(ROOT), 'sources':source_window(ROOT),
            'compiler_root':str(args.compiler_root), 'compiler_sources':source_window(args.compiler_root),
            'materialized':materialized(args.output),
            'tools':[identity(p,512*1024*1024) for p in tools], 'rustc':identity(args.rustc,512*1024*1024)}


def env(args):
    return {'RUSTC':str(args.rustc)}


def host(args, label, argv, *, control):
    return command(args.output, label, ['rustup','run',TOOLCHAIN,'cargo',*argv], target=native_target(args,control), env_extra=env(args))


def verify(args):
    frozen_inputs()
    record = json.loads(read(args.output/'prepared.json'))
    require(same_json(record['root_lock'],identity(ROOT/'Cargo.lock')), 'reviewed root lock unchanged')
    for c in CONTROLS:
        p = project(args.output,c)
        for name, expected in project_inputs(c).items():
            require(read(p/name) == expected, 'exact materialized source/native/manifest/reference: '+c+'/'+name)
        lock_admission(read(p/'Cargo.lock'))
    require(same_json(record['materialized'],materialized(args.output)), 'exact prepared materialized lock/source inventory')
    require(same_json(record['sources'],source_window(ROOT)), 'source inputs changed since preparation')
    return record


def snapshot(p):
    if p == ROOT:
        return cargo_authoring.snapshot(ROOT)
    return {'mode':'cargo_library','package':PACKAGE,'manifest':'Cargo.toml','library':'src/lib.rs',
            'exports':{e:{'entry':CRATE+'::counter_'+e+'::puzzle','source':'src/counter_'+e+'.rs'} for e in EXPORTS},
            'sources':[{'path':n,'sha256':digest(read(p/n))} for n in (*FILES,*MUTANT_SOURCES)]}

def prepare(args):
    frozen_inputs()
    args.output.mkdir(parents=True, exist_ok=False)
    pinned = command(args.output, 'pinned-rustc-path', ['rustup','which','--toolchain',TOOLCHAIN,'rustc'], target=args.native_target, env_extra=env(args)).decode().strip()
    require(Path(pinned).resolve() == args.rustc, 'explicit RUSTC is pinned rustup compiler')
    for c in CONTROLS:
        p = project(args.output,c)
        for name, payload in project_inputs(c).items():
            path = p/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(payload)
        (p/'Cargo.lock').write_bytes(read(ROOT/'Cargo.lock'))
        # Resolve the local project while retaining the seeded external pins.
        # Actual Cargo probes matched full metadata's complete lock graph;
        # metadata --no-deps alone leaves the unrelated workspace lock intact.
        host(args, 'prepare-lock-'+c.replace('_','-'), ['update','--workspace','--manifest-path',p/'Cargo.toml','--offline'],control=c)
        lock_admission(read(p/'Cargo.lock'))
    write(args.output/'prepared.json', {'protocol':'chialisp-oxide-action-counter-prepared/v1','root_lock':identity(ROOT/'Cargo.lock'),
          'materialized':materialized(args.output),'sources':source_window(ROOT),'status':'prepared_only'})


def native(args):
    verify(args)
    before = quiet(args);write(args.output/'native-window-before.json',before)
    observations = []
    for c in CONTROLS:
        p = project(args.output,c);label = c.replace('_','-')
        host(args, 'fmt-'+label, ['fmt','--manifest-path',p/'Cargo.toml','--check'],control=c)
        # Same-name path packages may alias cached units across captures. Keep
        # dependencies cached, but rebuild this exact policy and test package.
        host(args, 'clean-'+label, ['clean','--manifest-path',p/'Cargo.toml','--package',PACKAGE,'--locked'],control=c)
        host(args, 'check-'+label, ['check','--manifest-path',p/'Cargo.toml','--all-targets','--locked'],control=c)
        stdout = host(args, 'native-'+label, ['test','--manifest-path',p/'Cargo.toml','--test','native','--locked','--','--nocapture'],control=c).decode()
        require(re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',stdout) == [('1','0','0')], 'one executed nonignored test per source project')
        actual = []
        for line in stdout.splitlines():
            for prefix in ('ACTION_COUNTER_NATIVE_ADAPTER ', 'ACTION_COUNTER_NATIVE_EXPORT '):
                if prefix in line:
                    actual.append((prefix.split()[0],json.loads(line.split(prefix,1)[1])))
        expected = []
        for row in spec(c)['cases']:
            if row['expected'] == {'status':'rejected','stage':'abi'}:
                expected.append(('ACTION_COUNTER_NATIVE_ADAPTER',{'export':row['export'],'name':row['name'],'status':'abi_rejected','scope':'Host adapter only; export not called'}))
            else:
                outcome = row['expected'] if row['expected']['status']=='accepted' else {'status':'policy_panic'}
                expected.append(('ACTION_COUNTER_NATIVE_EXPORT',{'export':row['export'],'name':row['name'],'actual':outcome}))
        require(same_json(actual,expected), 'actual native independent exact witnesses, including mutant MAX')
        observations.append({'control':c,'observations':actual,'stdout':identity(args.output/('native-'+label+'.stdout'))})
        host(args,'clippy-'+label,['clippy','--manifest-path',p/'Cargo.toml','--all-targets','--locked','--','-D','warnings'],control=c)
    # Last native host build precedes every extraction; one runner suffices for
    # reference/VM work and never calls policy Rust, so its policy control is nil.
    p = project(args.output,'original')
    host(args,'last-host-build',['build','--manifest-path',p/'Cargo.toml','--example','raw-vm','--locked'],control='original')
    host(args,'last-protocol-build',['build','--manifest-path',HERE/'Cargo.toml','--example','protocol','--locked'],control='original')
    protocol_runner = identity(protocol_path(args),512*1024*1024)
    runner = runner_path(args)
    after = quiet(args);write(args.output/'native-window-after.json',after)
    require(same_json(before,after),'source changed through native/LAST host build')
    write(args.output/'native.json',{'status':'passed','source_window':before,'runner':identity(runner,512*1024*1024),
          'protocol_runner':protocol_runner,'groups':observations,
          'test_groups':3,'actual_export_calls':177,'host_abi_rows':120})


def verify_native(args, record):
    require(record['status']=='passed' and type(record['test_groups']) is int and record['test_groups']==3
            and type(record['actual_export_calls']) is int and record['actual_export_calls']==177
            and type(record['host_abi_rows']) is int and record['host_abi_rows']==120, 'exact native count types')
    require([g['control'] for g in record['groups']]==list(CONTROLS),'exact native source groups')
    calls=0;adapters=0
    for group in record['groups']:
        c=group['control'];label='native-'+c.replace('_','-')
        stdout_path=args.output/(label+'.stdout')
        require(same_json(identity(stdout_path),group['stdout']),'native stdout identity')
        stdout=read(stdout_path).decode()
        require(re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',stdout)==[('1','0','0')], 'one actual native test, never zero-test success')
        actual=[]
        for line in stdout.splitlines():
            for prefix in ('ACTION_COUNTER_NATIVE_ADAPTER ', 'ACTION_COUNTER_NATIVE_EXPORT '):
                if prefix in line:actual.append([prefix.split()[0],json.loads(line.split(prefix,1)[1])])
        expected=[]
        for row in spec(c)['cases']:
            if row['expected']=={'status':'rejected','stage':'abi'}:
                adapters+=1
                expected.append(['ACTION_COUNTER_NATIVE_ADAPTER',{'export':row['export'],'name':row['name'],'status':'abi_rejected','scope':'Host adapter only; export not called'}])
            else:
                calls+=1
                outcome=row['expected'] if row['expected']['status']=='accepted' else {'status':'policy_panic'}
                expected.append(['ACTION_COUNTER_NATIVE_EXPORT',{'export':row['export'],'name':row['name'],'actual':outcome}])
        require(same_json(actual,expected) and same_json(expected,group['observations']),'exact actual native source witnesses')
        receipt=json.loads(read(args.output/(label+'.command.json')))
        require(receipt['status']=='passed' and type(receipt['exit_code']) is int and receipt['exit_code']==0
                and same_json(receipt['stdout'],identity(stdout_path)),'actual native successful command receipt')
    require(calls==record['actual_export_calls'] and adapters==record['host_abi_rows'],'native counts derived from exact observed rows')
    check_commands(args,all_phases=False)


def tools(args):
    n = json.loads(read(args.output/'native.json'))
    verify_native(args,n)
    runner = Path(n['runner']['path'])
    require(runner==runner_path(args),'exact LAST host runner output path')
    require(same_json(identity(runner,512*1024*1024),n['runner']),'LAST host runner unchanged')
    protocol = protocol_path(args)
    require(n['protocol_runner']['path']==str(protocol)
            and same_json(identity(protocol,512*1024*1024),n['protocol_runner']), 'LAST protocol build output unchanged before extraction/replay/SDK')
    require(same_json(n['source_window'],quiet(args))
            and same_json(n['source_window'],json.loads(read(args.output/'native-window-before.json')))
            and same_json(n['source_window'],json.loads(read(args.output/'native-window-after.json'))),'native/LAST host source window unchanged')
    return (args.driver,args.cargo_cli,protocol,runner)


def admit_bundle(args, c, mode, compiler, driver_hash):
    p = ROOT if c == 'original' else project(args.output,c)
    bundle = args.output/('bundle-'+c+'-'+mode)
    authoring = snapshot(p)
    result = []
    # load_declared_export admits the entire immutable bundle inventory before
    # returning a selected entry, including all37 original paired exports.
    for export_name in EXPORTS:
        entry = ('chialisp_oxide_puzzles' if c == 'original' else CRATE)+'::counter_'+export_name+'::puzzle'
        export = cargo_bundle.load_declared_export(bundle, root=p, authoring=authoring,selected_entry=entry,
          compiler=compiler,driver_sha256=driver_hash,toolchain=TOOLCHAIN,current_authoring=lambda:snapshot(p),
          optimize=mode=='on',cargo_options=['--manifest-path',str(p/('puzzles/Cargo.toml' if c=='original' else 'Cargo.toml')),'--locked'])
        require(export.manifest['schema_version']==2,'complete schema2 bundle')
        declared = {v['entry'] for v in authoring['exports'].values()}
        require({r['entry'] for r in export.manifest['entries']} == declared, 'complete exact authoring inventory before selection')
        labels = {f.label for f in export.source_files}
        required = {'package/counter_types.rs','package/counter-'+export_name+'/puzzle.rs'} if c=='original' else {'package/Cargo.toml','package/Cargo.lock','package/src/lib.rs',*('package/'+n for n in MUTANT_SOURCES)}
        require(required <= labels, 'actual complete selected-library source inventory')
        for f in export.source_files:
            if f.label.startswith('package/'):
                actual = ROOT/'puzzles'/f.label.removeprefix('package/') if c=='original' else p/f.label.removeprefix('package/')
            elif f.label.startswith('workspace/') and c=='original':
                actual = ROOT/f.label.removeprefix('workspace/')
            else:
                raise ValueError('unexpected selected-library source label: '+f.label)
            require(f.contents==read(actual),'actual source receipt bytes')
            if f.label.endswith('.rs'):
                require(f.kinds==('compiler_text',) and type(f.compiler_read_count) is int and f.compiler_read_count>=1,'actual rustc source-read receipt')
        require(export.program.get('ir_version')==5 and export.program.get('environment')=='strict','actual strict v5 Cons/borrowed-state ABI; no fallback')
        selected = next(r for r in export.manifest['entries'] if r['entry']==entry)
        raw = bytes.fromhex(selected['artifact']['clvm_hex']);inspected = inspect(raw)
        require(inspected['identity']['module_tree_hash']==selected['artifact']['tree_hash'],'independent bounded artifact tree hash')
        result.append({'id':c+'/'+mode+'/'+export_name,'control':c,'mode':mode,'export':export_name,'clvm_hex':raw.hex(),
             'inspection':inspected,'ir':identity(export.path),'manifest':identity(export.manifest_path),'source_receipt':export.source_receipt})
    return result

def extract(args):
    verify(args);active_tools = tools(args)
    before = quiet(args,active_tools);write(args.output/'extract-window-before.json',before)
    compiler = command(args.output,'compiler-version',[args.rustc,'-vV'],target=args.compiler_target,env_extra=env(args)).decode()
    declaration = command(args.output,'driver-version',[args.driver,'--compiler-version'],target=args.compiler_target,env_extra=env(args)).decode()
    require(declaration==compiler.splitlines()[0]+'\n','independent rustc/driver declaration agreement')
    artifacts = []
    for c in CONTROLS:
        p = ROOT/'puzzles' if c=='original' else project(args.output,c)
        extra = {**env(args),'CHIALISP_OXIDE_DRIVER':str(args.driver)}
        command(args.output,'subset-'+c.replace('_','-'),[args.cargo_cli,'check','--manifest-path',p/'Cargo.toml','--locked'],target=args.compiler_target,env_extra=extra)
        for mode in ('off','on'):
            argv = [args.cargo_cli,'build','--manifest-path',p/'Cargo.toml','--locked','--output-dir',args.output/('bundle-'+c+'-'+mode)]
            if mode=='off':argv.append('--no-optimize')
            command(args.output,'extract-'+c.replace('_','-')+'-'+mode,argv,target=args.compiler_target,env_extra=extra)
            artifacts += admit_bundle(args,c,mode,compiler,before['tools'][0]['sha256'])
    for c in CONTROLS:
        for e in EXPORTS:
            pair = [a for a in artifacts if a['control']==c and a['export']==e]
            require(len(pair)==2 and pair[0]['ir']['sha256']==pair[1]['ir']['sha256'],'same actual source IR across modes')
    # Reference module compilation is explicit, independent of Rust extraction.
    raw = command(args.output,'compile-reference',[active_tools[-1],'compile-reference',project(args.output,'original')/'reference'],target=native_target(args,'original'),env_extra=env(args))
    refs = json.loads(raw)
    require(refs['compiler']=='chialisp 0.4.6' and [r['export'] for r in refs['artifacts']]==list(EXPORTS),'independent reference compiler and exact modules')
    for row in refs['artifacts']:
        compiled = bytes.fromhex(row['clvm_hex'])
        artifacts.append({'id':'original/chialisp/'+row['export'],'control':'original','mode':'chialisp',**row,'inspection':inspect(compiled)})
    rue_raw = command(args.output,'compile-rue',[active_tools[-1],'compile-rue',ROOT/'puzzles'],target=native_target(args,'original'),env_extra=env(args))
    rue = json.loads(rue_raw)
    require(rue['rue_revision']=='245c107a8e795229ea08097142f005a4c0d91075', 'actual pinned Rue source compiler')
    require([(r['mode'],r['export']) for r in rue['artifacts']]==[(m,e) for m in ('rue-off','rue-on') for e in EXPORTS], 'actual four Rue-source compilations')
    for row in rue['artifacts']:
        compiled=bytes.fromhex(row['clvm_hex'])
        source_path=ROOT/'puzzles'/('counter-'+row['export'])/'puzzle.rue'
        artifacts.append({'id':'original/'+row['mode']+'/'+row['export'],'control':'original',**row,'source':identity(source_path),'inspection':inspect(compiled)})
    after=quiet(args,active_tools);write(args.output/'extract-window-after.json',after)
    require(same_json(after,before),'source/tools/runner changed during extraction/reference compilation')
    write(args.output/'artifacts.json',{'protocol':'chialisp-oxide-action-counter-artifacts/v2','compiler':compiler,'driver_declaration':declaration,'artifacts':artifacts})


def replay(args):
    verify(args);active_tools = tools(args)
    baseline = json.loads(read(args.output/'extract-window-before.json'))
    require(same_json(quiet(args,active_tools),baseline) and same_json(baseline,json.loads(read(args.output/'extract-window-after.json'))),'same source/tool window before replay')
    packet=json.loads(read(args.output/'artifacts.json'))
    from raw_evidence import recorded_queries
    recorded_queries(args,packet)
    references=json.loads(read(args.output/'compile-reference.stdout'))
    require(references['compiler']=='chialisp 0.4.6' and references['artifacts']==[{k:r[k] for k in ('export','clvm_hex')} for r in packet['artifacts'][12:14]],'retained reference compiler output matches inventory')
    rue=json.loads(read(args.output/'compile-rue.stdout'))
    require(rue['rue_revision']=='245c107a8e795229ea08097142f005a4c0d91075' and same_json(rue['artifacts'],[{k:r[k] for k in ('mode','export','optimize_lir','clvm_hex')} for r in packet['artifacts'][14:]]), 'retained actual Rue compilations match complete inventory')
    # Re-admit all retained bundles, not only rows chosen for replay.
    rust=[]
    for c in CONTROLS:
        for mode in ('off','on'):
            rust += admit_bundle(args,c,mode,packet['compiler'],baseline['tools'][0]['sha256'])
    require(same_json(packet['artifacts'][:12],rust),'fresh complete retained artifact readmission')
    from raw_evidence import validate_inventory, validate_report
    validate_inventory(packet)
    write(args.output/'request.json',request_recipe(packet))
    retained=retained_paths(args)
    before={'source_tools':quiet(args,active_tools),'retained':retained_identities(args.output,retained)}
    write(args.output/'replay-window-before.json',before)
    first = command(args.output,'raw-attempt1',[active_tools[-1],'replay',args.output/'request.json'],target=native_target(args,'original'),env_extra=env(args))
    # Validate each run independently and preserve both on disagreement.
    summary = validate_report(json.loads(first),packet)
    second = command(args.output,'raw-attempt2',[active_tools[-1],'replay',args.output/'request.json'],target=native_target(args,'original'),env_extra=env(args))
    validate_report(json.loads(second),packet)
    require(first==second,'independent repeat report differs; retained both attempts')
    require(retained_paths(args)==retained,'complete retained input inventory changed')
    after={'source_tools':quiet(args,active_tools),'retained':retained_identities(args.output,retained)}
    write(args.output/'replay-window-after.json',after);require(same_json(after,before),'source/runner/artifacts changed through repeated replay')
    write(args.output/'execution.json',{'status':'raw_vm_executed_repeat_identical','summary':summary,'first':identity(args.output/'raw-attempt1.stdout'),'repeat':identity(args.output/'raw-attempt2.stdout'),
          'scope':'Finite original Rust off/on, independently compiled Chialisp/Rue off/on actions plus actual-source Rust mutants. No outer/singleton/SDK/live or compiler-preservation credit.'})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=('prepare','native','extract','replay','read'))
    for name in ('output','native-target','compiler-target','compiler-root','rustc'):
        parser.add_argument('--'+name,type=Path,required=True)
    for name in ('driver','cargo-cli'):
        parser.add_argument('--'+name,type=Path)
    args=parser.parse_args()
    for name in ('output','native_target','compiler_target','compiler_root','rustc','driver','cargo_cli'):
        if getattr(args,name) is not None:setattr(args,name,getattr(args,name).resolve())
    require(args.output != args.native_target and args.output != args.compiler_target,'separate target/output directories')
    for path in (args.output,args.native_target,args.compiler_target):
        require(not path.is_relative_to(ROOT/'experiments'),'outputs outside owned source')
    if args.phase in ('extract','replay','read'):require(args.driver and args.cargo_cli,'explicit prebuilt coordinated tools required')
    if args.phase=='read':
        from raw_evidence import recorded
        recorded(args)
    else:
        globals()[args.phase](args)

if __name__=='__main__':main()
