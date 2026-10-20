"""Generic reviewed current-corpus append root, then PURE retained-checkpoint restoration.

The profile ``corpus/corpus-append.json`` records the retained complete
delegated-conditions era checkpoint, the preserved slug order, every slug added
since, the closed source transition and the reviewed capture. A new annotated
export extends the profile data through ``check-corpus-append.py --record``; it
does not add a migration module, CLI or checkpoint copy.

Recorded consistency is not authenticated execution, state equality, BLS,
compiler preservation, exhaustive authorization or production readiness.
"""
import copy
import json
from pathlib import Path
import re
import runpy

from cargo_authoring import check_recorded, contained_file, snapshot, unique_object
from cargo_bundle import load_declared_export
import runtime_cost_migration as cost
import runtime_composition_migration as composition
import p2dc_append_migration as outer
import p2_delegated_conditions_evidence as component
import corpus_compiler_transition as transitions
import p2_singleton_evidence as ledger

ROOT = Path(__file__).resolve().parents[1]
PATH = 'corpus/corpus-append.json'
PIN = '4af89d6e2a3568aee162f62a196cd45245b5fc58a570f5f02684f0d0c8438a83'
PENDING = 'pending_capture_and_review'
FINAL = 'reviewed_final'
PROTOCOL = 'chialisp-oxide-corpus-append/v1'
CHECKPOINT = 'corpus/checkpoints/p2dc-append-validation.json'
OPTIONS = ['--manifest-path', 'puzzles/Cargo.toml', '--package', 'chialisp-oxide-puzzles', '--locked']
# Existing pay-to-singleton policy-control and conditions-cost sidecars captured
# for the current source window; see scripts/capture-p2-controls.py.
CONTROLS = 'corpus/corpus-append-existing-p2-controls.json'
METRICS = 'corpus/corpus-append-existing-p2-costs.json'
PRESERVED = component.OLD_SLUGS + [outer.SLUG]
MAX_ADDED = 64
BASELINE_KIND = 'unsigned_reviewed_content_addressed_checkpoint'
SCOPE = ('Reviewed current corpus admission over the retained complete delegated-conditions checkpoint. '
         'Every preserved row must restore byte-for-byte after removing the added rows and the closed '
         'source transition; added rows carry their own finite native/VM evidence in the current report. '
         'No authenticated execution, BLS/state/preservation proof or production-readiness credit.')
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same
report_payload = composition.report_payload
SLUG_PATTERN = re.compile(r'[a-z0-9]+(?:-[a-z0-9]+)*')


def read(path, limit=16 * 1024 * 1024):
    payload = report_payload(contained_file(ROOT, path))
    require(len(payload) <= limit, 'corpus append input byte bound')
    return payload


def digest(value):
    return type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None


def entry_for(slug):
    return f"chialisp_oxide_puzzles::{slug.replace('-', '_')}::puzzle"


def rows_map(rows):
    require(type(rows) is list, 'source rows required')
    for row in rows:
        ledger.closed(row, ['path', 'sha256'], 'source identity')
        require(type(row['path']) is str and digest(row['sha256']), 'invalid source identity')
    require([r['path'] for r in rows] == sorted({r['path'] for r in rows}), 'sorted unique source identities required')
    return {r['path']: r['sha256'] for r in rows}


def added_slugs(profile):
    return [row['slug'] for row in profile['added']]


def current_slugs(profile=None):
    """Preserved order followed by every added slug; the complete current corpus."""
    profile = load() if profile is None else profile
    return PRESERVED + added_slugs(profile)


def capture_fields(after, profile):
    ledger.closed(after, ['report_sha256', 'canonical_report_sha256', 'added_puzzles', 'raw_ir', 'bundles',
                          'controls_sha256', 'metrics_sha256'], 'corpus append capture')
    require(all(digest(after[key]) for key in ('report_sha256', 'canonical_report_sha256', 'controls_sha256', 'metrics_sha256')),
            'missing capture report/sidecar pins')
    slugs = current_slugs(profile)
    added = after['added_puzzles']
    require(type(added) is list and [row.get('slug') for row in added] == added_slugs(profile), 'capture must pin every added puzzle in order')
    for row in added:
        ledger.closed(row, ['slug', 'sha256'], 'added puzzle pin')
        require(digest(row['sha256']), 'added puzzle digest')
    expected = {entry_for(slug) for slug in slugs}
    rows = after['raw_ir']
    require(type(rows) is list and len(rows) == len(slugs), 'raw IR cardinality must equal the current corpus')
    for row in rows:
        ledger.closed(row, ['entry', 'sha256'], 'raw IR')
        require(type(row['entry']) is str and digest(row['sha256']), 'raw IR identity')
    require({r['entry'] for r in rows} == expected and len({r['entry'] for r in rows}) == len(slugs), 'raw IR export set')
    ledger.closed(after['bundles'], ['on', 'off'], 'paired bundles')
    for name, mode in [('on', True), ('off', False)]:
        value = after['bundles'][name]
        ledger.closed(value, ['manifest_sha256', 'optimize_lir', 'cargo_options'], 'recorded bundle metadata')
        require(digest(value['manifest_sha256']) and value['optimize_lir'] is mode, 'recorded bundle mode/hash')
        options = value['cargo_options']
        require(type(options) is list and 0 < len(options) <= 64
                and all(type(v) is str and 0 < len(v) <= 4096 for v in options), 'recorded exact Cargo options')
    return after


def validate_profile(value):
    ledger.closed(value, ['schema_version', 'protocol', 'status', 'scope', 'baseline', 'preserved_slugs', 'added',
                          'source_transition', 'compiler_transition', 'reviewed_after'], 'corpus append profile')
    require(type(value['schema_version']) is int and value['schema_version'] == 1 and value['protocol'] == PROTOCOL
            and value['status'] in (PENDING, FINAL) and type(value['scope']) is str and value['scope'], 'corpus append profile schema')
    baseline = value['baseline']
    ledger.closed(baseline, ['kind', 'checkpoint_path', 'report_sha256', 'canonical_report_sha256',
                             'authorizing_transition_path', 'authorizing_transition_sha256'], 'retained checkpoint context')
    require(baseline['kind'] == BASELINE_KIND and baseline['checkpoint_path'] == CHECKPOINT
            and digest(baseline['report_sha256']) and digest(baseline['canonical_report_sha256'])
            and baseline['authorizing_transition_path'] == outer.PATH
            and baseline['authorizing_transition_sha256'] == outer.PIN, 'changed retained delegated-conditions checkpoint context')
    require(value['preserved_slugs'] == PRESERVED, 'preserved slug order must be the complete delegated-conditions era corpus')
    added = value['added']
    require(type(added) is list and 0 < len(added) <= MAX_ADDED, 'one to sixty-four added exports required')
    seen = set()
    for row in added:
        ledger.closed(row, ['slug', 'entry', 'source'], 'added export')
        slug = row['slug']
        require(type(slug) is str and SLUG_PATTERN.fullmatch(slug) and slug not in PRESERVED and slug not in seen, 'added slug must be new and unique')
        require(row['entry'] == entry_for(slug) and row['source'] == f'puzzles/{slug}/puzzle.rs', 'added export identity/source')
        seen.add(slug)
    transition = value['source_transition']
    ledger.closed(transition, ['added', 'changed', 'embedded_native_paths', 'embedded_dependency_paths'], 'source transition')
    added_paths = rows_map(transition['added'])
    for row in added:
        require(row['source'] in added_paths, 'every added export source must be an added Cargo input')
    changed = transition['changed']
    require(type(changed) is list and [r['path'] for r in changed] == sorted({r['path'] for r in changed}), 'sorted unique changed sources')
    for row in changed:
        ledger.closed(row, ['path', 'before_sha256', 'after_sha256'], 'changed source')
        require(type(row['path']) is str and digest(row['before_sha256']) and digest(row['after_sha256'])
                and row['before_sha256'] != row['after_sha256'], 'non-noop source transition')
        require(row['path'] not in added_paths, 'a source is either added or changed')
    native = transition['embedded_native_paths']
    changed_paths = {r['path'] for r in changed}
    require(type(native) is list and native == sorted(set(native)) and set(native) <= changed_paths, 'embedded native paths must be changed sources')
    dependencies = transition['embedded_dependency_paths']
    require(type(dependencies) is list and dependencies == sorted(set(dependencies)) and set(dependencies) <= changed_paths,
            'embedded dependency paths must be changed sources')
    if value['compiler_transition'] is not None:
        transitions.validate_declaration(value['compiler_transition'], PRESERVED)
    if value['status'] == PENDING:
        require(value['reviewed_after'] is None, 'mixed pending capture')
    else:
        capture_fields(value['reviewed_after'], value)
        require(value['compiler_transition'] is None or value['compiler_transition']['metrics'] is not None,
                'a final capture must record its compiler transition metrics')
    return value


def load(payload=None):
    payload = read(PATH) if payload is None else payload
    require(sha(payload) == PIN, 'corpus append profile bytes changed')
    value = validate_profile(json.loads(payload, object_pairs_hook=unique_object))
    require(sha(read(outer.PATH)) == outer.PIN, 'changed reviewed delegated-conditions append profile')
    authority = outer.load()
    require(authority['status'] == outer.FINAL, 'delegated-conditions append pending; the corpus append requires its reviewed final checkpoint')
    after = authority['reviewed_after']
    require(after['report_sha256'] == value['baseline']['report_sha256']
            and after['canonical_report_sha256'] == value['baseline']['canonical_report_sha256'],
            'retained checkpoint not bound to the exact reviewed delegated-conditions capture')
    if value['status'] == FINAL:
        prior = {r['entry']: r['sha256'] for r in after['raw_ir']}
        actual = {r['entry']: r['sha256'] for r in value['reviewed_after']['raw_ir']}
        require(all(actual[entry] == pin for entry, pin in prior.items()), 'retained raw IR changed')
    return value


def predecessor(profile=None):
    """Exact immutable retained checkpoint data only; never current source checks."""
    profile = load() if profile is None else profile
    payload = read(CHECKPOINT)
    require(sha(payload) == profile['baseline']['report_sha256'], 'changed retained checkpoint bytes')
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(sha(canonical(value)) == profile['baseline']['canonical_report_sha256'], 'changed retained checkpoint object')
    return value


def is_append(report):
    """True when the report carries any export beyond the retained checkpoint."""
    slugs = {p.get('slug') for p in report.get('puzzles', [])}
    return bool(slugs - set(PRESERVED))


def reviewed_after(report):
    profile = load()
    require(profile['status'] == FINAL, 'corpus append pending capture/review')
    after = profile['reviewed_after']
    capture_fields(after, profile)
    require([p.get('slug') for p in report.get('puzzles', [])] == current_slugs(profile)
            and sha(canonical(report)) == after['canonical_report_sha256'], 'unreviewed complete current report')
    added = report['puzzles'][len(PRESERVED):]
    for puzzle, pin in zip(added, after['added_puzzles'], strict=True):
        require(puzzle['slug'] == pin['slug'] and sha(canonical(puzzle)) == pin['sha256'], 'changed added puzzle row')
    return profile, after


def compare_append(profile, before, current):
    """PURE closed metadata restoration; reads NO current or predecessor files."""
    validate_profile(profile)
    require(sha(canonical(before)) == profile['baseline']['canonical_report_sha256'], 'comparison requires the exact retained checkpoint')
    slugs = current_slugs(profile)
    require([p['slug'] for p in before['puzzles']] == PRESERVED
            and [p['slug'] for p in current['puzzles']] == slugs, 'exact append order')
    a, b = before['authoring'], current['authoring']
    require({k: v for k, v in a.items() if k not in ('exports', 'sources')}
            == {k: v for k, v in b.items() if k not in ('exports', 'sources')}, 'authoring header changed')
    added = {row['slug']: row for row in profile['added']}
    require(set(a) == set(b) and {k: v for k, v in b['exports'].items() if k not in added} == a['exports'], 'closed annotated export addition')
    for slug, row in added.items():
        require(b['exports'].get(slug) == dict(entry=row['entry'], source=row['source']), 'added export declaration')
    old, new = rows_map(a['sources']), rows_map(b['sources'])
    transition = profile['source_transition']
    added_paths = rows_map(transition['added'])
    require(not set(old) & set(added_paths), 'new source paths already present')
    expected = dict(old)
    expected.update(added_paths)
    for row in transition['changed']:
        require(old.get(row['path']) == row['before_sha256'], 'before source hash differs from checkpoint')
        expected[row['path']] = row['after_sha256']
    require(new == expected, 'source table changed outside the reviewed transition')
    if profile['status'] == FINAL:
        for puzzle, pin in zip(current['puzzles'][len(PRESERVED):], profile['reviewed_after']['added_puzzles'], strict=True):
            require(puzzle['slug'] == pin['slug'] and sha(canonical(puzzle)) == pin['sha256'], 'added row differs from reviewed capture')
    restored = copy.deepcopy(current)
    restored['authoring'] = copy.deepcopy(a)
    del restored['puzzles'][len(PRESERVED):]
    transitioned = set(profile['compiler_transition']['slugs']) if profile['compiler_transition'] else set()
    changes = {r['path']: r for r in transition['changed']}
    native = set(transition['embedded_native_paths'])
    dependencies = set(transition['embedded_dependency_paths'])
    substitutions = []
    for index, (previous, actual, target) in enumerate(zip(before['puzzles'], current['puzzles'][:len(PRESERVED)], restored['puzzles'], strict=True)):
        # Declared source dependencies embed the file text: a changed SDK facade
        # restores to the checkpoint's recorded row only through the reviewed
        # transition, exactly like embedded native sources below.
        old_rows, new_rows = previous.get('source_dependencies', []), actual.get('source_dependencies', [])
        require(type(old_rows) is list and type(new_rows) is list and len(old_rows) == len(new_rows), 'dependency metadata count')
        for position, (x, y) in enumerate(zip(old_rows, new_rows, strict=True)):
            ledger.closed(x, ['path', 'sha256', 'source'], 'old dependency')
            ledger.closed(y, ['path', 'sha256', 'source'], 'new dependency')
            require(x['path'] == y['path'], 'dependency path/order changed')
            if x['path'] in dependencies:
                row = changes[x['path']]
                require(x['sha256'] == row['before_sha256'] and y['sha256'] == row['after_sha256']
                        and sha(y['source'].encode()) == row['after_sha256'], 'embedded dependency hashes not bound')
                target['source_dependencies'][position] = copy.deepcopy(x)
                substitutions.append(dict(slug=previous['slug'], path=x['path']))
            else:
                require(x == y, 'other dependency changed')
        require(('native_sources' in previous) == ('native_sources' in actual), 'native metadata presence changed')
        if 'native_sources' in previous:
            require(len(previous['native_sources']) == len(actual['native_sources']), 'native metadata count')
            for position, (x, y) in enumerate(zip(previous['native_sources'], actual['native_sources'], strict=True)):
                ledger.closed(x, ['path', 'sha256'], 'old native source')
                ledger.closed(y, ['path', 'sha256'], 'new native source')
                require(x['path'] == y['path'], 'native path/order changed')
                if x['path'] in native:
                    row = changes[x['path']]
                    require(x['sha256'] == row['before_sha256'] and y['sha256'] == row['after_sha256'], 'embedded native source hashes not bound')
                    target['native_sources'][position] = copy.deepcopy(x)
                    substitutions.append(dict(slug=previous['slug'], path=x['path']))
                else:
                    require(x == y, 'other native source changed')
        if previous['slug'] in transitioned:
            # A reviewed compiler transition admits only the optimized Rust
            # artifact, its costs and the module-derived protocol fields, after
            # the source transition above; the checkpoint row is then restored
            # for pure historical projection.
            transitions.equivalent(previous, target)
            restored['puzzles'][index] = copy.deepcopy(previous)
    if transitioned and profile['compiler_transition']['metrics'] is not None:
        require(profile['compiler_transition']['metrics'] == transitions.metrics(before, current, profile['compiler_transition']['slugs']),
                'compiler transition metrics differ from the retained and current rows')
    require(same(restored, before), 'whole retained source/ABI/artifact/OFF/typed/coverage/protocol/metadata regression')
    return substitutions


def project_validated(report):
    """PURE whole current → exact retained checkpoint. Current/fresh checks belong to the caller."""
    profile, _ = reviewed_after(report)
    before = predecessor(profile)
    compare_append(profile, before, report)
    return before


def check_report_bytes(payload, report):
    require(sha(payload) == reviewed_after(report)[1]['report_sha256'], 'current report raw bytes changed')


def hash_diagnostic_profile(report):
    reviewed_after(report)
    before = predecessor()
    for slug in ('sha256tree', 'singleton-launcher'):
        old = next(p for p in before['puzzles'] if p['slug'] == slug)
        new = next(p for p in report['puzzles'] if p['slug'] == slug)
        require(new['artifacts'] == old['artifacts'] and new.get('resource_controls') == old.get('resource_controls'), 'retained hash slots changed')
    return True


def current_native_sources(report, profile=None):
    """FULL current paths/bytes; never call an older native checker on a view."""
    check_recorded(report['authoring'], ROOT, current_slugs(profile))
    inputs = rows_map(report['authoring']['sources'])
    for puzzle in report['puzzles']:
        manifest = json.loads(read(f"puzzles/{puzzle['slug']}/puzzle.json"), object_pairs_hook=unique_object)
        paths = manifest.get('native_sources', [])
        rows = puzzle.get('native_sources', [])
        require(type(paths) is list and len(paths) == len(set(paths)) and type(rows) is list
                and [r['path'] for r in rows] == paths, 'current native closure')
        for row, path in zip(rows, paths, strict=True):
            ledger.closed(row, ['path', 'sha256'], 'native source')
            require(path in inputs and row['sha256'] == inputs[path] == sha(read(path)), 'stale current native source')


def p2_component_view(report):
    matches = [p for p in report['puzzles'] if p['slug'] == 'p2-singleton']
    require(len(matches) == 1, 'unique existing P2 component required')
    return {**report, 'puzzles': [matches[0]]}


def check_sidecars(report):
    """Actual current source/typed checks must precede this pure P2-last view."""
    _, after = reviewed_after(report)
    controls, metrics = read(CONTROLS), read(METRICS)
    require(sha(controls) == after['controls_sha256'] and sha(metrics) == after['metrics_sha256'], 'stale current P2 sidecar bytes')
    view = p2_component_view(report)
    counts = composition.source.validate_controls(view, json.loads(controls, object_pairs_hook=unique_object))
    measured = composition.source.validate_metrics(view, json.loads(metrics, object_pairs_hook=unique_object))
    require(sha(metrics) == outer.load()['reviewed_after']['metrics_sha256'], 'existing fifteen conditions metrics changed')
    return {**counts, **measured}


def validate_added(report, profile):
    """Added rows need actual finite evidence in the current report, not only identity pins."""
    for puzzle in report['puzzles'][len(PRESERVED):]:
        require(puzzle['validated'] is True and puzzle['cases'] and 'rue' in puzzle, f"{puzzle['slug']}: added export lacks executed five-build cases")
        manifest = json.loads(read(f"puzzles/{puzzle['slug']}/puzzle.json"), object_pairs_hook=unique_object)
        require(manifest['slug'] == puzzle['slug'] and manifest['provenance'] == puzzle['provenance'], f"{puzzle['slug']}: manifest provenance differs from the report")


def validate_current(report):
    """Explicit current root; pending/missing/malformed never falls back to the retained checkpoint."""
    profile, _ = reviewed_after(report)
    require([p.get('slug') for p in report['puzzles']] == current_slugs(profile), 'current exact coverage')
    counts = cost.READABLE['check_report'](report)
    cost.READABLE['check_sources'](report, ROOT, require_rue=True)
    current_native_sources(report, profile)
    validate_added(report, profile)
    cost.password_evidence(report)
    runpy.run_path(str(ROOT / 'scripts/check-hashing-puzzles.py'))['validate'](report)
    runpy.run_path(str(ROOT / 'scripts/check-p2-singleton.py'))['validate'](report)
    delegated = [p for p in report['puzzles'] if p['slug'] == outer.SLUG]
    require(len(delegated) == 1, 'unique delegated-conditions component required')
    component.validate_component(delegated[0], root=ROOT)
    check_sidecars(report)
    return counts


def verify_fresh_bundles(report, on, off, *, compiler, driver_sha256, toolchain, cargo_options=OPTIONS):
    profile, after = reviewed_after(report)
    require(type(cargo_options) is list and 0 < len(cargo_options) <= 64
            and all(type(x) is str and 0 < len(x) <= 4096 for x in cargo_options), 'explicit expected Cargo options required')
    expected = {r['entry']: r['sha256'] for r in after['raw_ir']}
    source = report['authoring']
    check_recorded(source, ROOT, current_slugs(profile))
    require(snapshot(ROOT) == source, 'current snapshot differs before fresh load')
    puzzles = {source['exports'][p['slug']]['entry']: p for p in report['puzzles']}
    count = len(current_slugs(profile))
    result = []
    for mode, directory in [('on', on), ('off', off)]:
        optimize = mode == 'on'
        loaded = load_declared_export(directory, root=ROOT, authoring=source, selected_entry=outer.ENTRY,
                                      compiler=compiler, driver_sha256=driver_sha256, toolchain=toolchain,
                                      current_authoring=lambda: snapshot(ROOT), optimize=optimize,
                                      rue_revision=report['rue_revision'], cargo_options=cargo_options)
        entries = loaded.manifest['entries']
        require(len(entries) == count and {r['entry'] for r in entries} == set(expected), 'fresh complete corpus exports required')
        for row in entries:
            require(sha(report_payload(loaded.manifest_path.parent / row['ir'])) == expected[row['entry']], 'current fresh raw IR changed')
            artifact = cost.rust_on(puzzles[row['entry']]['artifacts']) if optimize else next(
                a for a in puzzles[row['entry']]['artifacts'] if a['backend'] == 'rust_mir' and a['optimize_lir'] is False)
            require(same(row['artifact'], artifact), 'fresh artifact/report differs')
        result.append(dict(mode=optimize, exports=count, fresh_manifest_sha256=sha(report_payload(loaded.manifest_path)),
                           recorded_manifest_sha256=after['bundles'][mode]['manifest_sha256'],
                           expected_cargo_options=list(cargo_options), recorded_cargo_options=after['bundles'][mode]['cargo_options'],
                           scope='Independent actual current compiler/driver/mode/source/options/raw IR/artifact consistency; recorded manifest/options remain capture metadata, not cross-host admission pins or authentication.'))
    require(snapshot(ROOT) == source, 'current snapshot changed after fresh load')
    return result


def source_transition(before, current):
    """Derive the closed source transition from the retained and current authoring tables."""
    old, new = rows_map(before['authoring']['sources']), rows_map(current['authoring']['sources'])
    require(set(old) <= set(new), 'removing retained Cargo inputs is not an append')
    added = [dict(path=path, sha256=new[path]) for path in sorted(set(new) - set(old))]
    changed = [dict(path=path, before_sha256=old[path], after_sha256=new[path]) for path in sorted(old) if old[path] != new[path]]
    changed_paths = {r['path'] for r in changed}
    retained = current['puzzles'][:len(PRESERVED)]
    embedded = sorted({row['path'] for puzzle in retained for row in puzzle.get('native_sources', [])
                       if row['path'] in changed_paths})
    dependencies = sorted({row['path'] for puzzle in retained for row in puzzle.get('source_dependencies', [])
                           if row['path'] in changed_paths})
    return dict(added=added, changed=changed, embedded_native_paths=embedded, embedded_dependency_paths=dependencies)


def record(report, payload, on, off, *, compiler, driver_sha256, toolchain, cargo_options=OPTIONS):
    """Build the reviewed capture from the actual current report and fresh paired bundles.

    The caller reviews and commits the printed profile and its new pin; recording
    never validates history and never writes the checkpoint.
    """
    profile = json.loads(read(PATH), object_pairs_hook=unique_object)
    # Re-recording discards any earlier capture and derives the closed source
    # transition from the retained checkpoint before the schema is checked, so a
    # newly listed export only needs its `added` row.
    profile['status'] = PENDING
    profile['reviewed_after'] = None
    if profile['compiler_transition'] is not None:
        profile['compiler_transition']['metrics'] = None
    before = predecessor(profile)
    slugs = current_slugs(profile)
    require([p['slug'] for p in report['puzzles']] == slugs, 'report must list the preserved slugs then every added slug in order')
    profile['source_transition'] = source_transition(before, report)
    validate_profile(profile)
    compare_append(profile, before, report)
    source = report['authoring']
    require(snapshot(ROOT) == source, 'current snapshot differs before fresh load')
    raw_ir, bundles = {}, {}
    for mode, directory in [('on', on), ('off', off)]:
        optimize = mode == 'on'
        loaded = load_declared_export(directory, root=ROOT, authoring=source, selected_entry=outer.ENTRY,
                                      compiler=compiler, driver_sha256=driver_sha256, toolchain=toolchain,
                                      current_authoring=lambda: snapshot(ROOT), optimize=optimize,
                                      rue_revision=report['rue_revision'], cargo_options=cargo_options)
        entries = loaded.manifest['entries']
        require(len(entries) == len(slugs) and {r['entry'] for r in entries} == {entry_for(s) for s in slugs}, 'fresh complete corpus exports required')
        for row in entries:
            value = sha(report_payload(loaded.manifest_path.parent / row['ir']))
            require(raw_ir.setdefault(row['entry'], value) == value, 'raw IR differs between optimization modes')
        bundles[mode] = dict(manifest_sha256=sha(report_payload(loaded.manifest_path)), optimize_lir=optimize, cargo_options=list(cargo_options))
    if profile['compiler_transition'] is not None:
        profile['compiler_transition']['metrics'] = transitions.metrics(before, report, profile['compiler_transition']['slugs'])
    profile['status'] = FINAL
    profile['reviewed_after'] = dict(
        report_sha256=sha(payload), canonical_report_sha256=sha(canonical(report)),
        controls_sha256=sha(read(CONTROLS)), metrics_sha256=sha(read(METRICS)),
        added_puzzles=[dict(slug=p['slug'], sha256=sha(canonical(p))) for p in report['puzzles'][len(PRESERVED):]],
        raw_ir=[dict(entry=entry, sha256=raw_ir[entry]) for entry in sorted(raw_ir)], bundles=bundles)
    validate_profile(profile)
    return profile
