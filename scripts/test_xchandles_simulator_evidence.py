"""Independent codec/resource controls and coherently rehashed actual packet controls.

Set XCHANDLES_SIMULATOR_PACKET to an admitted packet, or place it at the default
corpus directory. Missing recorded data is an error, never skipped/pass credit.
These tests execute only Python readers; no native/SDK/Cargo/VM replay.
"""
import copy
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import xchandles_simulator_evidence as e


def wire(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


class PureControls(unittest.TestCase):
    def test_coin_amount_boundaries_independent_sha(self):
        # Matches canonical SDK coin.rs sign-prefix boundaries, independently
        # specified byte strings rather than invoking the reader's encoder.
        import hashlib
        for n, encoded in [(0, b''), (127, b'\x7f'), (128, b'\0\x80'),
                           ((1 << 64) - 1, b'\0' + b'\xff' * 8)]:
            c = dict(amount=n, parent_coin_id='11' * 32, puzzle_hash='22' * 32,
                     coin_id=hashlib.sha256(b'\x11' * 32 + b'\x22' * 32 + encoded).hexdigest())
            self.assertEqual(e.coin(c), c['coin_id'])
        with self.assertRaises(ValueError):
            e.atomint(True)

    def test_duplicate_keys_at_root_nested_and_escaped(self):
        for raw in [b'{"x":1,"x":2}', b'{"a":[{"x":1,"x":2}]}', b'{"x":1,"\\u0078":1}']:
            with self.subTest(raw=raw), self.assertRaisesRegex(ValueError, 'duplicate'):
                e.json_read(raw)
        self.assertEqual(e.json_read(b'{"a":{"x":1},"b":{"x":2}}')['b']['x'], 2)

    def test_json_depth_tokens_integer_float_and_size(self):
        for raw in [b'[' * 97 + b'0' + b']' * 97, b'[0,' * 180001,
                    b'123456789012345678901', b'1.0', b'NaN', b'{} {}']:
            with self.subTest(prefix=raw[:20]), self.assertRaises(ValueError):
                e.json_read(raw)
        with self.assertRaises(ValueError):
            e.json_read(b' ' * (e.MAX_MEMBER + 1))

    def test_clvm_preflight_backref_depth_nodes_truncation_and_payload(self):
        for raw in [b'\xfe\x01', b'\xff' * 161 + b'\x80' * 162,
                    b'\xa0short', b'\x80\x80', b'']:
            with self.subTest(raw=raw[:20]), self.assertRaises(ValueError):
                e.decode(raw)
        self.assertEqual(e.decode(b'\x81\xfe'), b'\xfe')  # marker in atom payload is ordinary data
        for tree in [b'', b'\x80', (b'\xfe', b''), ((b'a', b'b'), b'c')]:
            self.assertEqual(e.decode(e.serialize(tree)), tree)

    def test_file_special_alias_refusals_and_bounded_work(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            (base / 'valid').write_bytes(b'x')
            os.symlink(base / 'valid', base / 'link')
            os.mkfifo(base / 'fifo')
            os.link(base / 'valid', base / 'hard')
            # An alarm makes an accidentally blocking FIFO implementation fail
            # this process rather than silently hanging the suite indefinitely.
            def timeout(*_):
                raise TimeoutError('reader blocked on special descriptor')
            old = signal.signal(signal.SIGALRM, timeout)
            try:
                for name in ('fifo', 'link', 'hard', 'valid'):
                    signal.setitimer(signal.ITIMER_REAL, 1)
                    with self.subTest(name=name), self.assertRaises((ValueError, OSError)):
                        e.read_file(base / name)
                    signal.setitimer(signal.ITIMER_REAL, 0)
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, old)
            with self.assertRaises(ValueError):
                e.inventory(base)
        with tempfile.TemporaryDirectory() as tmp:
            for i in range(129):
                (Path(tmp) / str(i)).mkdir()
            with self.assertRaises(ValueError):
                e.inventory(Path(tmp))

    def test_precharged_descriptor_size_checked_before_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'changed'; p.write_bytes(b'larger')
            with patch.object(os, 'read', side_effect=AssertionError('read before reservation check')):
                with self.assertRaisesRegex(ValueError, 'precharged size'):
                    e.read_file(p, expected_size=1)

    def test_closed_paths(self):
        for name in ['../x', '/x', 'x//y', 'x\\y', 'é', 'x:y', './x', 'x/../y']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                e.path_name(name)


class RecordedControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.packet = Path(os.environ.get('XCHANDLES_SIMULATOR_PACKET',
                          str(Path(__file__).resolve().parents[1] / 'corpus/xchandles-simulator')))
        if not cls.packet.is_dir():
            raise FileNotFoundError('Missing actual recorded native pilot packet: ' + str(cls.packet))
        cls.index, cls.data, cls.index_raw = e.load_packet(cls.packet)
        cls.report = e.json_read(cls.data['capture/full-first.stdout'])
        cls.baseline = e.validate(cls.packet)

    def write_packet(self, data, index=None):
        base = Path(self.tmp.name)
        for name, raw in data.items():
            dest = base / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
        idx = copy.deepcopy(index or self.index)
        idx['files'] = [e.descriptor(name, raw) for name, raw in sorted(data.items())]
        (base / 'report.json').write_bytes(wire(idx))
        return base

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def changed_report(self, change):
        data = self.data.copy()
        report = copy.deepcopy(self.report)
        change(report)
        raw = wire(report)
        for role in ('full-first', 'full-repeat', 'resume-first'):
            data['capture/' + role + '.stdout'] = raw
        pause = dict(report, status='paused_after_validated_step', registration_steps=report['registration_steps'][:1],
                     transactions=report['transactions'][:7], component_vm_runs=20, sdk_transaction_attempts=7)
        data['capture/pause-first.stdout'] = wire(pause)
        # Refresh outer final-receipt and actual stream digests. A semantic
        # rejection MUST NOT be an unchanged original report hash mismatch.
        receipt = e.json_read(data['capture/final-receipt.json'])
        receipt['full_report_sha256'] = e.sha(raw)
        data['capture/final-receipt.json'] = wire(receipt)
        for role in ('full-first', 'full-repeat', 'resume-first', 'pause-first'):
            command = e.json_read(data['capture/' + role + '.command.json'])
            content = data['capture/' + role + '.stdout']
            command['stdout'].update(bytes=len(content), sha256=e.sha(content))
            data['capture/' + role + '.command.json'] = wire(command)
        return data

    def rejects(self, data, pattern=None, index=None):
        base = self.write_packet(data, index)
        with self.assertRaisesRegex(ValueError, pattern or '.'):
            e.validate(base)

    def test_actual_baseline_runs_without_subprocess_network_vm(self):
        with (patch.object(subprocess, 'run', side_effect=AssertionError('no process')),
             patch.object(subprocess, 'Popen', side_effect=AssertionError('no process')),
             patch.object(socket, 'socket', side_effect=AssertionError('no network'))):
            result = e.validate(self.packet)
        self.assertEqual(result['summary']['sdk_transaction_attempts'], 20)
        self.assertEqual(result['summary']['component_vm_runs'], 48)
        self.assertEqual(result['summary']['plain_generator_cost_passes'], 11)
        self.assertEqual(len(result['files']), 81)

    def test_protocol_unknown_and_inventory_corruption(self):
        idx = copy.deepcopy(self.index); idx['unexpected'] = True
        self.rejects(self.data, 'closed object', idx)

    def test_bool_not_integer_count(self):
        self.rejects(self.changed_report(lambda r: r.update(sdk_transaction_attempts=True)), 'strict bounded integer')

    def test_exact_consensus_code_not_resource(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][4].update(code='CostExceeded')), 'exact typed SDK negative')

    def test_runtime_umbrella_cannot_be_semantic_credit(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][3].update(semantic_consensus_rejection_credit=True)), 'runtime umbrella')

    def test_component_failure_resource_is_inconclusive(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][3]['component_vm'][0]['failure'].update(kind='resource', code='cost_exceeded')), 'exact recorded program failure')

    def test_reveal_coherent_coin_change_breaks_actual_identity(self):
        def change(r):
            s = r['transactions'][0]['spends'][0]
            s['reveal_hex'] = '80'
            s['coin']['puzzle_hash'] = e.th(b'').hex()
            s['coin']['coin_id'] = e.sha(bytes.fromhex(s['coin']['parent_coin_id']) + e.th(b'') + e.atomint(s['coin']['amount']))
            r['transactions'][0]['input_coin_ids'][0] = s['coin']['coin_id']
            r['transactions'][0]['component_vm'][0]['coin'] = copy.deepcopy(s['coin'])
        self.rejects(self.changed_report(change))

    def test_program_hash_raw_tamper(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][0]['spends'][0].update(reveal_hex='80')), 'reveal hash')

    def test_output_child_coherent_fee_metric_tamper(self):
        def change(r):
            step = r['registration_steps'][0]
            step['complete_registry_children'].pop()
        self.rejects(self.changed_report(change), 'complete ephemeral')

    def test_creator_coherent_coin_id_breaks_curry(self):
        def change(r):
            slot = r['registration_steps'][0]['surviving_slots'][0]
            c = slot['creator_registry_coin']
            c['puzzle_hash'] = '01' * 32
            c['coin_id'] = e.sha(bytes.fromhex(c['parent_coin_id'] + c['puzzle_hash']) + e.atomint(c['amount']))
            slot['coin']['parent_coin_id'] = c['coin_id']
            slot['coin']['coin_id'] = e.sha(bytes.fromhex(c['coin_id'] + slot['coin']['puzzle_hash']))
        self.rejects(self.changed_report(change), 'creator lineage')

    def test_neighbor_coherent_slot_hash_breaks_actual_creation(self):
        self.rejects(self.changed_report(lambda r: r['registration_steps'][0]['surviving_slots'][1].update(left_hash='01' * 32)), 'slot value hash')

    def test_current_controller_exact_relation(self):
        self.rejects(self.changed_report(lambda r: r['registration_steps'][1].update(previous_registry=r['registration_steps'][0]['previous_registry'])), 'controller spend/current')

    def test_state_root_coherent_hash_still_bound_to_reveal(self):
        def change(r):
            for step in r['registration_steps']:
                s = step['registry_state']; s['pricing_puzzle_hash'] = '01' * 32
                step['registry_state_tree_hash'] = e.th((bytes.fromhex(s['cat_maker_puzzle_hash']),
                    (bytes.fromhex(s['pricing_puzzle_hash']), bytes.fromhex(s['expired_handle_pricing_puzzle_hash'])))).hex()
        self.rejects(self.changed_report(change), 'state commitment')

    def test_generator_size_and_coherent_byte_cost_tamper(self):
        def change(r):
            m = r['transactions'][0]['spendbundle_metrics']
            m['plain_generator_bytes'] += 1; m['charged_generator_bytes'] += 1
            m['byte_cost'] += 12000; m['total_cost'] += 12000
        self.rejects(self.changed_report(change), 'generator bytes')

    def test_fee_conservation(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][5]['spendbundle_metrics'].update(fee_mojos=0)), 'conservation/fee')

    def test_no_bls_profile(self):
        self.rejects(self.changed_report(lambda r: r['transactions'][0]['spendbundle_metrics'].update(cost_pass_validates_signature=True)), 'no-BLS')

    def test_instruction_cost_coherent_total_is_recorded_profile(self):
        def change(r):
            r['transactions'][0]['component_vm'][0]['cost'] += 1
            m = r['transactions'][0]['spendbundle_metrics']; m['execution_cost'] += 1; m['total_cost'] += 1
        self.rejects(self.changed_report(change), 'recorded component cost profile')

    def test_repeat_bytes_not_merely_equal_json(self):
        data = self.data.copy(); data['capture/full-repeat.stdout'] += b'\n'
        self.rejects(data, 'exact full repeat')

    def test_source_fingerprint_not_receipt_hash_only(self):
        data = self.data.copy(); data['source/src/main.rs'] += b'\n'
        receipt = e.json_read(data['capture/final-receipt.json'])
        receipt['source_files'] = sorted([e.descriptor(e.PACKAGE + n, data['source/' + n]) for n in e.PACKAGE_NAMES], key=lambda x:x['path'])
        data['capture/final-receipt.json'] = wire(receipt)
        self.rejects(data, 'source binding')

    def test_unreviewed_lock_tuple(self):
        data = self.data.copy(); data['source/Cargo.lock'] = data['source/Cargo.lock'].replace(b'version = "0.36.0"', b'version = "0.36.9"', 1)
        self.rejects(data, 'dependency tuple')

    def test_command_argv_exit_and_log_relation(self):
        data = self.data.copy(); c = e.json_read(data['capture/full-first.command.json'])
        c['argv'] += ['--stop-after', '1']; data['capture/full-first.command.json'] = wire(c)
        self.rejects(data, 'full run argv')

    def test_failure_process_cannot_claim_timeout(self):
        data = self.data.copy(); c = e.json_read(data['capture/refuse-fifo.command.json'])
        c['failure_kind'] = 'timeout'; data['capture/refuse-fifo.command.json'] = wire(c)
        self.rejects(data, 'typed process completion')

    def test_unknown_journal_control_cannot_be_arbitrary_data(self):
        data = self.data.copy(); data['capture/checkpoint-unknown-field.json'] = wire(dict(e.json_read(data['capture/pause-first.stdout']), other=True))
        self.rejects(data, 'unknown whole-prefix')

    def test_filesystem_extra_symlink_hardlink(self):
        base = self.write_packet(self.data)
        (base / 'extra').write_bytes(b'')
        with self.assertRaises(ValueError): e.validate(base)
        (base / 'extra').unlink(); os.symlink(base / 'report.json', base / 'extra')
        with self.assertRaises(ValueError): e.validate(base)
        (base / 'extra').unlink(); os.link(base / 'report.json', base / 'extra')
        with self.assertRaises(ValueError): e.validate(base)


    def test_future_neighbor_version_with_same_handle_is_unavailable(self):
        def change(r):
            r['registration_steps'][1]['neighbors'][0]['left'] = copy.deepcopy(r['registration_steps'][2]['surviving_slots'][0])
        self.rejects(self.changed_report(change), 'currently available action slot version')

    def test_consumed_neighbor_version_with_same_handle_is_unavailable(self):
        def change(r):
            r['registration_steps'][1]['neighbors'][0]['left'] = copy.deepcopy(r['registration_steps'][0]['neighbors'][0]['left'])
        self.rejects(self.changed_report(change), 'currently available action slot version')

    def test_later_current_transaction_ephemeral_neighbor_is_not_yet_available(self):
        def change(r):
            r['registration_steps'][1]['neighbors'][0]['left'] = copy.deepcopy(r['registration_steps'][1]['neighbors'][1]['left'])
        self.rejects(self.changed_report(change), 'currently available action slot version')

    def solution_mutation(self, transform):
        # Change BOTH valid actual singleton solution and wrong-neighbor witness
        # in the same field. Thus the exact two-neighbor diff remains valid.
        def change(r):
            for txindex, key in ((5, 'valid'), (3, 'witness')):
                target = r['transactions'][txindex]
                if key == 'valid':
                    row = target['spends'][-1]
                else:
                    row = target['component_vm'][0]
                tree = e.decode(e.blob(row['solution_hex']))
                row['solution_hex'] = e.serialize(transform(tree)).hex()
        return self.changed_report(change)

    def test_bad_selector_proof_with_matching_wrong_neighbor_witness(self):
        def transform(tree):
            outer = e.proper(tree); raw = outer[2]
            # raw = (puzzles selectors/proofs solutions . finalizer)
            selector = raw[1][0]
            first, tail = selector
            selector = ((first[0], (b'\x04', first[1][1])), tail)
            outer[2] = (raw[0], (selector, raw[1][1]))
            return e.list_tree(outer)
        self.rejects(self.solution_mutation(transform), 'selector/proof')

    def test_bad_precommit_curry_arg_with_matching_wrong_neighbor_witness(self):
        def transform(tree):
            outer = e.proper(tree); raw = outer[2]
            puzzle = e.proper(raw[0])[0]; module, args = e.uncurry(puzzle)
            args[2] = b'\x01' * 32
            env = b'\1'
            for arg in reversed(args): env = e.list_tree([b'\4', (b'\1', arg), env])
            puzzle = e.list_tree([b'\2', (b'\1', module), env])
            outer[2] = (e.list_tree([puzzle]), raw[1])
            return e.list_tree(outer)
        self.rejects(self.solution_mutation(transform), 'first-curry arguments')

    def test_bad_slot_curry_arg_with_matching_wrong_neighbor_witness(self):
        def transform(tree):
            outer = e.proper(tree); raw = outer[2]
            module, args = e.uncurry(e.proper(raw[0])[0]); args[3] = b'\x02' * 32
            env = b'\1'
            for arg in reversed(args): env = e.list_tree([b'\4', (b'\1', arg), env])
            outer[2] = (e.list_tree([e.list_tree([b'\2', (b'\1', module), env])]), raw[1])
            return e.list_tree(outer)
        self.rejects(self.solution_mutation(transform), 'first-curry arguments')

    def test_complete_root_finalizer_derivation_uses_all_eight_actions(self):
        # A focused actual-data check exercises root/finalizer validation after
        # reveal hashing (whose generic whole-packet check is tested separately).
        r = copy.deepcopy(self.report)
        t = next(t for t in r['transactions'] if t['name'] == 'register_handle')
        module, args = e.uncurry(e.decode(e.blob(t['spends'][-1]['reveal_hex'])))
        action, action_args = e.uncurry(args[1])
        mods = {m['name']: bytes.fromhex(m['tree_hash']) for m in e.MODULES}
        launcher = args[0][1][0]
        launch_tx = next(t for t in r['transactions'] if t['name'] == 'launch_registry_price_and_nft')
        price = e.price_launcher_from_launch(launch_tx, mods)
        _, root, (path, proof), finalizer = e.allowed_actions(mods, launcher, price)
        self.assertEqual(action_args[1], root)
        self.assertEqual(e.th(action_args[0]), finalizer)
        self.assertEqual(path, 3); self.assertEqual(len(proof), 3)
        outer = e.proper(e.decode(e.blob(t['spends'][-1]['solution_hex'])))
        raw = outer[2]; puzzles, tail = raw; selectors, tail = tail; _, finalizer_solution = tail
        register_args = e.uncurry(e.proper(puzzles)[0])[1]
        e.check_action_commitments(action_args, register_args, selectors, finalizer_solution, mods, launcher, price, 0)
        for which in (0, 1):
            changed = list(action_args)
            changed[which] = b'\x01' * 32
            with self.subTest(commitment=which), self.assertRaisesRegex(ValueError, 'root/finalizer'):
                e.check_action_commitments(changed, register_args, selectors, finalizer_solution, mods, launcher, price, 0)
        with self.assertRaisesRegex(ValueError, 'nil finalizer solution'):
            e.check_action_commitments(action_args, register_args, selectors, b'\1', mods, launcher, price, 0)

        # Every allowed sibling action must contribute to the closed root.
        for name in e.ACTION_MODULES:
            changed = dict(e.ACTION_MODULES); changed[name] = '01' * 32
            with patch.object(e, 'ACTION_MODULES', changed):
                self.assertNotEqual(e.allowed_actions(mods, launcher, price)[1], root)
        self.assertNotEqual(e.allowed_actions(mods, launcher, b'\x01' * 32)[1], root)

    def test_injected_dependency_in_both_locks_does_not_extend_allowlist(self):
        data = self.data.copy()
        row = b'\n[[package]]\nname = "unreviewed"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "' + b'00' * 32 + b'"\n'
        for name in ('source/Cargo.lock', 'reference/root-Cargo.lock'): data[name] += row
        self.rejects(data, 'baseline lock identity')

    def test_direct_path_replacement_is_not_version_equivalence(self):
        data = self.data.copy()
        data['source/Cargo.toml'] = data['source/Cargo.toml'].replace(b'anyhow = "=1.0.104"', b'anyhow = {version = "=1.0.104", path = "/tmp/unreviewed"}')
        self.rejects(data, 'closed reviewed direct dependencies')

    def test_direct_feature_broadening_is_rejected(self):
        data = self.data.copy()
        data['source/Cargo.toml'] = data['source/Cargo.toml'].replace(b'features = ["action-layer"]', b'features = ["action-layer", "unreviewed"]', 1)
        self.rejects(data, 'closed reviewed direct dependencies')

    def test_integer_cannot_replace_boolean_direct_feature_flag(self):
        data = self.data.copy()
        data['source/Cargo.toml'] = data['source/Cargo.toml'].replace(b'default-features = false', b'default-features = 0')
        self.rejects(data, 'closed reviewed direct dependencies')


    def test_named_duplicate_controls_cannot_be_malformed_substitutions(self):
        for name in ('duplicate-root', 'duplicate-nested', 'duplicate-escaped-nested'):
            with self.subTest(name=name):
                data = self.data.copy(); data['capture/checkpoint-' + name + '.json'] = b'{'
                stderr = data['capture/refuse-malformed.stderr']
                data['capture/refuse-' + name + '.stderr'] = stderr
                command = e.json_read(data['capture/refuse-' + name + '.command.json'])
                command['stderr'].update(bytes=len(stderr), sha256=e.sha(stderr))
                data['capture/refuse-' + name + '.command.json'] = wire(command)
                base = self.write_packet(data)
                with self.assertRaisesRegex(ValueError, 'exact named JSON refusal fixture'):
                    e.validate(base)


    def test_archive_known_identity(self):
        data = self.data.copy(); data['reference/driver.crate'] += b'\0'
        self.rejects(data, 'official driver crate pin')


if __name__ == '__main__':
    unittest.main()
