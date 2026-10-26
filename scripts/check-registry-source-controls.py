"""Offline registry controls packet reader. Install an actual partial3 base captured with all40 authoring; the current full40 report stays external. Full40-to-packet input fails rather than projecting. Explicit --partial-base permits historical feasibility only."""
import argparse, json, sys
from pathlib import Path
sys.dont_write_bytecode = True
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--packet', type=Path, required=True)
parser.add_argument('--report', type=Path, required=True)
parser.add_argument('--source-root', type=Path, default=Path(__file__).resolve().parents[1])
parser.add_argument('--partial-base', action='store_true')
parser.add_argument('--json', action='store_true')
parser.add_argument('--self-test', action='store_true')
parser.add_argument('--replay', action='store_true')
parser.add_argument('--cli', type=Path)
parser.add_argument('--install-capture', type=Path)
parser.add_argument('--native-dir', type=Path)
parser.add_argument('--last-dir', type=Path)
parser.add_argument('--base-dir', type=Path)
args = parser.parse_args()
root = args.source_root.resolve()
sys.path.insert(0, str(root / 'scripts'))
import registry_control_packet as p
import cargo_authoring, registry_protocol, port_evidence
from cargo_source_receipt import decode_json
from vm_failures import PROTOCOL

def current(packet):
    report = decode_json(p.regular_read(args.report, 16 * 1024 * 1024))
    declared = cargo_authoring.snapshot(root)
    cargo_authoring.check_recorded(report['authoring'], root, list(declared['exports']))
    p.require(report['authoring'] == packet.load('base/report.json')['authoring'], 'packet/current complete source authoring relation')
    p.require(type(report['schema_version']) is int and report['schema_version'] == 1 and (report['vm_failure_protocol'] == PROTOCOL), 'current typed report schema')
    slugs = [r['slug'] for r in report['puzzles']]
    if args.partial_base:
        p.require(slugs == list(registry_protocol.SLUGS), 'explicit partial base exact3')
    else:
        p.require(packet.index['protocol'] == p.PRODUCTION_PROTOCOL, 'current publication requires production capture protocol v2')
        p.require(len(slugs) == 40 and set(slugs) == set(declared['exports']), 'current full40 report required')
        import corpus_append
        corpus_append.validate_current(report, sidecar_root=args.report.parent)
    registry_protocol.validate_report(report, sidecar_root=args.report.parent)
    old = packet.load('base/report.json')
    for slug in registry_protocol.SLUGS:
        row = next((x for x in report['puzzles'] if x['slug'] == slug))
        p.require(row == next((x for x in old['puzzles'] if x['slug'] == slug)), 'current/base exact raw/native/protocol component relation')
        port_evidence.validate_port(report, slug)
    native = packet.load('native/frozen-source-after.json')
    if not args.partial_base:
        import registry_control_capture as core
        complete = core.full_sources(root)
        p.require(complete == native == packet.load('last/frozen-source-after.json'), 'fresh complete current native/LAST inventory including added and removed inputs')
    for row in native:
        path = cargo_authoring.contained_file(root, row['path'])
        raw = p.regular_read(path)
        p.require(p.identity(raw) == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'current complete native source window')
if args.install_capture is not None:
    p.require(args.native_dir is not None and args.last_dir is not None and (args.base_dir is not None), 'installation requires explicit native/LAST/base origins')
    p.install_capture(root, args.install_capture, args.native_dir, args.last_dir, args.base_dir, args.packet)
else:
    p.require(args.native_dir is None and args.last_dir is None and (args.base_dir is None), 'installation paths require --install-capture')
result = p.validate(args.packet, current_callback=current)
result['status'] = 'partial_feasibility_recorded_consistency_checked' if args.partial_base else 'recorded_consistency_checked'
result['scope'] = 'Explicit partial base feasibility only; no whole40/current publication credit.' if args.partial_base else 'Current source/native/protocol and recorded packet consistency only; no new native/VM/SDK execution, execution authentication, Program authentication, proof or production-readiness credit.'
keep = ['status', 'scope', 'component_vm', 'component_failure_protocol', 'index_sha256', 'downloads', 'native_counts', 'templates', 'sdk_capture', 'repeat_byte_identical', 'counts', 'physical_files', 'bytes', 'exports_admitted', 'commands_checked']
summary = {k: result[k] for k in keep}
if args.self_test:
    summary['corruption_controls'] = p.corruption_controls(args.packet)
if args.replay:
    p.require(args.cli is not None, '--replay requires an explicit current --cli executable')
    summary['replay'] = p.replay_templates(args.packet, args.cli)
else:
    p.require(args.cli is None, '--cli requires --replay')
print(json.dumps(summary, separators=(',', ':')) if args.json else json.dumps({k: v for k, v in summary.items() if k not in ['downloads', 'templates', 'sdk_capture']}, indent=2))
