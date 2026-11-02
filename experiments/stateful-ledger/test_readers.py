"""Bounded reader controls; these tests execute no compiler, VM or SDK."""
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from capture_support import ROOT,admit_retained_bundle,discover_tool_source_root,retained_files,source_corruption_controls
import ledger_reader as l

class ReaderControls(unittest.TestCase):
    def test_source_inventory_deletion_extra_and_old_hash(self):
        source={'a.rs':'11'*32,'b.rs':'22'*32}
        self.assertEqual(source_corruption_controls(source,source),{k:'rejected' for k in ('omitted_source','extra_source','old_source_hash')})

    def test_nested_and_standard_build_target_workspace_discovery(self):
        with tempfile.TemporaryDirectory(dir=ROOT/'target') as temp:
            source=Path(temp)
            for name in ('Cargo.toml','rust-toolchain.toml'):shutil.copyfile(ROOT/name,source/name)
            for name in ('chialisp-oxide-driver','chialisp-oxide-ir','cargo-chialisp-oxide'):(source/'crates'/name).mkdir(parents=True)
            for suffix in ('target/debug','target/continuations-ci/host/debug'):
                tools=source/suffix;tools.mkdir(parents=True,exist_ok=True)
                self.assertEqual(discover_tool_source_root(tools),source)

    def test_exact_child_effects_coin_names_and_integer_height(self):
        initial=l.coin(bytes(32),bytes([0x11])*32,1000);paid=l.coin(initial['coin_id'],bytes([0x22])*32,1000)
        step=dict(height=1,removals=[initial],outputs=[paid],measurement={})
        l.effects(step,[initial],[paid],1)
        for field,value in [('amount',1001),('parent','00'*32),('coin_id','00'*32)]:
            bad=copy.deepcopy(step);bad['outputs'][0][field]=value
            with self.assertRaises(ValueError):l.effects(bad,[initial],[paid],1)
        bad=copy.deepcopy(step);bad['height']=True
        with self.assertRaises(ValueError):l.effects(bad,[initial],[paid],1)

    def test_nested_receipt_file_counted_and_symlink_rejected(self):
        with tempfile.TemporaryDirectory(dir=ROOT/'target') as temp:
            root=Path(temp);(root/'nested').mkdir();(root/'nested/receipt.json').write_text('{}')
            self.assertEqual(set(retained_files(root)),{'nested/receipt.json'})
            (root/'link').symlink_to(root/'nested/receipt.json')
            with self.assertRaises(ValueError):retained_files(root)

    def test_attached_selected_source_is_bound_and_portable(self):
        retained=ROOT/'experiments/authenticated-continuations/evidence/rust-on'
        m=json.loads((retained/'manifest.json').read_text())
        header=dict(compiler=m['compiler'],tools={'chialisp-oxide-driver':m['driver_sha256']},toolchain=m['toolchain'])
        with tempfile.TemporaryDirectory(dir=ROOT/'target') as temp:
            root=Path(temp);output=root/'output';bundle=output/'retained';shutil.copytree(retained,bundle)
            (output/'latest.json').write_text(json.dumps(dict(bundle='retained',manifest='retained/manifest.json')))
            # Read attachments at a distinct local source path; captured absolute
            # build origins remain declarations and are never opened.
            selected=root/'relocated-source';(selected/'src').mkdir(parents=True)
            for row in m['source_receipt']['files']:
                if row['path'].startswith('package/'):
                    dest=selected/row['path'].removeprefix('package/');dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(retained/row['blob'],dest)
            admit_retained_bundle(output,header,'chialisp-oxide-authenticated-continuations',True,selected)
            lib=selected/'src/lib.rs';lib.write_bytes(lib.read_bytes()+b'\n// source substitution control\n')
            with self.assertRaises(ValueError):admit_retained_bundle(output,header,'chialisp-oxide-authenticated-continuations',True,selected)

if __name__=='__main__':unittest.main()
