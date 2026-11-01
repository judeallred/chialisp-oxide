"""Host reconciliation of retained SDK records; no new SDK execution credit."""
import json
from pathlib import Path
import tempfile
import unittest

import check_ledger
import workflow as w


class RetainedLedgerRecovery(unittest.TestCase):
    def test_both_rust_chains_reconcile_exact_retained_children(self):
        base = Path(__file__).resolve().parent
        check_ledger.check(base)
        report = json.loads((base/'ledger/evidence/base-report.json').read_text())
        for mode in ('on', 'off'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                lane = next(row for row in report['lanes'] if row['name'] == 'rust_'+mode)
                path = Path(directory)/'journal.json'
                recipients = [row['payout']['puzzle_hash'] for row in lane['steps']]
                state = w.initialize(path, mode, recipients)
                state = w.mutate(path, w.token(state), parent=lane['initial_coin']['parent_id'])
                for row in lane['steps']:
                    state = w.load(path)
                    self.assertEqual(w.view(state)['current_coin'], row['coin'])
                    children = [row['payout']]
                    if row['successor'] is not None:
                        children.append(row['successor'])
                    state = w.mutate(path, w.token(state), observation={
                        'network': 'testnet11', 'coin_id': row['coin']['coin_id'],
                        'children': children})
                self.assertEqual(w.load(path)['completed'], 4)
                self.assertIsNone(w.view(w.load(path))['current_coin'])


if __name__ == '__main__':
    unittest.main()
