"""Host recovery tests; no new VM/SDK/live-network execution credit."""
import copy
import json
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

import workflow as w


class WorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Actual complete retained admission is deliberately not mocked here.
        cls.artifact = w.selected('on')

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)/'journal.json'
        self.recipients = [f'{x:02x}'*32 for x in (17, 34, 51)]
        self.mock = patch.object(w, 'selected', return_value=self.artifact)
        self.mock.start()
        self.addCleanup(self.mock.stop)

    def initialize(self):
        return w.initialize(self.path, 'on', self.recipients)

    def funded(self):
        state = self.initialize()
        return w.mutate(self.path, w.token(state), parent='aa'*32)

    def observation(self, state):
        index = state['completed']
        current = w.expected_coin(state, index)
        children = [w.coin(current['coin_id'], state['plan']['steps'][index]['recipient'], 1000)]
        if index+1 < len(self.recipients):
            children.append(w.expected_coin(state, index+1))
        return dict(network='testnet11', coin_id=current['coin_id'], children=children)

    def test_whole_queue_and_terminal(self):
        state = self.initialize()
        self.assertEqual(state['plan']['steps'][0]['amount'], 3000)
        self.assertEqual(state['plan']['steps'][-1]['next_digest'], w.o.terminal().hex())
        for recipients in ([], ['aa'*32]*9, ['AA'*32], ['a'*63], [True]):
            with self.assertRaises(ValueError):
                w.plan(self.artifact, recipients)

    def test_admission_identity_and_queue_tampering(self):
        self.initialize()
        original = self.path.read_bytes()
        for change in ('hash', 'head', 'recipient', 'network', 'count'):
            state = json.loads(original)
            if change == 'hash': state['plan']['artifact']['module_hash'] = 'bb'*32
            if change == 'head': state['plan']['steps'][0]['puzzle_hash'] = 'bb'*32
            if change == 'recipient': state['plan']['recipients'][0] = 'bb'*32
            if change == 'network': state['network'] = 'mainnet'
            if change == 'count': state['completed'] = True
            self.path.write_bytes(w.encode(state))
            with self.assertRaises(ValueError): w.load(self.path)
        self.path.write_bytes(original)
        self.assertEqual(w.load(self.path)['revision'], 0)

    def test_stale_revision_preserves_bytes_and_no_double_record(self):
        initial = self.initialize()
        state = w.mutate(self.path, w.token(initial), parent='aa'*32)
        before = self.path.read_bytes()
        with self.assertRaisesRegex(ValueError, 'stale'):
            w.mutate(self.path, w.token(initial), parent='bb'*32)
        self.assertEqual(before, self.path.read_bytes())
        with self.assertRaisesRegex(ValueError, 'already funded'):
            w.mutate(self.path, w.token(state), parent='bb'*32)

    def test_three_steps_and_replayed_observation(self):
        state = self.funded()
        for index in range(3):
            observation = self.observation(state)
            old_token = w.token(state)
            state = w.mutate(self.path, old_token, observation=observation)
            self.assertEqual(state['completed'], index+1)
            before = self.path.read_bytes()
            with self.assertRaises(ValueError):
                w.mutate(self.path, w.token(state), observation=observation)
            self.assertEqual(before, self.path.read_bytes())
        self.assertIsNone(w.view(state)['current_coin'])
        self.assertIsNone(w.view(state)['next_step'])

    def test_wrong_observations_never_change_journal(self):
        state = self.funded()
        original = self.observation(state)
        before = self.path.read_bytes()
        for change in ('network', 'coin', 'child', 'extra', 'missing', 'order'):
            observation = copy.deepcopy(original)
            if change == 'network': observation['network'] = 'mainnet'
            if change == 'coin': observation['coin_id'] = 'bb'*32
            if change == 'child': observation['children'][1]['amount'] += 1
            if change == 'extra': observation['children'].append(original['children'][0])
            if change == 'missing': observation['children'].pop()
            if change == 'order': observation['children'].reverse()
            with self.assertRaises(ValueError):
                w.mutate(self.path, w.token(state), observation=observation)
            self.assertEqual(before, self.path.read_bytes())

    def test_json_and_file_controls(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'['*25+b']'*25, b' '*65537):
            self.path.write_bytes(raw)
            with self.assertRaises(ValueError): w.read(self.path)
        self.path.unlink()
        target = self.path.with_name('target')
        target.write_text('{}')
        self.path.symlink_to(target)
        with self.assertRaises(OSError): w.read(self.path)
        self.path.unlink()
        self.path.hardlink_to(target)
        with self.assertRaises(ValueError): w.read(self.path)

    def test_lock_and_existing_journal(self):
        self.initialize()
        with self.assertRaises(ValueError): self.initialize()
        with w.locked(self.path):
            with self.assertRaises(BlockingIOError):
                w.mutate(self.path, w.token(w.load(self.path)), parent='aa'*32)

    def test_atomic_precommit_failure_preserves_old_journal(self):
        state = self.initialize()
        before = self.path.read_bytes()
        with patch.object(w.os, 'replace', side_effect=OSError('injected rename failure')):
            with self.assertRaises(OSError):
                w.mutate(self.path, w.token(state), parent='aa'*32)
        self.assertEqual(before, self.path.read_bytes())
        self.assertEqual(sorted(p.name for p in self.path.parent.iterdir()), ['journal.json', 'journal.json.lock'])

    def test_postcommit_failure_requires_reload(self):
        state = self.initialize()
        original_fsync = w.os.fsync
        calls = 0
        def fail_directory(fd):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('injected directory fsync failure')
            return original_fsync(fd)
        with patch.object(w.os, 'fsync', side_effect=fail_directory):
            with self.assertRaisesRegex(w.PostcommitError, 'reload'):
                w.mutate(self.path, w.token(state), parent='aa'*32)
        self.assertEqual(w.load(self.path)['revision'], 1)
        with self.assertRaisesRegex(ValueError, 'stale'):
            w.mutate(self.path, w.token(state), parent='aa'*32)

    def test_actual_corrupt_packet_rejected_before_journal_creation(self):
        package = self.path.parent/'isolated'/'experiments'/'authenticated-continuations'
        package.mkdir(parents=True)
        receipt = json.loads((w.BASE/'evidence'/'receipt.json').read_bytes())
        for name in receipt['source_files']:
            destination = package/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(w.BASE/name, destination)
        for name in receipt['reader_dependencies']:
            destination = package.parents[1]/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(w.BASE.parents[1]/name, destination)
        shutil.copytree(w.BASE/'evidence', package/'evidence')
        manifest = package/'evidence'/'rust-on'/'manifest.json'
        altered = json.loads(manifest.read_bytes())
        altered['entries'][0]['artifact']['tree_hash'] = 'aa'*32
        manifest.write_bytes(w.encode(altered))
        self.mock.stop()
        with patch.object(w, 'BASE', package):
            with self.assertRaisesRegex(ValueError, 'stale retained file'):
                w.initialize(self.path, 'on', self.recipients)
        self.mock.start()
        self.assertFalse(self.path.exists())

    def test_actual_process_restart_cli(self):
        recipients = self.path.with_name('recipients.json')
        recipients.write_text(json.dumps(self.recipients))
        def run(*args):
            result = subprocess.run([sys.executable, '-B', str(w.BASE/'workflow.py'), *args],
                                    capture_output=True, check=True, timeout=60)
            return json.loads(result.stdout)
        initial = run('init', '--mode', 'on', '--recipients', str(recipients), '--journal', str(self.path))
        funded = run('record-funding', '--journal', str(self.path), '--revision-token', initial['revision_token'], '--parent', 'aa'*32)
        observation = self.path.with_name('observation.json')
        observation.write_text(json.dumps(self.observation(funded['state'])))
        resumed = run('reconcile', '--journal', str(self.path), '--revision-token', funded['revision_token'], '--observation', str(observation))
        shown = run('show', '--journal', str(self.path))
        self.assertEqual(resumed, shown)
        self.assertEqual(shown['state']['completed'], 1)
        self.assertEqual(shown['current_coin'], w.expected_coin(funded['state'], 1))


if __name__ == '__main__':
    unittest.main()
