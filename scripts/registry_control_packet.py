"""Bounded registry control transport and complete Cargo inventory reader.

No build, VM, network or origin-path reads occur on import/recorded validation.
Checks complete recorded Cargo inventories, exact 105 command/log relations,
native/template outputs and SDK records. Actual Rust Program validation and
recompilation remain retained capture observations, never new reader execution.
"""
import hashlib, json, os, re, shutil, stat, tempfile
from pathlib import Path, PurePosixPath
from cargo_source_receipt import decode_json, validate_receipt
import cargo_bundle
PROTOCOL = 'chialisp-oxide-registry-controls-packet/v1'
PRODUCTION_PROTOCOL = 'chialisp-oxide-registry-controls-packet/v2'
MAX_FILES = 1024
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 16 * 1024 * 1024
# Explicit caller-selected host executables are not transport members. Cargo
# legitimately hardlinks its built binaries; retain the already reviewed host
# executable ceiling without changing any packet/member/aggregate allowance.
MAX_EXECUTABLE = 512 * 1024 * 1024
PROFILE = {'max_files': MAX_FILES, 'max_file_bytes': MAX_FILE, 'max_total_bytes': MAX_TOTAL}
SCOPE = 'Recorded consistency only. Origins are opaque metadata, never paths followed by the recorded reader. Partial base captures and repeat aliases add no whole-corpus, execution-authentication, compiler-preservation or production-readiness credit.'
ORIGINS = {'source_root', 'capture', 'native', 'last', 'base', 'controls', 'drafts'}
NAMES = ('alpha-drop-send', 'alpha-drop-create', 'beta-drop-send', 'beta-drop-create')

def require(v, msg):
    if not v:
        raise ValueError(msg)

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

def relative(value):
    require(type(value) is str and 0 < len(value) <= 512 and re.fullmatch('[A-Za-z0-9_./-]+', value), 'bounded ASCII packet path')
    p = PurePosixPath(value)
    require(not p.is_absolute() and p.as_posix() == value and all((x not in ('.', '..') for x in p.parts)), 'confined packet path')
    return value

def absolute(value):
    require(type(value) is str and 0 < len(value) <= 1024 and value.startswith('/') and (str(PurePosixPath(value)) == value) and ('..' not in PurePosixPath(value).parts), 'opaque absolute origin')
    return value

def regular_read(path, maximum=MAX_FILE):
    s = path.lstat()
    require(stat.S_ISREG(s.st_mode) and s.st_nlink == 1 and (0 <= s.st_size <= maximum), 'bounded regular unaliased file')
    with path.open('rb') as f:
        raw = f.read(maximum + 1)
    require(len(raw) == s.st_size and len(raw) <= maximum, 'file changed during bounded read')
    return raw

def executable_identity(path, maximum=MAX_EXECUTABLE):
    """Hash one explicitly selected host binary without packet-file policy.

    The replay caller canonicalizes its explicit CLI selection once, preserving
    existing alias-selection semantics. This descriptor open rejects a leaf
    symlink introduced afterward. Parent-directory mutation is outside the
    trusted local build-directory assumption, not a filesystem sandbox claim.
    Hardlinks are admitted only in this host role, never by regular_read.
    """
    path = Path(path)
    require(type(maximum) is int and 0 < maximum <= MAX_EXECUTABLE,
            'fixed host executable byte ceiling')
    require(hasattr(os, 'O_NOFOLLOW') and hasattr(os, 'O_NONBLOCK'),
            'host executable hashing requires nofollow/nonblocking descriptors')
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(descriptor)
        require(stat.S_ISREG(before.st_mode) and 0 < before.st_size <= maximum,
                'bounded regular host executable before hashing')
        hasher = hashlib.sha256()
        count = 0
        while True:
            block = os.read(descriptor, min(64 * 1024, maximum - count + 1))
            if not block:
                break
            require(len(block) <= maximum - count, 'host executable grew beyond byte ceiling')
            count += len(block)
            hasher.update(block)
        after = os.fstat(descriptor)
        metadata = lambda info: (info.st_dev, info.st_ino, info.st_size,
                                 info.st_mtime_ns, info.st_ctime_ns)
        require(stat.S_ISREG(after.st_mode) and metadata(before) == metadata(after)
                and count == before.st_size, 'host executable changed during bounded hashing')
        return {'path': str(path), 'bytes': count, 'sha256': hasher.hexdigest()}
    finally:
        os.close(descriptor)

def discover(root):
    require(root.is_dir() and (not root.is_symlink()), 'regular packet root')
    pending = [(root, 0)]
    result = []
    seen = 0
    while pending:
        d, depth = pending.pop()
        require(depth <= 16, 'packet depth')
        with os.scandir(d) as entries:
            for entry in entries:
                seen += 1
                require(seen <= MAX_FILES * 4, 'discovery work bound')
                p = Path(entry.path)
                relative(p.relative_to(root).as_posix())
                s = entry.stat(follow_symlinks=False)
                if stat.S_ISDIR(s.st_mode):
                    pending.append((p, depth + 1))
                else:
                    require(stat.S_ISREG(s.st_mode) and s.st_nlink == 1, 'regular unaliased packet member')
                    require(len(result) < MAX_FILES, 'physical file bound')
                    result.append(p)
    return sorted(result)

def strict_equal(left, right):
    return json.dumps(left, sort_keys=True, separators=(',', ':'), allow_nan=False) == json.dumps(right, sort_keys=True, separators=(',', ':'), allow_nan=False)

def identity(raw):
    return {'bytes': len(raw), 'sha256': digest(raw)}

def encode(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + '\n').encode()

class Packet:

    def __init__(self, root):
        self.root = Path(root).absolute()
        require(self.root.is_dir() and (not self.root.is_symlink()), 'regular packet root before index read')
        self.root = self.root.resolve(strict=True)
        index_raw = regular_read(self.root / 'report.json')
        self.index = decode_json(index_raw)
        require(type(self.index) is dict and set(self.index) == {'protocol', 'profile', 'origins', 'files', 'aliases', 'scope'}, 'closed packet index')
        require(self.index['protocol'] in (PROTOCOL, PRODUCTION_PROTOCOL) and self.index['profile'] == PROFILE and all((type(v) is int for v in self.index['profile'].values())) and (self.index['scope'] == SCOPE), 'exact packet protocol/profile/scope')
        self.code_key = 'drafts' if self.index['protocol'] == PROTOCOL else 'capture_sources'
        self.code_prefix = 'drafts' if self.code_key == 'drafts' else 'capture-sources'
        expected_origins = ORIGINS if self.code_key == 'drafts' else ORIGINS - {'drafts'} | {'capture_sources'}
        require(type(self.index['origins']) is dict and set(self.index['origins']) == expected_origins, 'closed versioned opaque origins')
        for name, value in self.index['origins'].items():
            if name == self.code_key:
                require(type(value) is list and len(value) == 3, 'three opaque draft origins')
                [absolute(x) for x in value]
            else:
                absolute(value)
        require(type(self.index['files']) is list and len(self.index['files']) < MAX_FILES, 'complete payload count')
        self.rows = {}
        remaining = MAX_TOTAL - len(index_raw)
        for row in self.index['files']:
            require(type(row) is dict and set(row) == {'path', 'bytes', 'sha256'}, 'closed payload identity')
            name = relative(row['path'])
            n = row['bytes']
            require(name != 'report.json' and name not in self.rows and (type(n) is int) and (0 <= n <= MAX_FILE) and (n <= remaining), 'complete payload precharge')
            require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'payload digest')
            remaining -= n
            self.rows[name] = row
        aliases = self.index['aliases']
        require(type(aliases) is dict and len(aliases) <= MAX_FILES, 'bounded aliases')
        for name, target in aliases.items():
            relative(name)
            relative(target)
            require(name not in self.rows and name != 'report.json' and (target in self.rows) and ('/bundles/' not in name) and (not name.startswith('controls/')), 'one-step nonbundle nonsource aliases')
        actual = discover(self.root)
        require({p.relative_to(self.root).as_posix() for p in actual} == set(self.rows) | {'report.json'}, 'exact physical inventory')
        remaining = MAX_TOTAL
        for p in actual:
            n = p.lstat().st_size
            require(n <= MAX_FILE and n <= remaining, 'complete physical precharge')
            remaining -= n
        for name, row in self.rows.items():
            require(identity(regular_read(self.root / name, row['bytes'])) == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'complete payload byte identity')

    def raw(self, name):
        name = relative(name)
        name = self.index['aliases'].get(name, name)
        require(name in self.rows, 'declared logical payload')
        return regular_read(self.root / name, self.rows[name]['bytes'])

    def load(self, name):
        return decode_json(self.raw(name))

    def origin_identity(self, row):
        """Resolve captured origin metadata to retained bytes without opening any host origin."""
        require(type(row) is dict and set(row) == {'path', 'bytes', 'sha256'}, 'closed retained origin identity')
        absolute(row['path'])
        mappings = [(self.index['origins'][key], key) for key in ['capture', 'native', 'last', 'base', 'controls']]
        mappings.extend(((p, self.code_prefix + '/' + str(n) + '-' + PurePosixPath(p).name) for n, p in enumerate(self.index['origins'][self.code_key])))
        for origin, prefix in sorted(mappings, key=lambda x: len(x[0]), reverse=True):
            if row['path'] == origin and prefix.startswith(self.code_prefix + '/'):
                name = prefix
            elif row['path'].startswith(origin + '/'):
                name = prefix + '/' + relative(row['path'][len(origin) + 1:])
            else:
                continue
            raw = self.raw(name)
            require(type(row['bytes']) is int and identity(raw) == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'opaque origin-to-retained-byte relation')
            return raw
        raise ValueError('origin identity outside declared retained origins')

def admit_bundles(packet):
    """Admit all four complete schema-2 inventories before selecting eight controls.
This checks retained wire/byte relations, never invokes Program validation."""
    capture = packet.load('capture/capture/capture.json')
    base = packet.load('base/report.json')
    window = capture['source_window']
    require(packet.code_key in window and ('capture_sources' if packet.code_key == 'drafts' else 'drafts') not in window, 'exact versioned capture code inventory shape')
    if packet.code_key == 'capture_sources':
        expected_code = [packet.index['origins']['source_root'] + '/scripts/' + name for name in ('registry_control_capture.py', 'run-registry-source-controls.py', 'registry_source_controls.py')]
        require(packet.index['origins']['capture_sources'] == expected_code and [row['path'] for row in window['capture_sources']] == expected_code, 'exact production capture source roles')
    require(type(capture['vm']) is list and strict_equal([(v['name'], v['optimize_lir']) for v in capture['vm']], [(name, mode) for mode in (False, True) for name in NAMES]), 'exact eight ordered unique template observations')
    workspace = base['authoring']
    control = capture['source_window']['controls']
    admitted = {}
    bundle_summaries = []
    require(len(workspace['exports']) == 40 and set(control['exports']) == set(NAMES), 'complete baseline/control export declarations')
    compiler = packet.raw('capture/capture/logs/00-compiler.stdout').decode().strip()
    host = [x[6:] for x in compiler.splitlines() if x.startswith('host: ')]
    require(len(host) == 1, 'one compiler host')
    for family in ['baseline', 'controls']:
        for mode in ['off', 'on']:
            key = family + '-' + mode
            row = capture['bundles'][key]
            manifest_name = 'capture/capture/' + relative(row['manifest'])
            raw = packet.raw(manifest_name)
            require(digest(raw) == row['manifest_sha256'], 'captured manifest identity')
            manifest = decode_json(raw)
            directory = packet.root / str(PurePosixPath(manifest_name).parent)
            authoring = workspace if family == 'baseline' else control
            origin = packet.index['origins']['source_root'] if family == 'baseline' else packet.index['origins']['controls']
            manifest_path = origin + ('/puzzles/Cargo.toml' if family == 'baseline' else '/Cargo.toml')
            require(type(manifest['schema_version']) is int and manifest['schema_version'] == 2 and (manifest['status'] == 'wip_prototype_not_ready_for_use') and (manifest['toolchain'] == 'nightly-2026-07-14') and (manifest['rue_revision'] == '245c107a8e795229ea08097142f005a4c0d91075') and (manifest['workspace_root'] == origin) and (manifest['manifest_path'] == manifest_path) and (manifest['package'] == authoring['package']), 'exact bundle origin/package/schema')
            require(manifest['compiler'].strip() == compiler and manifest['analysis_target'] == host[0] and (manifest['driver_sha256'] == capture['tools']['chialisp-oxide-driver']['sha256']) and (manifest['cargo_options'] == (['--locked', '--manifest-path', manifest_path] if family == 'baseline' else ['--manifest-path', manifest_path, '--package', 'chialisp-oxide-registry-controls', '--locked', '--offline'])), 'exact compiler/driver/Cargo relation')
            reader = cargo_bundle._Reader()
            declarations = cargo_bundle._preflight_ir_declarations(manifest, reader, mode == 'on')
            require(set(declarations) == {r['entry'] for r in authoring['exports'].values()}, 'all exports admitted before selection')
            details = {}
            paths = set()
            required = {'manifest.json'}
            rows = {}
            for entry in manifest['entries']:
                program, path, sha, size = cargo_bundle._artifact_details(entry, directory, reader, paths, mode == 'on', 2, ir_declaration=declarations[entry['entry']])
                details[entry['entry']] = (entry['ir'], size, sha)
                stem = digest(entry['entry'].encode())
                require(entry['ir'] == stem + '.ir.json' and entry['clvm'] == stem + '.clvm.hex', 'deterministic export names')
                prefix = directory.relative_to(packet.root).as_posix()
                companion = packet.load(prefix + '/' + stem + '.artifact.json')
                cargo_bundle._artifact_metadata(companion, mode == 'on', 2)
                require(companion == entry['artifact'] and packet.raw(prefix + '/' + stem + '.clvm.txt') == (companion['disassembly'] + '\n').encode(), 'exact artifact/disassembly companions')
                required.update([entry['ir'], entry['clvm'], stem + '.artifact.json', stem + '.clvm.txt'])
                rows[entry['entry']] = (entry, program)
            attachments = validate_receipt(manifest, directory, details, reader, paths)
            fingerprints = {r['path']: r['sha256'] for r in authoring['sources']}
            for source in attachments:
                label = source.label
                if family == 'baseline':
                    require(label.startswith('package/') or label.startswith('workspace/'), 'baseline attachment label')
                    source_path = 'puzzles/' + label[8:] if label.startswith('package/') else label[10:]
                    require(fingerprints.get(source_path) == digest(source.contents), 'baseline attachment matches recorded full authoring closure')
                else:
                    require(label.startswith('package/') and source.contents == packet.raw('controls/' + relative(label[8:])), 'control actual source attachment relation')
            required.update((r['blob'] for r in manifest['source_receipt']['files']))
            require({p.relative_to(directory).as_posix() for p in discover(directory)} == required, 'exact complete bundle physical inventory')
            parent = directory.parent
            pointer = packet.load(parent.relative_to(packet.root).as_posix() + '/latest.json')
            require(pointer == {'bundle': directory.name, 'manifest': directory.name + '/manifest.json'}, 'exact complete bundle pointer')
            require({p.relative_to(parent).as_posix() for p in discover(parent)} == {'latest.json', *(directory.name + '/' + n for n in required)}, 'no undeclared bundle-root member')
            admitted[key] = rows
            bundle_summaries.append({'family': family, 'mode': mode, 'exports': len(rows), 'attachments': len(attachments)})
    for item in capture['vm']:
        entry = control['exports'][item['name']]['entry']
        bundle = 'controls-' + ('on' if item['optimize_lir'] else 'off')
        artifact, program = admitted[bundle][entry]
        require(item['artifact'] == artifact['artifact'] and program.get('ir_version') == 5 and (program.get('environment') == 'strict'), 'selected actual control artifact and strict pair envelope')
    return {'bundles': bundle_summaries, 'exports_admitted': sum((len(x) for x in admitted.values())), 'selected_template_artifacts': len(capture['vm'])}

def registry_references(base):
    """Select exactly the three registry references in protocol order, even from a full40 report."""
    import registry_protocol
    refs = []
    for slug in registry_protocol.SLUGS:
        rows = [row for row in base['puzzles'] if row['slug'] == slug]
        require(len(rows) == 1, 'exact single registry reference row')
        row = rows[0]
        artifacts = [a for a in row['artifacts'] if a['backend'] == 'chialisp']
        require(len(artifacts) == 1, 'exact Chialisp reference artifact')
        refs.append({'slug': slug, 'source': row['reference'], 'module_hex': artifacts[0]['clvm_hex']})
    return refs

def recorded_semantics(packet):
    """Check actual retained command/native/template/SDK record crosslinks without executing tools."""
    import registry_protocol
    import registry_source_controls
    from vm_failures import check_execution
    capture = packet.load('capture/capture/capture.json')
    base = packet.load('base/report.json')
    window = capture['source_window']
    require(packet.code_key in window and ('capture_sources' if packet.code_key == 'drafts' else 'drafts') not in window, 'exact versioned capture code inventory shape')
    if packet.code_key == 'capture_sources':
        expected_code = [packet.index['origins']['source_root'] + '/scripts/' + name for name in ('registry_control_capture.py', 'run-registry-source-controls.py', 'registry_source_controls.py')]
        require(packet.index['origins']['capture_sources'] == expected_code and [row['path'] for row in window['capture_sources']] == expected_code, 'exact production capture source roles')
    require(type(capture['vm']) is list and strict_equal([(v['name'], v['optimize_lir']) for v in capture['vm']], [(name, mode) for mode in (False, True) for name in NAMES]), 'exact eight ordered unique template observations')
    base_result = registry_protocol.validate_report(base, sidecar_root=packet.root / 'base')
    require(capture['source_window']['workspace']['authoring'] == base['authoring'], 'complete captured baseline authoring relation')
    for row in capture['source_window']['base_files_current'] + capture['source_window'][packet.code_key] + capture['source_window']['retained_gate_files_current']:
        packet.origin_identity(row)
    controls = capture['source_window']['controls']
    fingerprints = {r['path']: r['sha256'] for r in controls['sources']}
    require({n.removeprefix('controls/') for n in packet.rows if n.startswith('controls/')} == set(fingerprints), 'complete private control package inventory')
    for name, sha in fingerprints.items():
        require(digest(packet.raw('controls/' + relative(name))) == sha, 'private source package identity')
    original = {f"puzzles/{r['slug']}/puzzle.rs": r['source'].encode() for r in base['puzzles']}
    for row in base['puzzles']:
        for source in row['source_dependencies']:
            original[source['path']] = source['source'].encode()
    recipes = packet.load('controls/fixtures/source-recipes.json')
    for row in recipes['variants']:
        raw = original[row['original']['path']]
        require(identity(raw) == {k: row['original'][k] for k in ['bytes', 'sha256']}, 'exact original mutation recipe source')
        text = raw.decode()
        for edit in row['source_edits']:
            require(type(edit['matches']) is int and edit['matches'] == 1 and (text.count(edit['before']) == 1), 'exact single source mutation')
            text = text.replace(edit['before'], edit['after'], 1)
        expected = (row['additional_doc_prefix'] + text).encode()
        relative_variant = row['variant']['path'].removeprefix('experiments/action-registry-source-controls/')
        actual = packet.raw('controls/' + relative(relative_variant))
        require(actual == expected and identity(actual) == {k: row['variant'][k] for k in ['bytes', 'sha256']}, 'actual exact mutated source bytes')
    require(packet.raw('controls/src/registry_recipe.rs') == original['puzzles/registry_recipe.rs'], 'exact original shared recipe copy')
    receipt = packet.load('native/receipt.json')
    native = packet.load('native/actual-native-observations.json')
    vectors = packet.load('controls/fixtures/native-vectors.json')
    counts = {'baseline_policy_calls': 4, 'variant_policy_calls': 4, 'abi_observations': 0, 'VM_executions': 0, 'SDK_submissions': 0}
    require(receipt['status'] == 'passed' and receipt['source_and_tool_window_quiet'] is True and strict_equal(receipt['native_counts'], counts) and strict_equal(native['summary'], counts) and strict_equal(capture['native']['counts'], counts), 'exact native counts and gate receipt')
    require(capture['native']['receipt'] == identity(packet.raw('native/receipt.json')) and capture['native']['observations'] == native['observations'], 'actual native receipt/output relation')
    require(packet.load('native/frozen-source-before.json') == packet.load('native/frozen-source-after.json') == capture['native']['sources'], 'quiet complete native source inventory')
    native_sources = capture['native']['sources']
    require(type(native_sources) is list and 0 < len(native_sources) <= 4096, 'bounded complete native source declarations')
    native_by_path = {}
    remaining_source_bytes = 64 * 1024 * 1024
    for source in native_sources:
        require(type(source) is dict and set(source) == {'path', 'bytes', 'sha256'}, 'closed native source declaration')
        name = relative(source['path'])
        require(name not in native_by_path and type(source['bytes']) is int and (0 < source['bytes'] <= 16 * 1024 * 1024) and (source['bytes'] <= remaining_source_bytes), 'native source inventory precharge')
        require(type(source['sha256']) is str and re.fullmatch('[0-9a-f]{64}', source['sha256']), 'native source digest spelling')
        remaining_source_bytes -= source['bytes']
        native_by_path[name] = source
    expected_source_order = sorted(native_by_path, key=PurePosixPath) if packet.index['protocol'] == PROTOCOL else sorted(native_by_path)
    require(list(native_by_path) == expected_source_order, 'versioned ordered complete native source declarations')
    expected_sources = sorted(native_sources, key=lambda row: row['path'])
    for declaration in (capture['source_window']['complete_native_last_sources_current'], capture['source_window']['admitted_inputs']['native']['sources'], capture['source_window']['admitted_inputs']['last']['sources']):
        require(type(declaration) is list and len(declaration) == len(native_sources) and strict_equal(sorted(declaration, key=lambda row: row['path']), expected_sources), 'all captured native/LAST complete source declarations agree')
    if packet.code_key == 'capture_sources':
        for source in capture['source_window']['capture_sources']:
            name = 'scripts/' + PurePosixPath(source['path']).name
            require(name in native_by_path and {key: native_by_path[name][key] for key in ('bytes', 'sha256')} == {key: source[key] for key in ('bytes', 'sha256')}, 'production capture source bytes bind complete actual native window')
    require(packet.load('native/tools-before.json') == packet.load('native/tools-after.json'), 'quiet native tool inventory')
    require([x['name'] for x in receipt['gates']] == ['recipe-before', 'fmt', 'check', 'test', 'clippy', 'recipe-after'] and all((type(x['exit_code']) is int and x['exit_code'] == 0 for x in receipt['gates'])), 'all six successful native gates')
    package = packet.index['origins']['controls']
    common = ['--manifest-path', package + '/Cargo.toml', '--all-targets', '--locked', '--offline', '--features', 'native-vm', '-j', '2']
    expected_native = [['python3', package + '/verify_sources.py'], ['cargo', 'fmt', '--manifest-path', package + '/Cargo.toml', '-p', 'chialisp-oxide-registry-controls', '--', '--check'], ['cargo', 'check', *common], ['cargo', 'test', *common, '--', '--nocapture', '--test-threads=1'], ['cargo', 'clippy', *common, '--', '-D', 'warnings'], ['python3', package + '/verify_sources.py']]
    require([x['command'] for x in receipt['gates']] == expected_native, 'exact native recipe/fmt/check/test/Clippy commands')
    for row in receipt['gates']:
        packet.raw('native/' + row['name'] + '.stdout')
        packet.raw('native/' + row['name'] + '.stderr')
    last = packet.load('last/receipt.json')
    require(last['status'] == 'passed' and last['source_and_tool_window_quiet'] is True and (last['toolchain'] == 'nightly-2026-07-14') and (type(last['jobs']) is int) and (last['jobs'] == 2), 'actual final host build gate')
    require(len(last['gates']) == 1 and last['gates'][0]['command'] == ['cargo', 'build', '--workspace', '--bins', '--examples', '--locked', '--offline', '-j', '2'] and (type(last['gates'][0]['exit_code']) is int) and (last['gates'][0]['exit_code'] == 0), 'exact LAST workspace build command')
    require(packet.load('last/frozen-source-before.json') == packet.load('last/frozen-source-after.json') == capture['native']['sources'], 'LAST/native source quiet relation')
    require(packet.load('last/tools-before.json') == packet.load('last/tools-after.json'), 'LAST compiler tool quiet relation')
    require(set(capture['tools']) == {'chialisp-oxide-driver', 'cargo-chialisp-oxide', 'chialisp-oxide', 'chialisp-oxide-verifier', 'registry_controls'}, 'closed five captured tool roles')
    built_tools = packet.load('last/built-tool-identities.json')
    if packet.index['protocol'] == PROTOCOL:
        require(last['built_tools'] == built_tools, 'historical LAST embedded executable inventory')
    else:
        require('built_tools' not in last and capture['source_window']['admitted_inputs']['last']['built'] == built_tools, 'production LAST sidecar executable inventory')
    require(type(built_tools) is list and sorted((t['name'] for t in built_tools)) == sorted(['chialisp-oxide-driver', 'cargo-chialisp-oxide', 'chialisp-oxide', 'chialisp-oxide-verifier', 'examples/registry_controls']), 'exact five unique LAST tool names')
    for tool in built_tools:
        key = 'registry_controls' if tool['name'] == 'examples/registry_controls' else tool['name']
        want = capture['tools'][key]
        require(set(tool) == {'name', 'path', 'bytes', 'sha256'} and set(want) == {'path', 'bytes', 'sha256'}, 'closed actual executable identity roles')
        require({k: tool[k] for k in ['path', 'bytes', 'sha256']} == want and type(tool['bytes']) is int and (0 < tool['bytes'] <= 512 * 1024 * 1024), 'opaque LAST captured executable identity relation')
    require(len(native['observations']) == len(vectors['cases']) == 4, 'four exact actual native output pairs')
    native_stdout = packet.raw('native/test.stdout').decode()
    parsed = [decode_json(line[line.index('REGISTRY_RUST_CONTROL_NATIVE ') + len('REGISTRY_RUST_CONTROL_NATIVE '):].encode()) for line in native_stdout.splitlines() if 'REGISTRY_RUST_CONTROL_NATIVE ' in line]
    require(parsed == native['observations'], 'actual native stdout exact output records')
    native_summaries = [decode_json(line[line.index('REGISTRY_RUST_CONTROL_NATIVE_SUMMARY ') + len('REGISTRY_RUST_CONTROL_NATIVE_SUMMARY '):].encode()) for line in native_stdout.splitlines() if 'REGISTRY_RUST_CONTROL_NATIVE_SUMMARY ' in line]
    require(strict_equal(native_summaries, [counts]), 'one exact strictly typed native summary marker')
    test_results = re.findall('^test result: (\\w+)\\. (\\d+) passed; (\\d+) failed', native_stdout, re.M)
    require(test_results and all((status == 'ok' and failed == '0' for status, _, failed in test_results)) and (sum((int(passed) for _, passed, _ in test_results)) == 2), 'two nonzero successful actual native tests')
    for actual, want in zip(native['observations'], vectors['cases'], strict=True):
        require(actual['id'] == want['id'] and actual['baseline'] == want['baseline_expected'] and (actual['variant'] == want['variant_expected']), 'native independent output fixture relation')
    commands = capture['commands']
    require(type(commands) is list and len(commands) == 105, 'complete105commands')
    by_role = {}
    origin = packet.index['origins']['capture'] + '/capture'
    alllogs = set()
    allrecords = set()
    for n, row in enumerate(commands):
        require(type(row) is dict and set(row) == {'role', 'argv', 'exit_code', 'stdout', 'stderr'} and (type(row['exit_code']) is int) and (row['exit_code'] == 0) and (row['role'] not in by_role), 'closed successful unique command record')
        require(packet.load(f'capture/capture/command-records/{n + 1:02}.json') == row, 'numbered command record binding')
        allrecords.add(f'capture/capture/command-records/{n + 1:02}.json')
        for stream in ['stdout', 'stderr']:
            name = f"logs/{n:02}-{row['role']}.{stream}"
            require(row[stream] == name, 'exact ordered command log name')
            packet.raw('capture/capture/' + name)
            alllogs.add('capture/capture/' + name)
        by_role[row['role']] = row
    require({n for n in packet.rows if n.startswith('capture/capture/logs/')} == alllogs and {n for n in packet.rows if n.startswith('capture/capture/command-records/')} == allrecords, 'closed complete command/log inventory')
    expected_roles = {'compiler', 'driver', 'source-recipes', 'sdk-1', 'sdk-2'}
    require(by_role['compiler']['argv'] == ['rustup', 'run', 'nightly-2026-07-14', 'rustc', '-vV'] and by_role['driver']['argv'] == [capture['tools']['chialisp-oxide-driver']['path'], '--compiler-version'], 'exact compiler version commands')
    require(by_role['source-recipes']['argv'] == ['python3', '-B', packet.index['origins']['controls'] + '/verify_sources.py'], 'exact recipe verification command')
    for key, bundle in capture['bundles'].items():
        family, mode = key.split('-')
        authoring = base['authoring'] if family == 'baseline' else controls
        manifest = packet.load('capture/capture/' + bundle['manifest'])
        entry_map = {e['entry']: e for e in manifest['entries']}
        prefix = str(PurePosixPath(bundle['manifest']).parent)
        options = manifest['cargo_options']
        expected_roles.add('extract-' + key)
        require(by_role['extract-' + key]['argv'] == [capture['tools']['cargo-chialisp-oxide']['path'], 'build', *options, '--output-dir', origin + '/bundles/' + family + '/' + mode, *(['--no-optimize'] if mode == 'off' else [])], 'exact full library extraction command')
        recompiled = bundle.get('recompiled')
        if family == 'controls':
            require(recompiled is None, 'control observations use command logs directly')
            recompiled = [{'entry': e['entry'], 'role': 'recompile-' + next((k for k, v in authoring['exports'].items() if v['entry'] == e['entry'])) + '-' + mode, 'optimize': mode == 'on', 'bytes': e['artifact']['bytes'], 'tree_hash': e['artifact']['tree_hash']} for e in manifest['entries']]
        require(len(recompiled) == len(entry_map), 'complete actual recompile observations')
        observed = set()
        for row in recompiled:
            require(row['entry'] in entry_map and row['entry'] not in observed, 'complete unique recompiled entry')
            observed.add(row['entry'])
            entry = entry_map[row['entry']]
            slug = next((k for k, v in authoring['exports'].items() if v['entry'] == row['entry']))
            role = f'recompile-{family}-{slug}-{mode}' if family == 'baseline' else f'recompile-{slug}-{mode}'
            expected_roles.add(role)
            require(row['role'] == role and row['optimize'] is (mode == 'on') and (row['bytes'] == entry['artifact']['bytes']) and (row['tree_hash'] == entry['artifact']['tree_hash']), 'recorded recompile metric crosslink')
            require(by_role[role]['argv'] == [capture['tools']['chialisp-oxide-verifier']['path'], origin + '/' + prefix + '/' + entry['ir'], '--artifacts'], 'exact actual recompile command')
            artifacts = packet.load('capture/capture/' + by_role[role]['stdout'])
            require(type(artifacts) is list and len(artifacts) == 2, 'two actual verifier mode artifacts')
            selected = [a for a in artifacts if a['optimize'] is (mode == 'on')]
            require(len(selected) == 1 and all((selected[0][k] == entry['artifact'][k] for k in ['bytes', 'tree_hash', 'clvm_hex'])), 'actual retained recompiled artifact bytes')
    for item in capture['vm']:
        name = item['name']
        mode = 'on' if item['optimize_lir'] else 'off'
        role = 'template-' + name + '-' + mode
        expected_roles.add(role)
        bundle = capture['bundles']['controls-' + mode]
        manifest = packet.load('capture/capture/' + bundle['manifest'])
        entry = next((e for e in manifest['entries'] if e['entry'] == controls['exports'][name]['entry']))
        prefix = str(PurePosixPath(bundle['manifest']).parent)
        require(by_role[role]['argv'] == [capture['tools']['chialisp-oxide']['path'], 'run-batch', origin + '/' + prefix + '/' + entry['clvm'], '--solutions', origin + '/inputs/' + name + '-' + mode + '.json', '--max-cost', '10000000'], 'exact template execution command')
        require(packet.load('capture/capture/inputs/' + name + '-' + mode + '.json') == [item['input']] and packet.load('capture/capture/' + by_role[role]['stdout']) == [item['execution']], 'actual template input/output log relation')
        check_execution(item['execution'], 'registry source control template')
        want = next((v for v in vectors['cases'] if v['id'] == name))
        require(item['input'] == want['environment_assembly'] and item['execution']['outcome'] == want['variant_expected'], 'template actual native expected output relation')
    require(set(by_role) == expected_roles, 'exact all105command roles')
    ordered_roles = ['compiler', 'driver', 'source-recipes']
    require(set(capture['bundles']) == {'baseline-off', 'baseline-on', 'controls-off', 'controls-on'}, 'closed four bundle roles')
    for family in ('baseline', 'controls'):
        for mode in ('off', 'on'):
            ordered_roles.append(f'extract-{family}-{mode}')
            bundle = capture['bundles'][family + '-' + mode]
            manifest = packet.load('capture/capture/' + bundle['manifest'])
            if family == 'baseline':
                for entry in manifest['entries']:
                    slug = next((k for k, v in base['authoring']['exports'].items() if v['entry'] == entry['entry']))
                    ordered_roles.append(f'recompile-baseline-{slug}-{mode}')
            else:
                ordered_roles.extend((f'recompile-{name}-{mode}' for name in NAMES))
    ordered_roles.extend((f'template-{name}-{mode}' for mode in ('off', 'on') for name in NAMES))
    ordered_roles.extend(['sdk-1', 'sdk-2'])
    require([row['role'] for row in commands] == ordered_roles and len(ordered_roles) == 105, 'exact extraction/recompile/template/SDK command sequence')
    request = packet.load('capture/capture/sdk-request.json')
    sdk1 = packet.raw('capture/capture/sdk-1.json')
    sdk2 = packet.raw('capture/capture/sdk-2.json')
    require(sdk1 == sdk2, 'byte-identical actual SDK repeat')
    for number in [1, 2]:
        row = by_role['sdk-' + str(number)]
        require(row['argv'] == [capture['tools']['registry_controls']['path'], origin + '/sdk-request.json'] and packet.raw('capture/capture/' + row['stdout']) == sdk1, 'actual SDK runner command/output binding')
    for item in capture['vm']:
        selected = next((x for x in request['runs'] if x['name'] == item['name'] and x['optimize_lir'] is item['optimize_lir']))
        require(selected['module_hex'] == item['artifact']['clvm_hex'], 'SDK request to retained template artifact')
    refs = registry_references(base)
    sdk = decode_json(sdk1)
    index = packet.load('base/registry-slot-protocol/index.json')
    sdk_result = registry_source_controls.validate_capture(sdk, request, refs, index['header']['canonical_components'])
    require(base['execution_vm'] == 'clvmr 0.21.0 MEMPOOL_MODE' and base['vm_failure_protocol'] == 'clvmr-0.21.0/v1', 'exact template and component primary VM profile')
    return {'component_vm': base['execution_vm'], 'component_failure_protocol': base['vm_failure_protocol'], 'native_counts': counts, 'templates': capture['vm'], 'sdk_capture': sdk, 'repeat_byte_identical': True, 'counts': sdk_result['counts'], 'commands_checked': len(commands), 'base_protocol': base_result}

def validate(root, *, current_callback=None):
    """Run the caller current gate before any historical bundle/record presentation."""
    packet = Packet(root)
    if current_callback is not None:
        current_callback(packet)
    bundles = admit_bundles(packet)
    semantics = recorded_semantics(packet)
    return {'status': 'recorded_consistency_checked', 'physical_files': len(packet.rows) + 1, 'bytes': sum((p.stat().st_size for p in discover(packet.root))), **bundles, **semantics, 'index_sha256': digest(regular_read(packet.root / 'report.json')), 'downloads': [{'path': 'report.json', **identity(regular_read(packet.root / 'report.json'))}, *packet.index['files']], 'current_callback_executed': current_callback is not None, 'scope': 'Recorded source/native/template/SDK/105command and full88Cargo inventory consistency. Actual Program validation/recompilation are retained observations, not new reader execution. No authentication, proof or whole40/current publication credit without the external current gate.'}

def install(plan, destination, origins):
    """Materialize unchanged declared inputs only after complete bounded precharge."""
    require(type(plan) is list and 0 < len(plan) < MAX_FILES, 'plan count')
    names = set()
    rows = []
    total = 0
    for source, name in plan:
        relative(name)
        require(name not in names and name != 'report.json', 'unique planned payload')
        names.add(name)
        p = Path(source)
        s = p.lstat()
        require(stat.S_ISREG(s.st_mode) and s.st_nlink == 1 and (s.st_size <= MAX_FILE) and (s.st_size <= MAX_TOTAL - total), 'complete input precharge before read/copy')
        total += s.st_size
        rows.append((p, name, s.st_size))
    identities = [{'path': name, **identity(regular_read(p, n))} for p, name, n in rows]
    protocol = PROTOCOL if set(origins) == ORIGINS else PRODUCTION_PROTOCOL
    require(set(origins) in (ORIGINS, ORIGINS - {'drafts'} | {'capture_sources'}), 'explicit historical or production origin profile')
    index = {'protocol': protocol, 'profile': PROFILE, 'origins': origins, 'files': identities, 'aliases': {}, 'scope': SCOPE}
    raw = encode(index)
    require(len(raw) <= MAX_FILE and total + len(raw) <= MAX_TOTAL, 'complete transport budget including index')
    destination = Path(destination)
    require(not destination.exists() and (not destination.is_symlink()), 'fresh destination')
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.registry-packet-', dir=destination.parent))
    try:
        for (p, name, n), want in zip(rows, identities, strict=True):
            data = regular_read(p, n)
            require(identity(data) == {'bytes': want['bytes'], 'sha256': want['sha256']}, 'source quiet during materialization')
            target = stage / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        (stage / 'report.json').write_bytes(raw)
        result = validate(stage)
        os.rename(stage, destination)
        return result
    finally:
        if stage.exists():
            shutil.rmtree(stage)

def install_capture(source_root, capture_root, native_dir, last_dir, base_dir, destination):
    """Build a bounded complete transport from explicitly supplied quiet host inputs.

    The base is an actual three-row report carrying all40 authoring declarations;
    the current full40 report is a separate callback input, never a packet member.
    This function performs no build, VM, SDK or network operation.
    """
    source_root, capture_root, native_dir, last_dir, base_dir = [Path(value).resolve() for value in (source_root, capture_root, native_dir, last_dir, base_dir)]
    record = decode_json(regular_read(capture_root / 'capture/capture.json'))
    window = record['source_window']
    require(('drafts' in window) != ('capture_sources' in window), 'one explicit capture code profile')
    code_key = 'drafts' if 'drafts' in window else 'capture_sources'
    code_prefix = 'drafts' if code_key == 'drafts' else 'capture-sources'
    require(type(window[code_key]) is list and len(window[code_key]) == 3, 'three exact captured source roles before origin reads')
    if code_key == 'capture_sources':
        require([row['path'] for row in window[code_key]] == [str(source_root / 'scripts' / name) for name in ('registry_control_capture.py', 'run-registry-source-controls.py', 'registry_source_controls.py')], 'production installer reads only exact declared current source scripts')
    controls = source_root / 'experiments/action-registry-source-controls'
    origins = {'source_root': str(source_root), 'capture': str(capture_root), 'native': str(native_dir), 'last': str(last_dir), 'base': str(base_dir), 'controls': str(controls), code_key: [row['path'] for row in window[code_key]]}
    require((base_dir / 'report.json').lstat().st_size <= MAX_FILE, 'packet base must be actual partial3 report below2MiB; full40 remains external, never silently projected')
    require([row['slug'] for row in decode_json(regular_read(base_dir / 'report.json'))['puzzles']] == ['registry-initialize', 'registry-set-alpha', 'registry-set-beta'], 'actual retained partial3 base required; no full40 embedding or implicit projection')
    plan = []

    def collect(directory, prefix, excluded=()):
        pending = [(directory, 0)]
        work = 0
        while pending:
            current, depth = pending.pop()
            require(depth <= 16, 'source inventory depth')
            with os.scandir(current) as entries:
                for entry in entries:
                    work += 1
                    require(work <= MAX_FILES * 4, 'source discovery work bound')
                    if entry.name in excluded:
                        continue
                    path = Path(entry.path)
                    info = entry.stat(follow_symlinks=False)
                    name = prefix + '/' + relative(path.relative_to(directory).as_posix())
                    if stat.S_ISDIR(info.st_mode):
                        pending.append((path, depth + 1))
                    else:
                        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, 'regular source transport member')
                        require(len(plan) < MAX_FILES - 1, 'source member count before accumulation')
                        plan.append((str(path), name))
    collect(capture_root, 'capture')
    collect(native_dir, 'native', {'frozen-test-executables', '__pycache__'})
    collect(last_dir, 'last', {'__pycache__'})
    collect(base_dir, 'base')
    collect(controls, 'controls', {'target', 'node_modules', '.git', '__pycache__'})
    for number, row in enumerate(window[code_key]):
        require(type(row) is dict and set(row) == {'path', 'bytes', 'sha256'}, 'exact captured code identity')
        actual = Path(absolute(row['path']))
        require(identity(regular_read(actual)) == {'bytes': row['bytes'], 'sha256': row['sha256']}, 'capture code unchanged before packaging')
        plan.append((str(actual), code_prefix + '/' + str(number) + '-' + actual.name))
    return install(sorted(plan, key=lambda row: row[1]), destination, origins)

def corruption_controls(root):
    """Construct private retained-record mutations after complete caller admission.

    These controls never execute a compiler, native policy, VM or SDK. They are
    separate from actual source mutants and the SDK record reader's controls.
    """
    validate(root)
    original = Packet(root)
    results = []
    with tempfile.TemporaryDirectory(prefix='registry-packet-controls-') as temporary:
        copied = Path(temporary).resolve() / 'packet'
        shutil.copytree(original.root, copied)
        index_raw = regular_read(copied / 'report.json')
        index_value = decode_json(index_raw)

        def reject(label, action, expected=None):
            try:
                action()
            except (ValueError, KeyError, TypeError, FileNotFoundError) as error:
                require(expected is None or expected in str(error), 'control failed at unexpected gate: ' + label)
                results.append(label)
            else:
                raise ValueError('accepted constructed corruption: ' + label)

        def index_control(label, mutate):
            value = decode_json(index_raw)
            mutate(value)
            (copied / 'report.json').write_bytes(encode(value))
            try:
                reject(label, lambda: Packet(copied))
            finally:
                (copied / 'report.json').write_bytes(index_raw)
        index_control('boolean-size', lambda value: value['files'][0].update(bytes=True))
        index_control('oversized-size', lambda value: value['files'][0].update(bytes=MAX_FILE + 1))
        index_control('aggregate-precharge', lambda value: [row.update(bytes=MAX_FILE) for row in value['files'][:9]])
        index_control('duplicate-identity', lambda value: value['files'].append(value['files'][0]))
        index_control('unconfined-path', lambda value: value['files'][0].update(path='../escape'))
        index_control('unknown-index-field', lambda value: value.update(invented=True))
        index_control('boolean-profile', lambda value: value['profile'].update(max_files=True))
        index_control('alias-chain', lambda value: value['aliases'].update({'x': 'y', 'y': value['files'][0]['path']}))
        index_control('bundle-alias', lambda value: value['aliases'].update({'capture/capture/bundles/extra': value['files'][0]['path']}))
        extra = copied / 'undeclared'
        extra.write_bytes(b'')
        reject('undeclared-physical-file', lambda: Packet(copied))
        extra.unlink()
        first = copied / index_value['files'][0]['path']
        raw = regular_read(first)
        first.write_bytes(raw + b'x')
        reject('changed-payload-bytes', lambda: Packet(copied))
        first.write_bytes(raw)
        symlink = copied / 'symlink'
        symlink.symlink_to(first)
        reject('symlink-member', lambda: Packet(copied))
        symlink.unlink()
        hardlink = copied / 'hardlink'
        os.link(first, hardlink)
        reject('hardlink-member', lambda: Packet(copied))
        hardlink.unlink()

        def payload_control(label, mutate, expected=None, checker=validate):
            changed, renames = ({}, {})

            def load(name):
                return decode_json(regular_read(copied / relative(name)))

            def put(name, value):
                changed[relative(name)] = encode(value)
            mutate(load, put, changed, renames)
            capture_name = 'capture/capture/capture.json'
            capture = decode_json(changed[capture_name]) if capture_name in changed else load(capture_name)
            origins = index_value['origins']

            def patch_relations(value, path, data):
                if type(value) is dict:
                    if value.get('path') == path and {'bytes', 'sha256'} <= set(value):
                        value.update(identity(data))
                    for child in value.values():
                        patch_relations(child, path, data)
                elif type(value) is list:
                    for child in value:
                        patch_relations(child, path, data)
            for name, data in list(changed.items()):
                for prefix in ('native', 'last', 'base', 'controls'):
                    if name.startswith(prefix + '/'):
                        patch_relations(capture, origins[prefix] + '/' + name[len(prefix) + 1:], data)
            put(capture_name, capture)
            before = {name: regular_read(copied / name) for name in set(changed) | set(renames)}
            value = decode_json(index_raw)
            for name, data in changed.items():
                (copied / name).write_bytes(data)
            for old, new in renames.items():
                (copied / old).unlink()
                (copied / new).write_bytes(before[old])
            for row in value['files']:
                row['path'] = renames.get(row['path'], row['path'])
                if row['path'] in changed:
                    row.update(identity(changed[row['path']]))
            (copied / 'report.json').write_bytes(encode(value))
            try:
                reject(label, lambda: checker(copied), expected)
            finally:
                for name in renames.values():
                    (copied / name).unlink()
                for name, data in before.items():
                    (copied / name).write_bytes(data)
                (copied / 'report.json').write_bytes(index_raw)
        capture_name = 'capture/capture/capture.json'
        capture = original.load(capture_name)
        manifest_name = 'capture/capture/' + capture['bundles']['baseline-on']['manifest']
        manifest = original.load(manifest_name)
        unselected = next((row for row in manifest['entries'] if 'registry_' not in row['entry']))
        ir = str(PurePosixPath(manifest_name).parent / unselected['ir'])

        def corrupt_ir(load, put, *_):
            value = load(ir)
            value.update(invented=True)
            put(ir, value)
        payload_control('unselected-actual-ir', corrupt_ir, checker=lambda value: admit_bundles(Packet(value)))

        def corrupt_native(load, put, *_):
            name = 'native/actual-native-observations.json'
            value = load(name)
            value['observations'][0]['variant']['output_hex'] = '80'
            put(name, value)
        payload_control('actual-native-output', corrupt_native)

        def corrupt_sdk(load, put, *_):
            name = 'capture/capture/sdk-1.json'
            value = load(name)
            value['record']['counts']['mutation_primary_observations'] = 11
            put(name, value)
        payload_control('actual-sdk-primary-count', corrupt_sdk)

        def duplicate_vm(load, put, *_):
            value = load(capture_name)
            value['vm'][7] = decode_json(encode(value['vm'][6]))
            put(capture_name, value)
        payload_control('duplicate-template-eight-rows', duplicate_vm, 'exact eight ordered unique')

        def reorder_vm(load, put, *_):
            value = load(capture_name)
            value['vm'][0], value['vm'][1] = (value['vm'][1], value['vm'][0])
            put(capture_name, value)
        payload_control('template-order', reorder_vm, 'exact eight ordered unique')

        def boolean_native_zero(load, put, *_):
            value = load(capture_name)
            value['native']['counts']['VM_executions'] = False
            put(capture_name, value)
        payload_control('boolean-native-zero-counter', boolean_native_zero, 'exact native counts')

        def extra_tool(load, put, *_):
            value = load(capture_name)
            value['tools']['invented'] = value['tools']['chialisp-oxide-driver']
            put(capture_name, value)
        payload_control('extra-tool-role', extra_tool, 'closed five captured tool')

        def duplicate_last_tool(load, put, *_):
            last = load('last/receipt.json')
            tools = load('last/built-tool-identities.json')
            tools[-1] = decode_json(encode(tools[0]))
            value = load(capture_name)
            if original.index['protocol'] == PROTOCOL:
                last['built_tools'] = tools
            else:
                value['source_window']['admitted_inputs']['last']['built'] = tools
            put('last/receipt.json', last)
            put('last/built-tool-identities.json', tools)
            put(capture_name, value)
        payload_control('duplicate-last-tool-coherent', duplicate_last_tool, 'exact five unique LAST tool')

        def shuffle_commands(load, put, changed, renames):
            value = load(capture_name)
            first_index = next((n for n, row in enumerate(value['commands']) if row['role'] == 'template-alpha-drop-send-off'))
            second_index = next((n for n, row in enumerate(value['commands']) if row['role'] == 'sdk-1'))
            value['commands'][first_index], value['commands'][second_index] = (value['commands'][second_index], value['commands'][first_index])
            for n in (first_index, second_index):
                row = value['commands'][n]
                for stream in ('stdout', 'stderr'):
                    old = 'capture/capture/' + row[stream]
                    new = f"capture/capture/logs/{n:02}-{row['role']}.{stream}"
                    renames[old] = new
                    row[stream] = new.removeprefix('capture/capture/')
                put(f'capture/capture/command-records/{n + 1:02}.json', row)
            put(capture_name, value)
        payload_control('sdk-template-command-shuffle-coherent', shuffle_commands, 'exact extraction/recompile/template/SDK command sequence')

        def remove_summary(load, put, changed, *_):
            name = 'native/test.stdout'
            lines = regular_read(copied / name).decode().splitlines()
            changed[name] = ('\n'.join((line for line in lines if 'REGISTRY_RUST_CONTROL_NATIVE_SUMMARY ' not in line)) + '\n').encode()
        payload_control('missing-native-summary-marker', remove_summary, 'one exact strictly typed native summary')

        def zero_tests(load, put, changed, *_):
            name = 'native/test.stdout'
            changed[name] = regular_read(copied / name).replace(b'2 passed; 0 failed', b'0 passed; 0 failed')
        payload_control('zero-native-successful-tests', zero_tests, 'two nonzero successful actual native tests')
        history = globals()['admit_bundles']
        seen = []

        def forbidden_history(_):
            seen.append(True)
            raise AssertionError('history ran before failed current gate')
        globals()['admit_bundles'] = forbidden_history
        try:
            reject('current-gate-before-history', lambda: validate(copied, current_callback=lambda _: require(False, 'current gate rejected')))
        finally:
            globals()['admit_bundles'] = history
        require(not seen, 'current rejection must precede historical admission')
        path_open = Path.open

        def packet_only_open(path, *args, **kwargs):
            require(path.absolute().is_relative_to(copied.absolute()), 'recorded reader attempted external origin read')
            return path_open(path, *args, **kwargs)
        Path.open = packet_only_open
        try:
            validate(copied)
        finally:
            Path.open = path_open
    require(len(results) == 25, 'complete constructed packet control inventory')
    import registry_source_controls
    sdk_controls = registry_source_controls.corruption_controls(original.load('capture/capture/sdk-1.json'), original.load('capture/capture/sdk-request.json'), registry_references(original.load('base/report.json')), original.load('base/registry-slot-protocol/index.json')['header']['canonical_components'])
    return {'constructed_corruption_controls': len(results), 'sdk_record_controls': sdk_controls, 'rejected': results, 'external_origin_reads': 0, 'scope': 'Constructed retained transport/record controls only; no native, VM, SDK or authentication credit.'}

def replay_templates(root, cli):
    """Run eight admitted retained templates through the caller's explicit VM CLI.

    This adds only retained-template VM replay observations. It never re-extracts,
    recompiles, executes native policies or submits an SDK spend.
    """
    import signal, subprocess, time
    from vm_failures import check_execution
    validate(root)
    packet = Packet(root)
    capture = packet.load('capture/capture/capture.json')
    vectors = packet.load('controls/fixtures/native-vectors.json')
    cli = Path(cli).resolve(strict=True)
    executable_before = executable_identity(cli)
    results = []
    with tempfile.TemporaryDirectory(prefix='registry-retained-template-replay-') as temporary:
        temporary = Path(temporary)
        for number, row in enumerate(capture['vm']):
            program = temporary / f'{number}.clvm.hex'
            solutions = temporary / f'{number}.solutions.json'
            program.write_text(row['artifact']['clvm_hex'] + '\n')
            solutions.write_bytes(encode([row['input']]))
            stdout_path = temporary / f'{number}.stdout'
            stderr_path = temporary / f'{number}.stderr'
            argv = [str(cli), 'run-batch', str(program), '--solutions', str(solutions), '--max-cost', '10000000']
            with stdout_path.open('xb') as stdout, stderr_path.open('xb') as stderr:
                process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True)
                start = time.monotonic()
                try:
                    while process.poll() is None:
                        require(time.monotonic() - start <= 120 and stdout_path.stat().st_size <= MAX_FILE and (stderr_path.stat().st_size <= MAX_FILE), 'retained replay resource limit: inconclusive')
                        time.sleep(0.05)
                except BaseException:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                    raise
            require(process.returncode == 0, 'actual retained replay CLI failure: inconclusive')
            executions = decode_json(regular_read(stdout_path))
            regular_read(stderr_path)
            require(type(executions) is list and len(executions) == 1, 'one exact retained template execution')
            execution = executions[0]
            check_execution(execution, 'registry retained template replay')
            expected = next((value for value in vectors['cases'] if value['id'] == row['name']))['variant_expected']
            require(strict_equal(execution, row['execution']) and strict_equal(execution['outcome'], expected), 'actual retained VM cost/failure/output differs from captured template and independent fixture')
            results.append({'name': row['name'], 'optimize_lir': row['optimize_lir'], 'cost': execution['cost']})
    require(len(results) == 8, 'eight retained template replays')
    require(executable_identity(cli) == executable_before, 'VM replay executable changed during observations')
    return {'retained_template_vm_replays': len(results), 'executable': executable_before, 'runs': results, 'scope': 'Actual retained-template VM replay only; no new extraction/native/SDK/live/proof credit.'}
