#!/usr/bin/env python3
"""Check finite P2 singleton evidence and exact canonical source provenance.

Default checks retained current records, source bytes and independent finite
expected relations. It does not execute CLVM, native Rust or consensus; actual
submissions are produced by the normal Cargo corpus validation command.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import unittest

from cargo_authoring import check_recorded, contained_file, unique_object
import hashing_provenance as upstream
import p2_singleton_evidence as ledger
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
BASE = upstream.BASE
SLUG = 'p2-singleton'
SOURCE = BASE + '/puzzles/p2_singleton.clsp'
ARTIFACT = SOURCE + '.hex'
PINS = {
    SOURCE: '29848917e64931da06e0370c1d4f3b880f5600da2e30c52a55cef371f2223542',
    ARTIFACT: '2945bea6e98da698fefd660184a712345f24490cd39a9c09c6683d480b0076eb',
    BASE + '/puzzles/curry-and-treehash.clib': '5132c3d2428264deb1c405a1debf7a75a84495f96717b0795f18ea0f0f16120e',
    'puzzles/p2-singleton/vectors.py': '03151960654e231cf716fae6a48355ca4ba4b1715fe7093dfa1a2c2ec3e17642',
    'puzzles/p2-singleton/vectors.json': 'f63430784781815a20f77ee86f8bc0f222f2f14c655507ba0b253370655d988c',
    'puzzles/p2-singleton/generated-cases.json': '35e34823d944a9b8652dc2c7549e00802d8594d48d55a53e885c3dd84c43718a',
}
HASH = '40f828d8dd55603f4ff9fbf6b73271e904e69406982f4fbefae2c8dcceaf9834'
require = ledger.require
SDK_INPUTS = [f'crates/chialisp-oxide/src/{name}.rs' for name in ['lib', 'tree', 'hashing']]
VECTOR_INPUTS = [f'puzzles/{SLUG}/{name}' for name in ['vectors.py', 'vectors.json', 'generated-cases.json']]
SOURCE_INPUTS = set(SDK_INPUTS + VECTOR_INPUTS) | set(ledger.RUNTIME_INPUTS)
NATIVE_INPUTS = upstream.NATIVE_SOURCES['singleton-launcher'] + [
    'crates/chialisp-oxide-corpus/src/p2_singleton.rs',
    'crates/chialisp-oxide-corpus/src/p2_singleton/codec.rs',
    'crates/chialisp-oxide-corpus/src/p2_singleton/modules.rs', ledger.TABLE_PATH,
]
PROVENANCE_FIELDS = {'upstream', 'license', 'commit', 'tree', 'classification', 'references',
    'source_adaptation', 'upstream_artifact', 'include_dependencies', 'module_identity_obligation', 'limitations'}

def digest(payload):
    return hashlib.sha256(payload).hexdigest()

def load(path):
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)

def read_source(path):
    return contained_file(ROOT, path).read_bytes()

def json_source(reader, path):
    return json.loads(reader(path), object_pairs_hook=unique_object)

def named_puzzle(report):
    selected = [p for p in report['puzzles'] if p['slug'] == SLUG]
    require(len(selected) == 1, 'Missing/duplicate P2 component')
    return selected[0]

def provenance_headers(value):
    """Bind metadata directly to retained Git/license bytes, not another fixture."""
    ledger.closed(value, PROVENANCE_FIELDS, 'P2 provenance')
    expected = {
        'license': {'spdx':'Apache-2.0', 'source_path':'LICENSE', 'snapshot':upstream.LICENSE,
            'sha256':upstream.PINS[upstream.LICENSE][0], 'git_blob_sha1':upstream.PINS[upstream.LICENSE][1]},
        'commit': {'snapshot':upstream.COMMIT, 'sha256':upstream.COMMIT_SHA,
            'git_commit_sha1':upstream.PIN, 'root_tree_sha1':upstream.TREE},
        'tree': {'manifest':upstream.TREE_PATH, 'sha256':upstream.TREE_SHA, 'root_tree_sha1':upstream.TREE},
    }
    descriptions = {'license':'scope', 'commit':'verification', 'tree':'scope'}
    for name, fixed in expected.items():
        description = descriptions[name]
        ledger.closed(value[name], set(fixed) | {description}, 'P2 ' + name)
        require({k:value[name][k] for k in fixed} == fixed
                and type(value[name][description]) is str and value[name][description],
                'Changed P2 ' + name + ' provenance')

def sources(reader=None):
    reader = reader or read_source
    upstream.check_tree(reader)
    for path, pin in {**ledger.RUNTIME_INPUTS,**PINS}.items():
        require(digest(reader(path)) == pin, 'Changed pinned P2 source/vector: '+path)
    entries = {r['path']:r for r in json_source(reader, upstream.TREE_PATH)['entries']}
    for path in set(ledger.RUNTIME_INPUTS)|{SOURCE,ARTIFACT}:
        row = entries[path.removeprefix(BASE+'/')]
        require(row['kind'] == 'blob' and row['mode'] == '100644'
                and row['git_sha1'] == upstream.git_object('blob',reader(path))
                and row['size_bytes'] == len(reader(path)), 'P2 counterpart Git tree membership differs')
    manifest = json_source(reader, 'puzzles/p2-singleton/puzzle.json')
    require(manifest['slug'] == SLUG and manifest['kind'] == 'protocol_component'
            and manifest['environment'] == 'raw' and type(manifest['validation_max_cost']) is int
            and manifest['validation_max_cost'] == 10_000_000,
            'Changed P2 protocol component/environment/budget contract')
    includes = {name:BASE+'/puzzles/'+name for name in ['condition_codes.clib','curry-and-treehash.clib']}
    require(manifest['reference_includes'] == includes, 'Changed canonical compiler includes')
    require(manifest.get('reference_mode', 'module') == 'module' and manifest.get('rue_dependencies', []) == [],
            'Changed module/Rue dependency contract')
    dependencies = manifest['source_dependencies']
    require(type(dependencies) is list and all(type(path) is str for path in dependencies)
            and len(dependencies) == len(set(dependencies)) and set(dependencies) == SOURCE_INPUTS,
            'Changed P2 source dependency closure')
    require(manifest['native_sources'] == NATIVE_INPUTS, 'Changed P2 native dependency closure')
    provenance = manifest['provenance']
    provenance_headers(provenance)
    require(provenance['classification'] == 'canonical_puzzle_port', 'Changed P2 canonical classification')
    expected = {'repository':upstream.REPOSITORY,'revision':upstream.PIN,
                'source_path':'puzzles/p2_singleton.clsp','snapshot':SOURCE,
                'source_sha256':PINS[SOURCE],'git_blob_sha1':entries['puzzles/p2_singleton.clsp']['git_sha1'],
                'root_tree_sha1':upstream.TREE}
    require(provenance['upstream'] == expected, 'Changed P2 upstream identity')
    canonical = bytes.fromhex(reader(ARTIFACT).decode().strip())
    require(reader('puzzles/p2-singleton/reference.clsp') == reader(SOURCE),
            'Canonical module source/artifact was adapted')
    require(len(canonical) == 403 and ledger.CODEC['tree_hash'](ledger.deserialize(canonical)).hex() == HASH,
            'Changed exact canonical module/hash')
    a = provenance['upstream_artifact']
    require(type(a['bytes']) is int and a == {'path':ARTIFACT,'source_url':upstream.REPOSITORY+'/blob/'+upstream.PIN+'/puzzles/p2_singleton.clsp.hex',
            'source_sha256':PINS[ARTIFACT],'git_blob_sha1':entries['puzzles/p2_singleton.clsp.hex']['git_sha1'],
            'clvm_hex':canonical.hex(),'bytes':403,'tree_hash':HASH}, 'Changed canonical artifact provenance')
    ledger.closed(provenance['source_adaptation'], ['kind', 'reference_sha256', 'description'], 'source adaptation')
    require(type(provenance['source_adaptation']['description']) is str and provenance['source_adaptation']['description'],
            'Missing canonical adaptation description')
    require(provenance['references'] == [upstream.REPOSITORY+'/blob/'+upstream.PIN+'/puzzles/p2_singleton.clsp',
            upstream.REPOSITORY+'/blob/'+upstream.PIN+'/puzzles/p2_singleton.clsp.hex',
            upstream.REPOSITORY+'/blob/'+upstream.PIN+'/LICENSE'], 'Changed canonical source references')
    require(provenance['source_adaptation']['kind'] == 'none'
            and provenance['source_adaptation']['reference_sha256'] == PINS[SOURCE], 'Hidden canonical adapter')
    expected_includes = [{'source_path':'puzzles/'+name,'snapshot':path,'source_sha256':digest(reader(path)),
        'git_blob_sha1':entries['puzzles/'+name]['git_sha1']} for name,path in includes.items()]
    require(provenance['include_dependencies'] == expected_includes, 'Changed canonical include provenance')
    spec = importlib.util.spec_from_file_location('p2_independent_expected', ROOT/'puzzles/p2-singleton/vectors.py')
    model = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = model
    spec.loader.exec_module(model)
    vectors = json_source(reader, 'puzzles/p2-singleton/vectors.json')
    require(model.check_expected_relations()['named'] == 50, 'Independent codec/relation controls failed')
    require(vectors['named'] == model.records(model.named()) and vectors['generated'] == model.records(model.generated()),
            'Stale independent expected vector relation')
    require(manifest['cases'] == [{k:r[k] for k in ['label','solution']} for r in vectors['named']]
            and json_source(reader, 'puzzles/p2-singleton/generated-cases.json') == [{k:r[k] for k in ['label','solution']} for r in vectors['generated']],
            'Corpus vectors differ from independent expected recipes')
    return manifest,vectors,canonical

def validate(report):
    manifest,vectors,canonical = sources()
    puzzle = named_puzzle(report)
    check_recorded(report['authoring'], ROOT, [p['slug'] for p in report['puzzles']])
    for field in ['slug', 'title', 'kind', 'security_scope', 'environment', 'reference_includes']:
        require(puzzle[field] == manifest[field], 'Stale P2 recorded contract: ' + field)
    require(puzzle.get('reference_mode', 'module') == 'module', 'Changed recorded canonical reference mode')
    require(puzzle['source'] == read_source('puzzles/p2-singleton/puzzle.rs').decode()
            and puzzle['reference'] == read_source(SOURCE).decode(), 'Stale P2 policy source')
    rue_path = 'puzzles/p2-singleton/puzzle.rue'
    ledger.closed(puzzle['rue'], ['path', 'source', 'sha256', 'dependencies'], 'P2 Rue source')
    rue_source = read_source(rue_path)
    require(puzzle['rue']['path'] == rue_path and puzzle['rue']['source'] == rue_source.decode()
            and puzzle['rue']['sha256'] == digest(rue_source) and puzzle['rue']['dependencies'] == [],
            'Stale P2 Rue source/dependencies')
    require(puzzle['provenance'] == manifest['provenance'], 'Changed recorded provenance')
    dependencies = puzzle['source_dependencies']
    require(type(dependencies) is list and [r['path'] for r in dependencies] == manifest['source_dependencies'],
            'Changed recorded P2 source closure')
    for row in dependencies:
        ledger.closed(row, ['path', 'sha256', 'source'], 'P2 dependency')
        payload = read_source(row['path'])
        require(row['sha256'] == digest(payload) and row['source'] == payload.decode(), 'Stale P2 dependency: ' + row['path'])
    native = puzzle['native_sources']
    require(type(native) is list and [r['path'] for r in native] == NATIVE_INPUTS, 'Changed recorded P2 native closure')
    for row in native:
        ledger.closed(row, ['path', 'sha256'], 'P2 native source')
        require(row['sha256'] == digest(read_source(row['path'])), 'Stale P2 native source: ' + row['path'])
    budget = puzzle['validation_max_cost']
    require(type(budget) is int and budget == 10_000_000 and type(puzzle['generated_cases']) is int
            and puzzle['generated_cases'] == 2187, 'Changed fixed ordinary P2 coverage/budget')
    require([(r['label'],r['solution']) for r in puzzle['cases']] == [(r['label'],r['solution']) for r in vectors['named']],
            'Missing/reordered independent named cases')
    for row,expected in zip(puzzle['cases'],vectors['named'],strict=True):
        require(row['native'] == expected['expected'], 'Native P2 output differs from independent relation')
        require([(r['backend'],r['optimize_lir']) for r in row['runs']] == ledger.MODES
                and all(type(r['optimize_lir']) is bool for r in row['runs']), 'Missing/invalid P2 execution mode')
        for run in row['runs']:
            execution = run['execution']
            require(check_execution(execution) == expected['expected'], 'P2 VM differs from independent relation')
            require(execution['cost'] is None or execution['cost'] <= budget, 'Recorded P2 VM cost exceeds its budget')
    artifact = puzzle['artifacts'][2]
    require(artifact['backend'] == 'chialisp' and artifact['clvm_hex'] == canonical.hex()
            and artifact['bytes'] == 403 and artifact['tree_hash'] == HASH, 'Independent compiler did not reproduce published module')
    counts = ledger.validate_component(puzzle,root=ROOT)
    require(puzzle['validated'] is True, 'P2 record lacks successful validation status')
    expected_identity = {'upstream_hex':canonical.hex(), 'upstream_tree_hash':HASH,
        'comparisons':[{'backend':a['backend'], 'optimize_lir':a['optimize_lir'],
            'serialized_bytes_equal':ledger.artifact_bytes(a) == canonical,
            'tree_hash_equal':a['tree_hash'] == HASH} for a in puzzle['artifacts']]}
    require(puzzle['module_identity'] == expected_identity
            and all(type(row[key]) is bool for row in puzzle['module_identity']['comparisons']
                    for key in ['optimize_lir', 'serialized_bytes_equal', 'tree_hash_equal']),
            'False/missing canonical P2 module identity comparison')
    return {'named':50,'generated':2187,'independent_generated_recipes':len(vectors['generated']),**counts,
            'scope':'50 recorded named comparisons, 128 independently regenerated recipes and the producer summary of 2187 generated cases, plus typed ledger consistency; no new VM/native/consensus execution, authentication, whole singleton implementation or compiler-preservation proof'}

class Controls(unittest.TestCase):
    report_path = ROOT/'corpus/validation.json'

    def test_pinned_source_mutations(self):
        sources()
        for path in list(PINS)+list(ledger.RUNTIME_INPUTS):
            with self.subTest(path=path),self.assertRaises(ValueError):
                sources(lambda p: (contained_file(ROOT,p).read_bytes()+b'\n') if p==path else contained_file(ROOT,p).read_bytes())

    def test_actual_record_corruptions(self):
        report = load(self.report_path)
        validate(report)
        for field in ['hash','case','coverage','native','mode','budget','protocol','provenance',
                      'environment','rue_source','includes','SDK_source','native_source','numeric_mode',
                      'float_coverage','float_budget','cost_over_budget','unknown_counterpart','stale_authoring',
                      'Rue_hash','reference_mode','false_identity','numeric_identity','unvalidated']:
            bad=copy.deepcopy(report);p=named_puzzle(bad)
            if field=='hash':p['artifacts'][2]['tree_hash']='00'*32
            elif field=='case':p['cases'].pop()
            elif field=='coverage':p['generated_cases']-=1
            elif field=='native':p['cases'][0]['native']={'status':'rejected'}
            elif field=='mode':p['cases'][0]['runs'].pop()
            elif field=='budget':p['validation_max_cost']+=1
            elif field=='protocol':p['protocol_validation']['observations'][0]['negatives'][0]['consensus']['code']='CostExceeded'
            elif field=='provenance':p['provenance']['source_adaptation']['kind']='wrapper'
            elif field=='environment':p['environment']='strict'
            elif field=='rue_source':p['rue']['source']='fun unrelated() { 0 }'
            elif field=='includes':p['reference_includes']={}
            elif field=='SDK_source':p['source_dependencies'][0]['sha256']='0'*64
            elif field=='native_source':p['native_sources'][0]['sha256']='0'*64
            elif field=='numeric_mode':p['cases'][0]['runs'][0]['optimize_lir']=0
            elif field=='float_coverage':p['generated_cases']=2187.0
            elif field=='float_budget':p['validation_max_cost']=10_000_000.0
            elif field=='cost_over_budget':p['cases'][0]['runs'][0]['execution']['cost']=10_000_001
            elif field=='unknown_counterpart':p['protocol_validation']['modules']['modules'].append(copy.deepcopy(p['protocol_validation']['modules']['modules'][0]))
            elif field=='stale_authoring':bad['authoring']['sources'][0]['sha256']='0'*64
            elif field=='Rue_hash':p['rue']['sha256']='0'*64
            elif field=='reference_mode':p['reference_mode']='expression'
            elif field=='false_identity':p['module_identity']['comparisons'][0]['tree_hash_equal']=True
            elif field=='numeric_identity':p['module_identity']['comparisons'][2]['tree_hash_equal']=1
            elif field=='unvalidated':p['validated']=False
            with self.subTest(field=field),self.assertRaises(ValueError):validate(bad)

    def test_manifest_closure_and_provenance_corruptions(self):
        path = 'puzzles/p2-singleton/puzzle.json'
        original = json_source(read_source, path)
        controls = {
            'missing_source':lambda m:m['source_dependencies'].pop(),
            'unknown_source':lambda m:m['source_dependencies'].append('puzzles/unknown/puzzle.rs'),
            'duplicate_source':lambda m:m['source_dependencies'].append(m['source_dependencies'][0]),
            'missing_native':lambda m:m['native_sources'].pop(),
            'unknown_native':lambda m:m['native_sources'].append('unknown.rs'),
            'reference_mode':lambda m:m.__setitem__('reference_mode','expression'),
            'float_budget':lambda m:m.__setitem__('validation_max_cost',10_000_000.0),
            'float_artifact_size':lambda m:m['provenance']['upstream_artifact'].__setitem__('bytes',403.0),
            'Rue_dependency':lambda m:m.__setitem__('rue_dependencies',['unknown.rue']),
            'license':lambda m:m['provenance']['license'].__setitem__('spdx','CC0-1.0'),
            'commit':lambda m:m['provenance']['commit'].__setitem__('git_commit_sha1','0'*40),
            'tree':lambda m:m['provenance']['tree'].__setitem__('root_tree_sha1','0'*40),
            'unknown_provenance':lambda m:m['provenance'].__setitem__('unknown_counterpart',{}),
            'source_adaptation':lambda m:m['provenance']['source_adaptation'].__setitem__('unknown_wrapper',True),
        }
        for name,mutate in controls.items():
            sources()  # A fresh passing control precedes every corruption.
            altered=copy.deepcopy(original);mutate(altered)
            payload=json.dumps(altered).encode()
            with self.subTest(mutation=name),self.assertRaises(ValueError):
                sources(lambda p:payload if p==path else read_source(p))
        # Metadata on another puzzle is not an oracle for this port's license.
        sources(lambda p:b'{}' if p=='puzzles/singleton-launcher/puzzle.json' else read_source(p))

    def test_p2_record_selection_is_independent_of_report_order(self):
        report = load(self.report_path)
        expected = validate(report)
        reordered = copy.deepcopy(report)
        reordered['puzzles'].reverse()
        self.assertEqual(validate(reordered), expected)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report',type=Path,default=ROOT/'corpus/validation.json')
    parser.add_argument('--self-test',action='store_true')
    args=parser.parse_args()
    print('PASS: finite pay-to-singleton records',validate(load(args.report)))
    if args.self_test:
        Controls.report_path = args.report.resolve()
        result=unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Controls))
        if not result.wasSuccessful():raise SystemExit(1)

if __name__=='__main__':main()
