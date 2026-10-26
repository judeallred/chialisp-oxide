#!/usr/bin/env python3
"""Run coordinated native, LAST-build and registry control capture phases.

Each phase requires --execute; omission prints a plan. Native calls, template
VM observations and SDK attempts remain separate finite evidence lanes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]


def require(value, message):
    if not value:
        raise ValueError(message)


def raw(path, limit=16 * 1024 * 1024, *, empty=False):
    path = Path(path)
    require(not path.is_symlink(), 'no symlink input')
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        require(stat.S_ISREG(info.st_mode) and info.st_size <= limit, 'bounded regular input')
        data = stream.read(limit + 1)
    require(len(data) <= limit and (empty or data), 'bounded nonempty input')
    return data


def identity(path, limit=16 * 1024 * 1024):
    data = raw(path, limit)
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def run_capture(args):
    require(all(getattr(args, key) is not None for key in ('native_dir', 'last_dir', 'base_report', 'output')),
            'all explicit capture inputs required')
    root = ROOT
    require(root.is_dir(), 'existing source worktree')
    native_dir, last_dir, base_path, output = [getattr(args, key).resolve()
        for key in ('native_dir', 'last_dir', 'base_report', 'output')]
    require(not output.exists() and output.is_relative_to(root / 'target'), 'new output inside worktree target')
    require(all(p.is_relative_to(root / 'target') for p in (native_dir, last_dir, base_path)), 'isolated target receipts required')
    # The core creates this child; wrapper receipt is preserved alongside it even
    # when admission fails before any commands execute.
    output.mkdir(parents=True)
    receipt_path = output / 'wrapper-receipt.json'
    receipt = {'status': 'in_progress', 'source_root': str(root),
        'native_dir': str(native_dir), 'last_dir': str(last_dir), 'base_report': str(base_path),
        'scope': 'Capture consistency and finite local observations; no execution authentication, whole40 execution, live chain, BLS/security or compiler-preservation credit.'}
    try:
        import cargo_authoring
        import cargo_bundle
        from cargo_source_receipt import decode_json
        import precommit_source_controls as p
        import registry_protocol as registry
        import registry_source_controls as sdk
        import registry_control_capture as core
        require(p.ROOT.resolve() == root and registry.ROOT.resolve() == root,
                'imports must use current worktree roots; no ROOT override')
        load = lambda path: sdk.load_bytes(raw(path))
        package = root / core.DIRECTORY
        input_records = {}
        def admitted_rows(directory, before_name, after_name):
            before, after = load(directory / before_name), load(directory / after_name)
            require(before == after and type(before) is list and 0 < len(before) <= 4096, 'quiet complete retained window')
            paths = set()
            remaining = 64 * 1024 * 1024
            for row in before:
                require(type(row) is dict and set(row) == {'path', 'bytes', 'sha256'}
                    and type(row['bytes']) is int and 0 < row['bytes'] <= 16 * 1024 * 1024,
                    'exact retained source row')
                name = row['path']
                require(type(name) is str and not Path(name).is_absolute()
                    and Path(name).as_posix() == name and '..' not in Path(name).parts and name not in paths,
                    'unique confined retained source')
                paths.add(name)
                require(row['bytes'] <= remaining, 'aggregate source window precharge')
                remaining -= row['bytes']
            for row in before:
                name = row['path']
                require(identity(root / name) == {'bytes': row['bytes'], 'sha256': row['sha256']},
                        'retained source changed since actual gate')
            return before
        def admitted_tools(directory):
            before, after = load(directory / 'tools-before.json'), load(directory / 'tools-after.json')
            require(before == after and type(before) is list and len(before) == 4, 'four quiet actual compiler tools')
            require([row['tool'] for row in before] == ['cargo', 'rustc', 'rustfmt', 'clippy-driver'], 'exact tool inventory')
            for row in before:
                require(set(row) == {'tool', 'path', 'sha256'}
                    and identity(row['path'], 512 * 1024 * 1024)['sha256'] == row['sha256'], 'actual tool identity')
            return before
        # Host gate receipts contain finite elapsed-second measurements. Keep
        # the integer-only registry parser for protocol data and native outputs.
        native = decode_json(raw(native_dir / 'receipt.json'))
        require(native.get('status') == 'passed' and native.get('source_and_tool_window_quiet') is True,
                'actual native phase must pass quietly')
        gates = native['gates']
        require([g['name'] for g in gates] == ['recipe-before', 'fmt', 'check', 'test', 'clippy', 'recipe-after']
                and all(type(g['exit_code']) is int and g['exit_code'] == 0 for g in gates), 'complete successful native gates')
        expected_gate_commands = {
            'fmt': ['cargo', 'fmt', '--manifest-path', str(package / 'Cargo.toml'), '-p', core.PACKAGE, '--', '--check'],
            'recipe-before': ['python3', str(package / 'verify_sources.py')],
            'recipe-after': ['python3', str(package / 'verify_sources.py')],
        }
        common = ['--manifest-path', str(package / 'Cargo.toml'), '--all-targets', '--locked', '--offline', '--features', 'native-vm', '-j', '2']
        expected_gate_commands.update(check=['cargo', 'check', *common],
            test=['cargo', 'test', *common, '--', '--nocapture', '--test-threads=1'],
            clippy=['cargo', 'clippy', *common, '--', '-D', 'warnings'])
        require(all(g['command'] == expected_gate_commands[g['name']] for g in gates), 'exact native commands')
        native_sources = admitted_rows(native_dir, 'frozen-source-before.json', 'frozen-source-after.json')
        native_tools = admitted_tools(native_dir)
        fixture = load(package / 'fixtures/native-vectors.json')
        lines = raw(native_dir / 'test.stdout', 2 * 1024 * 1024).decode().splitlines()
        prefix, summary_prefix = 'REGISTRY_RUST_CONTROL_NATIVE ', 'REGISTRY_RUST_CONTROL_NATIVE_SUMMARY '
        observed = [sdk.load_bytes(line[line.index(prefix) + len(prefix):].encode()) for line in lines if prefix in line]
        summaries = [sdk.load_bytes(line[line.index(summary_prefix) + len(summary_prefix):].encode()) for line in lines if summary_prefix in line]
        require([row['id'] for row in observed] == list(core.NAMES), 'four ordered actual native markers')
        for observation, case in zip(observed, fixture['cases'], strict=True):
            require(observation['baseline'] == case['baseline_expected'] and observation['variant'] == case['variant_expected'],
                    'actual original and mutant native outputs')
        counts = {'baseline_policy_calls': 4, 'variant_policy_calls': 4, 'abi_observations': 0, 'VM_executions': 0, 'SDK_submissions': 0}
        registry.same(summaries, [counts], 'exact separate native summary counts')
        registry.same(native.get('native_counts'), counts, 'exact separate native receipt counts')
        test_results = re.findall(r'^test result: (\w+)\. (\d+) passed; (\d+) failed', '\n'.join(lines), re.M)
        require(test_results and all(status == 'ok' and failed == '0' for status, _, failed in test_results)
                and sum(int(passed) for _, passed, _ in test_results) == 2, 'two actual successful native tests')
        # Compare actual external tuple content. Never replace a stale lock digest
        # in a retained observation to make this admission succeed.
        def external(path):
            lock = tomllib.loads(raw(path).decode())
            return sorted((row['name'], row['version'], row.get('source'), row.get('checksum'))
                          for row in lock['package'] if row.get('source'))
        external_root, external_controls = external(root / 'Cargo.lock'), external(package / 'Cargo.lock')
        require(len(external_controls) == 127 and set(external_controls) <= set(external_root), '127 exact pinned external tuples')
        require(raw(package / 'root-lock.seed') == raw(root / 'Cargo.lock'), 'reviewed seed remains exact current root lock')
        input_records['native'] = {'receipt': identity(native_dir / 'receipt.json'), 'sources': native_sources,
            'tools': native_tools, 'observations': observed, 'counts': counts, 'external_pins': 127}
        last = decode_json(raw(last_dir / 'receipt.json'))
        require(last.get('status') == 'passed' and last.get('source_and_tool_window_quiet') is True,
                'LAST build must pass quietly')
        last_sources = admitted_rows(last_dir, 'frozen-source-before.json', 'frozen-source-after.json')
        last_tools = admitted_tools(last_dir)
        require(last_tools == native_tools, 'same dated compiler tools across native and LAST')
        require((native_dir / 'receipt.json').stat().st_mtime_ns <= (last_dir / 'frozen-source-before.json').stat().st_mtime_ns,
                'native actual receipt must precede LAST source window')
        builds = [g for g in last['gates'] if g.get('command', [])[:2] == ['cargo', 'build']]
        require(len(builds) == 1 and builds[0]['command'] == ['cargo', 'build', '--workspace', '--bins', '--examples', '--locked', '--offline', '-j', '2']
                and type(builds[0]['exit_code']) is int and builds[0]['exit_code'] == 0
                and all(type(g['exit_code']) is int and g['exit_code'] == 0 for g in last['gates']), 'successful exact LAST workspace build')
        built = load(last_dir / 'built-tool-identities.json')
        expected_names = {'cargo-chialisp-oxide', 'chialisp-oxide-driver', 'chialisp-oxide', 'chialisp-oxide-verifier', 'examples/registry_controls'}
        require(type(built) is list and len(built) == 5 and {row['name'] for row in built} == expected_names, 'five LAST built tool identities')
        target = Path(last['target']).resolve()
        for row in built:
            expected = target / ('debug/' + row['name'])
            require(Path(row['path']).resolve() == expected and identity(expected, 512 * 1024 * 1024)
                    == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'actual LAST executable unchanged')
        input_records['last'] = {'receipt': identity(last_dir / 'receipt.json'), 'sources': last_sources, 'tools': last_tools, 'built': built}
        # Admit the current base record and its complete canonical/journal sidecars
        # before using the three reference artifacts or matching baseline bundles.
        base = load(base_path)
        require(base['authoring'] == cargo_authoring.snapshot(root) and len(base['authoring']['exports']) == 40,
                'current complete40 authoring metadata')
        require(type(base['puzzles']) is list, 'base puzzle list')
        declared = set(base['authoring']['exports'])
        slugs = [row['slug'] for row in base['puzzles']]
        require(len(slugs) == len(set(slugs)) and (slugs == list(registry.SLUGS) or set(slugs) == declared),
                'exact triplet or complete40 base report')
        base_registry = [row for slug in registry.SLUGS for row in base['puzzles'] if row['slug'] == slug]
        require(len(base_registry) == 3, 'complete registry selection after current40 source admission')
        registry.validate_report(base, sidecar_root=base_path.parent)
        index = registry.json_value(registry.read_member(base_path.parent / registry.INDEX,
            base_registry[0]['protocol_validation']['index']))
        canonical = index['header']['canonical_components']
        references = []
        for row in base_registry:
            source = raw(root / 'puzzles' / row['slug'] / 'reference.clsp').decode()
            require(row['reference'] == source, 'current independently compiled Chialisp source')
            selected = [a for a in row['artifacts'] if a['backend'] == 'chialisp' and a['optimize_lir'] is True]
            require(len(selected) == 1, 'unique actual Chialisp artifact')
            references.append({'slug': row['slug'], 'source': source, 'module_hex': selected[0]['clvm_hex']})
        base_files = [base_path, base_path.parent / registry.INDEX]
        base_files.extend(base_path.parent / b['journals'][lane]['path'] for b in index['builds'] for lane in ('ordinary', 'adversarial'))
        input_records['base'] = [{'path': str(path), **identity(path)} for path in base_files]
        def source_snapshot():
            capture_sources = [root / 'scripts/registry_control_capture.py', Path(__file__), root / 'scripts/registry_source_controls.py']
            return {'workspace': p.source_window(), 'controls': core.private_snapshot(package),
                'capture_sources': [{'path': str(path), **identity(path)} for path in capture_sources],
                'admitted_inputs': input_records,
                'base_files_current': [{'path': str(path), **identity(path)} for path in base_files]}
        supporting = []
        for directory in (native_dir, last_dir):
            supporting.extend(directory / name for name in ('receipt.json', 'frozen-source-before.json',
                'frozen-source-after.json', 'tools-before.json', 'tools-after.json'))
        supporting.extend([native_dir / 'test.stdout', last_dir / 'built-tool-identities.json'])
        complete_sources = {}
        for row in [*native_sources, *last_sources]:
            require(row['path'] not in complete_sources or complete_sources[row['path']] == row,
                    'native and LAST must agree on every overlapping source identity')
            complete_sources[row['path']] = row
        snapshot_base = source_snapshot
        def source_snapshot():
            complete_current = core.full_sources(root)
            require(complete_current == native_sources == last_sources,
                'complete source membership and bytes must match native and LAST windows')
            value = snapshot_base()
            actual_sources = []
            for name, retained in sorted(complete_sources.items()):
                actual = {'path': name, **identity(root / name)}
                require(actual == retained, 'complete native/LAST retained source changed')
                actual_sources.append(actual)
            value['complete_native_last_sources_current'] = actual_sources
            value['retained_gate_files_current'] = [{'path': str(path), **identity(path)} for path in supporting]
            value['compiler_tools_current'] = core.compiler_tools(root, core.environment(root, target))
            require(value['compiler_tools_current'] == native_tools,
                'current installed dated tool paths/bytes must equal admitted native and LAST tools')
            return value
        frozen = source_snapshot()
        def verify_native():
            require(source_snapshot() == frozen, 'native gate/source relationship changed')
            for row in native_sources:
                require(identity(root / row['path']) == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'native source changed')
            return input_records['native']
        def validate_baseline(admitted, mode):
            manifest = admitted.manifest
            require({e['entry'] for e in manifest['entries']} == {e['entry'] for e in base['authoring']['exports'].values()},
                    'all40 baseline entries admitted before registry selection')
            for row in base_registry:
                entry = base['authoring']['exports'][row['slug']]['entry']
                actual = [e['artifact'] for e in manifest['entries'] if e['entry'] == entry]
                expected = [a for a in row['artifacts'] if a['backend'] == 'rust_mir' and a['optimize_lir'] is (mode == 'on')]
                require(len(actual) == len(expected) == 1 and all(actual[0][k] == expected[0][k]
                    for k in ('bytes', 'tree_hash', 'clvm_hex')), 'actual fresh baseline artifact equals base report')
        sdk_results = []
        def validate_sdk(capture, runs):
            request = {'protocol': 'chialisp-oxide-registry-source-controls-request/v1', 'runs': runs}
            sdk_results.append(sdk.validate_capture(capture, request, references, canonical))
        core.capture(root, target, output / 'capture', source_snapshot=source_snapshot,
            verify_native=verify_native, validate_sdk=validate_sdk, validate_baseline=validate_baseline)
        request = load(output / 'capture/sdk-request.json')
        first, repeat = [raw(output / f'capture/sdk-{index}.json') for index in (1, 2)]
        require(first == repeat and len(sdk_results) == 2, 'both independently validated repeat records match')
        corruption = sdk.corruption_controls(sdk.load_bytes(first), request, references, canonical)
        require(source_snapshot() == frozen, 'final complete quiet capture relationship')
        receipt.update(status='passed_capture_consistency', native=input_records['native'], last=input_records['last'],
            base=input_records['base'], sdk_checks=sdk_results, corruption_controls=corruption,
            template_primary_observations=8, native_observations=8,
            sdk_attempts=24, sdk_primary_observations=36, repeats_add_coverage=False)
    except BaseException as error:
        receipt.update(status='failed_or_inconclusive', error=repr(error))
        raise
    finally:
        with receipt_path.open('x') as stream:
            json.dump(receipt, stream, indent=2)
            stream.write('\n')
    print(json.dumps({'status': receipt['status'], 'receipt': str(receipt_path)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    phases = parser.add_subparsers(dest='phase')
    for phase in ('native', 'last', 'capture'):
        command = phases.add_parser(phase)
        command.add_argument('--execute', action='store_true', help='execute this explicit phase instead of printing its plan')
        command.add_argument('--output', type=Path, required=True)
        if phase in ('native', 'last'):
            command.add_argument('--target', type=Path, required=True)
        if phase in ('last', 'capture'):
            command.add_argument('--native-dir', type=Path, required=True)
        if phase == 'capture':
            command.add_argument('--last-dir', type=Path, required=True)
            command.add_argument('--base-report', type=Path, required=True)
    args = parser.parse_args()
    if args.phase is None or not args.execute:
        print(json.dumps({'status': 'plan_only_no_execution', 'source_root': str(ROOT),
            'phase': args.phase, 'order': ['native', 'last', 'capture'],
            'requires': 'Choose an explicit phase, provide fresh isolated target paths and pass --execute.',
            'scope': 'Native8, templateVM8 and SDK24/primary36 are separate planned lanes; no live funds, full40 execution or preservation proof.'}, indent=2))
        return
    if args.phase == 'capture':
        run_capture(args)
    else:
        import registry_control_capture as core
        result = core.run_phase(ROOT, args.target, args.output, args.phase,
            native_dir=getattr(args, 'native_dir', None))
        print(json.dumps({'status': result['status'], 'phase': args.phase, 'receipt': str(args.output / 'receipt.json')}))


if __name__ == '__main__':
    main()
