#!/usr/bin/env python3
"""Capture the dedicated SDK lane after a complete current48 corpus/bundle capture.

Default plans only. --execute uses explicit existing local tools, never builds,
fetches, signs with user keys or submits to a network. Failed outputs are retained.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import stat
import subprocess
import sys

import packet
from cargo_source_receipt import decode_json
import cargo_authoring
import cargo_bundle

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def host_tool(path):
    path = path.resolve(strict=True)
    before = path.stat()
    packet.require(stat.S_ISREG(before.st_mode) and 0 < before.st_size <= 512 * 1024 * 1024, 'bounded explicit host executable')
    result = {'path': str(path), 'bytes': before.st_size, 'sha256': digest(path)}
    after = path.stat()
    packet.require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), 'host executable quiet')
    return result


def save(path, value):
    with path.open('x') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def check_compiler_identity(verbose, short, driver):
    """Compare independent rustc formats without replacing verbose metadata."""
    packet.require(all(type(value) is str for value in (verbose, short, driver)), 'compiler identity strings')
    lines = short.splitlines()
    packet.require(len(lines) == 1 and lines[0].startswith('rustc ')
                   and lines[0].strip() == lines[0], 'independent short compiler version')
    packet.require(verbose.splitlines() and verbose.splitlines()[0] == lines[0], 'independent compiler formats agree')
    packet.require(driver.splitlines() == lines, 'driver compiler identity')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute', action='store_true')
    parser.add_argument('--report', type=Path, default=ROOT / 'corpus/validation.json')
    parser.add_argument('--rust-off', type=Path)
    parser.add_argument('--rust-on', type=Path)
    parser.add_argument('--tools', type=Path)
    parser.add_argument('--expected-host')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not args.execute:
        print(json.dumps({'status': 'prepared_not_executed', 'requires': ['complete current48 corpus report', 'fresh complete schema2 Rust bundles both modes', 'final current source host build', 'explicit local tools and expected host'], 'sdk_attempts_per_run': 80, 'repeat_adds_unique_coverage': False, 'network_actions': 0}, indent=2))
        return
    packet.require(all((args.rust_off, args.rust_on, args.tools, args.expected_host, args.output)), 'all explicit capture inputs required')
    out = args.output.resolve()
    packet.require(out.is_relative_to(ROOT / 'target') and not out.exists(), 'fresh target-only capture output')
    full_raw = packet.read(args.report.resolve(), 32 * 1024 * 1024)
    full = decode_json(full_raw)
    packet.require(len(full['puzzles']) == 48, 'official complete current48 report required')
    authoring = cargo_authoring.snapshot(ROOT)
    cargo_authoring.check_recorded(full['authoring'], ROOT, [p['slug'] for p in full['puzzles']])
    compiler = subprocess.check_output(['rustup', 'run', 'nightly-2026-07-14', 'rustc', '-vV'], cwd=ROOT, timeout=30).decode()
    compiler_short = subprocess.check_output(['rustup', 'run', 'nightly-2026-07-14', 'rustc', '--version'], cwd=ROOT, timeout=30).decode()
    hosts = [line[6:] for line in compiler.splitlines() if line.startswith('host: ')]
    packet.require(hosts == [args.expected_host], 'explicit independently queried host')
    tool_paths = {name: args.tools / name for name in ('cargo-chialisp-oxide', 'chialisp-oxide-driver', 'chialisp-oxide')}
    tool_paths['standard_tutorial_ledger'] = args.tools / 'examples/standard_tutorial_ledger'
    tools = {name: host_tool(path) for name, path in tool_paths.items()}
    driver_compiler = subprocess.check_output([tools['chialisp-oxide-driver']['path'], '--compiler-version'], cwd=ROOT, timeout=30).decode()
    check_compiler_identity(compiler, compiler_short, driver_compiler)
    selected = {}
    bundle_inputs = {'rust-off': args.rust_off.resolve(), 'rust-on': args.rust_on.resolve()}
    for lane, folder in bundle_inputs.items():
        pointer = decode_json(packet.read(folder / 'latest.json'))
        manifest = decode_json(packet.read(folder / pointer['manifest']))
        loaded = cargo_bundle.load_declared_export(folder, root=ROOT, authoring=authoring, selected_entry=packet.ENTRY,
            compiler=compiler, driver_sha256=tools['chialisp-oxide-driver']['sha256'], toolchain='nightly-2026-07-14',
            current_authoring=lambda: cargo_authoring.snapshot(ROOT), optimize=lane == 'rust-on',
            rue_revision='245c107a8e795229ea08097142f005a4c0d91075', cargo_options=manifest['cargo_options'])
        packet.require(loaded.manifest['schema_version'] == 2, 'complete schema2 source bundles required')
        selected[lane] = loaded
    source_paths = packet.source_paths(ROOT, authoring)
    before = {name: digest(ROOT / name) for name in source_paths}
    projection = {**full, 'puzzles': [p for p in full['puzzles'] if p['slug'] == packet.SLUG]}
    packet.require(len(projection['puzzles']) == 1, 'unique standard template')
    mapping = {}
    for lane, folder in bundle_inputs.items():
        for name in packet.discover(folder):
            mapping[packet.BUNDLES[lane] + '/' + name] = folder / name
    for name in source_paths:
        mapping['sources/' + name] = ROOT / name
    total = 0
    for name, path in mapping.items():
        packet.relative(name)
        info = path.lstat()
        packet.require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= packet.PROFILE['max_file_bytes'], 'copy member precharge')
        total += info.st_size
        packet.require(len(mapping) < packet.PROFILE['max_files'] and total < packet.PROFILE['max_total_bytes'], 'complete input copy precharge')
    out.mkdir(parents=True)
    for name, path in mapping.items():
        destination = out / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
    save(out / packet.BINDINGS['template'], projection)
    save(out / packet.BINDINGS['source_before'], before)
    save(out / packet.BINDINGS['tools_before'], tools)
    standard = projection['puzzles'][0]
    lanes = [('rust-off', 'rust_mir', False), ('rust-on', 'rust_mir', True), ('chialisp', 'chialisp', True), ('rue-off', 'rue_source', False), ('rue-on', 'rue_source', True)]
    runs = []
    for lane, backend, mode in lanes:
        artifact = next(a for a in standard['artifacts'] if a['backend'] == backend and a['optimize_lir'] is mode)
        if lane in selected:
            entry = next(e for e in selected[lane].manifest['entries'] if e['entry'] == packet.ENTRY)
            packet.require(entry['artifact'] == artifact, 'selected template/source bundle exact artifact')
        runs.append({'lane': lane, 'module_hex': artifact['clvm_hex']})
    save(out / packet.BINDINGS['request'], {'protocol': 'chialisp-oxide-standard-tutorial-ledger-request/v1', 'runs': runs})
    def limit_output():
        resource.setrlimit(resource.RLIMIT_FSIZE, (packet.PROFILE['max_file_bytes'], packet.PROFILE['max_file_bytes']))
    for number, name in ((1, 'report'), (2, 'repeat')):
        with (out / packet.BINDINGS[name]).open('xb') as stdout, (out / f'sdk-{number}.stderr').open('xb') as stderr:
            result = subprocess.run([tools['standard_tutorial_ledger']['path'], str(out / packet.BINDINGS['request'])], cwd=ROOT,
                stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, timeout=180, preexec_fn=limit_output)
        packet.require(result.returncode == 0, 'SDK capture failed/inconclusive; retained output is not an admission')
    packet.require(packet.read(out / packet.BINDINGS['report']) == packet.read(out / packet.BINDINGS['repeat']), 'independent SDK repeat byte equality')
    after = {name: digest(ROOT / name) for name in source_paths}
    packet.require(after == before and source_paths == packet.source_paths(ROOT, cargo_authoring.snapshot(ROOT)), 'source window quiet')
    tools_after = {name: host_tool(Path(row['path'])) for name, row in tools.items()}
    packet.require(tools_after == tools, 'tool window quiet')
    save(out / packet.BINDINGS['source_after'], after)
    save(out / packet.BINDINGS['tools_after'], tools_after)
    inventory = {name: {'bytes': n, 'sha256': digest(out / name)} for name, n in packet.discover(out).items()}
    receipt = {'protocol': packet.PROTOCOL, 'status': 'captured', 'profile': packet.PROFILE, 'scope': packet.SCOPE,
        'origin': {'source_root': str(ROOT), 'full_report_path': str(args.report.resolve()), 'full_report_sha256': packet.sha(full_raw)},
        'compiler': {'version': compiler, 'driver_sha256': tools['chialisp-oxide-driver']['sha256']}, 'authoring': authoring,
        'bindings': packet.BINDINGS, 'bundles': packet.BUNDLES, 'files': inventory}
    encoded = (json.dumps(receipt, indent=2) + '\n').encode()
    packet.require(len(encoded) <= packet.PROFILE['max_file_bytes'] and sum(r['bytes'] for r in inventory.values()) + len(encoded) <= packet.PROFILE['max_total_bytes'], 'complete packet budget including receipt')
    (out / 'receipt.json').write_bytes(encoded)
    result = packet.validate(out, current_root=ROOT)
    print(json.dumps({'status': 'captured_and_recorded_consistency_checked', 'output': str(out), 'counts': result['counts'], 'repeat_unique_credit': False}))


if __name__ == '__main__':
    main()
