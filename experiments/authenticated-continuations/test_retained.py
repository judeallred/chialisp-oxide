"""Constructed evidence corruptions; no actual-source or execution credit."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import check_retained as c


class RetainedControls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base=Path(self.temp.name)/'experiments/authenticated-continuations'
        shutil.copytree(Path(__file__).resolve().parent,self.base,
            ignore=shutil.ignore_patterns('ledger','__pycache__'))
        for name in c.DEPENDENCIES:
            target=Path(self.temp.name)/name;target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(Path(__file__).resolve().parents[2]/name,target)

    def receipt(self):
        return json.loads((self.base/'evidence/receipt.json').read_text())

    def write_receipt(self,value):
        (self.base/'evidence/receipt.json').write_text(json.dumps(value))

    def update_file(self,relative):
        r=self.receipt();r['files'][relative]=hashlib.sha256((self.base/'evidence'/relative).read_bytes()).hexdigest();self.write_receipt(r)

    def test_original_complete_packet(self):
        c.check(self.base)

    def test_coherent_source_omission_is_rejected(self):
        r=self.receipt();del r['source_files']['src/lib.rs'];self.write_receipt(r)
        (self.base/'src/lib.rs').write_text('// altered policy')
        with self.assertRaisesRegex(ValueError,'closed expected current source set'):
            c.check(self.base)

    def test_complete_inventory_before_export_selection(self):
        p=self.base/'evidence/rust-on/manifest.json';m=json.loads(p.read_text());m['entries'].append(m['entries'][0]);p.write_text(json.dumps(m));self.update_file('rust-on/manifest.json')
        with self.assertRaises(ValueError):c.check(self.base)

    def test_automatic_source_is_bound_to_current_library(self):
        p=self.base/'src/lib.rs';p.write_text(p.read_text()+'\n// coherent current-source hash update\n')
        r=self.receipt();r['source_files']['src/lib.rs']=hashlib.sha256(p.read_bytes()).hexdigest();self.write_receipt(r)
        with self.assertRaisesRegex(ValueError,'automatic source bytes'):
            c.check(self.base)

    def test_mode_and_ir_inventory_are_checked(self):
        p=self.base/'evidence/rust-on/manifest.json';m=json.loads(p.read_text());m['entries'][0]['artifact']['optimize_lir']=False;p.write_text(json.dumps(m));self.update_file('rust-on/manifest.json')
        with self.assertRaises(ValueError):c.check(self.base)

    def test_missing_companion_is_rejected_even_if_index_edited(self):
        p=next((self.base/'evidence/rust-on').glob('*.artifact.json'));r=self.receipt();del r['files'][p.relative_to(self.base/'evidence').as_posix()];self.write_receipt(r);p.unlink()
        with self.assertRaises(ValueError):c.check(self.base)


    def test_coherent_typed_vm_failure_rehash_is_rejected(self):
        path=self.base/'evidence/rust_on.json';rows=json.loads(path.read_text());rows[3]['failure']['code']='invalid_op_arg';path.write_text(json.dumps(rows));self.update_file('rust_on.json')
        with self.assertRaisesRegex(ValueError,'exact typed failure'):
            c.check(self.base)

    def test_coherent_vm_output_rehash_is_rejected(self):
        path=self.base/'evidence/rust_on.json';rows=json.loads(path.read_text());rows[0]['outcome']['output_hex']='80';path.write_text(json.dumps(rows));self.update_file('rust_on.json')
        with self.assertRaisesRegex(ValueError,'exact independently expected outcome'):
            c.check(self.base)

if __name__=='__main__':unittest.main()
