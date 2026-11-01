"""Injected provider scenarios only: no real requests or network submissions."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import live_workflow as live
import prepare_step
import workflow as w


def wire_record(coin, *, spent=0, confirmed=5):
    return dict(coin=dict(parent_coin_info='0x'+coin['parent_id'], puzzle_hash='0x'+coin['puzzle_hash'],amount=coin['amount']),
                confirmed_block_index=confirmed,spent_block_index=spent,spent=bool(spent),coinbase=False,timestamp=1000)


class Provider:
    def __init__(self, prepared):
        self.prepared = prepared
        self.calls = []
        self.receipts = []
        self.spent = False
        self.confirmed = 5
        self.missing = False
        self.push_error = False
        self.network_name = 'testnet11'
        self.change_tip = False
        self.tips = 0
        self.mutate_children = None
        self.wrong_solution = False
        self.push_hook = None

    def call(self, endpoint, body):
        result=self._call(endpoint,body)
        request=w.encode(body);response=w.encode(result)
        self.receipts.append(dict(endpoint=endpoint,http_status=200,request_bytes=len(request),
            request_sha256=hashlib.sha256(request).hexdigest(),response_bytes=len(response),
            response_sha256=hashlib.sha256(response).hexdigest()))
        return result

    def _call(self, endpoint, body):
        self.calls.append((endpoint,copy.deepcopy(body)))
        if endpoint == 'get_network_info': return dict(success=True,network_name=self.network_name,network_prefix='txch')
        if endpoint == 'get_blockchain_state':
            self.tips += 1
            return dict(success=True,blockchain_state=dict(sync=dict(synced=True,sync_mode=False),peak=dict(height=10,prev_transaction_block_height=9,header_hash=('bb' if self.change_tip and self.tips%2==0 else 'aa')*32)))
        if endpoint == 'get_coin_record_by_name':
            return dict(success=True,coin_record=None if self.missing else wire_record(self.prepared['current_coin'],spent=8 if self.spent else 0,confirmed=self.confirmed))
        if endpoint == 'get_puzzle_and_solution':
            value=copy.deepcopy(self.prepared['spend_bundle']['coin_spends'][0])
            value['coin']['parent_coin_info']='0x'+value['coin']['parent_coin_info']
            value['coin']['puzzle_hash']='0x'+value['coin']['puzzle_hash']
            value['puzzle_reveal']='0x'+value['puzzle_reveal']
            value['solution']='0x'+value['solution']
            if self.wrong_solution: value['solution']='80'
            return dict(success=True,coin_solution=value)
        if endpoint == 'get_coin_records_by_parent_ids':
            rows=[wire_record(coin,confirmed=8) for coin in self.prepared['children']]
            if self.mutate_children: self.mutate_children(rows)
            return dict(success=True,coin_records=rows)
        if endpoint == 'push_tx':
            if self.push_hook: self.push_hook()
            if self.push_error: raise TimeoutError('injected push timeout')
            return dict(success=True,status='SUCCESS')
        raise AssertionError(endpoint)


@unittest.skipUnless(os.environ.get('CHIALISP_CONTINUATION_PREPARE'), 'explicit locally built prepare-step executable required')
class LiveWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.exe=Path(os.environ['CHIALISP_CONTINUATION_PREPARE']).resolve(strict=True)

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path=Path(self.temp.name)/'journal.json'
        state=w.initialize(self.path,'on',['11'*32,'22'*32,'33'*32])
        self.state=w.mutate(self.path,w.token(state),parent='aa'*32)
        self.intent=live.prepare(self.path,self.exe,0,w.token(self.state))
        self.provider=Provider(self.intent['prepared'])

    def test_explicit_submit_persists_before_push_and_never_retries(self):
        with self.assertRaisesRegex(ValueError,'explicit'):
            live.submit(self.path,self.provider,w.token(self.state))
        self.assertEqual(self.provider.calls,[])
        def check_started(): self.assertEqual(live.read_intent(self.path)['status'],'push_started')
        self.provider.push_hook=check_started
        result=live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        self.assertEqual(result['status'],'submitted')
        pushed=next(body['spend_bundle'] for endpoint,body in self.provider.calls if endpoint=='push_tx')
        self.assertEqual(pushed['aggregated_signature'],'0x'+self.intent['prepared']['spend_bundle']['aggregated_signature'])
        self.assertEqual(pushed['coin_spends'][0]['puzzle_reveal'],'0x'+self.intent['prepared']['spend_bundle']['coin_spends'][0]['puzzle_reveal'])
        self.assertEqual(result['prepared']['spend_bundle_hex'],self.intent['prepared']['spend_bundle_hex'])
        with self.assertRaisesRegex(ValueError,'already attempted'):
            live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        self.assertEqual(sum(endpoint=='push_tx' for endpoint,_ in self.provider.calls),1)
        self.assertEqual(w.load(self.path),self.state)
        self.provider.spent=True
        confirmed=live.reconcile(self.path,self.provider)
        self.assertIn('push_tx',[row['endpoint'] for row in confirmed['http_receipts']])
        self.assertEqual(len(confirmed['confirmation']['http_receipts']),6)

    def test_ambiguous_timeout_preserves_exact_intent_without_repush(self):
        self.provider.push_error=True
        with self.assertRaises(TimeoutError): live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        intent=live.read_intent(self.path)
        self.assertEqual(intent['status'],'ambiguous')
        self.assertEqual(intent['prepared']['spend_bundle_hex'],self.intent['prepared']['spend_bundle_hex'])
        self.assertEqual(intent['prepared']['measurement']['previous_transaction_height'],9)
        self.provider.missing=True
        with self.assertRaisesRegex(ValueError,'already attempted'):
            live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        self.assertEqual(sum(endpoint=='push_tx' for endpoint,_ in self.provider.calls),1)

    def test_unconfirmed_height_zero_cannot_push(self):
        self.provider.confirmed=0
        with self.assertRaises(ValueError):
            live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        self.assertFalse(any(endpoint=='push_tx' for endpoint,_ in self.provider.calls))
        self.assertEqual(live.read_intent(self.path)['status'],'prepared')

    def test_confirmed_exact_spend_advances_once_without_push(self):
        self.provider.spent=True
        result=live.reconcile(self.path,self.provider)
        self.assertEqual(result['status'],'confirmed')
        self.assertEqual(result['confirmation']['spent_height'],8)
        self.assertEqual(result['confirmation']['tip']['height'],10)
        self.assertEqual(len(result['confirmation']['http_receipts']),6)
        corrupted=copy.deepcopy(result)
        corrupted['confirmation']['spent_height']=7
        with self.assertRaises(ValueError):live.check_intent(corrupted)
        corrupted=copy.deepcopy(result)
        corrupted['confirmation']['http_receipts'].pop()
        with self.assertRaises(ValueError):live.check_intent(corrupted)
        corrupted=copy.deepcopy(result)
        corrupted['confirmation']['http_receipts'][0]['http_status']=500
        with self.assertRaises(ValueError):live.check_intent(corrupted)
        self.assertEqual(w.load(self.path)['completed'],1)
        self.assertEqual(live.reconcile(self.path,self.provider)['status'],'confirmed')
        self.assertEqual(w.load(self.path)['completed'],1)
        self.assertFalse(any(endpoint=='push_tx' for endpoint,_ in self.provider.calls))

    def test_crash_between_journal_advance_and_intent_confirmation_recovers(self):
        advanced=copy.deepcopy(self.state)
        advanced['completed']+=1;advanced['revision']+=1
        w.write(self.path,advanced)
        self.provider.spent=True
        self.assertEqual(live.reconcile(self.path,self.provider)['status'],'confirmed')
        self.assertEqual(w.load(self.path),advanced)

    def test_provider_disagreements_preserve_journal(self):
        original=self.path.read_bytes()
        for kind in ('wrong_network','tip_change','wrong_solution','wrong_child','extra_child','wrong_height'):
            with self.subTest(kind=kind):
                provider=Provider(self.intent['prepared']);provider.spent=True
                if kind=='wrong_network': provider.network_name='mainnet'
                if kind=='tip_change': provider.change_tip=True
                if kind=='wrong_solution': provider.wrong_solution=True
                if kind=='wrong_child': provider.mutate_children=lambda rows: rows[0]['coin'].update(amount=1001)
                if kind=='extra_child': provider.mutate_children=lambda rows: rows.append(rows[0])
                if kind=='wrong_height': provider.mutate_children=lambda rows: rows[0].update(confirmed_block_index=7)
                with self.assertRaises(ValueError): live.reconcile(self.path,provider)
                self.assertEqual(self.path.read_bytes(),original)

    def test_later_child_spends_do_not_erase_parent_confirmation(self):
        self.provider.spent=True
        def later_spent(rows):
            for row in rows: row.update(spent=True,spent_block_index=9)
        self.provider.mutate_children=later_spent
        self.assertEqual(live.reconcile(self.path,self.provider)['status'],'confirmed')
        self.assertEqual(w.load(self.path)['completed'],1)

    def test_newer_coin_record_is_snapshot_uncertainty_without_progress(self):
        before=self.path.read_bytes()
        class Newer(Provider):
            def _call(self,endpoint,body):
                if endpoint=='get_coin_record_by_name':
                    return dict(success=True,coin_record=wire_record(self.prepared['current_coin'],spent=11))
                return super()._call(endpoint,body)
        with self.assertRaisesRegex(live.Inconclusive,'newer than'):
            live.reconcile(self.path,Newer(self.intent['prepared']))
        self.assertEqual(self.path.read_bytes(),before)

    def test_stale_token_and_tampered_wire_forbid_push(self):
        with self.assertRaisesRegex(ValueError,'stale'):
            live.submit(self.path,self.provider,'00'*32,explicitly_submit=True,executable=self.exe)
        intent=copy.deepcopy(self.intent)
        intent['prepared']['spend_bundle_hex']+='00'
        live.write_intent(live.intent_path(self.path),intent)
        with self.assertRaises(ValueError): live.submit(self.path,self.provider,w.token(self.state),explicitly_submit=True,executable=self.exe)
        self.assertEqual(self.provider.calls,[])

    def test_prepared_and_attempted_intents_cannot_be_overwritten(self):
        with self.assertRaisesRegex(ValueError,'confirmed'):
            live.prepare(self.path,self.exe,0,w.token(self.state))
        self.provider.spent=True
        live.reconcile(self.path,self.provider)
        next_intent=live.prepare(self.path,self.exe,0,w.token(w.load(self.path)))
        self.assertEqual(next_intent['original_state']['completed'],1)

    def test_observe_readonly_stable_exact_funding_coin(self):
        before=self.path.read_bytes()
        result=live.observe_current(self.path,self.provider)
        self.assertEqual(result['coin']['coin'],self.intent['prepared']['current_coin'])
        self.assertEqual(before,self.path.read_bytes())
        self.provider.change_tip=True;self.provider.tips=0
        with self.assertRaises(ValueError):live.observe_current(self.path,self.provider)


class TransportControls(unittest.TestCase):
    def test_push_response_requires_durable_directory_and_retains_raw_error(self):
        with self.assertRaisesRegex(ValueError,'durable'):
            live.CurlTransport().call('push_tx',{})
        raw=b'{"success":false,"error":"provider observation, not consensus evidence"}'
        def transport(args,**kwargs):
            self.assertEqual(kwargs['stdout_limit'],live.MAX_PUSH_RESPONSE+4)
            return subprocess.CompletedProcess(args,0,raw+b'\n200',b'')
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'responses'
            with patch.object(live.bounded_process,'run',side_effect=transport):
                response=live.CurlTransport(path).call('push_tx',{})
            self.assertFalse(response['success'])
            files=list(path.iterdir());self.assertEqual(len(files),1)
            record=live.decode(w.payload(files[0],live.MAX_BYTES))
            self.assertEqual(bytes.fromhex(record['response_hex']),raw)
            self.assertEqual(record['receipt']['response_sha256'],hashlib.sha256(raw).hexdigest())
            self.assertEqual(record['receipt']['response_bytes'],len(raw))

    def test_unique_bounded_json(self):
        for raw in (b'{"success":true,"success":false}',b'{"x":NaN}',b'['*33+b']'*33,b' '*1048577):
            with self.assertRaises(ValueError):live.decode(raw)

    def test_fixed_origin_no_redirects_deadline_and_http_status(self):
        def transport(args,**kwargs):
            self.assertEqual(args[0:2],['/usr/bin/curl','--disable'])
            self.assertEqual(args[-1],live.ORIGIN+'/get_network_info')
            self.assertIn('--max-time',args);self.assertIn('--noproxy',args)
            self.assertNotIn('--location',args)
            self.assertLessEqual(kwargs['stdout_limit'],1048580)
            self.assertEqual(kwargs['timeout'],25)
            return subprocess.CompletedProcess(args,0,b'{"success":true}\n302',b'')
        with patch.object(live.bounded_process,'run',side_effect=transport):
            with self.assertRaises(ValueError):live.CurlTransport().call('get_network_info',{})
        with self.assertRaises(ValueError):live.CurlTransport().call('https://evil.test',{})
        with patch.object(live.bounded_process,'run',side_effect=subprocess.TimeoutExpired('curl',25)):
            with self.assertRaises(live.Inconclusive):live.CurlTransport().call('get_network_info',{})


if __name__=='__main__':unittest.main()
