"""Constructed report corruption controls, not additional live/SDK observations.

The model uses real admitted policy bytes and retained measurement shapes;
synthetic timestamps/HTTP response records are deliberately constructed here.
"""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest

import live_report as r
import live_workflow as live
import workflow as w


def receipt(endpoint,body,response=b'{}'):
    raw=w.encode(body)
    return dict(endpoint=endpoint,http_status=200,request_bytes=len(raw),request_sha256=r.digest(raw),
                response_bytes=len(response),response_sha256=r.digest(response))


def fixture():
    artifact,blob=r.admitted()
    initial=dict(schema=1,scope=w.SCOPE,network='testnet11',revision=1,completed=0,
                 funding_parent='c4ed0b566583068b96dc6a539062b7546556f0d8ee3a66b2af60929109b804d8',
                 plan=w.plan(artifact,[r.RECIPIENT]*4))
    first=r.expected_step(initial,0,blob)[0]
    def tip(height):return dict(height=height,header_hash='aa'*32,previous_transaction_height=height-1)
    funding=dict(scope=live.SCOPE,tip=tip(4904651),coin=dict(coin=first,confirmed=4904647,spent=0),journal_revision_token=w.token(initial),
                 http_receipts=[receipt(endpoint,body) for endpoint,body in [('get_network_info',{}),('get_blockchain_state',{}),('get_coin_record_by_name',{'name':'0x'+first['coin_id']}),('get_blockchain_state',{})]])
    report=json.loads((w.BASE/'ledger/evidence/base-report.json').read_bytes())
    costs=next(lane for lane in report['lanes'] if lane['name']=='rust_on')['steps']
    files={'funding-observed.json':w.encode(funding)}
    intents=[]
    for index in range(4):
        state=copy.deepcopy(initial);state.update(completed=index,revision=index+1)
        current,children,reveal,solution=r.expected_step(initial,index,blob)
        bundle=dict(coin_spends=[dict(coin=dict(parent_coin_info=current['parent_id'],puzzle_hash=current['puzzle_hash'],amount=current['amount']),puzzle_reveal=reveal,solution=solution)],aggregated_signature='c0'+'00'*95)
        wire=b'\0\0\0\1'+bytes.fromhex(current['parent_id'])+bytes.fromhex(current['puzzle_hash'])+current['amount'].to_bytes(8,'big')+bytes.fromhex(reveal+solution+bundle['aggregated_signature'])
        measurement=copy.deepcopy(costs[index]['costs'])
        prepared=dict(current_coin=current,children=children,spend_bundle=bundle,spend_bundle_hex=wire.hex(),spend_bundle_id=r.digest(wire),spend_bundle_sha256=r.digest(wire),
                      measurement=measurement,measurement_protocol=live.PROTOCOL,host_executable_sha256='ab'*32)
        spent=4904700+index*10
        requests=[('get_network_info',{}),('get_blockchain_state',{}),('get_coin_record_by_name',{'name':'0x'+current['coin_id']}),('get_puzzle_and_solution',{'coin_id':'0x'+current['coin_id'],'height':spent}),('get_coin_records_by_parent_ids',{'parent_ids':['0x'+current['coin_id']],'include_spent_coins':True}),('get_blockchain_state',{})]
        confirmation=dict(scope=live.SCOPE,tip=tip(spent+2),spent_height=spent,parent=dict(coin=current,confirmed=4904647 if index==0 else spent-10,spent=spent),
                          children=[dict(coin=coin,confirmed=spent,spent=0) for coin in children],actual_spend=bundle['coin_spends'][0],http_receipts=[receipt(endpoint,body) for endpoint,body in requests])
        request=r.rpc_request(bundle,True)
        response=json.dumps(dict(status='SUCCESS',success=True,tx_id='0x'+prepared['spend_bundle_id']),separators=(',',':')).encode()
        push=dict(endpoint='push_tx',http_status=200,request_bytes=len(request),request_sha256=r.digest(request),response_bytes=len(response),response_sha256=r.digest(response))
        intent=dict(schema=1,scope=live.SCOPE,status='confirmed',original_state=state,revision_token=w.token(state),prepared=prepared,confirmation=confirmation,http_receipts=[push])
        intents.append(intent)
        files[f'step-{index}-confirmed.json']=w.encode(intent)
        name='responses/'+push['request_sha256']+'-'+push['response_sha256']+'.json'
        files[name]=w.encode(dict(schema=1,scope=live.SCOPE,receipt=push,response_hex=response.hex()))
    original=copy.deepcopy(intents[0]);original.update(status='ambiguous',confirmation=None)
    original_request=r.rpc_request(original['prepared']['spend_bundle'],False)
    original['http_receipts']=[dict(endpoint='push_tx',http_status=200,request_bytes=len(original_request),request_sha256=r.digest(original_request),response_bytes=757,response_sha256=r.MISSING_RESPONSE_SHA)]
    archive='journal.json.retry-original-'+original['prepared']['spend_bundle_id']+'.json'
    files['step-0-submit.json']=w.encode(original);files[archive]=w.encode(original)
    context=dict(schema=1,scope='explicit exact-bundle retry context; absence does not prove failure',bundle_id=original['prepared']['spend_bundle_id'],revision_token=original['revision_token'],mempool='absent; not proof of previous submission failure',tip=tip(4904677),http_receipts=[receipt('get_mempool_item_by_tx_id',{'tx_id':'0x'+original['prepared']['spend_bundle_id']})])
    files[archive+'.context.json']=w.encode(context)
    retry=copy.deepcopy(intents[0]);retry.update(status='submitted',confirmation=None)
    files['step-0-explicit-retry.json']=w.encode(dict(action='one_explicit_exact_retry',original_intent_archive='target/'+archive,recovery_context_archive='target/'+archive+'.context.json',intent=retry))
    final=copy.deepcopy(initial);final.update(completed=4,revision=5)
    files['journal.json']=w.encode(final)
    events=[]
    for epoch in range(2):
        events.append(dict(kind='started',time=f'2026-11-01T17:0{epoch}:00+00:00',policy='no automatic retry; zero-fee fixed queue only',runtime_sha256={name:('ab' if epoch==0 else 'cd')*32 for name in ('live_workflow.py','workflow.py','prepare_step.py')}))
    for index,intent in enumerate(intents):
        events.append(dict(kind='confirmed',time=f'2026-11-01T17:1{index}:00+00:00',index=index,bundle_id=intent['prepared']['spend_bundle_id'],spent_height=intent['confirmation']['spent_height']))
    events.append(dict(kind='complete',time='2026-11-01T17:20:00+00:00',completed=4,payout_mojos=4000))
    files['monitor/advancement-events.jsonl']=b''.join(w.encode(event) for event in events)
    files['monitor/advance.py']=b'# constructed nonexecuted monitor fixture\n'
    files['monitor/advance.stderr']=b'constructed observer failure; not policy rejection\n'
    files['operator/step-1-manual-reconcile.json']=files['step-1-confirmed.json']
    return files,artifact,blob


class LiveReportControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.files,cls.artifact,cls.blob=fixture()

    def changed(self,name,change):
        files=copy.deepcopy(self.files);value=live.decode(files[name]);change(value);files[name]=w.encode(value);return files

    def test_constructed_complete_report_counts_lineage_and_uncertainty(self):
        result=r.summarize(self.files,self.artifact,self.blob)
        self.assertEqual(result['counts'],dict(provider_observed_spends=4,retained_http_push_attempts=5,unique_prepared_bundles=4,success_acknowledgements=4))
        self.assertEqual(result['steps'][0]['coin']['coin_id'],r.FUNDING_COIN)
        self.assertIsNone(result['steps'][-1]['successor'])
        self.assertEqual(result['original_ambiguous_response']['availability'],'not_retained')
        self.assertFalse(result['original_ambiguous_response']['policy_rejection_credit'])

    def test_partial_journal_cannot_gain_completion_credit(self):
        files=self.changed('journal.json',lambda value:value.update(completed=3,revision=4))
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory);(path/'journal.json').write_bytes(files['journal.json'])
            with self.assertRaises(r.PartialTrial):r.collect(path)
        files=copy.deepcopy(self.files)
        files['monitor/advancement-events.jsonl']=b'\n'.join(files['monitor/advancement-events.jsonl'].splitlines()[:-1])+b'\n'
        with self.assertRaises(r.PartialTrial):r.summarize(files,self.artifact,self.blob)

    def test_fees_and_full_children_are_not_declarations(self):
        for change in (lambda value:value['prepared']['children'][0].update(amount=999),lambda value:value['confirmation']['children'].pop()):
            files=self.changed('step-1-confirmed.json',change)
            with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)

    def test_actual_reveal_and_opening_cannot_be_swapped(self):
        for key in ('puzzle_reveal','solution'):
            files=self.changed('step-1-confirmed.json',lambda value:value['confirmation']['actual_spend'].update({key:'80'}))
            with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)

    def test_relative_height_and_request_coin_binding(self):
        for change in (lambda value:value['confirmation']['parent'].update(confirmed=1),lambda value:value['confirmation']['http_receipts'][2].update(request_sha256='ab'*32)):
            files=self.changed('step-2-confirmed.json',change)
            with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)

    def test_confirmation_read_set_status_is_executed_metadata(self):
        for change in (lambda value:value['confirmation']['http_receipts'].pop(),lambda value:value['confirmation']['http_receipts'][0].update(http_status=500)):
            files=self.changed('step-3-confirmed.json',change)
            with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)

    def test_raw_ack_byte_hash_and_transaction_id_controls(self):
        name=next(name for name in self.files if name.startswith('responses/'))
        for change in (lambda value:value.update(response_hex='00'),lambda value:value['receipt'].update(response_sha256='ab'*32)):
            files=self.changed(name,change)
            with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        files=self.changed(name,lambda value:value.update(response_hex=json.dumps(dict(status='SUCCESS',success=True,tx_id='0x'+'ab'*32)).encode().hex()))
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        # Coherently update bytes, receipt, filename and selected intent so the
        # independent transaction-ID check, rather than a stale hash, rejects.
        files=copy.deepcopy(self.files)
        intent=live.decode(files['step-2-confirmed.json']);push=r.push_receipt(intent)
        old='responses/'+push['request_sha256']+'-'+push['response_sha256']+'.json'
        captured=live.decode(files.pop(old));ack=live.decode(bytes.fromhex(captured['response_hex']))
        ack['tx_id']='0x'+'ab'*32
        raw=json.dumps(ack,separators=(',',':')).encode()
        push.update(response_bytes=len(raw),response_sha256=r.digest(raw))
        captured.update(receipt=push,response_hex=raw.hex())
        new='responses/'+push['request_sha256']+'-'+push['response_sha256']+'.json'
        files[new]=w.encode(captured);files['step-2-confirmed.json']=w.encode(intent)
        with self.assertRaisesRegex(ValueError,'acknowledgement'):r.summarize(files,self.artifact,self.blob)
        files=copy.deepcopy(self.files)
        intent=live.decode(files['step-2-confirmed.json']);push=r.push_receipt(intent)
        old='responses/'+push['request_sha256']+'-'+push['response_sha256']+'.json'
        captured=live.decode(files.pop(old));raw=bytes.fromhex(captured['response_hex'])
        raw+=b' '*(live.MAX_PUSH_RESPONSE+1-len(raw))
        push.update(response_bytes=len(raw),response_sha256=r.digest(raw))
        captured.update(receipt=push,response_hex=raw.hex())
        new='responses/'+push['request_sha256']+'-'+push['response_sha256']+'.json'
        files[new]=w.encode(captured);files['step-2-confirmed.json']=w.encode(intent)
        with self.assertRaisesRegex(ValueError,'transport cap before hex allocation'):r.summarize(files,self.artifact,self.blob)

    def test_retry_archive_and_unknown_body_gap_cannot_be_erased(self):
        archive=next(name for name in self.files if name.startswith('journal.json.retry-original-') and not name.endswith('.context.json'))
        files=self.changed(archive,lambda value:value.update(status='submitted'))
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        files=self.changed('step-0-submit.json',lambda value:value['http_receipts'][0].update(response_bytes=0))
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)

    def test_bounded_closed_packet_and_unique_json(self):
        files=copy.deepcopy(self.files);files['unexpected.json']=b'{}'
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        files=copy.deepcopy(self.files);files['journal.json']=b'{"completed":4,"completed":3}'
        with self.assertRaises(ValueError):r.summarize(files,self.artifact,self.blob)
        with self.assertRaises(ValueError):r.fixed_files({'../escape':b'{}'})
        with self.assertRaises(ValueError):r.fixed_files({'large':b' '*1048577})
        files=copy.deepcopy(self.files)
        lines=[live.decode(line) for line in files['monitor/advancement-events.jsonl'].splitlines()]
        duplicate=dict(kind='submitted_once',time='2026-11-01T17:02:00+00:00',index=0,status='submitted',bundle_id=live.decode(files['step-0-confirmed.json'])['prepared']['spend_bundle_id'])
        files['monitor/advancement-events.jsonl']=b''.join(w.encode(row) for row in lines[:2]+[duplicate,duplicate]+lines[2:])
        with self.assertRaisesRegex(ValueError,'duplicate submitted-once'):r.summarize(files,self.artifact,self.blob)

    def test_bounded_preflight_inventory_and_roundtrip_checker(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)
            for index in range(r.MAX_FILES+5):(path/f'entry-{index}').write_bytes(b'')
            with self.assertRaisesRegex(ValueError,'budget|count'):r.check(path)
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as external:
            path=Path(directory);(path/'report.json').write_bytes(b'not JSON; must not read')
            (path/'responses').symlink_to(Path(external),target_is_directory=True)
            with self.assertRaisesRegex(ValueError,'symlink packet entry'):r.check(path)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory);os.mkfifo(path/'pipe')
            with self.assertRaisesRegex(ValueError,'regular single-link'):r.check(path)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)
            for name,raw in self.files.items():
                target=path/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
            report=dict(summary=r.summarize(self.files,self.artifact,self.blob),
                files={name:dict(bytes=len(raw),sha256=r.digest(raw)) for name,raw in self.files.items()},
                current_source_files={name:r.hash_file(w.BASE/name) for name in r.SOURCE_FILES},
                current_dependency_files={name:r.hash_file(w.BASE.parents[1]/name) for name in r.DEPENDENCIES},
                current_tool_files={name:'ef'*32 for name in r.TOOLS},current_host_executable_sha256='ab'*32,
                freshness_scope='Current exporter/source/tool consistency, not authentication of prior executions or runtime source epochs.')
            (path/'report.json').write_bytes(w.encode(report))
            self.assertEqual(r.check(path)['summary']['counts']['provider_observed_spends'],4)
            report['summary']['counts']['retained_http_push_attempts']=4
            (path/'report.json').write_bytes(w.encode(report))
            with self.assertRaisesRegex(ValueError,'report reconstruction'):r.check(path)


if __name__=='__main__':unittest.main()
