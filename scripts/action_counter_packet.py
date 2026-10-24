"""Portable bounded counter packet; readers never follow captured host paths.

Raw Cargo manifests, complete IR/blob/source inventories and command logs remain
unchanged. Installation requires current host-native/source/bundle checks first.
Recorded readers add no execution authentication or new execution coverage.
"""
import sys
sys.dont_write_bytecode = True
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import sys
import tempfile
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "experiments/action-counter"
sys.path.insert(0, str(HERE))
import capture
import capture_support as support
import protocol_capture
import protocol_oracle
same_json = protocol_oracle.same_json
import raw_evidence
import vectors
import cargo_authoring
import cargo_bundle
from cargo_source_receipt import decode_json, validate_receipt
from registry_codegen_evidence import require
from action_counter_provenance import check as check_provenance

PROTOCOL = "chialisp-oxide-action-counter-packet/v1"
MAX_FILES = 1024
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 16 * 1024 * 1024
PROFILE = {"max_files": MAX_FILES, "max_file_bytes": MAX_FILE, "max_total_bytes": MAX_TOTAL}
SCOPE = "Recorded consistency only. Host origins are opaque relation metadata, never paths followed by this reader. Repeat aliases retain exact bytes and add no scenario coverage. No execution authentication, compiler-preservation or production-readiness claim."


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def relative(value):
    require(type(value) is str and 0 < len(value) <= 512 and re.fullmatch(r"[A-Za-z0-9_./-]+",value), "bounded ASCII packet path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and path.as_posix() == value and all(p not in (".","..") for p in path.parts), "confined packet path")
    return value


def absolute(value):
    require(type(value) is str and 0 < len(value) <= 1024 and Path(value).is_absolute()
            and str(Path(value)) == value and ".." not in Path(value).parts, "opaque bounded origin path")
    return value


def host_identity(row):
    require(type(row) is dict and set(row)=={'path','bytes','sha256'},'closed host executable identity')
    absolute(row['path'])
    require(type(row['bytes']) is int and 0<row['bytes']<=512*1024*1024
            and type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}',row['sha256']),
            'bounded opaque host executable identity')


def files(root):
    require(root.is_dir() and not root.is_symlink(), "regular packet root")
    pending=[(root,0)];result=[];scanned=0
    while pending:
        directory,depth=pending.pop()
        require(depth<=16,"packet directory depth")
        with os.scandir(directory) as entries:
            for item in entries:
                scanned+=1
                require(scanned<=MAX_FILES*4,"packet discovery bound before accumulation")
                path=Path(item.path)
                relative(path.relative_to(root).as_posix())
                info=item.stat(follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):pending.append((path,depth+1))
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink==1,"regular unaliased packet file")
                    require(len(result)<MAX_FILES,"packet file count before append")
                    result.append(path)
    return sorted(result)


def read(path, maximum=MAX_FILE):
    info=path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink==1 and info.st_size<=maximum,"bounded regular packet read")
    with path.open('rb') as stream:raw=stream.read(maximum+1)
    require(len(raw)==info.st_size and len(raw)<=maximum,"packet grew or changed")
    return raw


def preflight(paths, maximum=MAX_TOTAL):
    require(len(paths)<=MAX_FILES,"complete packet count")
    require(type(maximum) is int and 0<=maximum<=support.MAX_RETAINED_BYTES,"bounded declared transport profile")
    remaining=maximum
    planned=[]
    for path in paths:
        info=path.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_nlink==1 and info.st_size<=MAX_FILE
                and info.st_size<=remaining,"complete packet budget before read/copy")
        remaining-=info.st_size
        planned.append((path,info.st_size))
    return planned


class Packet:
    def __init__(self, root):
        self.root=Path(root).resolve()
        self.index=decode_json(read(self.root/'report.json'))
        require(set(self.index)=={'protocol','profile','origin_raw','origin_protocol','origin_root','files','aliases','scope'}
                and self.index['protocol']==PROTOCOL and same_json(self.index['profile'],PROFILE)
                and self.index['scope']==SCOPE,"closed counter packet protocol/profile/scope")
        for name in ('origin_raw','origin_protocol','origin_root'):absolute(self.index[name])
        rows=self.index['files']
        require(type(rows) is list and len(rows)<MAX_FILES,"bounded complete packet inventory")
        self.rows={};remaining=MAX_TOTAL-(self.root/'report.json').stat().st_size
        for row in rows:
            require(type(row) is dict and set(row)=={'path','bytes','sha256'},"exact packet identity fields")
            name=relative(row['path']);length=row['bytes']
            require(name!='report.json' and name not in self.rows and type(length) is int
                    and 0<=length<=MAX_FILE and length<=remaining,"complete declared packet precharge")
            require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}',row['sha256']),"packet digest spelling")
            remaining-=length;self.rows[name]=row
        actual=files(self.root)
        require({p.relative_to(self.root).as_posix() for p in actual}==set(self.rows)|{'report.json'},"complete physical packet inventory")
        preflight(actual)
        aliases=self.index['aliases']
        require(type(aliases) is dict and len(aliases)<=MAX_FILES,"bounded packet aliases")
        for name,target in aliases.items():
            relative(name);relative(target)
            require(name not in self.rows and name!='report.json' and target in self.rows
                    and '/bundle-' not in name,"one-step nonbundle alias only")
        for name,row in self.rows.items():
            raw=read(self.root/name,row['bytes'])
            require(len(raw)==row['bytes'] and digest(raw)==row['sha256'],"complete packet payload identity")

    def raw(self,name):
        name=relative(name);name=self.index['aliases'].get(name,name)
        require(name in self.rows,"declared logical packet input")
        return read(self.root/name,self.rows[name]['bytes'])

    def load(self,name):return decode_json(self.raw(name))

    def identity(self,row):
        require(type(row) is dict and set(row)=={'path','bytes','sha256'},"exact origin file identity")
        path=row['path']
        for key,prefix in (('origin_raw','raw'),('origin_protocol','protocol')):
            origin=self.index[key]
            if type(path) is str and path.startswith(origin+'/'):
                raw=self.raw(prefix+'/'+relative(path[len(origin)+1:]))
                require(type(row['bytes']) is int and row['bytes']==len(raw) and row['sha256']==digest(raw),"origin retained byte relation")
                return raw
        raise ValueError('origin file identity escapes declared capture roots')


def check_sources(window, origin):
    require(window['source_root']==origin and window['compiler_root']==origin,"same bounded source/compiler origin")
    actual=capture.source_window(ROOT)
    for key in ('sources','compiler_sources'):
        rows=window[key]
        require(type(rows) is list and len(rows)==len(actual),"complete current source input closure")
        for row,want in zip(rows,actual,strict=True):
            require(set(row)=={'path','bytes','sha256'} and type(row['bytes']) is int
                    and row['path']==origin+'/'+Path(want['path']).relative_to(ROOT).as_posix()
                    and row['bytes']==want['bytes'] and row['sha256']==want['sha256'],"fresh source bytes and origin relation")


def admit_companions(packet,base,entry,mode):
    stem=digest(entry['entry'].encode())
    require(entry['ir']==stem+'.ir.json' and entry['clvm']==stem+'.clvm.hex',
            'deterministic complete Cargo export filenames')
    artifact_name=stem+'.artifact.json';text_name=stem+'.clvm.txt'
    artifact=packet.load(base+'/'+artifact_name)
    cargo_bundle._artifact_metadata(artifact,mode=='on',2)
    require(same_json(artifact,entry['artifact']),'exact typed Cargo companion artifact')
    require(packet.raw(base+'/'+text_name)==(entry['artifact']['disassembly']+'\n').encode(),
            'exact Cargo companion disassembly bytes and newline')
    return {artifact_name,text_name}


def admit_bundle(packet,control,mode,artifacts,extraction):
    prefix=f'raw/bundle-{control}-{mode}'
    pointer=packet.load(prefix+'/latest.json')
    require(set(pointer)=={'bundle','manifest'} and len(PurePosixPath(relative(pointer['bundle'])).parts)==1
            and pointer['manifest']==pointer['bundle']+'/manifest.json',"closed complete bundle pointer")
    base=prefix+'/'+pointer['bundle'];directory=packet.root/base
    manifest=packet.load(base+'/manifest.json')
    root=packet.index['origin_root'] if control=='original' else packet.index['origin_raw']+'/projects/'+control
    authoring=cargo_authoring.snapshot(ROOT) if control=='original' else capture.snapshot(packet.root/'raw/projects'/control)
    expected={r['entry'] for r in authoring['exports'].values()}
    require(manifest['schema_version']==2 and manifest['status']=='wip_prototype_not_ready_for_use'
            and manifest['workspace_root']==root and manifest['manifest_path']==root+('/puzzles/Cargo.toml' if control=='original' else '/Cargo.toml')
            and manifest['cargo_options']==['--manifest-path',manifest['manifest_path'],'--locked']
            and manifest['package']==authoring['package'] and manifest['compiler']==artifacts['compiler']
            and manifest['driver_sha256']==extraction['tools'][0]['sha256'] and manifest['toolchain']==capture.TOOLCHAIN,"exact schema2 origin/compiler/options")
    hosts=[line[6:] for line in artifacts['compiler'].splitlines() if line.startswith('host: ')]
    require(len(hosts)==1 and manifest['analysis_target']==hosts[0],"independent compiler host")
    reader=cargo_bundle._Reader();declarations=cargo_bundle._preflight_ir_declarations(manifest,reader,mode=='on')
    require(set(declarations)==expected,"complete original/all-mutant export inventory before selection")
    admitted={};details={};paths=set();companions=set()
    for entry in manifest['entries']:
        program,path,sha,size=cargo_bundle._artifact_details(entry,directory,reader,paths,mode=='on',2,ir_declaration=declarations[entry['entry']])
        admitted[entry['entry']]=(entry,program,path,sha,size)
        details[entry['entry']]=(entry['ir'],size,sha)
        companions.update(admit_companions(packet,base,entry,mode))
    attachments=validate_receipt(manifest,directory,details,reader,paths)
    for source in attachments:
        if control=='original':
            if source.label.startswith('package/'):payload=read(ROOT/'puzzles'/source.label.removeprefix('package/'))
            else:
                require(source.label.startswith('workspace/'),"closed original source label")
                payload=read(ROOT/source.label.removeprefix('workspace/'))
        else:
            require(source.label.startswith('package/'),"closed mutant source label")
            payload=packet.raw('raw/projects/'+control+'/'+source.label.removeprefix('package/'))
        require(source.contents==payload,"complete attached actual source byte relation")
        if source.label.endswith('.rs'):require(source.kinds==('compiler_text',) and type(source.compiler_read_count) is int and source.compiler_read_count>0,"actual compiler source-read declaration")
    if control!='original':
        require({s.label for s in attachments}=={'package/Cargo.toml','package/Cargo.lock','package/src/lib.rs',*('package/'+n for n in capture.MUTANT_SOURCES)},"complete mutant library source set")
    required={'manifest.json',*companions,*(e['ir'] for e in manifest['entries']),*(e['clvm'] for e in manifest['entries']),*(s['blob'] for s in manifest['source_receipt']['files'])}
    require({p.relative_to(directory).as_posix() for p in files(directory)}==required,"closed full bundle payload inventory")
    require({n.removeprefix(prefix+'/') for n in set(packet.rows)|set(packet.index['aliases']) if n.startswith(prefix+'/')}
            =={'latest.json',*(pointer['bundle']+'/'+n for n in required)},"closed complete published bundle root")
    for export in capture.EXPORTS:
        name=('chialisp_oxide_puzzles' if control=='original' else capture.CRATE)+'::counter_'+export+'::puzzle'
        entry,program,path,sha,size=admitted[name]
        require(program.get('ir_version')==5 and program.get('environment')=='strict',"selected strict v5 counter ABI")
        selected=next(a for a in artifacts['artifacts'] if (a['control'],a['mode'],a['export'])==(control,mode,export))
        require(selected['clvm_hex']==entry['artifact']['clvm_hex'] and same_json(selected['source_receipt'],manifest['source_receipt']),"selected artifact/source receipt binding")
        origin_directory=packet.index['origin_raw']+'/'+base.removeprefix('raw/')
        require(selected['manifest']['path']==origin_directory+'/manifest.json'
                and selected['ir']['path']==origin_directory+'/'+entry['ir'],"exact selected origin manifest/IR path relation")
        require(packet.identity(selected['manifest'])==packet.raw(base+'/manifest.json')
                and packet.identity(selected['ir'])==path.read_bytes() and selected['ir']['sha256']==sha
                and selected['ir']['bytes']==size,"selected complete retained IR/manifest relation")
    return manifest


def check_native(packet,native):
    require(native['status']=='passed' and type(native['test_groups']) is int and native['test_groups']==3
            and type(native['actual_export_calls']) is int and native['actual_export_calls']==177
            and type(native['host_abi_rows']) is int and native['host_abi_rows']==120,"fixed native observation counts")
    require([g['control'] for g in native['groups']]==list(capture.CONTROLS),"complete native control groups")
    for group in native['groups']:
        control=group['control'];label='native-'+control.replace('_','-')
        stdout=packet.raw('raw/'+label+'.stdout')
        require(packet.identity(group['stdout'])==stdout,"actual native witness stream identity")
        text=stdout.decode()
        require(re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)==[('1','0','0')],"one actual nonignored native test per control")
        actual=[]
        for line in text.splitlines():
            for prefix in ('ACTION_COUNTER_NATIVE_ADAPTER ','ACTION_COUNTER_NATIVE_EXPORT '):
                if prefix in line:actual.append([prefix.split()[0],decode_json(line.split(prefix,1)[1].encode())])
        expected=[]
        for row in capture.spec(control)['cases']:
            if row['expected']=={'status':'rejected','stage':'abi'}:
                expected.append(['ACTION_COUNTER_NATIVE_ADAPTER',{'export':row['export'],'name':row['name'],'status':'abi_rejected','scope':'Host adapter only; export not called'}])
            else:
                outcome=row['expected'] if row['expected']['status']=='accepted' else {'status':'policy_panic'}
                expected.append(['ACTION_COUNTER_NATIVE_EXPORT',{'export':row['export'],'name':row['name'],'actual':outcome}])
        require(same_json(actual,expected) and same_json(expected,group['observations']),"actual native exact witness sequence")


def check_command(packet,name,argv,target,args,prefix='raw',driver=None):
    command=packet.load(prefix+'/'+name+'.command.json')
    expected={'argv':[str(x) for x in argv],'cwd':packet.index['origin_root'],'target':str(target),
        'jobs':2,'offline':True,'rustc':str(args.rustc),'driver':str(driver) if driver else None,
        'exit_code':0,'status':'passed','failure_kind':None,'failure_detail':None,
        'timeout_seconds':support.COMMAND_TIMEOUT,'cleanup_timeout_seconds':support.COMMAND_CLEANUP_TIMEOUT,
        'max_stream_bytes':support.MAX_FILE}
    require(set(command)==set(expected)|{'stdout','stderr'},"closed bounded command schema")
    for key,want in expected.items():require(type(command[key]) is type(want) and command[key]==want,'exact captured command role: '+name+'/'+key)
    for stream in ('stdout','stderr'):require(packet.identity(command[stream])==packet.raw(prefix+'/'+name+'.'+stream),"exact retained command stream")


def validate(root):
    packet=Packet(root)
    check_provenance()
    origin=packet.index['origin_root'];raw_origin=packet.index['origin_raw']
    metadata=packet.load('protocol/report.json')
    require(set(metadata)=={'protocol','status',*protocol_capture.PATH_FIELDS,'first','repeat','source_window_unchanged','repeat_byte_identical','scope'}
            and metadata['scope']==protocol_capture.SCOPE,'closed protocol receipt fields/scope')
    require(metadata['protocol']=='chialisp-oxide-action-counter-protocol-capture/v1'
            and metadata['status']=='five-mode-counter-repeat-passed' and metadata['raw_capture']==raw_origin
            and metadata['compiler_root']==origin,"complete protocol/raw origin relation")
    args=SimpleNamespace(output=Path(raw_origin),**{name:Path(absolute(metadata[name])) for name in
        ('native_target','compiler_target','compiler_root','rustc','driver','cargo_cli')})
    roles=capture.command_roles(args,all_phases=True,source_root=Path(origin))
    raw_top={name+'.'+extension for name in roles for extension in ('stdout','stderr','command.json')}
    raw_top|={'prepared.json','native.json','artifacts.json','request.json','execution.json',
              *(prefix+'-window-'+end+'.json' for prefix in ('native','extract','replay') for end in ('before','after'))}
    protocol_top={'inputs-before.json','inputs-after.json','report.json',
        *(name+'.'+extension for name in ('raw-schema2-read','raw-schema2-read-after','protocol-attempt1','protocol-attempt2') for extension in ('stdout','stderr','command.json')),
        'protocol-attempt1.reader.json','protocol-attempt2.reader.json'}
    logical=set(packet.rows)|set(packet.index['aliases'])
    require({n.removeprefix('raw/') for n in logical if n.startswith('raw/') and '/' not in n[4:]}==raw_top
            and {n.removeprefix('protocol/') for n in logical if n.startswith('protocol/')}==protocol_top
            and all(n.startswith(('raw/','protocol/')) for n in logical),"closed complete raw/SDK root file inventories")
    directories={'projects',*(f'bundle-{c}-{m}' for c in capture.CONTROLS for m in ('off','on'))}
    require(all('/' not in n[4:] or n[4:].split('/')[0] in directories for n in logical if n.startswith('raw/')),"closed raw capture directories")
    require(all(n.split('/')[2] in capture.CONTROLS for n in logical if n.startswith('raw/projects/')),"closed materialized control directories")
    prepared=packet.load('raw/prepared.json')
    require(set(prepared)=={'protocol','root_lock','materialized','sources','status'} and prepared['protocol']=='chialisp-oxide-action-counter-prepared/v1'
            and prepared['status']=='prepared_only',"fixed actual preparation protocol")
    require(set(prepared['root_lock'])=={'path','bytes','sha256'} and type(prepared['root_lock']['bytes']) is int
            and prepared['root_lock']['path']==origin+'/Cargo.lock'
            and prepared['root_lock']['bytes']==len(read(ROOT/'Cargo.lock'))
            and prepared['root_lock']['sha256']==digest(read(ROOT/'Cargo.lock')),"reviewed root lock freshness")
    for control in capture.CONTROLS:
        expected=capture.project_inputs(control,source_root=Path(origin))
        for name,payload in expected.items():require(packet.raw('raw/projects/'+control+'/'+name)==payload,"reconstructed actual-source/native/reference/manifest recipe")
        capture.lock_admission(packet.raw('raw/projects/'+control+'/Cargo.lock'))
        prefix='raw/projects/'+control+'/'
        require({n.removeprefix(prefix) for n in logical if n.startswith(prefix)}==set(expected)|{'Cargo.lock'},"closed materialized source inventory")
    native=packet.load('raw/native.json')
    extraction=packet.load('raw/extract-window-before.json')
    check_sources(extraction,origin)
    require(packet.raw('raw/extract-window-before.json')==packet.raw('raw/extract-window-after.json')
            and packet.raw('raw/native-window-before.json')==packet.raw('raw/native-window-after.json')
            and packet.raw('raw/replay-window-before.json')==packet.raw('raw/replay-window-after.json'),"retained quiet source/tool windows")
    native_window=packet.load('raw/native-window-before.json')
    check_sources(native_window,origin)
    require(same_json(native['source_window'],native_window) and same_json(native_window['sources'],extraction['sources']) and same_json(prepared['sources'],extraction['sources'])
            and same_json(native_window['materialized'],extraction['materialized']) and same_json(extraction['materialized'],prepared['materialized'])
            and native_window['tools']==[] and same_json(native_window['rustc'],extraction['rustc'])
            and extraction['rustc']['path']==str(args.rustc),"native/LAST-host/extraction relation")
    require(packet.raw('raw/pinned-rustc-path.stdout').decode().strip()==str(args.rustc),"independent rustup-selected compiler path")
    for row in prepared['materialized']:packet.identity(row)
    expected_materialized=[raw_origin+'/projects/'+c+'/'+name for c in capture.CONTROLS for name in (*capture.FILES,*(capture.MUTANT_SOURCES if c!='original' else ()))]
    require([r['path'] for r in prepared['materialized']]==expected_materialized,"complete exact materialized source inventory")
    replay=packet.load('raw/replay-window-before.json')
    require(same_json(replay['source_tools'],extraction),"raw replay source/tool relation")
    exclusions={f'raw/raw-attempt{i}.{extension}' for i in (1,2) for extension in ('stdout','stderr','command.json')}
    exclusions|={'raw/replay-window-before.json','raw/replay-window-after.json','raw/execution.json'}
    retained={raw_origin+'/'+n.removeprefix('raw/') for n in logical if n.startswith('raw/') and n not in exclusions}
    require([r['path'] for r in replay['retained']]==sorted(retained),"independently complete retained replay input inventory")
    for row in replay['retained']:packet.identity(row)
    artifacts=packet.load('raw/artifacts.json')
    raw_evidence.validate_queries(artifacts,packet.raw('raw/compiler-version.stdout'),packet.raw('raw/driver-version.stdout'))
    require(len(extraction['tools'])==4 and extraction['tools'][0]['path']==str(args.driver)
            and extraction['tools'][1]['path']==str(args.cargo_cli)
            and same_json(extraction['tools'][2],native['protocol_runner'])
            and same_json(extraction['tools'][3],native['runner'])
            and native['runner']['path']==str(capture.runner_path(args))
            and native['protocol_runner']['path']==str(capture.protocol_path(args)),"exact actual driver/Cargo/native-LAST-runner identities")
    for row in [extraction['rustc'],*extraction['tools']]:
        host_identity(row)
    for control in capture.CONTROLS:
        for mode in ('off','on'):admit_bundle(packet,control,mode,artifacts,extraction)
    # Current source identity is checked independently of the opaque host path.
    current=json.loads(json.dumps(artifacts))
    for row in current['artifacts'][14:]:
        local=ROOT/'puzzles'/('counter-'+row['export'])/'puzzle.rue'
        require(row['source']['path']==origin+'/'+local.relative_to(ROOT).as_posix(),"actual paired Rue source origin")
        row['source']['path']=str(local)
    raw_evidence.validate_inventory(current)
    reference=packet.load('raw/compile-reference.stdout')
    require(reference['compiler']=='chialisp 0.4.6' and reference['artifacts']==[{k:r[k] for k in ('export','clvm_hex')} for r in artifacts['artifacts'][12:14]],"independent compiled Chialisp artifact relation")
    rue=packet.load('raw/compile-rue.stdout')
    require(rue['rue_revision']=='245c107a8e795229ea08097142f005a4c0d91075' and same_json(rue['artifacts'],[{k:r[k] for k in ('mode','export','optimize_lir','clvm_hex')} for r in artifacts['artifacts'][14:]]),"actual Rue source compiler/artifact relation")
    raw_evidence.validate_request(packet.load('raw/request.json'),artifacts)
    first=packet.raw('raw/raw-attempt1.stdout')
    require(first==packet.raw('raw/raw-attempt2.stdout'),"raw independent repeat exact bytes")
    raw_summary=raw_evidence.validate_report(decode_json(first),current)
    execution=packet.load('raw/execution.json')
    require(execution['status']=='raw_vm_executed_repeat_identical' and same_json(execution['summary'],raw_summary)
            and packet.identity(execution['first'])==first and packet.identity(execution['repeat'])==first,"exact final raw capture receipt")
    check_native(packet,native)
    for name,(argv,target,driver) in roles.items():
        check_command(packet,name,argv,target,args,driver=driver)
    protocol_window=packet.load('protocol/inputs-before.json')
    require(packet.raw('protocol/inputs-before.json')==packet.raw('protocol/inputs-after.json')
            and same_json(protocol_window['raw_source_tools'],extraction)
            and packet.identity(protocol_window['artifact_packet'])==packet.raw('raw/artifacts.json'),"complete SDK/raw declared quiet window")
    host_identity(protocol_window['protocol_binary'])
    host_identity(protocol_window['capture_python'])
    require(protocol_window['protocol_binary']['path']==str(capture.native_target(args,'original')/'debug/examples/protocol'),"actual protocol binary output role")
    require(same_json(protocol_window['protocol_binary'],native['protocol_runner']),'SDK binary bound at native LAST build before extraction')
    program=Path(protocol_window['protocol_binary']['path'])
    python=absolute(protocol_window['capture_python']['path'])
    for name in ('raw-schema2-read','raw-schema2-read-after'):
        check_command(packet,name,[python,Path(origin)/'experiments/action-counter/capture.py','read',*protocol_capture.source_args(args)],capture.native_target(args,'original'),args,'protocol')
    sdk_first=packet.raw('protocol/protocol-attempt1.stdout')
    require(sdk_first==packet.raw('protocol/protocol-attempt2.stdout'),"SDK independent repeat exact bytes")
    for index in (1,2):
        label=f'protocol-attempt{index}'
        check_command(packet,label,[program,Path(raw_origin)/'artifacts.json'],capture.native_target(args,'original'),args,'protocol')
    # The reader consumes retained wire bytes and local pinned sources only.
    summary=protocol_oracle.check_source_controls(decode_json(sdk_first),packet.root/'raw/artifacts.json')
    require(same_json(summary,packet.load('protocol/protocol-attempt1.reader.json')) and same_json(summary,packet.load('protocol/protocol-attempt2.reader.json'))
            and packet.identity(metadata['first'])==sdk_first and packet.identity(metadata['repeat'])==sdk_first
            and metadata['source_window_unchanged'] is True and metadata['repeat_byte_identical'] is True,"recorded SDK reader and capture receipt")
    return {'protocol':PROTOCOL,'status':'recorded_consistency_checked','files':len(packet.rows)+1,
        'bytes':sum(r['bytes'] for r in packet.rows.values())+len(read(packet.root/'report.json')),
        'raw':raw_summary,'sdk':summary,'scope':'Recorded consistency only; no new execution, live-network, BLS, compiler-preservation or whole issue14 credit.'}


def encode_record(record):
    output=bytearray()
    for chunk in json.JSONEncoder(indent=2,ensure_ascii=True).iterencode(record):
        raw=chunk.encode('ascii')
        require(len(raw)+len(output)+1<=MAX_FILE,"packet index before allocation")
        output.extend(raw)
    output.extend(b'\n')
    return bytes(output)


def install(raw_capture,protocol_directory,destination):
    raw_capture=Path(raw_capture).resolve();protocol_directory=Path(protocol_directory).resolve()
    destination=Path(destination).absolute()
    metadata=decode_json(read(protocol_directory/'report.json'))
    require(metadata['raw_capture']==str(raw_capture),"actual matching raw capture")
    args=SimpleNamespace(output=raw_capture,**{name:Path(metadata[name]) for name in
        ('native_target','compiler_target','compiler_root','rustc','driver','cargo_cli')})
    # No captured-reader bypass: fresh complete root37 and mutant bundle/source
    # admission and exact native/VM/SDK readback must succeed on the origin host.
    protocol_capture.recorded(args,protocol_directory)
    source_paths=[(p,'raw/'+p.relative_to(raw_capture).as_posix()) for p in files(raw_capture)]
    source_paths += [(p,'protocol/'+p.relative_to(protocol_directory).as_posix()) for p in files(protocol_directory)]
    require(len(source_paths)<MAX_FILES,"complete raw/SDK source inventory")
    preflight([p for p,_ in source_paths],support.MAX_RETAINED_BYTES)
    physical=[];aliases={};seen={}
    for path,name in sorted(source_paths,key=lambda p:p[1]):
        raw=read(path);sha=digest(raw)
        # Keep complete Cargo bundle and materialized source directories physical
        # so shared strict bundle readers need no virtual filesystem or rewrite.
        eligible='/bundle-' not in name and not name.startswith('raw/projects/') and name!='raw/artifacts.json'
        key=(len(raw),sha)
        if eligible and key in seen:
            aliases[name]=seen[key]
        else:
            if eligible:seen[key]=name
            physical.append((path,{'path':name,'bytes':len(raw),'sha256':sha}))
    rows=[r for _,r in physical]
    index={'protocol':PROTOCOL,'profile':PROFILE,'origin_raw':str(raw_capture),
        'origin_protocol':str(protocol_directory),'origin_root':str(ROOT),'files':rows,'aliases':aliases,
        'scope':SCOPE}
    encoded=encode_record(index)
    require(len(rows)+1<=MAX_FILES and sum(r['bytes'] for r in rows)+len(encoded)<=MAX_TOTAL,"complete public packet precharge before copy")
    require(not destination.exists() and not destination.is_symlink(),"refuse existing packet destination")
    destination.parent.mkdir(parents=True,exist_ok=True)
    stage=Path(tempfile.mkdtemp(prefix='.counter-packet-',dir=destination.parent))
    try:
        for source,row in physical:
            raw=read(source,row['bytes'])
            require(len(raw)==row['bytes'] and digest(raw)==row['sha256'],"origin remained exact after complete inventory precharge")
            target=stage/relative(row['path']);target.parent.mkdir(parents=True,exist_ok=True)
            target.write_bytes(raw)
        (stage/'report.json').write_bytes(encoded)
        result=validate(stage)
        require(not destination.exists(),"packet destination appeared")
        os.rename(stage,destination)
        return result
    finally:
        if stage.exists():shutil.rmtree(stage)
