"""Bounded recorded standard-tutorial packet reader. Never follows origins.

Cargo inventory/wire/blob consistency and recorded SDK relations are checked;
this Python reader does not execute Program validation, compiler, VM or SDK.
Fresh source capture and structural hashes do not authenticate execution.
"""
from pathlib import Path, PurePosixPath
import hashlib
import os
import re
import stat
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from cargo_source_receipt import decode_json, validate_receipt
import cargo_bundle
import cargo_authoring
import port_evidence
import sdk_reader
from registry_codegen_evidence import inspect_compiled_artifact
from source_walk import source_files

PROTOCOL = 'chialisp-oxide-standard-tutorial-capture/v1'
PROFILE = {'max_files': 2048, 'max_file_bytes': 16 * 1024 * 1024, 'max_total_bytes': 32 * 1024 * 1024}
SLUG = 'standard-delegated-hidden'
ENTRY = 'chialisp_oxide_puzzles::standard_delegated_hidden::puzzle'
SCOPE = 'Recorded source/Cargo-inventory/template/SDK consistency only. Origins remain opaque. No new compiler/VM/SDK execution, source/binary authentication, BLS verification, compiler preservation, live-network or whole-page credit.'
BINDINGS = {'template': 'template-report.json', 'request': 'request.json', 'report': 'report.json', 'repeat': 'report-repeat.json', 'source_before': 'source-before.json', 'source_after': 'source-after.json', 'tools_before': 'tools-before.json', 'tools_after': 'tools-after.json'}
BUNDLES = {'rust-off': 'bundles/rust-off', 'rust-on': 'bundles/rust-on'}


def require(value, message):
    if not value:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def relative(name):
    require(type(name) is str and 0 < len(name) <= 1024 and '\\' not in name
            and PurePosixPath(name).as_posix() == name and not name.startswith('/')
            and all(part not in ('', '.', '..') for part in name.split('/')), 'confined packet path')
    return name


def read(path, maximum=PROFILE['max_file_bytes']):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and 0 <= info.st_size <= maximum,
            'bounded regular single-link member')
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) == info.st_size and len(raw) <= maximum, 'member changed during bounded read')
    return raw


def discover(root):
    require(root.is_dir() and not root.is_symlink(), 'regular packet root')
    pending = [(root, 0)]
    files = {}
    work = total = 0
    while pending:
        folder, depth = pending.pop()
        require(depth <= 64, 'packet depth')
        with os.scandir(folder) as entries:
            for entry in entries:
                work += 1
                require(work <= 8192, 'packet discovery work')
                path = Path(entry.path)
                name = relative(path.relative_to(root).as_posix())
                info = entry.stat(follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    pending.append((path, depth + 1))
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
                            'no packet symlink/hardlink/special member')
                    require(len(files) < PROFILE['max_files'] and info.st_size <= PROFILE['max_file_bytes'], 'packet file bound')
                    total += info.st_size
                    require(total <= PROFILE['max_total_bytes'], 'complete physical precharge')
                    files[name] = info.st_size
    return files


ROOT_SOURCES = {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml'}
CRITICAL_SOURCES = ROOT_SOURCES | {
    'puzzles/authoring.json', 'puzzles/Cargo.toml', 'puzzles/lib.rs',
    'puzzles/standard-delegated-hidden/puzzle.json',
    'puzzles/standard-delegated-hidden/puzzle.rs',
    'puzzles/standard-delegated-hidden/puzzle.rue',
    'puzzles/standard-delegated-hidden/reference.clsp',
    'puzzles/standard-delegated-hidden/fixture_public.json',
    'crates/chialisp-oxide-corpus/Cargo.toml',
    'crates/chialisp-oxide-corpus/examples/standard_tutorial_ledger.rs',
    'crates/chialisp-oxide-simulator/Cargo.toml',
    'crates/chialisp-oxide-simulator/src/lib.rs',
    'experiments/standard-tutorial/prepare_request.py',
    'experiments/standard-tutorial/sdk_reader.py',
    'experiments/standard-tutorial/test_sdk_reader.py',
    'experiments/standard-tutorial/packet.py',
    # These producer/CLI files are required even while their source is being
    # prepared. Capture cannot read this window until all required files exist.
    'experiments/standard-tutorial/capture.py',
    'experiments/standard-tutorial/check.py',
    'scripts/cargo_authoring.py', 'scripts/cargo_bundle.py',
    'scripts/cargo_source_receipt.py', 'scripts/port_evidence.py',
    'scripts/raw_tree.py', 'scripts/source_walk.py', 'scripts/vm_failures.py',
    'scripts/registry_codegen_evidence.py',
}


def declared_source_prefix(path):
    return path in ROOT_SOURCES or path.startswith(('crates/', 'puzzles/', 'corpus/upstream/',
        'corpus/reference-providers/')) or path == 'experiments/precommit-controls/naive.clsp'


def add_paths(result, paths):
    for path in paths:
        path = relative(path)
        require(path in result or len(result) < 4096, 'source path precharge')
        result.add(path)


def validate_authoring(authoring):
    """Validate the complete captured declaration without opening origin paths."""
    sdk_reader.closed(authoring, ('mode', 'package', 'manifest', 'library', 'exports', 'sources'), 'authoring')
    require(authoring['mode'] == 'cargo_library' and authoring['package'] == 'chialisp-oxide-puzzles'
            and authoring['manifest'] == 'puzzles/Cargo.toml' and authoring['library'] == 'puzzles/lib.rs',
            'exact captured Cargo authoring profile')
    exports = authoring['exports']
    require(type(exports) is dict and len(exports) == 48, 'bounded complete48 exports')
    entries = set()
    required = set(ROOT_SOURCES) | {'puzzles/authoring.json', 'puzzles/Cargo.toml', 'puzzles/lib.rs'}
    for slug, row in exports.items():
        require(type(slug) is str and 0 < len(slug) <= 128 and re.fullmatch('[a-z0-9]+(?:-[a-z0-9]+)*', slug), 'bounded export slug')
        sdk_reader.closed(row, ('entry', 'source'), 'authoring export')
        entry = 'chialisp_oxide_puzzles::' + slug.replace('-', '_') + '::puzzle'
        source = 'puzzles/' + slug + '/puzzle.rs'
        require(row['entry'] == entry and row['source'] == source and entry not in entries, 'exact unique export/source identity')
        entries.add(entry)
        required.add(source)
    require(SLUG in exports and exports[SLUG]['entry'] == ENTRY, 'required selected export')
    sources = authoring['sources']
    require(type(sources) is list and 0 < len(sources) <= 4096, 'bounded authoring source inventory')
    fingerprints = {}
    for row in sources:
        sdk_reader.closed(row, ('path', 'sha256'), 'authoring source')
        path = relative(row['path'])
        require(declared_source_prefix(path), 'reviewed authoring source prefix')
        require(path not in fingerprints and type(row['sha256']) is str
                and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'unique captured source fingerprint')
        fingerprints[path] = row['sha256']
    require(list(fingerprints) == sorted(fingerprints) and required <= set(fingerprints), 'ordered complete authoring source closure')
    return fingerprints


def manifest_paths(manifest):
    """Selected policy dependencies and provenance files, never source URLs."""
    require(type(manifest) is dict and manifest.get('slug') == SLUG, 'selected source manifest')
    result = {'puzzles/' + SLUG + '/' + name for name in ('puzzle.json', 'puzzle.rs', 'puzzle.rue', 'reference.clsp')}
    dependencies = manifest.get('source_dependencies', [])
    require(type(dependencies) is list and len(dependencies) <= 4096, 'bounded selected dependencies')
    for path in dependencies:
        add_paths(result, (path,))
    includes = manifest.get('reference_includes', {})
    require(type(includes) is dict and len(includes) <= 128, 'bounded selected include map')
    for path in includes.values():
        add_paths(result, (path,))
    pending = [(manifest.get('provenance'), 0)]
    work = 0
    while pending:
        value, depth = pending.pop()
        require(work < 262144 and depth <= 96, 'bounded selected provenance work')
        work += 1
        if type(value) is dict:
            # Match the shared port checker: snapshot takes precedence over an
            # upstream-relative path such as LICENSE in the same declaration.
            paths = [value[key] for key in ('snapshot', 'path', 'manifest') if type(value.get(key)) is str]
            if paths:
                add_paths(result, (paths[0],))
            if 'license_snapshot' in value:
                add_paths(result, (value['license_snapshot'],))
            require(len(value) <= 4096, 'bounded provenance object')
            require(len(value) <= 262144 - work - len(pending), 'provenance object precharge')
            pending.extend((child, depth + 1) for child in value.values())
        elif type(value) is list:
            require(len(value) <= 4096, 'bounded provenance list')
            require(len(value) <= 262144 - work - len(pending), 'provenance list precharge')
            pending.extend((child, depth + 1) for child in value)
        require(len(result) <= 4096 and len(pending) <= 262144 - work, 'provenance path/work precharge')
    require(all(declared_source_prefix(path) for path in result), 'reviewed selected provenance source prefix')
    return result


def allowed_source(path, declared):
    if path in ROOT_SOURCES or path in declared:
        return True
    if path.startswith(('crates/', 'puzzles/', 'experiments/backend/')):
        return path.endswith('.rs') or PurePosixPath(path).name == 'Cargo.toml'
    return path.count('/') == 1 and path.startswith('scripts/') and path.endswith('.py') or (path.count('/') == 2 and path.startswith('experiments/standard-tutorial/') and path.endswith('.py'))


def current_read(root, name):
    path = root
    for part in relative(name).split('/'):
        path /= part
        require(not path.is_symlink(), 'current source symlink')
    return read(path)


def source_paths(root, authoring):
    """Bounded current source path set; original producer origins are never used.

    These declared/required files are not a complete external build graph or a
    filesystem-authentication contract. All required producer files must exist
    before the capture caller reads the returned paths.
    """
    root = Path(root).resolve()
    fingerprints = validate_authoring(authoring)
    result = set()
    add_paths(result, CRITICAL_SOURCES)
    add_paths(result, fingerprints)
    add_paths(result, source_files(root, ('crates', 'puzzles', 'experiments/backend'),
                                 suffixes=('.rs',), names=('Cargo.toml', 'build.rs')))
    add_paths(result, source_files(root, ('scripts', 'experiments/standard-tutorial'),
                                 suffixes=('.py',), recursive=False))
    manifest = decode_json(current_read(root, 'puzzles/' + SLUG + '/puzzle.json'))
    add_paths(result, manifest_paths(manifest))
    require(len(result) <= 4096, 'complete current source count')
    return sorted(result)


def artifact_wire(artifact, label):
    """Canonical CLVM byte/tree consistency only, not executable semantics."""
    sdk_reader.closed(artifact, ('backend', 'optimize_lir', 'bytes', 'clvm_hex', 'tree_hash', 'disassembly'), label)
    require(type(artifact['bytes']) is int and 0 < artifact['bytes'] <= sdk_reader.MAX_MODULE_BYTES, label + ': decoded byte precharge')
    require(type(artifact['optimize_lir']) is bool and type(artifact['disassembly']) is str and artifact['disassembly'], label + ': mode/disassembly')
    raw = sdk_reader._hex(artifact['clvm_hex'], sdk_reader.MAX_MODULE_BYTES, label + '/program')
    require(len(raw) == artifact['bytes'], label + ': exact serialized byte count')
    tree = sdk_reader.decode(raw)
    require(sdk_reader.encode(tree) == raw and artifact['tree_hash'] == sdk_reader.tree_hash(tree).hex(), label + ': canonical tree/hash')
    return raw


def compiled_inventory_wire(artifact, label):
    """Complete Cargo inventory uses existing package transport depth, not SDK data depth."""
    sdk_reader.closed(artifact, ('backend', 'optimize_lir', 'bytes', 'clvm_hex', 'tree_hash', 'disassembly'), label)
    require(type(artifact['bytes']) is int and 0 < artifact['bytes'] <= sdk_reader.MAX_MODULE_BYTES, label + ': decoded byte precharge')
    require(type(artifact['optimize_lir']) is bool and type(artifact['disassembly']) is str and artifact['disassembly'], label + ': mode/disassembly')
    raw = sdk_reader._hex(artifact['clvm_hex'], sdk_reader.MAX_MODULE_BYTES, label + '/program')
    require(len(raw) == artifact['bytes'], label + ': exact serialized byte count')
    # The existing independent hash-only inspector caps depth at package defaults
    # (package/src/limits.rs). Keep the narrower byte/node caps of this packet.
    checked = inspect_compiled_artifact(raw, max_bytes=sdk_reader.MAX_MODULE_BYTES,
                                       max_nodes=sdk_reader.MAX_NODES, max_depth=4096)
    require(artifact['tree_hash'] == checked['identity']['module_tree_hash'], label + ': canonical tree/hash')
    return raw


class Packet:
    def __init__(self, root):
        self.root = Path(root).absolute()
        physical = discover(self.root)  # Before receipt or payload allocation.
        self.index = decode_json(read(self.root / 'receipt.json'))
        sdk_reader.closed(self.index, ('protocol', 'status', 'profile', 'scope', 'origin', 'compiler', 'authoring', 'bindings', 'bundles', 'files'), 'packet receipt')
        require(self.index['protocol'] == PROTOCOL and self.index['status'] == 'captured' and self.index['scope'] == SCOPE, 'packet protocol/status/scope')
        require(self.index['profile'] == PROFILE and all(type(x) is int for x in self.index['profile'].values()), 'exact packet profile')
        require(self.index['bindings'] == BINDINGS and self.index['bundles'] == BUNDLES, 'fixed packet bindings')
        rows = self.index['files']
        require(type(rows) is dict and 0 < len(rows) < PROFILE['max_files'], 'bounded complete inventory')
        remaining = PROFILE['max_total_bytes'] - physical['receipt.json']
        for name, row in rows.items():
            relative(name)
            sdk_reader.closed(row, ('bytes', 'sha256'), 'member identity')
            n = row['bytes']
            require(name != 'receipt.json' and type(n) is int and 0 <= n <= PROFILE['max_file_bytes'] and n <= remaining,
                    'complete declared byte precharge')
            remaining -= n
            require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'member hash')
        require(set(physical) == {'receipt.json', *rows}, 'closed physical inventory')
        self.rows = rows
        for name, row in rows.items():
            raw = read(self.root / name, row['bytes'])
            require(len(raw) == row['bytes'] and sha(raw) == row['sha256'], 'complete member byte identity')

    def raw(self, name):
        name = relative(name)
        require(name in self.rows, 'declared packet member')
        return read(self.root / name, self.rows[name]['bytes'])

    def load(self, name):
        return decode_json(self.raw(name))


def check_cargo_options(options, origin):
    """Exact reviewed producer argv only; never normalize paths or flags."""
    require(type(options) is list and type(origin) is str, 'reviewed normal Cargo options')
    require(options in (
        ['--locked', '--manifest-path', origin + '/puzzles/Cargo.toml'],
        ['--manifest-path', origin + '/puzzles/Cargo.toml', '--locked', '--offline'],
        ['--manifest-path', 'puzzles/Cargo.toml', '--package', 'chialisp-oxide-puzzles', '--locked'],
    ), 'reviewed normal Cargo options')


def bundle(packet, lane, template):
    """Check every IR declaration/blob/source attachment before selecting one.

    This is the existing Python transport reader, not fresh Rust Program
    semantic validation or IR/CLVM equivalence. Actual compilation is a retained
    observation of the source capture, never an execution authenticated here.
    """
    prefix = BUNDLES[lane]
    pointer = packet.load(prefix + '/latest.json')
    require(type(pointer) is dict and set(pointer) == {'bundle', 'manifest'}, 'bundle pointer')
    name = relative(pointer['bundle'])
    require('/' not in name and pointer['manifest'] == name + '/manifest.json', 'named bundle pointer')
    folder = prefix + '/' + name
    manifest = packet.load(folder + '/manifest.json')
    origin = packet.index['origin']['source_root']
    require(manifest['schema_version'] == 2 and type(manifest['schema_version']) is int
            and manifest['workspace_root'] == origin and manifest['manifest_path'] == origin + '/puzzles/Cargo.toml'
            and manifest['package'] == 'chialisp-oxide-puzzles'
            and manifest['status'] == 'wip_prototype_not_ready_for_use', 'complete producer schema2 package')
    require(manifest['compiler'] == packet.index['compiler']['version']
            and manifest['driver_sha256'] == packet.index['compiler']['driver_sha256']
            and manifest['toolchain'] == 'nightly-2026-07-14'
            and manifest['rue_revision'] == '245c107a8e795229ea08097142f005a4c0d91075', 'compiler/driver/backend relation')
    hosts = [line[6:] for line in packet.index['compiler']['version'].splitlines() if line.startswith('host: ')]
    require(len(hosts) == 1 and hosts[0] and manifest['analysis_target'] == hosts[0], 'captured compiler host/analysis-target relation')
    options = manifest['cargo_options']
    check_cargo_options(options, origin)
    reader = cargo_bundle._Reader()
    optimize = lane == 'rust-on'
    declared = cargo_bundle._preflight_ir_declarations(manifest, reader, optimize)
    exports = packet.index['authoring']['exports']
    require(len(exports) == 48 and set(declared) == {r['entry'] for r in exports.values()}, 'complete48 export identity')
    loaded, paths, required = {}, set(), {'manifest.json'}
    for row in manifest['entries']:
        value = cargo_bundle._artifact_details(row, packet.root / folder, reader, paths, optimize, 2, ir_declaration=declared[row['entry']])
        compiled_inventory_wire(row['artifact'], 'complete bundle ' + lane + '/' + row['entry'])
        loaded[row['entry']] = (row, value)
        stem = sha(row['entry'].encode())
        require(row['ir'] == stem + '.ir.json' and row['clvm'] == stem + '.clvm.hex', 'deterministic entry paths')
        require(packet.load(folder + '/' + stem + '.artifact.json') == row['artifact'], 'artifact companion')
        require(packet.raw(folder + '/' + stem + '.clvm.txt') == (row['artifact']['disassembly'] + '\n').encode(), 'disassembly companion')
        required.update((row['ir'], row['clvm'], stem + '.artifact.json', stem + '.clvm.txt'))
    details = {entry: (row['ir'], value[3], value[2]) for entry, (row, value) in loaded.items()}
    sources = validate_receipt(manifest, packet.root / folder, details, reader, paths)
    fingerprints = {s['path']: s['sha256'] for s in packet.index['authoring']['sources']}
    for source in sources:
        require(source.label.startswith(('package/', 'workspace/')), 'captured source label')
        path = 'puzzles/' + source.label[8:] if source.label.startswith('package/') else source.label[10:]
        require(fingerprints.get(path) == sha(source.contents), 'complete captured authoring attachment')
    required.update(row['blob'] for row in manifest['source_receipt']['files'])
    actual = {p.removeprefix(folder + '/') for p in packet.rows if p.startswith(folder + '/')}
    require(actual == required, 'closed complete bundle inventory')
    require({p for p in packet.rows if p.startswith(prefix + '/')} == {prefix + '/latest.json', *(folder + '/' + p for p in required)}, 'closed bundle root')
    selected, (wire, _, _, _) = loaded[ENTRY]
    require(wire.get('environment', 'strict') == 'raw'
            and wire['program']['parameters'] == [{'name': 'environment', 'ty': {'kind': 'tree'}}]
            and wire['program']['result'] == {'kind': 'tree'}, 'selected raw single-tree structural ABI')
    want = next(a for a in template['artifacts'] if a['backend'] == 'rust_mir' and a['optimize_lir'] is optimize)
    require(selected['artifact'] == want, 'fresh template to complete bundle artifact relation')
    return bytes.fromhex(selected['artifact']['clvm_hex'])


def validate(root, *, current_root=None):
    packet = Packet(root)
    fingerprints = validate_authoring(packet.index['authoring'])
    origin = packet.index['origin']
    sdk_reader.closed(origin, ('source_root', 'full_report_path', 'full_report_sha256'), 'opaque producer origin')
    require(all(type(origin[k]) is str and origin[k].startswith('/') and len(origin[k]) <= 4096 for k in ('source_root', 'full_report_path')),
            'bounded opaque origins')
    require(type(origin['full_report_sha256']) is str and re.fullmatch('[0-9a-f]{64}', origin['full_report_sha256']) is not None, 'full report captured digest')
    sdk_reader.closed(packet.index['compiler'], ('version', 'driver_sha256'), 'captured compiler identity')
    require(type(packet.index['compiler']['version']) is str and len(packet.index['compiler']['version']) <= 16384
            and packet.index['compiler']['version'].startswith('rustc ')
            and type(packet.index['compiler']['driver_sha256']) is str and re.fullmatch('[0-9a-f]{64}', packet.index['compiler']['driver_sha256']), 'compiler identity syntax')
    before = packet.load(BINDINGS['source_before'])
    require(before == packet.load(BINDINGS['source_after']) and type(before) is dict and 0 < len(before) <= 4096, 'quiet declared source window')
    manifest = packet.load('sources/puzzles/' + SLUG + '/puzzle.json')
    selected_paths = manifest_paths(manifest)
    required = set(CRITICAL_SOURCES) | set(fingerprints) | selected_paths
    require(required <= set(before), 'required SDK/authoring/policy source closure')
    allowed_declared = set(fingerprints) | selected_paths
    require(all(allowed_source(path, allowed_declared) for path in before), 'reviewed captured source prefixes')
    for path, digest in before.items():
        relative(path)
        require(type(digest) is str and re.fullmatch('[0-9a-f]{64}', digest) and sha(packet.raw('sources/' + path)) == digest, 'captured local source bytes')
    require({p.removeprefix('sources/') for p in packet.rows if p.startswith('sources/')} == set(before), 'closed captured source set')
    tools = packet.load(BINDINGS['tools_before'])
    require(tools == packet.load(BINDINGS['tools_after']) and type(tools) is dict
            and set(tools) == {'cargo-chialisp-oxide', 'chialisp-oxide-driver', 'chialisp-oxide', 'standard_tutorial_ledger'}, 'quiet captured tool window')
    for name, row in tools.items():
        sdk_reader.closed(row, ('path', 'bytes', 'sha256'), 'opaque tool identity')
        require(type(row['path']) is str and row['path'].startswith('/') and type(row['bytes']) is int
                and 0 < row['bytes'] <= 512 * 1024 * 1024 and type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'tool identity bound')
    require(tools['chialisp-oxide-driver']['sha256'] == packet.index['compiler']['driver_sha256'], 'independent driver byte relation')
    report = packet.load(BINDINGS['template'])
    require(len(report['puzzles']) == 1 and report['puzzles'][0]['slug'] == SLUG
            and report['authoring'] == packet.index['authoring'], 'declared one-row template projection')
    read_source = lambda path: packet.raw('sources/' + relative(path))
    template = port_evidence.puzzle_row(report, SLUG)
    artifacts = template['artifacts']
    require(type(artifacts) is list and len(artifacts) == 5, 'five bounded template artifacts')
    # The shared port checker uses a recursive legacy codec. Charge canonical
    # tree depth/nodes/bytes first for every template before invoking it.
    for artifact in artifacts:
        artifact_wire(artifact, 'template artifact')
    port = port_evidence.validate_port(report, SLUG, reader=read_source)
    expected = {lane: bundle(packet, lane, template) for lane in BUNDLES}
    for lane, backend, mode in [('chialisp', 'chialisp', True), ('rue-off', 'rue_source', False), ('rue-on', 'rue_source', True)]:
        artifact = next(a for a in template['artifacts'] if a['backend'] == backend and a['optimize_lir'] is mode)
        expected[lane] = bytes.fromhex(artifact['clvm_hex'])
    require(packet.raw(BINDINGS['report']) == packet.raw(BINDINGS['repeat']), 'independent SDK reports byte-identical')
    sdk = sdk_reader.validate(packet.load(BINDINGS['request']), packet.load(BINDINGS['report']), expected)
    current = False
    if current_root is not None:
        current_root = Path(current_root).resolve()
        require(set(before) == set(source_paths(current_root, packet.index['authoring'])), 'exact current required source path set')
        for path, digest in before.items():
            require(sha(current_read(current_root, path)) == digest, 'current declared source differs: ' + path)
        full_raw = read(current_root / 'corpus/validation.json', 32 * 1024 * 1024)
        full = decode_json(full_raw)
        require(sha(full_raw) == origin['full_report_sha256'], 'current complete report digest differs')
        cargo_authoring.check_recorded(full['authoring'], current_root, [p['slug'] for p in full['puzzles']])
        require(len(full['puzzles']) == 48 and port_evidence.puzzle_row(full, SLUG) == template, 'current48 template relation')
        current = True
    downloads = [{'path': 'receipt.json', 'bytes': len(read(packet.root / 'receipt.json')), 'sha256': sha(read(packet.root / 'receipt.json'))}, *({'path': name, **row} for name, row in packet.rows.items())]
    return {'status': 'recorded_standard_tutorial_consistency_checked', 'protocol': PROTOCOL, 'current_source_checked': current,
            'counts': sdk['counts'], 'primary_observations': sdk['primary_observations'], 'modules': sdk['modules'],
            'port': port, 'complete_cargo_entries_checked': 96, 'physical_files': len(downloads),
            'bytes': sum(row['bytes'] for row in downloads), 'downloads': downloads,
            'execution_credit': False, 'bls_verification': False, 'scope': SCOPE}
