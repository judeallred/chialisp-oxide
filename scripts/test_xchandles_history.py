"""Pure controls. Set XCHANDLES_HISTORY_PACKET for real retained-data controls.

The explicit packet is copied into a disposable directory; original research and
packet bytes are never modified. No VM, SDK, network, Cargo or driver execution.
"""
import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import xchandles_history as h


class BoundedControls(unittest.TestCase):
    def test_json_duplicate_escaped_alias_and_numeric_domains(self):
        for raw in (b'{"cost":1,"co\\u0073t":2}', b'{"elapsed_seconds":NaN}',
                    b'{"cost":1.5}', b'{"a":'+b'['*65+b'0'+b']'*65+b'}'):
            with self.subTest(raw=raw[:50]), self.assertRaises(ValueError):
                h.json_value(raw)
        self.assertEqual(h.json_value(b'{"elapsed_seconds":0.5}'), {'elapsed_seconds': 0.5})
        for value in (True, False, -1, 2**64):
            with self.assertRaises(ValueError):
                h.integer(value)

    def test_tree_preflight_before_decode(self):
        for raw in (b'\xff'*512+b'\x80'*513, b'\xff\x80', b'\x81\x01', b'\xfe', b'\x80\x80'):
            with self.subTest(raw=raw[:10]), self.assertRaises(ValueError):
                h.decode(raw)
        self.assertEqual(h.decode(b'\xff\x81\x80\x00'), (b'\x80', b'\x00'))
        self.assertEqual(h.assembly(b'\xff\x81\x80\x00'), '(0x80 . 0x00)')

    def test_coin_canonical_uint64_and_bool(self):
        coin = dict(parent_coin_info='0x'+'11'*32, puzzle_hash='0x'+'22'*32, amount=128)
        self.assertEqual(h.coin_id(coin), h.sha(bytes.fromhex('11'*32+'22'*32+'0080')))
        for amount in (True, -1, 2**64, '1'):
            with self.assertRaises(ValueError):
                h.coin_id(dict(coin, amount=amount))

    def test_manifest_precharge_opens_no_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            rows = [dict(path='inputs/premine.csv', role='pinned_input', bytes=h.MAX_MEMBER+1, sha256='0'*64)]
            (root/h.MANIFEST).write_text(json.dumps(dict(protocol='chialisp-oxide-history-packet/v1', profile=h.PROFILE, members=rows)))
            original = h.read_regular
            opened = []
            def monitored(path, *args):
                opened.append(Path(path).name)
                return original(path, *args)
            with patch.object(h, 'read_regular', monitored), self.assertRaises(ValueError):
                h.Packet(root)
            self.assertEqual(opened, [h.MANIFEST])


    def test_aggregate_and_count_precharge_before_payload_opens(self):
        for count, size in ((64, h.MAX_MEMBER), (2048, 0)):
            with self.subTest(count=count), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp).resolve()
                rows = [dict(path=f'attempt4/m{i}', role='original_execution', bytes=size, sha256='0'*64)
                        for i in range(count)]
                (root/h.MANIFEST).write_text(json.dumps(dict(protocol='chialisp-oxide-history-packet/v1', profile=h.PROFILE, members=rows)))
                original = h.read_regular
                opened = []
                def monitored(path, *args):
                    opened.append(Path(path).name)
                    return original(path, *args)
                with patch.object(h, 'read_regular', monitored), self.assertRaises(ValueError):
                    h.Packet(root)
                self.assertEqual(opened, [h.MANIFEST])

    def test_paths_and_tree_resource_limits_remain_closed(self):
        for name in ('/etc/passwd', '../x', 'attempt4/../x', 'attempt4//x', 'attempt4\\x'):
            with self.assertRaises(ValueError):
                h.relative(name)
        with patch.object(h, 'inspect_compiled_artifact', return_value={
                'stats': dict(nodes=100000, atom_payload_bytes=0)}), self.assertRaises(ValueError):
            h.decode(b'\x80')


@unittest.skipUnless(os.environ.get('XCHANDLES_HISTORY_PACKET'),
                     'Explicit retained packet required; set XCHANDLES_HISTORY_PACKET')
class RetainedControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = Path(os.environ['XCHANDLES_HISTORY_PACKET'])
        # Positive admission is always executed before mutation controls.
        cls.positive = h.verify(cls.source)
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name).resolve()/'packet'
        shutil.copytree(cls.source, cls.root)
        cls.manifest = (cls.root/h.MANIFEST).read_bytes()

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.changed = {}

    def tearDown(self):
        for name, raw in self.changed.items():
            (self.root/name).write_bytes(raw)
        (self.root/h.MANIFEST).write_bytes(self.manifest)

    def replace(self, name, raw):
        path = self.root/name
        if name not in self.changed:
            self.changed[name] = path.read_bytes()
        path.write_bytes(raw)
        manifest = json.loads((self.root/h.MANIFEST).read_bytes())
        row = next(x for x in manifest['members'] if x['path'] == name)
        row.update(h.ident(raw))
        (self.root/h.MANIFEST).write_text(json.dumps(manifest))

    def change_json(self, name, function):
        value = json.loads((self.root/name).read_bytes())
        function(value)
        self.replace(name, json.dumps(value).encode())

    def rejects(self):
        with self.assertRaises((ValueError, KeyError, TypeError)):
            h.verify(self.root)

    def test_positive_scope_and_counts(self):
        r = self.positive
        self.assertEqual((r['primary_component_observations'], r['new_reader_executions'],
                          r['exact_csv_handle_names'], r['original_execution_files']), (129, 0, 3836, 1026))
        self.assertIn('recipient/NFT/expiration agreement', r['scope'])
        self.assertEqual(r['earlier_unretained_uncredited_descriptors'], 20)
        self.assertFalse(r['bootstrap_child_lists_complete'])
        self.assertEqual(r['primary_cost_sum'], 5302597203)

    def test_missing_and_extra_payloads(self):
        path = self.root/'attempt4/steps/000.stderr.log'
        raw = path.read_bytes()
        path.unlink()
        try:
            self.rejects()
        finally:
            path.write_bytes(raw)
        extra = self.root/'extra'
        extra.write_bytes(b'')
        try:
            self.rejects()
        finally:
            extra.unlink()

    def test_symlink_hardlink_and_fifo_rejected(self):
        path = self.root/'attempt4/steps/000.stderr.log'
        raw = path.read_bytes()
        for kind in ('symlink', 'hardlink', 'fifo'):
            path.unlink()
            try:
                if kind == 'symlink':
                    path.symlink_to(self.root/'inputs/premine.csv')
                elif kind == 'hardlink':
                    os.link(self.root/'inputs/premine.csv', path)
                elif hasattr(os, 'mkfifo'):
                    os.mkfifo(path)
                else:
                    continue
                self.rejects()
            finally:
                path.unlink(missing_ok=True)
                path.write_bytes(raw)

    def test_duplicate_keys_rehashed_outer_inventory(self):
        self.replace('attempt4/steps/000.stdout.json', b'{"cost":1,"co\\u0073t":2}')
        self.rejects()

    def test_bool_count_not_equal_integer(self):
        self.change_json('attempt4/execution-receipt.json', lambda r: r['steps'][0].update(action_count=True))
        self.rejects()

    def test_swapped_raw_spend_bodies(self):
        a, b = 'attempt2/08-get_puzzle_and_solution.json', 'attempt2/11-get_puzzle_and_solution.json'
        ar, br = (self.root/a).read_bytes(), (self.root/b).read_bytes()
        self.replace(a, br)
        self.replace(b, ar)
        # Rehash outer dependency declarations too; coin/input relations must fail.
        def update_index(index):
            for row in index['requests']:
                if 'attempt2/'+row['path'] in (a, b):
                    row.update(h.ident((self.root/'attempt2'/row['path']).read_bytes()))
        self.change_json('attempt2/trace-receipt.json', update_index)
        def update(deps):
            for row in deps['files']:
                if row['path'] in (a, b):
                    row.update(h.ident((self.root/row['path']).read_bytes()))
            deps['index_receipt'].update(h.ident((self.root/'attempt2/trace-receipt.json').read_bytes()))
        self.change_json('attempt5/prefix-dependencies.json', update)
        self.rejects()

    def test_success_failure_null_and_boolean_cost(self):
        name = 'attempt4/steps/000.stdout.json'
        self.change_json(name, lambda v: v.update(failure=None))
        self.rejects()

    def test_boolean_cost_in_actual_stdout(self):
        self.change_json('attempt4/steps/000.stdout.json', lambda v: v.update(cost=True))
        self.rejects()

    def test_command_input_drift(self):
        self.change_json('attempt4/steps/000.command.json', lambda v: v.update(input_environment_hex='80'))
        self.rejects()

    def test_output_swap_rehashed_logs(self):
        name = 'attempt4/steps/000.stdout.json'
        self.replace(name, (self.root/'attempt4/steps/001.stdout.json').read_bytes())
        self.change_json('attempt4/steps/000.result-meta.json', lambda v: v.update(stdout_sha256=h.sha((self.root/name).read_bytes())))
        self.rejects()

    def test_bootstrap_record_amount_bool(self):
        self.change_json('attempt2/01-get_coin_record_by_name.json', lambda v: v['coin_record']['coin'].update(amount=True))
        self.rejects()


    def test_coherent_first_controller_creation_height(self):
        # Rebind all actual raw observations, receipts, checkpoints and nested
        # byte identities. This must reach the independent bootstrap edge check,
        # rather than fail only an outer inventory hash or equality comparison.
        def change_records(value):
            pending = [value]
            while pending:
                item = pending.pop()
                if type(item) is dict:
                    if type(item.get('coin')) is dict and 'confirmed_block_index' in item:
                        if h.coin_id(item['coin']) == h.FIRST:
                            item['confirmed_block_index'] = 0
                    pending.extend(item.values())
                elif type(item) is list:
                    pending.extend(item)
        raw_names = ('attempt2/06-get_coin_records_by_parent_ids.json',
                     'attempt2/07-get_coin_record_by_name.json')
        for name in raw_names:
            self.change_json(name, change_records)
        def change_index(index):
            change_records(index)
            for row in index['requests']:
                name = 'attempt2/'+row['path']
                if name in raw_names:
                    row.update(h.ident((self.root/name).read_bytes()))
        self.change_json('attempt2/trace-receipt.json', change_index)
        self.change_json('attempt4/execution-receipt.json', change_records)
        self.change_json('attempt4/six-prefix-states.json', change_records)
        def change_deps(deps):
            for row in deps['files']:
                if row['path'] in raw_names:
                    row.update(h.ident((self.root/row['path']).read_bytes()))
            deps['index_receipt'].update(h.ident((self.root/'attempt2/trace-receipt.json').read_bytes()))
        self.change_json('attempt5/prefix-dependencies.json', change_deps)
        self.change_json('attempt5/terminal-observation.json', lambda v:
                         v['attempt4_receipt'].update(h.ident((self.root/'attempt4/execution-receipt.json').read_bytes())))
        with self.assertRaisesRegex(ValueError, 'bootstrap creation-height edge'):
            h.verify(self.root)

    def test_terminal_creation_height(self):
        self.change_json('attempt5/coin-record.raw.json', lambda v: v['coin_record'].update(confirmed_block_index=9175931))
        def coherent(observation):
            observation['record']['confirmed_block_index'] = 9175931
            observation['raw_response'].update(h.ident((self.root/'attempt5/coin-record.raw.json').read_bytes()))
        self.change_json('attempt5/terminal-observation.json', coherent)
        self.rejects()

    def test_csv_recipient_byte_drift_invalidates_identity(self):
        raw = (self.root/'inputs/premine.csv').read_bytes()
        # Same handle names; changing a non-name column still breaks exact CSV pin.
        start = raw.index(b'\n')+1
        end = raw.index(b'\n', start)
        line = raw[start:end]
        pos = line.rfind(b',')+1
        changed = line[:pos]+(b'0' if line[pos:pos+1] != b'0' else b'1')+line[pos+1:]
        self.replace('inputs/premine.csv', raw[:start]+changed+raw[end:])
        self.rejects()

    def test_opaque_origin_not_followed(self):
        # Existing recorded absolute roots are not available in a portable copy.
        # All successful reads must stay under explicitly supplied packet root.
        original = h.read_regular
        def confined(path, *args):
            self.assertTrue(Path(path).is_relative_to(self.root))
            return original(path, *args)
        with patch.object(h, 'read_regular', confined):
            self.assertEqual(h.verify(self.root)['new_reader_executions'], 0)

    def test_actual_selector_and_finalizer_controls(self):
        packet = h.Packet(self.source)
        step = packet.json('attempt4/execution-receipt.json')['steps'][0]
        spend = packet.json(step['source_spend_response'])['coin_solution']
        program, solution = h.unhex(spend['puzzle_reveal']), h.unhex(spend['solution'])
        register = packet.raw('inputs/register.clvm')
        h.check_actions(program, solution, step, register)
        sol = h.fields(h.decode(solution), 3)
        inner, tail = h.take(sol[2], 3)
        selectors = h.fields(inner[1])
        for bad_selector in ((b'\3', selectors[0][1]), (b'\2', b'')):
            mutated = list(inner)
            mutated[1] = (bad_selector, b'')
            tree = (sol[0], (sol[1], ((mutated[0], (mutated[1], (mutated[2], tail))), b'')))
            with self.assertRaises(ValueError):
                h.check_actions(program, h.encode(tree), step, register)
        # Change the final proof sibling while retaining reviewed selector shape.
        proof = h.fields(selectors[0][1]);proof[-1] = b'\0'*32
        proper = lambda xs: self.proper(xs)
        mutated = list(inner);mutated[1] = ((b'\2', proper(proof)), b'')
        tree = proper([sol[0], sol[1], (mutated[0], (mutated[1], (mutated[2], tail)))])
        with self.assertRaises(ValueError):
            h.check_actions(program, h.encode(tree), step, register)
        # A nonnil finalizer solution is outside the actual closed profile.
        tree = proper([sol[0], sol[1], (inner[0], (inner[1], (inner[2], b'\1')))])
        with self.assertRaises(ValueError):
            h.check_actions(program, h.encode(tree), step, register)




    def repair_prefix_observation_hashes(self, name):
        def change_index(index):
            for row in index['requests']:
                if 'attempt2/'+row['path'] == name:
                    row.update(h.ident((self.root/name).read_bytes()))
        self.change_json('attempt2/trace-receipt.json', change_index)
        def change_deps(deps):
            for row in deps['files']:
                if row['path'] == name:
                    row.update(h.ident((self.root/name).read_bytes()))
            deps['index_receipt'].update(h.ident((self.root/'attempt2/trace-receipt.json').read_bytes()))
        self.change_json('attempt5/prefix-dependencies.json', change_deps)

    def test_chosen_bootstrap_candidate_bool_int(self):
        name = 'attempt2/06-get_coin_records_by_parent_ids.json'
        original = (self.root/name).read_bytes()
        for key, wrong in (('coinbase', 0), ('spent', 1)):
            self.replace(name, original)
            def mutate(value):
                chosen = next(x for x in value['coin_records'] if h.coin_id(x['coin']) == h.FIRST)
                chosen[key] = wrong
            self.change_json(name, mutate)
            self.repair_prefix_observation_hashes(name)
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'record bool flags'):
                h.verify(self.root)

    def test_older_network_success_integer_not_bool(self):
        name = 'attempt2/00-get_network_info.json'
        self.change_json(name, lambda v: v.update(success=1))
        self.repair_prefix_observation_hashes(name)
        with self.assertRaisesRegex(ValueError, 'strict mainnet/xch'):
            h.verify(self.root)

    def test_new_network_success_integer_not_bool(self):
        name = 'attempt4/http/000-get_network_info.json'
        self.change_json(name, lambda v: v.update(success=1))
        self.change_json('attempt4/execution-receipt.json', lambda v:
                         v['http_requests'][0].update(h.ident((self.root/name).read_bytes())))
        self.change_json('attempt5/terminal-observation.json', lambda v:
                         v['attempt4_receipt'].update(h.ident((self.root/'attempt4/execution-receipt.json').read_bytes())))
        with self.assertRaisesRegex(ValueError, 'strict mainnet/xch'):
            h.verify(self.root)

    def test_last_input_assembly_limit_before_any_tree_decode(self):
        # Balanced finite canonical tree fits bytes/nodes/depth/work but exceeds
        # only the existing assembly accounting bound: 276475 >240000 bytes.
        tree = b'a'*64
        for _ in range(11):
            tree = (tree, tree)
        raw = h.encode(tree)
        self.replace('attempt4/steps/128.solution.hex', raw.hex().encode()+b'\n')
        with patch.object(h, 'decode', side_effect=AssertionError('selected decode ran')) as decoded:
            with self.assertRaisesRegex(ValueError, 'assembly precharge'):
                h.verify(self.root)
            decoded.assert_not_called()

    def test_last_artifact_shape_rejected_before_any_tree_decode(self):
        name = 'attempt4/steps/128.stdout.json'
        original = (self.root/name).read_bytes()
        for kind, malformed in (('depth', b'\xff'*512+b'\x80'*513),
                                ('trailing', b'\x80\x80'), ('truncated', b'\xff\x80')):
            self.replace(name, original)
            self.change_json(name, lambda value: value['outcome'].update(output_hex=malformed.hex()))
            with self.subTest(kind=kind), patch.object(h, 'decode', side_effect=AssertionError('selected decode ran')) as decoded:
                with self.assertRaises(ValueError):
                    h.verify(self.root)
                decoded.assert_not_called()

    def test_last_input_work_precharge_before_any_tree_decode(self):
        original_inspect = h.inspect_compiled_artifact
        target = (self.root/'attempt4/steps/128.solution.hex').read_bytes().strip().decode()
        target_raw = bytes.fromhex(target)
        def limited(raw, **kwargs):
            value = original_inspect(raw, **kwargs)
            if raw == target_raw:
                value = copy.deepcopy(value)
                value['stats']['nodes'] = 100000
            return value
        with patch.object(h, 'inspect_compiled_artifact', limited), \
             patch.object(h, 'decode', side_effect=AssertionError('selected decode ran')) as decoded:
            with self.assertRaisesRegex(ValueError, 'modeled work bound'):
                h.verify(self.root)
            decoded.assert_not_called()

    def test_actual_output_assertion_amount_and_successor_controls(self):
        packet = h.Packet(self.source)
        step = packet.json('attempt4/execution-receipt.json')['steps'][0]
        raw = h.unhex(packet.json('attempt4/steps/000.stdout.json')['outcome']['output_hex'])
        h.output_successor(raw, step['record'], step['coin_id'], 1)
        conditions = [h.fields(c, 32) for c in h.fields(h.decode(raw))]
        for kind in ('parent', 'padded_one', 'extra_one', 'missing_one'):
            changed = copy.deepcopy(conditions)
            if kind == 'parent':
                next(c for c in changed if c[0] == b'G')[1] = b'\0'*32
            elif kind == 'padded_one':
                next(c for c in changed if c[0] == b'3' and c[2] == b'\1')[2] = b'\0\1'
            elif kind == 'extra_one':
                changed.append(copy.deepcopy(next(c for c in changed if c[0] == b'3' and c[2] == b'\1')))
            else:
                changed = [c for c in changed if not (c[0] == b'3' and c[2] == b'\1')]
            encoded = h.encode(self.proper([self.proper(c) for c in changed]))
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                h.output_successor(encoded, step['record'], step['coin_id'], 1)

    @staticmethod
    def proper(xs):
        tree = b''
        for x in reversed(xs):
            tree = (x, tree)
        return tree


if __name__ == '__main__':
    unittest.main()
