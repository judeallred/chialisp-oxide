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
                         'native_not_changed', 'mixed'):
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
            elif mutation == 'added_missing_source': changed['source_transition']['added'] = changed['source_transition']['added'][1:]
            elif mutation == 'changed_noop': changed['source_transition']['changed'][0]['after_sha256'] = changed['source_transition']['changed'][0]['before_sha256']
            elif mutation == 'native_not_changed': changed['source_transition']['embedded_native_paths'].append('puzzles/lib.rs.other')
            else: changed['reviewed_after'] = {}
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                migration.validate_profile(changed)

    def test_constructed_append_restores_and_non_noop_changes_reject(self):
        profile = pending_profile()
        before = migration.predecessor(profile)
        current = constructed(before, profile)
        substitutions = migration.compare_append(profile, before, current)
        self.assertEqual({row['path'] for row in substitutions}, set(profile['source_transition']['embedded_native_paths']))
        for mutation in ('order', 'retained_artifact', 'retained_case', 'authoring_header', 'extra_export',
                         'source_table', 'native_hash', 'dropped_added', 'duplicate_added'):
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
