"""Constructed and coherent packet corruption controls, never new execution."""
import sys
sys.dont_write_bytecode = True
import json
from pathlib import Path
import shutil
import tempfile

import action_counter_packet as packet


def structural_controls():
    checked=0
    for label,rows in (
        ('oversized-file',[{'path':'raw/declared','bytes':packet.MAX_FILE+1,'sha256':'00'*32}]),
        ('aggregate',[{'path':f'raw/declared-{i}','bytes':packet.MAX_FILE,'sha256':'00'*32} for i in range(9)]),
        ('file-count',[{'path':f'raw/declared-{i}','bytes':0,'sha256':'00'*32} for i in range(packet.MAX_FILES)]),
        ('boolean-bytes',[{'path':'raw/declared','bytes':True,'sha256':'00'*32}]),
        ('path-escape',[{'path':'../declared','bytes':0,'sha256':'00'*32}]),
        ('float-profile-files',[]),
        ('float-profile-total',[]),
    ):
        with tempfile.TemporaryDirectory(prefix='counter-profile-control-') as temp:
            root=Path(temp)
            index={'protocol':packet.PROTOCOL,'profile':dict(packet.PROFILE),'origin_raw':'/opaque/raw',
                'origin_protocol':'/opaque/protocol','origin_root':'/opaque/root','files':rows,'aliases':{},'scope':packet.SCOPE}
            if label=='float-profile-files':index['profile']['max_files']=float(packet.MAX_FILES)
            if label=='float-profile-total':index['profile']['max_total_bytes']=float(packet.MAX_TOTAL)
            (root/'report.json').write_bytes(packet.encode_record(index))
            reads=[];original=packet.read
            def observed(path,maximum=packet.MAX_FILE):
                reads.append(Path(path).name)
                return original(path,maximum)
            packet.read=observed
            try:
                try:packet.Packet(root)
                except ValueError:pass
                else:raise ValueError('constructed profile corruption accepted: '+label)
                packet.require(reads==['report.json'],'complete declared budget/path failure before any payload read')
                checked+=1
            finally:packet.read=original
    return checked


def refresh(root):
    index=packet.decode_json(packet.read(root/'report.json'))
    files=[p for p in packet.files(root) if p!=root/'report.json']
    packet.preflight(files)
    index['files']=[{'path':p.relative_to(root).as_posix(),'bytes':p.stat().st_size,'sha256':packet.digest(packet.read(p))} for p in files]
    (root/'report.json').write_bytes(packet.encode_record(index))


def change(name,function):
    def mutate(root):
        current=packet.Packet(root)
        target=current.index['aliases'].get(name,name)
        value=current.load(name);function(value)
        (root/target).write_bytes(packet.encode_record(value))
    return mutate


def self_test(root):
    root=Path(root).resolve();packet.validate(root)
    controls=[
        ('prepared-source-omission',change('raw/prepared.json',lambda x:x['sources'].pop())),
        ('prepared-source-edit',change('raw/prepared.json',lambda x:x['sources'][0].__setitem__('sha256','00'*32))),
        ('source-closure-omission',change('raw/extract-window-before.json',lambda x:x['sources'].pop())),
        ('coherent-window-float',change('protocol/inputs-before.json',lambda x:x['raw_source_tools']['sources'][0].__setitem__('bytes',float(x['raw_source_tools']['sources'][0]['bytes'])))),
        ('native-last-protocol-mismatch',change('raw/native.json',lambda x:x['protocol_runner'].__setitem__('sha256','00'*32))),
        ('compiler-query-substitution',change('raw/artifacts.json',lambda x:x.__setitem__('compiler','rustc forged\n'))),
        ('native-zero-tests',change('raw/native.json',lambda x:x.__setitem__('actual_export_calls',0))),
        ('missing-native-observation',change('raw/native.json',lambda x:x['groups'][0]['observations'].pop())),
        ('missing-artifact',change('raw/artifacts.json',lambda x:x['artifacts'].pop())),
        ('wrong-mode',change('raw/artifacts.json',lambda x:x['artifacts'][0].__setitem__('mode','on'))),
        ('raw-output',change('raw/raw-attempt1.stdout',lambda x:x['observations'][0]['execution']['outcome'].__setitem__('output_hex','80'))),
        ('raw-resource-rejection',change('raw/raw-attempt1.stdout',lambda x:next(r for r in x['observations'] if r['execution']['outcome']['status']=='rejected')['execution'].__setitem__('failure',{'kind':'resource','code':'cost_exceeded'}))),
        ('missing-sdk-mode',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'].pop())),
        ('float-primary-flags',change('protocol/protocol-attempt1.stdout',lambda x:x['primary_vm'].__setitem__('flags',535.0))),
        ('float-primary-max-cost',change('protocol/protocol-attempt1.stdout',lambda x:x['primary_vm'].__setitem__('max_cost',250000000.0))),
        ('float-sdk-tx-cost',change('protocol/protocol-attempt1.stdout',lambda x:x['sdk'].__setitem__('max_transaction_cost',5500000000.0))),
        ('sdk-child-identity',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'][0]['increment_child'].__setitem__('coin_id','00'*32))),
        ('sdk-reordered-actions',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'][0]['lifecycle_cases'][1].__setitem__('actions',[1,0]))),
        ('missing-double-reset-signature-control',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'][0]['lifecycle_cases'][3]['attempts'].pop(1))),
        ('sdk-consensus-code',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'][0]['lifecycle_cases'][0]['attempts'][0]['outcome'].__setitem__('code','DoubleSpend'))),
        ('sdk-state-equality',change('protocol/protocol-attempt1.stdout',lambda x:x['runs'][0]['lifecycle_cases'][0]['attempts'][0]['outcome'].__setitem__('complete_state_unchanged',False))),
        ('missing-source-control',change('protocol/protocol-attempt1.stdout',lambda x:x['source_control_runs'].pop())),
        ('source-mutant-commitment',change('protocol/protocol-attempt1.stdout',lambda x:x['source_control_runs'][0].__setitem__('merkle_root','00'*32))),
        ('changed-command-role',change('raw/extract-original-off.command.json',lambda x:x.__setitem__('argv',['/bin/true']))),
        ('wrong-cargo-cap',change('raw/extract-original-off.command.json',lambda x:x.__setitem__('timeout_seconds',601))),
        ('float-command-jobs',change('raw/extract-original-off.command.json',lambda x:x.__setitem__('jobs',2.0))),
        ('wrong-receipt-raw-capture',change('protocol/report.json',lambda x:x.__setitem__('raw_capture','/different/raw-capture'))),
        ('wrong-receipt-driver',change('protocol/report.json',lambda x:x.__setitem__('driver','/different/driver'))),
        ('omitted-retained-input',change('raw/replay-window-before.json',lambda x:x['retained'].pop())),
    ]
    for field in ('protocol_binary','capture_python'):
        controls.extend([
            (field+'-missing-sha',change('protocol/inputs-before.json',lambda x,f=field:x[f].pop('sha256'))),
            (field+'-garbled-sha',change('protocol/inputs-before.json',lambda x,f=field:x[f].__setitem__('sha256','garbled'))),
            (field+'-boolean-length',change('protocol/inputs-before.json',lambda x,f=field:x[f].__setitem__('bytes',True))),
            (field+'-zero-length',change('protocol/inputs-before.json',lambda x,f=field:x[f].__setitem__('bytes',0))),
        ])
    def corrupt_unused_ir(candidate):
        current=packet.Packet(candidate)
        pointer=current.load('raw/bundle-original-off/latest.json')
        base='raw/bundle-original-off/'+pointer['bundle']
        manifest=current.load(base+'/manifest.json')
        entry=next(e for e in manifest['entries'] if '::counter_' not in e['entry'])
        (candidate/base/entry['ir']).write_bytes(b'{}\n')
    controls.append(('unselected-original-IR',corrupt_unused_ir))
    def companion_control(kind):
        def mutate(candidate):
            current=packet.Packet(candidate)
            pointer=current.load('raw/bundle-original-off/latest.json')
            base='raw/bundle-original-off/'+pointer['bundle']
            manifest=current.load(base+'/manifest.json')
            entry=next(e for e in manifest['entries'] if '::counter_' not in e['entry'])
            stem=packet.digest(entry['entry'].encode())
            artifact=candidate/base/(stem+'.artifact.json')
            text=candidate/base/(stem+'.clvm.txt')
            if kind=='missing-artifact':artifact.unlink()
            elif kind=='missing-text':text.unlink()
            elif kind=='wrong-newline':text.write_bytes(text.read_bytes().removesuffix(b'\n'))
            elif kind=='wrong-text':text.write_bytes(b'wrong\n')
            elif kind=='unknown-file':(candidate/base/'unknown.txt').write_bytes(b'unknown\n')
            elif kind=='wrong-stem':
                entry['ir']='renamed.ir.json'
                (candidate/base/'manifest.json').write_bytes(packet.encode_record(manifest))
            else:
                value=current.load(base+'/'+stem+'.artifact.json')
                if kind=='float-bytes':value['bytes']=float(value['bytes'])
                elif kind=='unknown-field':value['unknown']=True
                elif kind=='wrong-artifact':value['disassembly']='wrong'
                artifact.write_bytes(packet.encode_record(value))
        return mutate
    for kind in ('missing-artifact','missing-text','wrong-newline','wrong-text',
                 'unknown-file','wrong-stem','float-bytes','unknown-field','wrong-artifact'):
        controls.append(('unselected-companion-'+kind,companion_control(kind)))
    for label,mutate in controls:
        with tempfile.TemporaryDirectory(prefix='counter-packet-control-') as temp:
            candidate=Path(temp)/'packet';shutil.copytree(root,candidate)
            mutate(candidate);refresh(candidate)
            try:packet.validate(candidate)
            except (ValueError,KeyError,IndexError,StopIteration):continue
            raise ValueError('coherent counter packet corruption accepted: '+label)
    return structural_controls()+len(controls)
