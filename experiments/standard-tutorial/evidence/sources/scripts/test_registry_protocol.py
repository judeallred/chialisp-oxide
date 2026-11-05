"""Pure transport/corruption tests; optional explicit historical semantic inputs."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import registry_protocol as r


def encoded(value):
    return (json.dumps(value, indent=2) + '\n').encode()


def canonical():
    result = []
    slot = r.ROOT / 'corpus/upstream/slot-machine/6edc94856306d0f77e065c6c99aacf1055e51e2f/rue-puzzles/singleton'
    chia = r.ROOT / 'corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles'
    for name,size,module_hash in r.CANONICAL:
        if name in ('action_layer','default_finalizer','slot'):
            filename = {'action_layer':'action','default_finalizer':'finalizer','slot':'slot'}[name]
            raw = bytes.fromhex(''.join((slot / f'{filename}.rue.hex').read_text().split()))
        elif name == 'standard_wallet':
            raw = bytes.fromhex(json.loads((r.ROOT/'crates/chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json').read_bytes())['module_hex'])
        else:
            filename = 'singleton_launcher' if name == 'singleton_launcher' else 'singleton_top_layer_v1_1'
            raw = bytes.fromhex(''.join((chia/f'{filename}.clsp.hex').read_text().split()))
        result.append({'name':name,'module_hex':raw.hex(),'bytes':size,'blob_sha256':r.digest(raw),'module_tree_hash':module_hash})
    return result


def make_report(base=None):
    rows = []
    for slug, action in zip(r.SLUGS, ('initialize','set_alpha','set_beta')):
        artifacts = []
        for backend,optimize,mode in r.MODES:
            text = '80' if base is None else next(a['clvm_hex'] for a in base if a['action']==action and a['mode']==mode)
            data = bytes.fromhex(text)
            artifacts.append({'backend':backend,'optimize_lir':optimize,'bytes':len(data),'clvm_hex':text,
                              'tree_hash':r.oracle.treehash(r.codec.decode(data)).hex()})
        rows.append({'slug':slug,'kind':'protocol_component','validated':True,'simulator':[],
                     'simulator_comparisons':[],'reference':(r.ROOT/'puzzles'/slug/'reference.clsp').read_text(),
                     'artifacts':artifacts})
    return {'simulator':{'rejection_protocol':'chia-consensus-0.36.1/v1'},'puzzles':rows}


def make_index(report, journals):
    builds=[]
    for backend,optimize,mode in r.MODES:
        artifacts=[r.identity(next(a for a in p['artifacts'] if a['backend']==backend and a['optimize_lir'] is optimize),p['slug']) for p in report['puzzles']]
        descriptions={}
        for lane in ('ordinary','adversarial'):
            name=f'registry-slot-protocol/{mode}-{lane}.json';data=journals[name]
            descriptions[lane]={'path':name,'bytes':len(data),'sha256':r.digest(data)}
        builds.append({'backend':backend,'optimize_lir':optimize,'mode':mode,'artifacts':artifacts,
                       'counts':copy.deepcopy(r.COUNTS),'journals':descriptions})
    references=[{'artifact':r.identity(p['artifacts'][2],p['slug']),'source_sha256':r.digest(p['reference'].encode())} for p in report['puzzles']]
    header={'schema_version':1,'protocol':'chialisp-oxide-action-registry/v1','status':'validated',
            'primary_vm':{'version':'clvmr 0.21.0','failure_protocol':'clvmr-0.21.0/v1','flags':535,'max_cost':250000000,'allocator_limit_bytes':33554432},
            'sdk':{'simulator':'chia-sdk-test 0.36.0','consensus':'chia-consensus 0.36.1','vm':'clvmr 0.16.2','network':'TESTNET11','transaction_max_cost':5500000000,'max_block_cost_clvm':11000000000},
            'owner_public_key':r.OWNER,'canonical_components':canonical(),'independently_compiled_chialisp':references,
            'totals':{lane:{key:value*5 for key,value in counts.items()} for lane,counts in r.COUNTS.items()},
            'source_control_scope':'No source controls are included in this base producer. Research Chialisp controls and actual Rust source-mutant observations belong to separate captures and receive no credit from this record.',
            'scope':r.HEADER_SCOPE}
    return {'schema_version':1,'transport':r.TRANSPORT,'header':header,'builds':builds}


def materialize(root, report, index, journals):
    directory=root/'registry-slot-protocol';directory.mkdir(exist_ok=True)
    for name,data in journals.items():(root/name).write_bytes(data)
    data=encoded(index);(root/r.INDEX).write_bytes(data)
    for p in report['puzzles']:p['protocol_validation']={'protocol':r.TRANSPORT,'role':p['slug'],
        'index':{'path':r.INDEX,'bytes':len(data),'sha256':r.digest(data)}}


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        self.report=make_report();self.journals={f'registry-slot-protocol/{m}-{lane}.json':b'{}' for _,_,m in r.MODES for lane in ('ordinary','adversarial')}
        self.index=make_index(self.report,self.journals);self.write()

    def write(self):materialize(self.root,self.report,self.index,self.journals)

    def validate(self):
        # Constructed transport profile only: semantic acceptance is deliberately
        # mocked here and never exposed through the public reader API.
        with patch.object(r,'validate_journals'),patch.object(r,'validate_journal_shape'):
            return r.validate_report(self.report,sidecar_root=self.root)

    def test_complete_positive_transport(self):self.assertEqual(self.validate()['files'],11)

    def test_precharge_before_any_journal_read(self):
        controls=[lambda i:i['builds'][4]['journals']['adversarial'].update(bytes=r.MEMBER_LIMIT+1),
                  lambda i:i['builds'][0]['journals']['ordinary'].update(bytes=True),
                  lambda i:i['builds'][0]['journals']['ordinary'].update(path='../outside')]
        for change in controls:
            with self.subTest(change=change):
                saved=copy.deepcopy(self.index);change(self.index);self.write()
                with patch.object(r,'read_member',wraps=r.read_member) as read:
                    with self.assertRaises(ValueError):self.validate()
                    self.assertEqual(read.call_count,1,'Must reject declarations before journal reads')
                self.index=saved

    def test_over_aggregate_before_journal_read(self):
        for build in self.index['builds']:
            for lane in ('ordinary','adversarial'):
                item=build['journals'][lane];item['bytes']=r.MEMBER_LIMIT
                with (self.root/item['path']).open('wb') as stream:stream.truncate(r.MEMBER_LIMIT)
        data=encoded(self.index);(self.root/r.INDEX).write_bytes(data)
        for p in self.report['puzzles']:p['protocol_validation']['index'].update(bytes=len(data),sha256=r.digest(data))
        with patch.object(r,'read_member',wraps=r.read_member) as read:
            with self.assertRaisesRegex(ValueError,'aggregate'):self.validate()
            self.assertEqual(read.call_count,1)

    def test_existing_one_mib_module_bound_is_preserved(self):
        artifact=self.report['puzzles'][2]['artifacts'][4]
        raw=r.codec.encode(b'\x00'*(1024*1024-3))
        self.assertEqual(len(raw),1024*1024)
        artifact.update(bytes=len(raw),clvm_hex=raw.hex(),tree_hash=r.oracle.treehash(r.codec.decode(raw)).hex())
        self.index['builds'][4]['artifacts'][2]=r.identity(artifact,r.SLUGS[2])
        self.write();self.assertEqual(self.validate()['files'],11)
        artifact['bytes']=1024*1024+1
        with self.assertRaisesRegex(ValueError,'precharge'):self.validate()

    def test_full_module_aggregate_before_first_decode(self):
        text='00'*(1024*1024)
        for puzzle in self.report['puzzles']:
            for artifact in puzzle['artifacts']:
                artifact.update(bytes=1024*1024,clvm_hex=text)
        with patch.object(r,'identity') as identity:
            with self.assertRaisesRegex(ValueError,'precharge'):self.validate()
            identity.assert_not_called()

    def test_all_hex_lengths_charged_before_first_decode(self):
        self.report['puzzles'][2]['artifacts'][4]['clvm_hex']='80'*100
        with patch.object(r,'identity') as identity:
            with self.assertRaisesRegex(ValueError,'precharge'):self.validate()
            identity.assert_not_called()

    def test_exact_inventory_and_unselected_artifact(self):
        (self.root/'registry-slot-protocol/extra.json').write_bytes(b'{}')
        with self.assertRaises(ValueError):self.validate()
        (self.root/'registry-slot-protocol/extra.json').unlink()
        self.report['puzzles'][2]['artifacts'][4]['clvm_hex']='8080'
        with self.assertRaises(ValueError):self.validate()

    def test_symlink_rejected_before_journal_read(self):
        name=next(iter(self.journals));path=self.root/name;path.unlink();path.symlink_to(self.root/r.INDEX)
        with self.assertRaises(ValueError):self.validate()

    def test_hash_and_size_closed(self):
        path=self.root/next(iter(self.journals));path.write_bytes(b'[]')
        with self.assertRaises(ValueError):self.validate()

    def test_triplet_and_crosslink_controls(self):
        for change in [lambda x:x['puzzles'].pop(),lambda x:x['puzzles'].append(copy.deepcopy(x['puzzles'][0])),
                       lambda x:x['puzzles'][0].update(kind='pure_function'),
                       lambda x:x['puzzles'][0]['protocol_validation'].update(role='registry-set-beta'),
                       lambda x:x['puzzles'][1]['protocol_validation']['index'].update(sha256='00'*32)]:
            saved=copy.deepcopy(self.report);change(self.report)
            with self.assertRaises(ValueError):self.validate()
            self.report=saved

    def test_header_and_build_controls(self):
        for change in [lambda x:x['builds'][0].update(optimize_lir=0),
                       lambda x:x['builds'][0]['counts']['ordinary'].update(accepted=True),
                       lambda x:x['header']['primary_vm'].update(flags=True),
                       lambda x:x['header'].update(owner_public_key='00'*48),
                       lambda x:x['builds'][4]['artifacts'][2].update(module_tree_hash='00'*32),
                       lambda x:x['header']['independently_compiled_chialisp'][2].update(source_sha256='00'*32)]:
            saved=copy.deepcopy(self.index);change(self.index);self.write()
            with self.assertRaises(ValueError):self.validate()
            self.index=saved

    def test_strict_json(self):
        for data in [b'{"x":1,"x":2}',b'{"x":1.0}',b'{"x":NaN}',b'['*129+b']'*129]:
            with self.assertRaises(ValueError):r.json_value(data)

    def test_repeat_checks_both_admissions_and_exact_members(self):
        import contextlib
        import io
        import sys
        with tempfile.TemporaryDirectory() as directory:
            other=Path(directory)
            materialize(other,self.report,copy.deepcopy(self.index),self.journals)
            first=self.root/'validation.json';second=other/'validation.json'
            first.write_bytes(encoded(self.report));second.write_bytes(first.read_bytes())
            with patch.object(sys,'argv',['registry_protocol.py','--report',str(first),'--repeat-report',str(second)]), \
                 patch.object(r,'validate_report',return_value={}) as admitted, contextlib.redirect_stdout(io.StringIO()):
                r.main();self.assertEqual(admitted.call_count,2)
            member=other/'registry-slot-protocol/rue-on-adversarial.json';member.write_bytes(b'[]')
            with patch.object(sys,'argv',['registry_protocol.py','--report',str(first),'--repeat-report',str(second)]), \
                 patch.object(r,'validate_report',return_value={}):
                with self.assertRaisesRegex(ValueError,'bytes/hash'):r.main()

    def test_dispatch_current_once_history_never(self):
        from ledger_rejections import check_ledger_report
        with patch.object(r,'validate_report') as validate:
            check_ledger_report(self.report,sidecar_root=self.root)
            validate.assert_called_once_with(self.report,sidecar_root=self.root)
            validate.reset_mock();check_ledger_report(self.report,require_typed=False,sidecar_root=self.root)
            validate.assert_not_called()


def historical_controls(prototype,artifacts):
    """Explicit retained research only; never current integration credit."""
    index=json.loads(prototype.read_bytes());base=json.loads(artifacts.read_bytes())['artifacts']
    journals={}
    for lane,kind in [('runs','ordinary'),('adversarial_runs','adversarial')]:
        for row in index[lane]:
            data=(prototype.parent/'journals'/row['path']).read_bytes()
            r.require(len(data)==row['bytes'] and r.digest(data)==row['sha256'],'historical journal identity')
            journals[f"registry-slot-protocol/{row['mode']}-{kind}.json"]=data
    with tempfile.TemporaryDirectory() as directory:
        root=Path(directory);report=make_report(base);transport=make_index(report,journals)
        materialize(root,report,transport,journals);r.validate_report(report,sidecar_root=root)
        controls=r.corruption_recipes()
        for label,lane,mutate in controls:
            bad=copy.deepcopy(journals);name=f'registry-slot-protocol/rust-off-{lane}.json';value=json.loads(bad[name]);mutate(value);bad[name]=encoded(value)
            materialize(root,report,make_index(report,bad),bad)
            try:r.validate_report(report,sidecar_root=root)
            except ValueError:pass
            else:raise ValueError('accepted historical semantic corruption '+label)
    print(f'PASS explicit historical research: complete transport/wire positive and {len(controls)} semantic corruption controls; no fresh worktree execution credit.')


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--historical-prototype',type=Path);parser.add_argument('--artifacts',type=Path)
    args,remaining=parser.parse_known_args()
    if args.historical_prototype:
        r.require(args.artifacts is not None,'explicit historical artifact input required')
        historical_controls(args.historical_prototype,args.artifacts)
    else:unittest.main(argv=['test_registry_protocol.py',*remaining])
