"""Constructed ledger-report corruptions; no extra native/VM/SDK credit."""
import json
import hashlib
import shutil
import tempfile
import check_retained
import check_ledger
from pathlib import Path
import unittest
import admission as a
import ledger_checks as w


class VisibleLedgerControls(unittest.TestCase):
    def setUp(self):
        base=Path(__file__).resolve().parent
        self.base=json.loads((base/'ledger/evidence/base-report.json').read_text())
        self.controls=json.loads((base/'ledger/evidence/controls-report.json').read_text())
        self.modules={};self.control_modules={}
        for mode in ('on','off'):
            m=json.loads((base/'evidence'/f'rust-{mode}'/'manifest.json').read_text())['entries'][0]['artifact']
            self.modules['rust_'+mode]=(bytes.fromhex(m['tree_hash']),m['bytes'])
        raw=bytes.fromhex((base/'evidence/reference.clvm.hex').read_text().strip())
        self.modules['chialisp']=(a.raw_tree.tree_hash(a.raw_tree.deserialize(raw)),len(raw))
        source=json.loads((base/'ledger/evidence/source-controls.json').read_text())
        for r in source['records']:
            m=json.loads((base/'ledger/evidence'/r['manifest']).read_text())['entries'][0]['artifact']
            self.control_modules[(r['kind'],r['role'],r['mode'])]=(bytes.fromhex(m['tree_hash']),m['bytes'])

    def test_original_visible_witnesses(self):
        w.base(self.base,self.modules);w.controls(self.controls,self.control_modules)

    def test_coherent_payout_recipient_change_is_rejected(self):
        s=self.base['lanes'][0]['steps'][0]
        s['payout']=w.coin(s['coin']['coin_id'],'55'*32,1000)
        with self.assertRaisesRegex(ValueError,'fixed ordered payout'):
            w.base(self.base,self.modules)

    def test_terminal_successor_injection_is_rejected(self):
        s=self.base['lanes'][0]['steps'][3]
        s['successor']=w.coin(s['coin']['coin_id'],'55'*32,1)
        with self.assertRaisesRegex(ValueError,'terminal no successor'):
            w.base(self.base,self.modules)

    def test_generator_cost_corruption_is_rejected(self):
        self.base['lanes'][0]['steps'][0]['costs']['generators'][0]['metrics']['total_cost']+=1
        with self.assertRaisesRegex(ValueError,'cost decomposition'):
            w.base(self.base,self.modules)

    def test_source_fee_counterexample_is_required(self):
        row=next(r for r in self.controls['controls'] if r['kind']=='drop_amount')
        row['mutant']['fees']=0
        with self.assertRaisesRegex(ValueError,'source fee witness'):
            w.controls(self.controls,self.control_modules)

    def test_runtime_umbrella_cannot_gain_policy_credit(self):
        self.base['lanes'][0]['runtime_umbrellas'][0]['policy_rejection_credit']=True
        with self.assertRaisesRegex(ValueError,'no policy rejection credit'):
            w.base(self.base,self.modules)


class CoherentSourceOutcomeControls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.base=Path(self.temp.name)/'experiments/authenticated-continuations'
        original=Path(__file__).resolve().parent
        shutil.copytree(original,self.base,ignore=shutil.ignore_patterns('__pycache__'))
        root=original.parents[1]
        for name in check_retained.DEPENDENCIES | check_ledger.CODE_FILES:
            dest=Path(self.temp.name)/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(root/name,dest)

    def changed(self,modify):
        packet=self.base/'ledger/evidence';p=packet/'source-controls.json';value=json.loads(p.read_text());modify(value);p.write_text(json.dumps(value))
        receipt=packet/'receipt.json';r=json.loads(receipt.read_text());r['files']['source-controls.json']=hashlib.sha256(p.read_bytes()).hexdigest();receipt.write_text(json.dumps(r))

    def test_coherent_primary_vm_rehash_is_rejected(self):
        self.changed(lambda r:r['primary_vm'][0]['execution']['failure'].update(code='invalid_op_arg'))
        with self.assertRaisesRegex(ValueError,'exact typed failure'):
            check_ledger.check(self.base)

    def test_coherent_native_outcome_rehash_is_rejected(self):
        self.changed(lambda r:r['native'][0].update(observed=dict(status='accepted',output_hex='80')))
        with self.assertRaisesRegex(ValueError,'native observation rows'):
            check_ledger.check(self.base)

if __name__=='__main__':unittest.main()
