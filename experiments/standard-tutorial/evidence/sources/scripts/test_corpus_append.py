"""Pure constructed corpus-append schema/order/restoration controls; no capture credit.

The retained input is the exact delegated-conditions era checkpoint. Added rows
in constructed reports are explicit minimal values, never fabricated native/VM
observations. These controls exercise pure algorithms only.
"""
import copy
import json
import unittest
from unittest.mock import patch

import corpus_append as migration
import runtime_cost_migration as cost


def pending_profile():
    """Test-only pending copy of the on-disk profile; never grants admission."""
    value = copy.deepcopy(migration.load())
    value['status'] = migration.PENDING
    value['reviewed_after'] = None
    if value['compiler_transition'] is not None:
        # A pending capture carries no measured transition; `record` fills it.
        value['compiler_transition']['metrics'] = None
    migration.validate_profile(value)
    return value


def constructed(before, profile):
    cost.require(profile['status'] == migration.PENDING and profile['reviewed_after'] is None,
                 'Constructed metadata requires an explicit test-only pending profile')
    current = copy.deepcopy(before)
    transition = profile['source_transition']
    for row in profile['added']:
        current['authoring']['exports'][row['slug']] = dict(entry=row['entry'], source=row['source'])
    current['authoring']['exports'] = dict(sorted(current['authoring']['exports'].items()))
    sources = {r['path']: r['sha256'] for r in current['authoring']['sources']}
    sources.update({r['path']: r['sha256'] for r in transition['added']})
    sources.update({r['path']: r['after_sha256'] for r in transition['changed']})
    current['authoring']['sources'] = [dict(path=p, sha256=h) for p, h in sorted(sources.items())]
    changed = {r['path']: r['after_sha256'] for r in transition['changed']}
    for puzzle in current['puzzles']:
        for row in puzzle.get('native_sources', []):
            if row['path'] in transition['embedded_native_paths']:
                row['sha256'] = changed[row['path']]
        for row in puzzle.get('source_dependencies', []):
            if row['path'] in transition['embedded_dependency_paths']:
                row['source'] = open(cost.ROOT / row['path']).read()
                row['sha256'] = cost.sha(row['source'].encode())
    for row in profile['added']:
        current['puzzles'].append(dict(slug=row['slug'], constructed_no_execution=True))
    return current


class SchemaAndRestoration(unittest.TestCase):
    def test_profile_loads_and_is_bound_to_the_delegated_checkpoint(self):
        profile = migration.load()
        self.assertIn(profile['status'], (migration.PENDING, migration.FINAL))
        self.assertEqual(profile['preserved_slugs'], migration.PRESERVED)
        self.assertEqual(profile['baseline']['report_sha256'], migration.outer.load()['reviewed_after']['report_sha256'])
        before = migration.predecessor(profile)
        self.assertEqual([p['slug'] for p in before['puzzles']], migration.PRESERVED)

    def test_pending_profile_never_admits_a_current_report(self):
        profile = pending_profile()
        before = migration.predecessor(profile)
        for report in ({}, before, constructed(before, profile)):
            with patch.object(migration, 'load', return_value=profile), self.assertRaisesRegex(ValueError, 'pending'):
                cost.validate_current(report)

    def test_closed_schema_rejects_unknown_fields_and_foreign_baselines(self):
        base = pending_profile()
        for mutation in ('extra', 'protocol', 'kind', 'checkpoint', 'transition_pin', 'preserved', 'added_empty',
                         'added_preserved', 'added_duplicate', 'added_entry', 'added_missing_source', 'changed_noop',
                         'native_not_changed', 'dependency_not_changed', 'mixed'):
            changed = copy.deepcopy(base)
            if mutation == 'extra': changed['signed'] = True
            elif mutation == 'protocol': changed['protocol'] = 'other/v1'
            elif mutation == 'kind': changed['baseline']['kind'] = 'signed_git_checkpoint'
            elif mutation == 'checkpoint': changed['baseline']['checkpoint_path'] = 'corpus/validation.json'
            elif mutation == 'transition_pin': changed['baseline']['authorizing_transition_sha256'] = '0' * 64
            elif mutation == 'preserved': changed['preserved_slugs'] = changed['preserved_slugs'][:-1]
            elif mutation == 'added_empty': changed['added'] = []
            elif mutation == 'added_preserved': changed['added'][0]['slug'] = 'p2-singleton'
            elif mutation == 'added_duplicate': changed['added'].append(copy.deepcopy(changed['added'][0]))
            elif mutation == 'added_entry': changed['added'][0]['entry'] = 'chialisp_oxide_puzzles::other::puzzle'
            elif mutation == 'added_missing_source':
                dropped = changed['added'][0]['source']
                changed['source_transition']['added'] = [r for r in changed['source_transition']['added'] if r['path'] != dropped]
            elif mutation == 'changed_noop': changed['source_transition']['changed'][0]['after_sha256'] = changed['source_transition']['changed'][0]['before_sha256']
            elif mutation == 'native_not_changed': changed['source_transition']['embedded_native_paths'].append('puzzles/lib.rs.other')
            elif mutation == 'dependency_not_changed': changed['source_transition']['embedded_dependency_paths'].append('puzzles/lib.rs.other')
            else: changed['reviewed_after'] = {}
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                migration.validate_profile(changed)

    def test_constructed_append_restores_and_non_noop_changes_reject(self):
        profile = pending_profile()
        before = migration.predecessor(profile)
        current = constructed(before, profile)
        substitutions = migration.compare_append(profile, before, current)
        transition = profile['source_transition']
        self.assertEqual({row['path'] for row in substitutions},
                         set(transition['embedded_native_paths']) | set(transition['embedded_dependency_paths']))
        for mutation in ('order', 'retained_artifact', 'retained_case', 'authoring_header', 'extra_export',
                         'source_table', 'native_hash', 'dependency_text', 'dropped_added', 'duplicate_added'):
            changed = copy.deepcopy(current)
            if mutation == 'order': changed['puzzles'][0], changed['puzzles'][1] = changed['puzzles'][1], changed['puzzles'][0]
            elif mutation == 'retained_artifact': changed['puzzles'][0]['artifacts'][0]['bytes'] += 1
            elif mutation == 'retained_case': changed['puzzles'][3]['cases'][0]['label'] += ' edited'
            elif mutation == 'authoring_header': changed['authoring']['package'] = 'other'
            elif mutation == 'extra_export': changed['authoring']['exports']['extra'] = dict(entry='chialisp_oxide_puzzles::extra::puzzle', source='puzzles/extra/puzzle.rs')
            elif mutation == 'source_table': changed['authoring']['sources'][0]['sha256'] = '0' * 64
            elif mutation == 'native_hash':
                target = next(p for p in changed['puzzles'] if p.get('native_sources'))
                target['native_sources'][0]['sha256'] = '0' * 64
            elif mutation == 'dependency_text':
                target = next(p for p in changed['puzzles'] if p.get('source_dependencies'))
                target['source_dependencies'][0]['source'] += '\n// unreviewed dependency edit\n'
            elif mutation == 'dropped_added': changed['puzzles'].pop()
            else: changed['puzzles'].append(copy.deepcopy(changed['puzzles'][-1]))
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                migration.compare_append(profile, before, changed)

    def test_checkpoint_bytes_and_added_row_pins_are_checked(self):
        profile = migration.load()
        if profile['status'] != migration.FINAL:
            self.skipTest('profile pending; final-only pins not recorded')
        payload = migration.read('corpus/validation.json')
        report = json.loads(payload)
        migration.reviewed_after(report)
        migration.check_report_bytes(payload, report)
        tampered = copy.deepcopy(report)
        tampered['puzzles'][-1]['title'] += ' edited'
        with self.assertRaisesRegex(ValueError, 'unreviewed complete current report'):
            migration.reviewed_after(tampered)
        with self.assertRaisesRegex(ValueError, 'raw bytes changed'):
            migration.check_report_bytes(payload + b'\n', report)
        broken = copy.deepcopy(profile)
        broken['baseline']['report_sha256'] = '0' * 64
        with patch.object(migration, 'load', return_value=broken), self.assertRaisesRegex(ValueError, 'checkpoint bytes'):
            migration.predecessor()
        with patch.object(migration, 'read', side_effect=lambda path, limit=16 * 1024 * 1024: b'{}' if path == migration.CHECKPOINT else migration.report_payload(migration.contained_file(migration.ROOT, path))), \
                self.assertRaisesRegex(ValueError, 'checkpoint bytes'):
            migration.predecessor(profile)

    def test_is_append_distinguishes_retained_from_extended_reports(self):
        profile = pending_profile()
        before = migration.predecessor(profile)
        self.assertFalse(migration.is_append(before))
        self.assertTrue(migration.is_append(constructed(before, profile)))
        self.assertTrue(migration.outer.is_append(constructed(before, profile)))

    def test_source_transition_derivation_rejects_removed_inputs(self):
        profile = pending_profile()
        before = migration.predecessor(profile)
        current = constructed(before, profile)
        self.assertEqual(migration.source_transition(before, current), profile['source_transition'])
        removed = copy.deepcopy(current)
        removed['authoring']['sources'].pop(0)
        with self.assertRaisesRegex(ValueError, 'removing retained'):
            migration.source_transition(before, removed)

    def test_root_selection_projects_through_the_retained_checkpoint(self):
        profile = migration.load()
        if profile['status'] != migration.FINAL:
            self.skipTest('profile pending; chain projection requires a reviewed capture')
        report = json.loads(migration.read('corpus/validation.json'))
        self.assertIs(cost.current22_migration(report), migration)
        checkpoint = cost.composition_checkpoint(report)
        self.assertEqual(cost.sha(cost.canonical(checkpoint)), migration.outer.BEFORE_CANONICAL)
        self.assertEqual([p['slug'] for p in cost.current_checkpoint(report)['puzzles']], migration.PRESERVED)


if __name__ == '__main__':
    unittest.main()


class CompilerTransition(unittest.TestCase):
    """Constructed optimized-artifact transitions for a retained row; no execution credit."""
    SLUG = 'p2-delegated-conditions'

    def declared(self):
        profile = pending_profile()
        profile['compiler_transition'] = dict(protocol=migration.transitions.PROTOCOL, slugs=[self.SLUG], metrics=None,
                                              reason='Constructed control: pair-aware owned passes change only the optimized Rust artifact.')
        migration.validate_profile(profile)
        return profile

    def transitioned(self, before, profile):
        current = constructed(before, profile)
        row = next(p for p in current['puzzles'] if p['slug'] == self.SLUG)
        off, on = row['artifacts'][0], row['artifacts'][1]
        on.update(bytes=off['bytes'], clvm_hex=off['clvm_hex'], tree_hash=off['tree_hash'], disassembly=off['disassembly'] + ' ; transitioned')
        for case in row['cases'] + row['protocol_validation']['raw_validation']['cases']:
            run = case['runs'][1]
            if type(run['execution']['cost']) is int:
                run['execution']['cost'] += 1
        build = row['protocol_validation']['builds'][1]
        build.update(module_hex=off['clvm_hex'], module_tree_hash=off['tree_hash'])
        return current

    def test_declared_transition_restores_the_checkpoint_row(self):
        profile = self.declared()
        before = migration.predecessor(profile)
        current = self.transitioned(before, profile)
        migration.compare_append(profile, before, current)
        measured = migration.transitions.metrics(before, current, [self.SLUG])
        self.assertEqual(measured[0]['before']['tree_hash'], before['puzzles'][22]['artifacts'][1]['tree_hash'])
        self.assertNotEqual(measured[0]['before']['tree_hash'], measured[0]['after']['tree_hash'])
        profile['compiler_transition']['metrics'] = measured
        migration.validate_profile(profile)
        migration.compare_append(profile, before, current)
        profile['compiler_transition']['metrics'][0]['after']['bytes'] += 1
        with self.assertRaisesRegex(ValueError, 'metrics differ'):
            migration.compare_append(profile, before, current)

    def test_undeclared_or_wider_changes_reject(self):
        profile = self.declared()
        before = migration.predecessor(profile)
        current = self.transitioned(before, profile)
        undeclared = copy.deepcopy(profile)
        undeclared['compiler_transition'] = None
        with self.assertRaises(ValueError):
            migration.compare_append(undeclared, before, current)
        for mutation in ('optimized_outcome', 'optimized_failure', 'off_artifact', 'chialisp_artifact', 'rue_artifact', 'native', 'raw_native',
                         'observation_code', 'observation_equality', 'off_build', 'count', 'module_identity', 'source', 'bad_hash'):
            changed = copy.deepcopy(current)
            row = next(p for p in changed['puzzles'] if p['slug'] == self.SLUG)
            protocol = row['protocol_validation']
            accepted = next(c for c in row['cases'] if c['native']['status'] == 'accepted')
            if mutation == 'optimized_outcome': accepted['runs'][1]['execution']['outcome'] = {'status': 'rejected'}
            elif mutation == 'optimized_failure': accepted['runs'][1]['execution']['error'] = 'spoofed'
            elif mutation == 'off_artifact': row['artifacts'][0]['bytes'] += 1
            elif mutation == 'chialisp_artifact': row['artifacts'][2]['clvm_hex'] = '80'
            elif mutation == 'rue_artifact': row['artifacts'][4]['tree_hash'] = '0' * 64
            elif mutation == 'native': accepted['native'] = {'status': 'rejected'}
            elif mutation == 'raw_native': protocol['raw_validation']['cases'][0]['native'] = {'status': 'rejected'}
            elif mutation == 'observation_code': protocol['builds'][1]['observations'][1]['code'] = 'CostExceeded'
            elif mutation == 'observation_equality': protocol['builds'][1]['observations'][1]['full_simulator_equality'] = False
            elif mutation == 'off_build': protocol['builds'][0]['module_tree_hash'] = '0' * 64
            elif mutation == 'count': protocol['accepted'] += 1
            elif mutation == 'module_identity': row['module_identity']['comparisons'][1]['tree_hash_equal'] = True
            elif mutation == 'source': row['source'] += '\n'
            else: row['artifacts'][1]['tree_hash'] = '0' * 64
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                migration.compare_append(profile, before, changed)

    def test_declaration_schema_is_closed(self):
        base = self.declared()
        for mutation in ('protocol', 'short_reason', 'unknown_slug', 'added_slug', 'duplicate', 'extra', 'metric_slugs'):
            changed = copy.deepcopy(base)
            value = changed['compiler_transition']
            if mutation == 'protocol': value['protocol'] = 'other/v1'
            elif mutation == 'short_reason': value['reason'] = 'because'
            elif mutation == 'unknown_slug': value['slugs'] = ['invented']
            elif mutation == 'added_slug': value['slugs'] = [changed['added'][0]['slug']]
            elif mutation == 'duplicate': value['slugs'] = [self.SLUG, self.SLUG]
            elif mutation == 'extra': value['sidecar'] = 'x'
            else: value['metrics'] = []
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                migration.validate_profile(changed)
        final = copy.deepcopy(migration.load())
        if final['status'] == migration.FINAL:
            final['compiler_transition'] = dict(base['compiler_transition'])
            with self.assertRaisesRegex(ValueError, 'metrics'):
                migration.validate_profile(final)
