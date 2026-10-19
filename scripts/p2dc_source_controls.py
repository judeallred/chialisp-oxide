"""Finite actual-source mutation controls, distinct from puzzle/SDK admission.

Default readers check current inputs and retained metadata/results only. Source
freshness and receipt consistency do not authenticate execution, prove compiler
preservation, or establish authorization/state/BLS/security properties.
"""
from pathlib import Path
import copy
import hashlib
import json
import os
import re
import stat
import tomllib

import cargo_authoring
import cargo_bundle
import p2_delegated_conditions_vectors as vectors
from hashing_vectors import deserialize, sha_tree
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = 'chialisp-oxide-p2dc-source-controls/v1'
PACKAGE = 'chialisp-oxide-p2dc-controls'
TEMPLATE = 'experiments/p2dc-source-controls/native.rs'
TEMPLATE_SHA = 'c3a061a63b56f855affb54b0f390886c283a949ca3a647111538eaa009c31996'
NAMES = ('baseline','wrong_atom_domain','wrong_conditions_tail','reject_ignored_improper_tail')
FILES = sorted(['Cargo.toml','src/lib.rs','src/baseline.rs','tests/native.rs','witnesses.json']+[f'src/{n}.rs' for n in NAMES[1:]])
TOOLS = ('cargo-chialisp-oxide','chialisp-oxide-driver','chialisp-oxide','chialisp-oxide-verifier')
MEMBERS = ('experiments/backend','experiments/byte-proofs','crates/chialisp-oxide-ir',
    'crates/chialisp-oxide-reference','crates/chialisp-oxide-rue','crates/chialisp-oxide-driver',
    'crates/chialisp-oxide','crates/chialisp-oxide-macros','crates/cargo-chialisp-oxide',
    'crates/chialisp-oxide-simulator','crates/chialisp-oxide-verifier','crates/chialisp-oxide-corpus',
    'crates/chialisp-oxide-cli','crates/chialisp-oxide-vm','puzzles','vendor/rue-lir')
CODE = ['scripts/'+n for n in ('p2dc_source_controls.py','check-p2dc-source-controls.py','run-p2dc-source-controls.py','test_p2dc_source_controls.py','p2_delegated_conditions_vectors.py','cargo_authoring.py','cargo_bundle.py','hashing_provenance.py','hashing_vectors.py','check-canonical-factories.py','vm_failures.py')]+[TEMPLATE,'corpus/validate_published_puzzles.py']
SCOPE = 'One actual native Rust test group checks three independent source-policy observations. Four actual annotated Cargo exports in both modes and twelve retained primary VM witnesses are separate from SDK spends, generic ledger groups and proofs. The baseline copy is explicitly a control. Recorded source/metadata/result consistency is not execution authentication, compiler preservation, exhaustive policy correctness or production readiness.'
require,closed = vectors.require,vectors.closed


def digest(value):return hashlib.sha256(value).hexdigest()
def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
def same(a,b):return canonical(a)==canonical(b)


def read(path,limit=16*1024*1024,*,allow_empty=False):
    descriptor=os.open(path,os.O_RDONLY|os.O_NONBLOCK)
    try:
        metadata=os.fstat(descriptor)
        require(stat.S_ISREG(metadata.st_mode) and (0 if allow_empty else 1)<=metadata.st_size<=limit,'bounded nonempty regular file required')
        with os.fdopen(descriptor,'rb',closefd=False) as stream:payload=stream.read(limit+1)
        require((0 if allow_empty else 1)<=len(payload)<=limit,'read byte bound')
        return payload
    finally:os.close(descriptor)


def load(path):return json.loads(read(path),object_pairs_hook=cargo_authoring.unique_object)
def sha(path,limit=16*1024*1024):
    descriptor=os.open(path,os.O_RDONLY|os.O_NONBLOCK)
    try:
        require(stat.S_ISREG(os.fstat(descriptor).st_mode),'regular hash input required')
        value=hashlib.sha256();total=0
        with os.fdopen(descriptor,'rb',closefd=False) as stream:
            while True:
                chunk=stream.read(min(65536,limit-total+1))
                if not chunk:break
                total+=len(chunk);require(total<=limit,'hash byte bound');value.update(chunk)
        return value.hexdigest()
    finally:os.close(descriptor)


def absolute(value):
    require(type(value) is str and value.startswith('/') and not value.startswith('//'),'captured absolute path required')
    path=Path(value);require(str(path)==value and '..' not in path.parts,'normalized captured path required')
    return path


def inputs(root=ROOT):
    _,rows,recipes=vectors.source_inputs(root)
    require(digest(read(root/TEMPLATE))==TEMPLATE_SHA,'reviewed native template changed')
    baseline=read(root/f'puzzles/{vectors.SLUG}/puzzle.rs').decode();by_label={r['label']:r for r in rows}
    sources={'src/baseline.rs':baseline};witnesses=[]
    raw={}
    for recipe,option in zip(recipes['mutations'],[{'atom_domain':b'\x02'},{'wrong_rest':True},{'strict_tail':True}],strict=True):
        require(baseline.count(recipe['before'])==1,'unique source mutation anchor')
        result=baseline.replace(recipe['before'],recipe['after'],1);raw[recipe['name']]=result
        # Exact relation from the retained actual fmt diagnostic efd019... .
        # No generic tokenizer, source semantics or formatter theorem is claimed.
        if recipe['name']=='reject_ignored_improper_tail':
            old='    assert!(matches!(environment, Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))));\n'
            new='    assert!(matches!(\n        environment,\n        Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))\n    ));\n'
            require(result.count(old)==1,'reviewed format site changed');result=result.replace(old,new,1)
        sources[f"src/{recipe['name']}.rs"]=result
        spec=by_label[recipe['witness']];expected=vectors.expected(spec['solution'],**option)
        require(expected!=spec['expected'],'source mutant witness must distinguish')
        witnesses.append(dict(name=recipe['name'],witness=recipe['witness'],environment_hex=spec['environment_hex'],solution=spec['solution'],baseline=spec['expected'],mutant=expected))
    return baseline,raw,sources,witnesses


def package_files(captured_root,root=ROOT):
    """Exact generated bytes, apart from Cargo.lock produced later."""
    baseline,raw,sources,witnesses=inputs(root);source_root=absolute(str(captured_root))
    files={path:text.encode() for path,text in sources.items()}
    files['Cargo.toml']=('[package]\nname="'+PACKAGE+'"\nversion="0.1.0"\nedition="2024"\npublish=false\n[workspace]\n[dependencies]\nchialisp-oxide={path='+json.dumps(str(source_root/'crates/chialisp-oxide'))+'}\n[dev-dependencies]\nserde_json="=1.0.154"\n').encode()
    files['src/lib.rs']=''.join('pub mod '+name+';\n' for name in sorted(NAMES)).encode()
    files['tests/native.rs']=read(root/TEMPLATE)
    files['witnesses.json']=(json.dumps(witnesses,indent=2)+'\n').encode()
    require(sorted(files)==FILES,'closed eight generated inputs')
    return files,raw,witnesses


def prepared_metadata(value,package,*,locked,root=ROOT):
    """Check exact prepared identities before any requested Cargo/extraction."""
    closed(value,['formatter','source_files','witnesses'],'prepared metadata')
    expected,raw,witnesses=package_files(root,root);baseline=expected['src/baseline.rs']
    require(same(value['witnesses'],witnesses),'independent prepared witnesses')
    formatter=value['formatter'];closed(formatter,['path','sha256','version','argv','relations'],'formatter receipt')
    absolute(formatter['path']);require(type(formatter['sha256']) is str and re.fullmatch('[0-9a-f]{64}',formatter['sha256'])
        and type(formatter['version']) is str and formatter['version'],'formatter identity')
    require(formatter['argv']==[formatter['path'],'--edition','2024','--emit','stdout','--config','skip_children=true'],'exact formatter options')
    relations=[dict(path='src/'+name+'.rs',raw_sha256=digest(baseline if name=='baseline' else raw[name].encode()),formatted_sha256=digest(expected['src/'+name+'.rs'])) for name in sorted(NAMES)]
    require(same(formatter['relations'],relations),'reviewed prepared formatting relation')
    paths=sorted(FILES+(['Cargo.lock'] if locked else []))
    require(type(value['source_files']) is list and [r['path'] for r in value['source_files']]==paths,'prepared source input set')
    for row in value['source_files']:
        closed(row,['path','sha256'],'prepared source');payload=read(Path(package)/row['path'])
        require(digest(payload)==row['sha256'],'prepared source fingerprint')
        if row['path']=='Cargo.lock':tomllib.loads(payload.decode())
        else:require(payload==expected[row['path']],'prepared source recipe/format bytes')


def private_snapshot(package,root=ROOT):
    package=Path(package).resolve();expected,_,_=package_files(root,root)
    for path,value in expected.items():require(read(package/path)==value,'prepared source differs from exact recipe/format/template: '+path)
    require(sorted(p.relative_to(package).as_posix() for p in package.rglob('*') if p.is_file())==sorted(FILES+['Cargo.lock']),'closed prepared nine inputs')
    tomllib.loads(read(package/'Cargo.lock').decode())
    return dict(mode='cargo_library',package=PACKAGE,manifest='Cargo.toml',library='src/lib.rs',
        exports={n:dict(entry=f'chialisp_oxide_p2dc_controls::{n}::puzzle',source=f'src/{n}.rs') for n in NAMES},
        sources=[dict(path=p,sha256=sha(package/p)) for p in sorted(FILES+['Cargo.lock'])])


def declared_source_entries(member,metadata):
    """Closed conventional target layouts; not a general Cargo/Rust scanner."""
    require(type(metadata.get('package')) is dict,'workspace package metadata')
    build=metadata['package'].get('build')
    require(build is None or build is False or build=='build.rs','unsupported compiler build source outside reviewed build.rs')
    for kind,directory in [('lib','src'),('bin','src'),('example','examples'),('test','tests'),('bench','benches')]:
        entries=([metadata[kind]] if kind in metadata else []) if kind=='lib' else metadata.get(kind,[])
        require(type(entries) is list and len(entries)<=64,'bounded declared source target list')
        for entry in entries:
            require(type(entry) is dict,'declared Cargo source target')
            source=entry.get('path')
            if source is None:continue # Cargo defaults stay in scanned conventional roots.
            require(type(source) is str and source and not Path(source).is_absolute()
                and '..' not in Path(source).parts and Path(source).as_posix()==source
                and Path(source).suffix=='.rs'
                and ((kind=='lib' and member=='puzzles' and source=='lib.rs')
                    or source.startswith(directory+'/')),
                'unsupported declared '+kind+' source outside reviewed '+directory+' layout')


def source_window(root=ROOT):
    """Declared corpus plus actual compiler/native/build/reader inputs; no caches."""
    root=Path(root);authoring=cargo_authoring.snapshot(root);paths={r['path'] for r in authoring['sources']}
    paths.update(['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo/config.toml','crates/chialisp-oxide-vm/failure-protocol.json',*CODE])
    def collect(path):
        paths.add(path);return vectors.read(root,path)
    vectors.source_inputs(root,reader=collect)
    manifest=tomllib.loads(vectors.read(root,'Cargo.toml').decode())
    members=manifest['workspace']['members'];require(type(members) is list and members==list(MEMBERS),'reviewed workspace source packages required')
    for member in members:
        require(type(member) is str and not Path(member).is_absolute() and '..' not in Path(member).parts,'workspace source path')
        paths.add(member+'/Cargo.toml');metadata=tomllib.loads(vectors.read(root,member+'/Cargo.toml').decode())
        declared_source_entries(member,metadata)
        if (root/member/'build.rs').is_file():paths.add(member+'/build.rs')
        # Include conventional implicit and explicit targets too. This is bounded
        # source-root discovery, never a recursive scan of target/cache outputs.
        for directory in ('src','examples','tests','benches'):
            paths.update(p.relative_to(root).as_posix() for p in (root/member/directory).rglob('*.rs') if p.is_file())
    total=0;rows=[];require(len(paths)<=4096,'implementation source count bound')
    for path in sorted(paths):
        payload=vectors.read(root,path);total+=len(payload);require(total<=64*1024*1024,'implementation source byte bound')
        rows.append(dict(path=path,sha256=digest(payload)))
    return dict(authoring=authoring,sources=rows)


def compiler_host(compiler):
    require(type(compiler) is str and compiler.endswith('\n') and len(compiler)<=4096,'full compiler identity')
    rows=compiler.splitlines();require(len(rows)==7 and rows[0].startswith('rustc '),'compiler identity lines')
    fields={}
    for row in rows[1:]:
        key,separator,value=row.partition(': ');require(separator and key not in fields and value,'compiler field identity');fields[key]=value
    require(set(fields)=={'binary','commit-hash','commit-date','host','release','LLVM version'}
        and fields['binary']=='rustc' and fields['commit-hash']=='daf2e5e18b3abc4311d9da3d96fd64138f43d508'
        and fields['commit-date']=='2026-07-13' and fields['release']=='1.99.0-nightly' and fields['LLVM version']=='22.1.8'
        and re.fullmatch('[A-Za-z0-9_.-]+',fields['host']) and rows[0]==f"rustc {fields['release']} ({fields['commit-hash'][:9]} {fields['commit-date']})",'pinned compiler contract')
    return fields['host']


def native_rows(text,witnesses):
    require(type(text) is str and len(text.encode())<=1024*1024,'native log byte bound')
    values=[json.loads(x,object_pairs_hook=cargo_authoring.unique_object) for x in re.findall(r'P2DC_MUTATION_NATIVE (\{[^\n]+\})',text)]
    require(len(values)==3 and text.count('test result: ok. 1 passed; 0 failed;')==1,'one native test group/three policy observations required')
    for row,spec in zip(values,witnesses,strict=True):
        require(same(row,dict(name=spec['name'],witness=spec['witness'],baseline=spec['baseline'],actual=spec['mutant'])),'native observations differ from independent witnesses')
    return values


def raw_abi(program,entry):
    closed(program,['ir_version','environment','program'],'IR envelope')
    require(type(program['ir_version']) is int and program['ir_version']==5 and program['environment']=='raw','v5 raw ABI required')
    body=program['program'];require(type(body) is dict and body.get('name')==entry,'IR entry identity')
    require(len(body['parameters'])==1 and body['parameters'][0]['ty']=={'kind':'tree'},'one logical raw Tree parameter')
    require(body['result']==dict(kind='pair',first=dict(kind='tuple',items=[dict(kind='int',bits=64,signed=False),dict(kind='tree'),dict(kind='bytes',len=32)]),rest=dict(kind='tree')),'exact Cons result ABI')


def options(package):return ['--manifest-path',str(package/'Cargo.toml'),'--package',PACKAGE,'--locked','--offline']


def fresh_bundle(directory,package,*,root,compiler,driver_sha256,toolchain,optimize):
    before=source_window(root);private=private_snapshot(package,root)
    def current():
        require(source_window(root)==before,'main implementation changed during fresh loading')
        return private_snapshot(package,root)
    result=None
    for name in NAMES:
        loaded=cargo_bundle.load_declared_export(directory,root=package,authoring=private,
            selected_entry=private['exports'][name]['entry'],compiler=compiler,driver_sha256=driver_sha256,toolchain=toolchain,
            current_authoring=current,optimize=optimize,rue_revision='245c107a8e795229ea08097142f005a4c0d91075',cargo_options=options(package))
        require(len(loaded.manifest['entries'])==4,'complete four exported controls')
        if result is not None:require(loaded.manifest_path==result.manifest_path,'bundle pointer changed')
        result=loaded
    require(current()==private and source_window(root)==before,'fresh input window changed')
    return result


def relative(base,path):
    parsed=cargo_bundle._relative(path);base=Path(base).resolve();original=base.joinpath(*parsed.parts)
    require(all(not current.is_symlink() for current in [original,*list(original.parents)[:len(parsed.parts)-1]]),'retained artifact path cannot contain symlinks')
    return cargo_bundle._contained(base,path)


def validate_recorded(report,base,root=ROOT):
    """NO processes/queries. Actual executed receipt claims remain unauthenticated."""
    closed(report,['schema_version','protocol','status','scope','context','prepared','native','bundles','runs','commands','files'],'source-control report')
    require(type(report['schema_version']) is int and report['schema_version']==1 and report['protocol']==PROTOCOL
        and report['status']=='actual_source_controls_captured' and report['scope']==SCOPE,'recorded protocol/status/scope')
    context=report['context'];closed(context,['source_root','package_root','capture_root','compiler','toolchain','source_window','tools','last_host_window'],'capture context')
    source=absolute(context['source_root']);package=absolute(context['package_root']);capture=absolute(context['capture_root']);host=compiler_host(context['compiler'])
    require(context['toolchain']==tomllib.loads(vectors.read(root,'rust-toolchain.toml').decode())['toolchain']['channel']=='nightly-2026-07-14','toolchain identity')
    require(same(context['source_window'],source_window(root)),'stale current compiler/native/SDK/reader inputs')
    require(len(context['source_window']['authoring']['exports'])==23,'full current23 authoring required')
    closed(context['tools'],TOOLS,'tool capture')
    for row in context['tools'].values():
        closed(row,['path','sha256'],'tool receipt');absolute(row['path']);require(re.fullmatch('[0-9a-f]{64}',row['sha256']) is not None,'tool digest')
    parents={absolute(row['path']).parent for row in context['tools'].values()}
    require(len(parents)==1 and next(iter(parents)).name=='debug' and all(absolute(row['path']).name==name for name,row in context['tools'].items()),'closed captured tool root/names')
    closed(context['last_host_window'],['path','sha256'],'last host quiet receipt');absolute(context['last_host_window']['path'])
    require(re.fullmatch('[0-9a-f]{64}',context['last_host_window']['sha256']) is not None,'last host receipt hash')
    prepared=report['prepared'];closed(prepared,['formatter','source_files','witnesses'],'prepared metadata')
    expected,_,witnesses=package_files(source,root);require(same(prepared['witnesses'],witnesses),'independent witnesses changed')
    formatter=prepared['formatter'];closed(formatter,['path','sha256','version','argv','relations'],'formatter receipt')
    absolute(formatter['path']);require(re.fullmatch('[0-9a-f]{64}',formatter['sha256']) and type(formatter['version']) is str and formatter['version'],'formatter identity')
    require(formatter['argv']==[formatter['path'],'--edition','2024','--emit','stdout','--config','skip_children=true'],'exact formatter options')
    baseline,raw,_,_=inputs(root)
    relations=[dict(path='src/'+name+'.rs',raw_sha256=digest((baseline if name=='baseline' else raw[name]).encode()),formatted_sha256=digest(expected['src/'+name+'.rs'])) for name in sorted(NAMES)]
    require(same(formatter['relations'],relations),'exact reviewed raw→formatted relation')
    require(type(prepared['source_files']) is list and [r['path'] for r in prepared['source_files']]==sorted(FILES+['Cargo.lock']),'prepared nine-file set')
    fingerprints=[]
    for row in prepared['source_files']:
        closed(row,['path','sha256'],'prepared source');payload=read(relative(base,'package/'+row['path']))
        require(digest(payload)==row['sha256'],'prepared retained source hash')
        if row['path']!='Cargo.lock':require(payload==expected[row['path']],'retained actual source not exact recipe→format output')
        else:tomllib.loads(payload.decode())
        fingerprints.append(row)
    native=report['native'];closed(native,['argv','exit_code','native_test_groups','native_policy_observations','log','log_sha256'],'native capture')
    quiet=load(relative(base,'last-host-window.json'))
    closed(quiet,['schema_version','protocol','status','source_root','package_root','compiler','toolchain','source_window','tools','native_receipt_sha256','scope'],'last host quiet window')
    require(type(quiet['schema_version']) is int and quiet['schema_version']==1
        and quiet['protocol']=='chialisp-oxide-p2dc-controls-quiet-window/v1' and quiet['status']=='LAST_host_build_terminal_tools_quiet'
        and all(quiet[key]==context[key] for key in ('source_root','package_root','compiler','toolchain','source_window','tools'))
        and type(quiet['scope']) is str and quiet['scope'],'last host/current source/tool linkage')
    require(sha(relative(base,'last-host-window.json'))==context['last_host_window']['sha256']
        and sha(relative(base,'native-receipt.json'))==quiet['native_receipt_sha256'],'last host/native receipt digests')
    receipt=load(relative(base,'native-receipt.json'))
    closed(receipt,['schema_version','protocol','status','source_root','package_root','toolchain','source_window','prepared','native','commands'],'native phase receipt')
    require(type(receipt['schema_version']) is int and receipt['schema_version']==1 and receipt['protocol']=='chialisp-oxide-p2dc-controls-native/v1'
        and receipt['status']=='native_phase_passed_before_LAST_host_build'
        and all(receipt[key]==context[key] for key in ('source_root','package_root','toolchain','source_window'))
        and receipt['prepared']==prepared and receipt['native']==native,'native package/source/log linkage')
    expected_commands=[['rustup','run',context['toolchain'],'cargo',*flags,'--manifest-path',str(package/'Cargo.toml'),*tail] for flags,tail in
        [(['generate-lockfile','--offline'],[]),(['fmt'],['--all','--check']),(['check'],['--locked','--offline']),(['test'],['--locked','--offline','--','--nocapture']),(['clippy'],['--locked','--offline','--all-targets','--','-D','warnings'])]]
    require(type(receipt['commands']) is list and len(receipt['commands'])==5,'native five mandatory command receipts')
    for row,argv in zip(receipt['commands'],expected_commands,strict=True):
        closed(row,['argv','exit_code','log_sha256'],'native command')
        require(row['argv']==argv and type(row['exit_code']) is int and row['exit_code']==0
            and type(row['log_sha256']) is str and re.fullmatch('[0-9a-f]{64}',row['log_sha256']),'native ordinary format/check/test/Clippy receipts')
    require(receipt['commands'][3]['log_sha256']==native['log_sha256'],'native test command/log identity')
    require(native['argv']==['rustup','run',context['toolchain'],'cargo','test','--manifest-path',str(package/'Cargo.toml'),'--locked','--offline','--','--nocapture']
        and type(native['exit_code']) is int and native['exit_code']==0
        and type(native['native_test_groups']) is int and native['native_test_groups']==1
        and type(native['native_policy_observations']) is int and native['native_policy_observations']==3,'native one-group/three-observation metadata')
    require(native['log']=='native.log','native log path');log=read(relative(base,native['log']),1024*1024)
    require(digest(log)==native['log_sha256'],'native log digest');native_rows(log.decode(),witnesses)
    closed(report['bundles'],['off','on'],'two control modes');reader=cargo_bundle._Reader();artifacts={};ir_hashes={};compiled_all=[];expected_commands=[]
    for mode in ('off','on'):
        value=report['bundles'][mode];closed(value,['manifest','manifest_sha256','recompiled'],'retained bundle')
        require(value['manifest']==f'bundles/{mode}/manifest.json','manifest retained path')
        manifest_path=relative(base,value['manifest']);payload=read(manifest_path);require(digest(payload)==value['manifest_sha256'],'manifest byte identity')
        manifest=json.loads(payload,object_pairs_hook=cargo_authoring.unique_object)
        closed(manifest,['analysis_target','cargo_options','compiler','driver_sha256','entries','manifest_path','package','package_id','rue_revision','schema_version','status','toolchain','workspace_root'],'actual manifest')
        require(type(manifest['schema_version']) is int and manifest['schema_version']==1 and manifest['status']=='wip_prototype_not_ready_for_use'
            and manifest['package']==PACKAGE and absolute(manifest['workspace_root'])==package and absolute(manifest['manifest_path'])==package/'Cargo.toml'
            and manifest['package_id']==f'path+{package.as_uri()}#{PACKAGE}@0.1.0'
            and manifest['analysis_target']==host and manifest['compiler']==context['compiler'] and manifest['toolchain']==context['toolchain']
            and manifest['driver_sha256']==context['tools']['chialisp-oxide-driver']['sha256'] and manifest['cargo_options']==options(package)
            and manifest['rue_revision']=='245c107a8e795229ea08097142f005a4c0d91075','captured manifest identities/paths/options')
        require(type(manifest['entries']) is list and len(manifest['entries'])==4
            and {r['entry'] for r in manifest['entries']}=={f'chialisp_oxide_p2dc_controls::{n}::puzzle' for n in NAMES},'four export identity set')
        expected_commands.append((f'export-{mode}',[context['tools']['cargo-chialisp-oxide']['path'],'build',*options(package),'--output-dir',str(capture/f'fresh-{mode}')]+(['--no-optimize'] if mode=='off' else []),None))
        paths=set();require(type(value['recompiled']) is list and len(value['recompiled'])==4,'four ordinary validated IR recompilation receipts')
        for entry in manifest['entries']:
            program,_,ir_hash=cargo_bundle._artifact(entry,manifest_path.parent,reader,paths,mode=='on');raw_abi(program,entry['entry'])
            record=next((x for x in value['recompiled'] if x.get('entry')==entry['entry']),None)
            closed(record,['entry','ir_sha256','artifacts'],'IR recompilation receipt')
            require(record['ir_sha256']==ir_hash and type(record['artifacts']) is list and len(record['artifacts'])==2
                and [x['optimize'] for x in record['artifacts']]==[False,True],'recompile modes/IR binding')
            for item in record['artifacts']:
                closed(item,['optimize','bytes','clvm_hex','tree_hash'],'ordinary compile artifact')
                require(type(item['optimize']) is bool and type(item['bytes']) is int and item['bytes']>0,'compile artifact scalar types')
            name=entry['entry'].split('::')[-2];key=(mode,name);ir_hashes[key]=ir_hash;compiled_all.append((name,record['artifacts']))
            expected_commands.append((f'recompile-{mode}-{name}',[context['tools']['chialisp-oxide-verifier']['path'],str(capture/f'bundles/{mode}'/entry['ir']),'--artifacts'],record['artifacts']))
            selected=record['artifacts'][1 if mode=='on' else 0]
            require(all(selected[k]==entry['artifact'][k] for k in ('bytes','tree_hash','clvm_hex')),'ordinary recompile/retained artifact differs')
            artifact=entry['artifact'];require(sha_tree(deserialize(bytes.fromhex(artifact['clvm_hex']))).hex()==artifact['tree_hash'],'actual serialized artifact tree hash')
            artifacts[(mode,name)]=artifact
        for spec in witnesses:
            for name in ('baseline',spec['name']):
                entry=next(row for row in manifest['entries'] if row['entry']==f'chialisp_oxide_p2dc_controls::{name}::puzzle')
                role=f'vm-{mode}-{name}-{spec["name"]}'
                expected_commands.append((role,[context['tools']['chialisp-oxide']['path'],'run-batch',str(capture/f'bundles/{mode}'/entry['clvm']),'--solutions',str(capture/f'{mode}-{name}-{spec["name"]}-solution.json'),'--max-cost','1000000'],('VM',mode,name,spec['witness'])))
    for name in NAMES:require(ir_hashes[('off',name)]==ir_hashes[('on',name)],'source IR differs across optimization modes')
    for name,compiled in compiled_all:
        for item in compiled:require(all(item[k]==artifacts[('on' if item['optimize'] else 'off',name)][k] for k in ('bytes','tree_hash','clvm_hex')),'both compile modes must match both retained artifacts')
    runs=report['runs'];require(type(runs) is list and len(runs)==12,'exact twelve finite VM comparisons')
    expected_runs=[(mode,spec,name) for mode in ('off','on') for spec in witnesses for name in ('baseline',spec['name'])]
    for row,(mode,spec,name) in zip(runs,expected_runs,strict=True):
        closed(row,['mode','name','witness','solution','artifact_sha256','execution'],'retained VM observation')
        require((row['mode'],row['name'],row['witness'],row['solution'])==(mode,name,spec['witness'],spec['solution'])
            and row['artifact_sha256']==digest(bytes.fromhex(artifacts[(mode,name)]['clvm_hex'])),'VM selected program/witness identity')
        expected=spec['baseline'] if name=='baseline' else spec['mutant']
        require(check_execution(row['execution'])==expected,'VM witness differs from independent full output/acceptance')
        if expected['status']=='rejected':require(row['execution'].get('failure')==dict(kind='program',code='raise'),'only conclusive typed Raise confirms source assertion')
    require(type(report['commands']) is list and len(report['commands'])==22,'exact two exports/eight compiles/twelve VM commands')
    command_files=set()
    for index,(row,(role,argv,result)) in enumerate(zip(report['commands'],expected_commands,strict=True)):
        closed(row,['role','argv','exit_code','stdout','stdout_sha256','stderr','stderr_sha256'],'capture command')
        require(row['role']==role and row['argv']==argv and type(row['exit_code']) is int and row['exit_code']==0
            and row['stdout']==f'logs/{index:02}.stdout' and row['stderr']==f'logs/{index:02}.stderr','actual command role/argv/normal status')
        stdout=read(relative(base,row['stdout']),allow_empty=True);stderr=read(relative(base,row['stderr']),allow_empty=True)
        require(digest(stdout)==row['stdout_sha256'] and digest(stderr)==row['stderr_sha256'],'actual command stream digests')
        command_files.update((row['stdout'],row['stderr']))
        if not role.startswith('export-'):
            require(not stderr,'JSON compile/VM command stderr must be quiet')
            actual=json.loads(stdout,object_pairs_hook=cargo_authoring.unique_object)
            if type(result) is tuple:
                _,mode,name,witness=result
                selected=next(x for x in runs if (x['mode'],x['name'],x['witness'])==(mode,name,witness))
                require(actual==[selected['execution']],'actual raw VM stdout/record binding')
                solution=row['argv'][4];relative_solution=Path(solution).relative_to(capture).as_posix()
                require(load(relative(base,relative_solution))==[selected['solution']],'actual VM input file/witness binding')
                command_files.add(relative_solution)
            else:require(same(actual,result),'actual raw recompile stdout/receipt binding')
    require(type(report['files']) is list and [r['path'] for r in report['files']]==sorted({r['path'] for r in report['files']}),'closed file inventory order')
    expected_files=set('package/'+p for p in FILES+['Cargo.lock'])|{'native.log','native-receipt.json','last-host-window.json'}|command_files
    for mode,value in report['bundles'].items():
        expected_files.add(value['manifest']);manifest=load(relative(base,value['manifest']))
        expected_files.update(f'bundles/{mode}/'+r[k] for r in manifest['entries'] for k in ('ir','clvm'))
    require({r['path'] for r in report['files']}==expected_files,'retained artifact set')
    for row in report['files']:
        closed(row,['path','sha256'],'retained file');require(sha(relative(base,row['path']))==row['sha256'],'retained artifact bytes changed')
    return dict(native_test_groups=1,native_policy_observations=3,exports=4,export_modes=8,retained_VM_observations=12,
        scope='Recorded current-source/recipe/format/identity/typed-result consistency only; no new execution, SDK admission, compiler preservation, authenticated build or formal-model credit.')
