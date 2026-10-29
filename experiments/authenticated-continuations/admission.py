"""Portable complete schema-2 structural admission, not IR/CLVM equivalence.

Captured host paths are compared only as recorded metadata. Source contents are
bound to explicit local files; no old captured host path is opened.
"""
import hashlib
import bounded_process
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'scripts'))
import cargo_bundle
import raw_tree
from cargo_source_receipt import validate_receipt

ENTRY='chialisp_oxide_authenticated_continuations::step'
PACKAGE='chialisp-oxide-authenticated-continuations'
TOOLCHAIN='nightly-2026-07-14'
RUE='245c107a8e795229ea08097142f005a4c0d91075'
SOURCE_NAMES={'Cargo.toml','Cargo.lock','src/lib.rs'}


def require(ok,message):
    if not ok: raise ValueError(message)


def source_snapshot(package):
    return dict(mode='cargo_library',package=PACKAGE,manifest='Cargo.toml',library='src/lib.rs',
        exports={'step':dict(entry=ENTRY,source='src/lib.rs')},
        sources=[dict(path=n,sha256=hashlib.sha256((package/n).read_bytes()).hexdigest()) for n in sorted(SOURCE_NAMES)])


def admit(directory,mode,*,compiler,driver_sha256,sources,cargo_options,drop_amount=False):
    reader=cargo_bundle._Reader()
    manifest,_=reader.json(cargo_bundle._contained(directory,'manifest.json'))
    require(mode in ('on','off') and manifest.get('schema_version')==2,'exact schema2 mode')
    require(manifest.get('status')=='wip_prototype_not_ready_for_use' and manifest.get('package')==PACKAGE,'fixed WIP package')
    require(manifest.get('compiler')==compiler and manifest.get('driver_sha256')==driver_sha256
        and manifest.get('toolchain')==TOOLCHAIN and manifest.get('rue_revision')==RUE,'independent compiler/driver/toolchain identity')
    hosts=[x.removeprefix('host: ') for x in compiler.splitlines() if x.startswith('host: ')]
    require(len(hosts)==1 and manifest.get('analysis_target')==hosts[0],'compiler host relation')
    require(manifest.get('cargo_options')==cargo_options,'exact requested Cargo arguments')
    captured=manifest.get('workspace_root')
    require(isinstance(captured,str) and Path(captured).is_absolute() and '..' not in Path(captured).parts
        and manifest.get('manifest_path')==captured+'/Cargo.toml','recorded isolated workspace relation')
    declarations=cargo_bundle._preflight_ir_declarations(manifest,reader,mode=='on')
    require(set(declarations)=={ENTRY},'exact complete export inventory before selection')
    paths=set();details={};companions=set()
    require(len(manifest['entries'])==1,'one exact annotated export')
    entry=manifest['entries'][0]
    program,_,digest,size=cargo_bundle._artifact_details(entry,directory,reader,paths,mode=='on',2,ir_declaration=declarations[ENTRY])
    require(program.get('ir_version')==6 and program.get('environment')=='strict','strict optional-output ABI')
    u64=dict(kind='int',bits=64,signed=False);u8=dict(kind='int',bits=8,signed=False);b32=dict(kind='bytes',len=32)
    tuple_ty=lambda *items:dict(kind='tuple',items=list(items))
    create=tuple_ty(u64,b32,u64);assertion=tuple_ty(u64,u64)
    suffix=dict(kind='pair',first=create,rest=dict(kind='optional',item=create))
    result=suffix if drop_amount else dict(kind='pair',first=assertion,rest=suffix)
    body=program['program']
    require(body['parameters']==[dict(name='terms',ty=tuple_ty(b32,b32,u8)),dict(name='opening',ty=tuple_ty(b32,b32))]
        and body['result']==result,'exact structural Terms/Opening/condition-list ABI')
    details[ENTRY]=(entry['ir'],size,digest)
    attachments=validate_receipt(manifest,directory,details,reader,paths)
    require({s.label for s in attachments}=={'package/'+n for n in SOURCE_NAMES},'closed automatic source inventory')
    require(set(sources)==SOURCE_NAMES,'closed materialized package source set')
    for source in attachments:
        require(source.contents==sources[source.label.removeprefix('package/')],'automatic source bytes differ from declared source')
        if source.label.endswith('.rs'):
            require(source.kinds==('compiler_text',) and source.compiler_read_count>=1,'actual compiler source-read declaration')
    blob=bytes.fromhex(entry['artifact']['clvm_hex'])
    tree=raw_tree.deserialize(blob)
    require(raw_tree.serialize(tree)==blob and raw_tree.tree_hash(tree).hex()==entry['artifact']['tree_hash'],'serialized blob canonical tree identity')
    stem=entry['clvm'].removesuffix('.clvm.hex')
    require(entry['ir']==stem+'.ir.json','exact entry file stem')
    artifact,raw=reader.json(cargo_bundle._contained(directory,stem+'.artifact.json'))
    require(artifact==entry['artifact'],'artifact companion differs from manifest')
    require(reader.bytes(cargo_bundle._contained(directory,stem+'.clvm.txt'))==(artifact['disassembly']+'\n').encode(),'assembly companion differs')
    companions={stem+'.artifact.json',stem+'.clvm.txt'}
    expected={'manifest.json',entry['ir'],entry['clvm'],*companions,*(s['blob'] for s in manifest['source_receipt']['files'])}
    actual={p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()}
    require(actual==expected and not any(p.is_symlink() for p in directory.rglob('*')),'closed complete bundle files')
    return manifest


def fresh(output,package,mode,*,compiler,driver_sha256,cargo_options,drop_amount=False):
    before=source_snapshot(package)
    loaded=cargo_bundle.load_declared_export(output,root=package,authoring=before,selected_entry=ENTRY,
        compiler=compiler,driver_sha256=driver_sha256,toolchain=TOOLCHAIN,current_authoring=lambda:source_snapshot(package),
        optimize=mode=='on',rue_revision=RUE,cargo_options=cargo_options)
    admit(loaded.manifest_path.parent,mode,compiler=compiler,driver_sha256=driver_sha256,
        sources={n:(package/n).read_bytes() for n in SOURCE_NAMES},cargo_options=cargo_options,drop_amount=drop_amount)
    require(before==source_snapshot(package),'package source changed during admission')
    return loaded


def recompile(directory,mode,verifier):
    """Actually validate/recompile retained IR; not a preservation proof."""
    import subprocess
    verifier=verifier.resolve(strict=True)
    digest=lambda:hashlib.sha256(verifier.read_bytes()).hexdigest()
    before=digest()
    reader=cargo_bundle._Reader()
    manifest=reader.json(cargo_bundle._contained(directory,'manifest.json'))[0]
    require(len(manifest['entries'])==1,'one admitted IR for actual recompilation')
    entry=manifest['entries'][0]
    ir=cargo_bundle._contained(directory,entry['ir'])
    completed=bounded_process.run([str(verifier),str(ir),'--artifacts'],check=True,timeout=60)
    require(len(completed.stdout)<=16*1024*1024,'bounded verifier artifact output')
    artifacts=cargo_bundle.decode_json(completed.stdout)
    require(type(artifacts) is list and len(artifacts)==2 and {r['optimize'] for r in artifacts}=={False,True},'exact two verifier compilation modes')
    selected=next(r for r in artifacts if r['optimize'] is (mode=='on'))
    require(all(selected[k]==entry['artifact'][k] for k in ('bytes','tree_hash','clvm_hex')),'actual retained IR recompile differs from emitted artifact')
    require(digest()==before,'verifier changed during actual recompilation')
    return dict(verifier_sha256=before,ir_sha256=hashlib.sha256(reader.bytes(ir)).hexdigest(),mode=mode,
        bytes=selected['bytes'],tree_hash=selected['tree_hash'],blob_sha256=hashlib.sha256(bytes.fromhex(selected['clvm_hex'])).hexdigest())


def check_recompiled(directory,mode,row,verifier_sha256):
    reader=cargo_bundle._Reader();manifest=reader.json(cargo_bundle._contained(directory,'manifest.json'))[0]
    require(len(manifest['entries'])==1,'one retained recompiled entry')
    entry=manifest['entries'][0];artifact=entry['artifact']
    expected=dict(verifier_sha256=verifier_sha256,ir_sha256=hashlib.sha256(reader.bytes(cargo_bundle._contained(directory,entry['ir']))).hexdigest(),
        mode=mode,bytes=artifact['bytes'],tree_hash=artifact['tree_hash'],blob_sha256=hashlib.sha256(bytes.fromhex(artifact['clvm_hex'])).hexdigest())
    require(row==expected,'retained actual IR recompile witness relation')
