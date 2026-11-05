"""Independent finite raw-tree specification and exact published input identities.

Python hashlib/codec expectations are separate from the Rust/Rue sources and VM.
No execution, BLS, collision-resistance or compiler-preservation claim is made.
"""
import hashlib
import json
from pathlib import Path

from cargo_authoring import contained_file, unique_object
from hashing_provenance import check_tree, git_object
from hashing_vectors import deserialize, parse, proper, serialize, sha_tree

ROOT = Path(__file__).resolve().parents[1]
SLUG = 'p2-delegated-conditions'
DIRECTORY = f'puzzles/{SLUG}'
PIN = 'cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8'
BASE = f'corpus/upstream/chia-puzzles/{PIN}'
SOURCE = f'{BASE}/puzzles/p2_delegated_conditions.clsp'
INCLUDE = f'{BASE}/puzzles/condition_codes.clib'
ARTIFACT = SOURCE + '.hex'
VECTOR_SHA = '3b55d630d95dfd3b4527a55b49dc6d0c3b7370fc79a4d17f12dcfd58f000887b'
RUST_SHA = '8503a4c04ab10f3273b771e1fd378082b8b3b5230c5b708931818486046b89aa'
RUE_SHA = 'e42563791addf1a77bd0ed2d923a90e3d0eb8dc4da380e7d1622da58cf250d20'
CANONICAL_HASH = '0ff94726f1a8dea5c3f70d3121945190778d3b2b3fcda3735a1f290977e98341'
PINS = {
    SOURCE: ('a73671d6fe8f33bfc5e10b44b4ce96a30d2d4089a9c9f4df4350d7b18925d907', '5be64e2deaca4b5e050a88ec71caeaf648e82d03'),
    ARTIFACT: ('cc06953dcac90df9455d02105e5003df615cd73c78f116fffd47a68fb80a983c', '46f9abe76535644320ae9811679c024c39a6f0fb'),
    INCLUDE: ('891ea562e6fdb783bcfceea0cdea92318acfe4a67c4b6deb238d5fc4af15377f', 'a6a5d22b5883487b87b51f5cf3f66d63440d68cf'),
    BASE + '/LICENSE': ('7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a', '37b84d60dd8a34d854a41e94c34eaf587648ac6c'),
}
MODES = [('rust_mir', False), ('rust_mir', True), ('chialisp', True), ('rue_source', False), ('rue_source', True)]
NAMED = [
    'named-key0-conditions0', 'named-key0-conditions1', 'named-key0-conditions2',
    'named-key0-conditions6', 'named-key1-conditions4', 'named-key2-conditions3',
    'named-key3-conditions7', 'named-key4-conditions0', 'named-key5-conditions5',
    'named-key6-conditions8', 'ignored-outer-tail0', 'ignored-outer-tail1',
    'ignored-outer-tail2', 'ignored-outer-tail3', 'generated-127',
    'missing-projection-0', 'missing-projection-1', 'missing-projection-2',
    'missing-projection-3', 'missing-projection-4',
]
DEPENDENCIES = [f'crates/chialisp-oxide/src/{name}.rs' for name in ('lib', 'cons', 'tree', 'hashing')]
DEPENDENCIES += [DIRECTORY + '/independent-vectors.json', DIRECTORY + '/mutation-recipes.json', SOURCE, ARTIFACT, INCLUDE]
NATIVE = [f'crates/chialisp-oxide-corpus/{name}' for name in (
    'Cargo.toml', 'build.rs', 'src/lib.rs', 'src/native.rs', 'src/native_hash.rs', 'src/native_tree.rs',
    'src/p2_delegated_conditions.rs', 'src/p2_delegated_conditions/ledger.rs')]
NATIVE += ['crates/chialisp-oxide-simulator/Cargo.toml', 'crates/chialisp-oxide-simulator/src/lib.rs',
           'crates/chialisp-oxide-simulator/rejection-protocol.json']


def require(condition, message):
    if not condition:
        raise ValueError(message)


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), f'{label}: missing/unknown fields')


def read(root, path):
    path = contained_file(Path(root), path)
    with path.open('rb') as file:
        data = file.read(16 * 1024 * 1024 + 1)
    require(0 < len(data) <= 16 * 1024 * 1024, 'source input size bound')
    return data


def load(data):
    try:
        return json.loads(data, object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError('malformed delegated-conditions JSON') from error


def digest(data):
    return hashlib.sha256(data).hexdigest()


def expected_tree(environment, *, atom_domain=b'\x01', wrong_rest=False, strict_tail=False):
    if not isinstance(environment, tuple) or not isinstance(environment[1], tuple):
        return {'status': 'rejected'}
    key, (conditions, tail) = environment
    if strict_tail and tail != b'':
        return {'status': 'rejected'}
    result = (proper(b'\x32', key, sha_tree(conditions, atom_domain=atom_domain)),
              environment if wrong_rest else conditions)
    return {'status': 'accepted', 'output_hex': serialize(result).hex()}


def expected(solution, **options):
    require(type(solution) is str and 0 < len(solution) <= 16 * 1024, 'unbounded independent solution')
    return expected_tree(parse(solution), **options)


def vectors(reader):
    data = reader(DIRECTORY + '/independent-vectors.json')
    require(digest(data) == VECTOR_SHA, 'changed independent195/5 vector bytes')
    value = load(data)
    closed(value, ['schema_version', 'status', 'accepted', 'rejected', 'scope'], 'independent vectors')
    require(type(value['schema_version']) is int and value['schema_version'] == 1
            and value['status'] == 'independent_canonical_reference_preflight_not_Rust_port'
            and type(value['accepted']) is list and len(value['accepted']) == 195
            and type(value['rejected']) is list and len(value['rejected']) == 5
            and type(value['scope']) is str and value['scope'], 'independent vector contract')
    rows, names = [], set()
    for positive, entries in ((True, value['accepted']), (False, value['rejected'])):
        for row in entries:
            fields = ['label', 'solution', 'environment_hex'] + (
                ['expected_output_hex', 'key_hex', 'conditions_hex', 'conditions_tree_hash', 'outer_tail_hex']
                if positive else ['expected_failure'])
            closed(row, fields, 'independent row')
            require(type(row['label']) is str and row['label'] and row['label'] not in names, 'duplicate/missing vector label')
            names.add(row['label'])
            environment = parse(row['solution'])
            require(serialize(environment).hex() == row['environment_hex'], 'environment bytes/assembly differ')
            result = expected_tree(environment)
            if positive:
                require(result == {'status': 'accepted', 'output_hex': row['expected_output_hex']}, 'false independent raw result')
                key, (conditions, tail) = environment
                require(serialize(key).hex() == row['key_hex'] and serialize(conditions).hex() == row['conditions_hex']
                        and sha_tree(conditions).hex() == row['conditions_tree_hash']
                        and serialize(tail).hex() == row['outer_tail_hex'], 'false independent input decomposition')
            else:
                require(result == {'status': 'rejected'} and row['expected_failure'] == 'path_into_atom', 'negative must lack a projection')
            rows.append({'label': row['label'], 'solution': row['solution'], 'environment_hex': row['environment_hex'], 'expected': result})
    require(len(names) == 200 and len(NAMED) == len(set(NAMED)) == 20 and set(NAMED) <= names, 'named/generated partition')
    return rows


def mutation_recipes(reader, rows):
    source = reader(DIRECTORY + '/puzzle.rs').decode()
    spec = load(reader(DIRECTORY + '/mutation-recipes.json'))
    closed(spec, ['schema_version', 'protocol', 'source', 'source_sha256', 'scope', 'mutations'], 'source mutations')
    require(type(spec['schema_version']) is int and spec['schema_version'] == 1
            and spec['protocol'] == 'chialisp-oxide-p2dc-source-mutations/v1'
            and spec['source'] == DIRECTORY + '/puzzle.rs' and spec['source_sha256'] == RUST_SHA,
            'source mutation identity')
    require(type(spec['mutations']) is list and len(spec['mutations']) == 3 and type(spec['scope']) is str and spec['scope'], 'mutation contract')
    signature = "pub fn puzzle<'a>(environment: &'a Tree<'a>) -> DelegatedConditions<'a> {\n"
    recipes = [
        ('wrong_atom_domain', 'const ATOM_DOMAIN: [u8; 1] = [1];', 'const ATOM_DOMAIN: [u8; 1] = [2];',
         'named-key0-conditions0', 'accepted_output_differs', {'atom_domain': b'\x02'}),
        ('wrong_conditions_tail', '                rest: conditions,', '                rest: environment,',
         'named-key0-conditions0', 'accepted_output_differs', {'wrong_rest': True}),
        ('reject_ignored_improper_tail', signature,
         signature + '    assert!(matches!(environment, Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))));\n',
         'ignored-outer-tail3', 'conclusive_program_rejection', {'strict_tail': True}),
    ]
    by_name = {row['label']: row for row in rows}
    for actual, (name, before, after, witness, distinguishing, options) in zip(spec['mutations'], recipes, strict=True):
        require(actual == dict(name=name, before=before, after=after, witness=witness, distinguishing=distinguishing), 'unreviewed source mutation recipe')
        require(source.count(before) == 1 and before != after, 'source mutation anchor not unique/non-noop')
        base = by_name[witness]['expected']
        changed = expected(by_name[witness]['solution'], **options)
        require(base['status'] == 'accepted' and base != changed, 'source mutation witness is vacuous')
    return spec


def source_inputs(root=ROOT, *, reader=None):
    reader = reader or (lambda path: read(root, path))
    # Existing retained full-tree checker verifies Git commit/tree objects and
    # common license/include bytes. Bind this newly retained module's two blobs too.
    check_tree(reader)
    tree_path = f'corpus/upstream/published-puzzles/Chia-Network--chia_puzzles/{PIN}/tree.json'
    tree = load(reader(tree_path))
    entries = {row['path']: row for row in tree['entries']}
    for path, (sha, blob) in PINS.items():
        data = reader(path)
        require(digest(data) == sha and git_object('blob', data) == blob, 'changed published source/include/license/artifact')
        row = entries[path.removeprefix(BASE + '/')]
        require(row['kind'] == 'blob' and row['mode'] == '100644' and row['git_sha1'] == blob
                and row['size_bytes'] == len(data), 'false published blob membership')
    require(reader(DIRECTORY + '/reference.clsp') == reader(SOURCE), 'canonical source was adapted')
    require(digest(reader(DIRECTORY + '/puzzle.rs')) == RUST_SHA
            and digest(reader(DIRECTORY + '/puzzle.rue')) == RUE_SHA, 'unreviewed Rust/Rue port source')
    canonical = bytes.fromhex(reader(ARTIFACT).decode().strip())
    require(len(canonical) == 137 and sha_tree(deserialize(canonical)).hex() == CANONICAL_HASH, 'canonical137 module identity')
    manifest = load(reader(DIRECTORY + '/puzzle.json'))
    closed(manifest, ['slug', 'title', 'kind', 'environment', 'security_scope', 'source_dependencies',
                      'native_sources', 'reference_includes', 'provenance', 'cases'], 'puzzle manifest')
    require(manifest['slug'] == SLUG and manifest['kind'] == 'protocol_component'
            and manifest['environment'] == 'raw' and manifest['source_dependencies'] == DEPENDENCIES
            and manifest['native_sources'] == NATIVE
            and manifest['reference_includes'] == {'condition_codes.clib': INCLUDE}, 'raw dedicated-component contract')
    for path in DEPENDENCIES + NATIVE:
        require(reader(path), 'empty declared input')
    p = manifest['provenance']
    closed(p, ['classification', 'upstream', 'license', 'commit', 'tree', 'source_adaptation', 'include_dependencies',
               'upstream_artifact', 'module_identity_obligation', 'references', 'limitations'], 'provenance')
    require(p['classification'] == 'canonical_puzzle_port'
            and p['upstream'] == dict(repository='https://github.com/Chia-Network/chia_puzzles', revision=PIN,
                source_path='puzzles/p2_delegated_conditions.clsp', snapshot=SOURCE, source_sha256=PINS[SOURCE][0],
                git_blob_sha1=PINS[SOURCE][1], root_tree_sha1=tree['root_tree_sha1']), 'canonical source identity')
    for value, keys, label in [
        (p['license'], ['spdx', 'source_path', 'snapshot', 'sha256', 'git_blob_sha1', 'scope'], 'license'),
        (p['commit'], ['snapshot', 'sha256', 'git_commit_sha1', 'root_tree_sha1', 'verification'], 'commit'),
        (p['tree'], ['manifest', 'sha256', 'root_tree_sha1', 'scope'], 'tree'),
        (p['source_adaptation'], ['kind', 'reference', 'reference_sha256', 'description'], 'source adaptation'),
        (p['upstream_artifact'], ['path', 'source_url', 'source_sha256', 'git_blob_sha1', 'clvm_hex', 'bytes', 'tree_hash'], 'published artifact'),
    ]:
        closed(value, keys, label)
    require(p['license']['spdx'] == 'Apache-2.0' and p['license']['source_path'] == 'LICENSE'
            and p['license']['snapshot'] == BASE + '/LICENSE'
            and p['license']['sha256'] == PINS[BASE + '/LICENSE'][0] and p['license']['git_blob_sha1'] == PINS[BASE + '/LICENSE'][1], 'license identity')
    require(p['commit']['snapshot'] == BASE + '/commit.git' and p['commit']['git_commit_sha1'] == PIN
            and p['commit']['sha256'] == digest(reader(BASE + '/commit.git'))
            and p['commit']['root_tree_sha1'] == tree['root_tree_sha1']
            and p['tree']['manifest'] == tree_path and p['tree']['sha256'] == digest(reader(tree_path))
            and p['tree']['root_tree_sha1'] == tree['root_tree_sha1'], 'commit/tree provenance')
    require(p['source_adaptation']['kind'] == 'none' and p['source_adaptation']['reference'] == DIRECTORY + '/reference.clsp'
            and p['source_adaptation']['reference_sha256'] == PINS[SOURCE][0], 'module must stay exact')
    require(p['include_dependencies'] == [dict(name='condition_codes.clib', snapshot=INCLUDE,
                sha256=PINS[INCLUDE][0], git_blob_sha1=PINS[INCLUDE][1])], 'include provenance')
    artifact = p['upstream_artifact']
    require(artifact['path'] == ARTIFACT and artifact['source_sha256'] == PINS[ARTIFACT][0]
            and artifact['source_url'] == f'https://github.com/Chia-Network/chia_puzzles/blob/{PIN}/puzzles/p2_delegated_conditions.clsp.hex'
            and artifact['git_blob_sha1'] == PINS[ARTIFACT][1] and artifact['clvm_hex'] == canonical.hex()
            and type(artifact['bytes']) is int and artifact['bytes'] == 137 and artifact['tree_hash'] == CANONICAL_HASH,
            'published artifact obligation')
    require(p['references'] == [f'https://github.com/Chia-Network/chia_puzzles/blob/{PIN}/{path}'
                for path in ('puzzles/p2_delegated_conditions.clsp', 'puzzles/p2_delegated_conditions.clsp.hex', 'LICENSE')]
            and type(p['limitations']) is list and len(p['limitations']) == 5
            and all(type(text) is str and text for text in p['limitations']), 'provenance scope')
    rows = vectors(reader)
    by_name = {row['label']: row for row in rows}
    require(manifest['cases'] == [{'label': name, 'solution': by_name[name]['solution']} for name in NAMED], 'twenty named cases changed')
    require(len([row for row in rows if row['label'] not in NAMED]) == 180, 'disjoint180generated changed')
    mutations = mutation_recipes(reader, rows)
    return manifest, rows, mutations
