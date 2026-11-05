"""Pure v2 controls; explicit XCHANDLES_HISTORY_V2_PACKET enables actual data.

All mutations target a disposable copy, with outer inventories and selected
nested RPC byte declarations coherently rehashed. No VM/SDK/network/builds.
"""
import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import xchandles_history as core
import xchandles_history_blocks as blocks
import xchandles_history_v2 as h


class Bounds(unittest.TestCase):
    def test_strict_json_and_equality(self):
        for raw in (b'{"fees":1,"fe\\u0065s":2}', b'{"elapsed_seconds":Infinity}',
                    b'{"fees":1.5}', b'['*65+b'0'+b']'*65):
            with self.assertRaises(ValueError):
                h.json_value(raw)
        self.assertEqual(h.json_value(b'{"cost":{"x":1}}'), {'cost': {'x': 1}})
        self.assertFalse(blocks.same({'success': 1}, {'success': True}))
        self.assertFalse(blocks.same([0], [False]))
        self.assertEqual(blocks.decimal('18446744073709551615'), '18446744073709551615')
        for value in (18446744073709551615, '01', '-1', '1.5', True):
            with self.assertRaises(ValueError):
                blocks.decimal(value)

    def test_all_outer_precharge_before_payload_open(self):
        for count, size in ((1, core.MAX_MEMBER+1), (64, core.MAX_MEMBER), (2048, 0)):
            with self.subTest(count=count), tempfile.TemporaryDirectory() as temp:
                root = Path(temp).resolve()
                rows = [dict(path=f'core/m{i}', role='preserved_primary_v1', bytes=size, sha256='0'*64)
                        for i in range(count)]
                (root/core.MANIFEST).write_text(json.dumps(dict(protocol=h.PROTOCOL, profile=h.PROFILE, members=rows)))
                opened = []
                original = core.read_regular
                def read(path, *args):
                    opened.append(Path(path).name)
                    return original(path, *args)
                with patch.object(core, 'read_regular', read), self.assertRaises(ValueError):
                    h.Packet(root)
                self.assertEqual(opened, [core.MANIFEST])


    def test_explicit_destination_aliases_and_parent_traversal_fail_early(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            inputs = [root/name for name in ('primary', 'blocks', 'cost')]
            for path in inputs:
                path.mkdir()
            license_path = root/'LICENSE'
            license_path.write_bytes(b'explicit fixture')
            outside = root/'outside'
            outside.mkdir()
            alias = outside/'alias'
            alias.symlink_to(inputs[0], target_is_directory=True)
            dangling = outside/'dangling'
            dangling.symlink_to(outside/'absent', target_is_directory=True)
            cases = (alias/'new', dangling/'new', outside/'leaf-link',
                     outside/'alias'/'..'/'new', inputs[0]/'new')
            cases[2].symlink_to(outside/'missing-leaf')
            for destination in cases:
                with self.subTest(destination=destination), \
                     patch.object(h, 'scan_inputs', side_effect=AssertionError('input scan reached')) as scan, \
                     patch.object(core, 'read_regular', side_effect=AssertionError('payload read reached')) as read, \
                     patch.object(Path, 'mkdir', side_effect=AssertionError('mkdir reached')) as mkdir:
                    with self.assertRaises(ValueError):
                        h.build_packet(*inputs, license_path, destination)
                    scan.assert_not_called()
                    read.assert_not_called()
                    mkdir.assert_not_called()
            self.assertEqual(list(inputs[0].iterdir()), [])



@unittest.skipUnless(os.environ.get('XCHANDLES_HISTORY_V2_PACKET'),
                     'Explicit v2 retained packet required')
class Retained(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = Path(os.environ['XCHANDLES_HISTORY_V2_PACKET'])
        cls.positive = h.verify(cls.source)
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name).resolve()/'packet'
        shutil.copytree(cls.source, cls.root)
        cls.manifest = (cls.root/core.MANIFEST).read_bytes()

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.changed = {}

    def tearDown(self):
        for name, raw in self.changed.items():
            (self.root/name).write_bytes(raw)
        (self.root/core.MANIFEST).write_bytes(self.manifest)

    def replace(self, name, raw):
        path = self.root/name
        if name not in self.changed:
            self.changed[name] = path.read_bytes()
        path.write_bytes(raw)
        manifest = json.loads((self.root/core.MANIFEST).read_bytes())
        row = next(x for x in manifest['members'] if x['path'] == name)
        row.update(core.ident(raw))
        (self.root/core.MANIFEST).write_text(json.dumps(manifest))

    def change(self, name, function):
        value = json.loads((self.root/name).read_bytes())
        function(value)
        self.replace(name, json.dumps(value).encode())

    def stem(self, index):
        endpoint = 'get_block_record_by_height' if index % 2 == 0 else 'get_block'
        return f'blocks/http/{index:03}-{endpoint}'

    def repair_raw(self, index):
        stem = self.stem(index)
        raw = (self.root/(stem+'.raw.json')).read_bytes()
        self.change(stem+'.observation.json', lambda v: v.update(core.ident(raw)))
        observation = json.loads((self.root/(stem+'.observation.json')).read_bytes())
        self.change('blocks/coverage-and-scope.json', lambda v: v['requests'].__setitem__(index, observation))

    def repair_request(self, index):
        stem = self.stem(index)
        initial = json.loads((self.root/(stem+'.request.json')).read_bytes())
        self.change(stem+'.observation.json', lambda v: v.update(initial))
        observation = json.loads((self.root/(stem+'.observation.json')).read_bytes())
        self.change('blocks/coverage-and-scope.json', lambda v: v['requests'].__setitem__(index, observation))

    def reject_before_core_decode(self, regex=None):
        with patch.object(core, 'decode', side_effect=AssertionError('selected core decode occurred')) as decoded:
            with self.assertRaisesRegex(ValueError, regex or '.*'):
                h.verify(self.root)
            decoded.assert_not_called()

    def test_positive_scope_rows_and_safe_fees(self):
        r = self.positive
        self.assertEqual((r['primaryComponentObservations'], r['newReaderExecutions'], r['blockRpcObservations']), (129, 0, 258))
        self.assertEqual(len(r['rows']), 129)
        self.assertLessEqual(r['physicalFiles'], 2048)
        self.assertLessEqual(r['physicalBytes'], 128*1024*1024)
        self.assertLessEqual(r['manifestBytes'], 512*1024)
        self.assertEqual(r['creationCounts']['zero_mojo_CREATE_COIN'], 11508)
        self.assertEqual(r['conditionalCreateCost']['zeroOnly'], 20714400000)
        self.assertEqual(r['wholeBlockFeesSumDecimal'], '3341788643120')
        self.assertTrue(all(type(row['fullBlockFeesDecimal']) is str for row in r['rows']))
        self.assertIn('conditional', r['conditionalCreateCost']['scope'])
        self.assertIn('opaque', r['blockScope'])
        self.assertTrue(all(not row['headerRecomputed'] for row in r['rows']))
        self.assertEqual(r['primary']['earlier_unretained_uncredited_descriptors'], 20)


    def test_late_record_role_bound_before_payload_open(self):
        name = self.stem(256)+'.raw.json'
        self.replace(name, b' '*65537)
        original = core.read_regular
        opened = []
        def read(path, *args):
            opened.append(Path(path).relative_to(self.root).as_posix())
            return original(path, *args)
        with patch.object(core, 'read_regular', read), self.assertRaises(ValueError):
            h.Packet(self.root)
        self.assertEqual(opened, [core.MANIFEST])

    def test_wrapper_late_role_bound_before_copy(self):
        self.replace(self.stem(256)+'.raw.json', b' '*65537)
        cost_input = self.root.parent/'cost-input'
        shutil.copytree(self.root/'cost', cost_input, ignore=shutil.ignore_patterns('LICENSE'))
        try:
            destination = self.root.parent/'oversize-wrapper-output'
            with patch.object(core, 'read_regular', side_effect=AssertionError('payload read')) as read, \
                 patch.object(Path, 'mkdir', side_effect=AssertionError('mkdir')) as mkdir:
                with self.assertRaisesRegex(ValueError, 'wrapper per-role precharge'):
                    h.build_packet(self.root/'core', self.root/'blocks', cost_input,
                                   self.root/'cost/LICENSE', destination)
                read.assert_not_called()
                mkdir.assert_not_called()
            self.assertFalse(destination.exists())
        finally:
            shutil.rmtree(cost_input)

    def test_actual_primary_tree_unchanged_through_output_alias(self):
        primary = self.root/'core'
        snapshot = sorted((p.relative_to(primary).as_posix(), p.stat().st_size, p.stat().st_mtime_ns)
                          for p in primary.rglob('*') if p.is_file())
        manifest = (primary/core.MANIFEST).read_bytes()
        outside = self.root.parent/'outside-destination'
        outside.mkdir()
        alias = outside/'alias-to-primary'
        alias.symlink_to(primary, target_is_directory=True)
        destination = alias/'new'
        try:
            with patch.object(h, 'scan_inputs', side_effect=AssertionError('scan')) as scan, \
                 patch.object(core, 'read_regular', side_effect=AssertionError('payload read')) as read, \
                 patch.object(Path, 'mkdir', side_effect=AssertionError('mkdir')) as mkdir:
                with self.assertRaisesRegex(ValueError, 'symlink ancestor'):
                    h.build_packet(primary, self.root/'blocks', self.root/'cost',
                                   self.root/'cost/LICENSE', destination)
                scan.assert_not_called()
                read.assert_not_called()
                mkdir.assert_not_called()
            self.assertFalse((primary/'new').exists())
            self.assertEqual((primary/core.MANIFEST).read_bytes(), manifest)
            self.assertEqual(sorted((p.relative_to(primary).as_posix(), p.stat().st_size, p.stat().st_mtime_ns)
                                    for p in primary.rglob('*') if p.is_file()), snapshot)
        finally:
            alias.unlink()
            outside.rmdir()

    def test_height_coherent_raw_hashes(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v['block']['reward_chain_block'].update(height=9175931))
        self.repair_raw(257)
        self.reject_before_core_decode('height/transaction')

    def test_header_request_coherent_metadata(self):
        self.change(self.stem(257)+'.request.json', lambda v: v['request'].update(header_hash='0x'+'00'*32))
        self.repair_request(257)
        self.reject_before_core_decode('exact fixed-origin block request')

    def test_transaction_flag_integer_not_bool(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v['block']['reward_chain_block'].update(is_transaction_block=1))
        self.repair_raw(257)
        self.reject_before_core_decode('height/transaction')

    def test_fee_relation_coherent_raw_hashes(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v['block']['transactions_info'].update(fees=1))
        self.repair_raw(257)
        self.reject_before_core_decode('record/body relations')

    def test_timestamp_relation(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v['block']['foliage_transaction_block'].update(timestamp=0))
        self.repair_raw(257)
        self.reject_before_core_decode('record/body relations')

    def test_cost_coherently_rebound_rows_totals(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v['block']['transactions_info'].__setitem__('cost', v['block']['transactions_info']['cost']+1))
        self.repair_raw(257)
        height = self.positive['rows'][-1]['height']
        self.change(f'blocks/height-{height}.json', lambda v: v.__setitem__('full_block_cost', v['full_block_cost']+1))
        changed = json.loads((self.root/f'blocks/height-{height}.json').read_bytes())
        def alter(v):
            v['observations'][-1] = changed
            v['total_observed_full_block_cost'] += 1
        self.change('blocks/coverage-and-scope.json', alter)
        self.reject_before_core_decode('captured block observation totals')

    def test_strict_rpc_success_flag(self):
        self.change(self.stem(257)+'.raw.json', lambda v: v.update(success=1))
        self.repair_raw(257)
        self.reject_before_core_decode('strict block provider success')

    def test_transport_bool_numeric_alias(self):
        self.change(self.stem(257)+'.request.json', lambda v: v.update(retries=False))
        self.repair_request(257)
        self.reject_before_core_decode('integer')

    def test_duplicate_json_key_last_body(self):
        self.replace(self.stem(257)+'.raw.json', b'{"success":true,"succ\\u0065ss":true}')
        self.repair_raw(257)
        self.reject_before_core_decode('Duplicate')

    def test_late_json_depth_before_core_decode(self):
        self.replace(self.stem(257)+'.raw.json', b'['*65+b'0'+b']'*65)
        self.repair_raw(257)
        self.reject_before_core_decode('depth')

    def test_late_executed_artifact_preflight_unchanged(self):
        name = 'core/attempt4/steps/128.stdout.json'
        self.change(name, lambda v: v['outcome'].update(output_hex=(b'\xff'*512+b'\x80'*513).hex()))
        raw = (self.root/name).read_bytes()
        def inner(manifest):
            row = next(x for x in manifest['members'] if x['path'] == name.removeprefix('core/'))
            row.update(core.ident(raw))
        self.change('core/'+core.MANIFEST, inner)
        self.reject_before_core_decode('shape')

    def test_missing_extra_and_duplicate_inventory(self):
        name = self.stem(257)+'.raw.json'
        path = self.root/name
        raw = path.read_bytes()
        path.unlink()
        try:
            self.reject_before_core_decode('missing/extraneous')
        finally:
            path.write_bytes(raw)
        extra = self.root/'extra'
        extra.write_bytes(b'')
        try:
            self.reject_before_core_decode('missing/extraneous')
        finally:
            extra.unlink()
        manifest = json.loads(self.manifest)
        manifest['members'].append(copy.deepcopy(manifest['members'][-1]))
        (self.root/core.MANIFEST).write_text(json.dumps(manifest))
        self.reject_before_core_decode('unique')

    def test_source_rule_pin_and_conditional_scope(self):
        name = 'cost/sources/src/opcodes.rs'
        raw = (self.root/name).read_bytes().replace(b'1_800_000', b'0_000_000')
        self.replace(name, raw)
        self.reject_before_core_decode('source/count observation pin')

    def test_opaque_backreference_generator_not_tree_decoded(self):
        # Deliberately not valid plain CLVM: server field stays opaque. Change
        # raw+all source-observed metadata coherently; no authentication claim.
        self.change(self.stem(257)+'.raw.json', lambda v: v['block'].update(transactions_generator='0xfe'))
        self.repair_raw(257)
        height = self.positive['rows'][-1]['height']
        self.change(f'blocks/height-{height}.json', lambda v: v.update(transaction_generator_present=True, transaction_generator_reported_bytes=1))
        row = json.loads((self.root/f'blocks/height-{height}.json').read_bytes())
        self.change('blocks/coverage-and-scope.json', lambda v: v['observations'].__setitem__(128, row))
        packet = h.Packet(self.root)
        receipt = packet.json('core/attempt4/execution-receipt.json')
        with patch.object(core, 'decode', side_effect=AssertionError('opaque generator decoded')) as decoded:
            result = blocks.verify(packet, receipt)
            self.assertEqual(len(result['rows']), 129)
            decoded.assert_not_called()

    def test_opaque_origins_never_followed(self):
        packet = h.Packet(self.root)
        original = core.read_regular
        def confined(path, *args):
            self.assertTrue(Path(path).is_relative_to(self.root))
            return original(path, *args)
        with patch.object(core, 'read_regular', confined), \
             patch.object(core, 'decode', side_effect=AssertionError('block/source validation decoded core')):
            receipt = packet.json('core/attempt4/execution-receipt.json')
            blocks.verify(packet, receipt)
            h.source_cost_observations(packet, receipt)


if __name__ == '__main__':
    unittest.main()
