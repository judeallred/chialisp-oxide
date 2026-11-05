#!/usr/bin/env python3
"""Capture the existing pay-to-singleton policy-control sidecar for the current source window.

Earlier append transitions assembled this packet by hand in a temporary checkout.
This producer runs the same four reviewed steps in order and assembles the exact
packet shape that ``p2_source_cost_migration.validate_controls`` reads:

1. a fresh three-export control bundle through ``cargo-chialisp-oxide``;
2. the identity gate ``check-p2-source-controls.py`` over that bundle;
3. the native three-test receipt (``cargo test`` on the control crate);
4. the six-run VM replay example ``p2_singleton_controls``.

Run it only after every workspace source edit is final: the recorded window hashes
the complete reviewed workspace and goes stale on any later change. The packet is
recorded consistency, not authenticated execution or compiler preservation.

This sidecar deliberately retains artifact-only Cargo schema 1 and its exact
manual source window. It earns no automatic compiler file-read receipt credit;
ordinary Cargo builds and the supplied-bundle identity gate retain schema 2.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess

from cargo_authoring import unique_object
import corpus_append
import p2_source_cost_migration as source
import runtime_cost_migration as cost

ROOT = cost.ROOT
GATE = importlib.util.spec_from_file_location("p2_source_controls_gate", ROOT / "scripts/check-p2-source-controls.py")
gate = importlib.util.module_from_spec(GATE)
GATE.loader.exec_module(gate)
NATIVE_ARGV = ["cargo", "test", "--locked", "--offline", "--manifest-path", f"{gate.CONTROL}/Cargo.toml", "--", "--nocapture"]


def run(argv, *, timeout):
    result = subprocess.run([str(a) for a in argv], cwd=ROOT, capture_output=True, timeout=timeout)
    cost.require(result.returncode == 0, f"command failed: {' '.join(map(str, argv))}\n{result.stderr.decode(errors='replace')[-4000:]}")
    return result


def native_receipt():
    result = subprocess.run(NATIVE_ARGV, cwd=ROOT, capture_output=True, timeout=1800)
    log = result.stdout
    cost.require(result.returncode == 0, 'native control tests failed:\n' + result.stderr.decode(errors='replace')[-4000:])
    text = log.decode(errors='replace')
    # Cargo prints one result line per test binary (unit, integration, doc).
    # The three policy tests must be the only passing tests and nothing may fail.
    results = re.findall(r'^test result: (\w+)\. (\d+) passed; (\d+) failed', text, flags=re.M)
    cost.require(results and all(status == 'ok' and failed == '0' for status, _, failed in results)
                 and sum(int(passed) for _, passed, _ in results) == 3, 'expected exactly three passing control tests and no failures')
    observations = []
    for name in gate.NAMES:
        match = re.findall(rf'^{re.escape(name)} digest=([0-9a-f]{{64}}) dollar=([0-9a-f]{{2}})$', text, flags=re.M)
        cost.require(len(match) == 1, f'missing or duplicate native observation for {name}')
        observations.append(dict(name=name, digest=match[0][0], dollar=match[0][1]))
    return dict(argv=NATIVE_ARGV, exit_code=result.returncode, tests=3, log_sha256=cost.sha(log), observations=observations)


def component_view(report):
    rows = [p for p in report['puzzles'] if p['slug'] == 'p2-singleton']
    cost.require(len(rows) == 1, 'unique p2-singleton row required')
    return {**report, 'puzzles': [rows[0]]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, default=ROOT / 'corpus/validation.json')
    parser.add_argument('--bundle', type=Path, required=True, help='NEW directory for the fresh control bundle')
    parser.add_argument('--replay', type=Path, required=True, help='NEW file for the VM replay example output')
    parser.add_argument('--driver', type=Path, default=ROOT / 'target/debug/chialisp-oxide-driver')
    parser.add_argument('--output', type=Path, default=ROOT / corpus_append.CONTROLS)
    args = parser.parse_args()
    cost.require(not args.bundle.exists() and not args.replay.exists(), 'NEW bundle directory and replay file required')
    report = json.loads(corpus_append.report_payload(args.report), object_pairs_hook=unique_object)
    view = component_view(report)
    before = source.control_source_window(view)
    compiler, driver_sha256, toolchain = gate.captured_identities(ROOT, args.driver)
    run([ROOT / 'target/debug/cargo-chialisp-oxide', 'build', '--artifact-only', *gate.CARGO_OPTIONS, '--output-dir', args.bundle], timeout=1800)
    identity = gate.validate_bundle(args.bundle.resolve(), root=ROOT, compiler=compiler, driver_sha256=driver_sha256, toolchain=toolchain)
    manifest = Path(identity['manifest_path']).read_text()
    native = native_receipt()
    run([ROOT / 'target/debug/examples/p2_singleton_controls', args.bundle, args.replay], timeout=1800)
    replay = json.loads(args.replay.read_bytes(), object_pairs_hook=unique_object)
    after = source.control_source_window(view)
    cost.require(before == after, 'workspace sources changed during capture')
    window = dict(source_root=str(ROOT), compiler=compiler, driver_sha256=driver_sha256, toolchain=toolchain, sources=after, unchanged=True)
    packet = dict(schema_version=1, protocol='chialisp-oxide-p2-source-cost-controls/v1', scope=source.CONTROLS_SCOPE,
                  identity=identity, manifest=manifest, vm_replay=replay, native=native, window=window)
    counts = source.validate_controls(view, packet)
    args.output.write_bytes((json.dumps(packet, indent=2) + '\n').encode())
    print(json.dumps(dict(written=str(args.output), **counts), indent=2))


if __name__ == '__main__':
    main()
