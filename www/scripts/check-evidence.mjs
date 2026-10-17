import { readFile, realpath } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { failureProtocol, checkCorpusExecutions, checkScalarProofReplays, testFailureProtocol } from './vm-failures.mjs';
import { checkLedgerReport, testLedgerProtocol } from './ledger-rejections.mjs';

const root = new URL('../../', import.meta.url);
const rootPath = await realpath(fileURLToPath(root));
const read = async relative => readFile(new URL(relative, root), 'utf8');
async function recordedSource(filename) {
  assert(typeof filename === 'string' && !path.isAbsolute(filename), 'Recorded source must be repo-relative');
  const resolved = await realpath(new URL(filename, root));
  const relative = path.relative(rootPath, resolved);
  assert(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'Recorded source escapes repository');
  return readFile(resolved, 'utf8');
}
async function checkRecordedHash(filename, expected) {
  assert.match(expected, /^[a-f0-9]{64}$/, 'Missing recorded SHA-256');
  const source = await recordedSource(filename);
  assert.equal(createHash('sha256').update(source).digest('hex'), expected, `${filename}: source changed; regenerate evidence`);
}
const report = JSON.parse(await read('corpus/validation.json'));
const tutorialExamples = JSON.parse(await read('corpus/tutorial-examples.json'));
assert.equal(report.schema_version, 1);
checkCorpusExecutions(report);
checkLedgerReport(report);
console.log(`Consensus rejection protocol: ${testLedgerProtocol()} independent corruption controls passed`);
console.log(`Typed VM failure protocol: ${testFailureProtocol()} independent corruption controls passed`);
assert.deepEqual(report.rue_local_patches, [108, 109, 110, 111, 112, 113].map(number => `https://github.com/xch-dev/rue/issues/${number}`), 'Rue patch provenance is incomplete');
assert.deepEqual(report.optimization_pipeline, {
  rust_mir: { unoptimized: [], optimized: ['typed_ir_tree_guards', 'runtime_frame_elision', 'rue_lir'] },
  rue_source: { unoptimized: [], optimized: ['rue_lir'] },
}, 'The recorded optimization pipeline is missing or unsupported');
assert(report.puzzles.length > 0, 'A successful validation report is required');
const configuration = JSON.parse(await read('puzzles/authoring.json'));
assert.equal(configuration.schema_version, 1, 'Missing Cargo authoring schema');
assert.equal(configuration.mode, 'cargo_library', 'Corpus must use Cargo library authoring');
assert(Array.isArray(configuration.inputs), 'Missing declared Cargo inputs');
assert.equal(new Set(configuration.inputs).size, configuration.inputs.length, 'Duplicate Cargo input');
assert.deepEqual(Object.keys(configuration.exports).sort(), report.puzzles.map(puzzle => puzzle.slug).sort(), 'Cargo exports differ from the validation corpus');
async function checkAuthoring(snapshot, label) {
  assert(snapshot && Array.isArray(snapshot.sources), `${label}: missing Cargo authoring provenance`);
  for (const field of ['mode', 'package', 'manifest', 'library', 'exports']) {
    assert.deepEqual(snapshot[field], configuration[field], `${label}: Cargo ${field} changed`);
  }
  assert.deepEqual(snapshot.sources.map(source => source.path), [...configuration.inputs].sort(), `${label}: declared Cargo inputs changed`);
  for (const required of ['puzzles/authoring.json', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml', configuration.manifest, configuration.library, 'puzzles/fixture_keys.rs', 'crates/chialisp-oxide/Cargo.toml', 'crates/chialisp-oxide/src/lib.rs', 'crates/chialisp-oxide/src/domain.rs', 'crates/chialisp-oxide/src/tree.rs', 'crates/chialisp-oxide/src/owned_tree.rs', 'crates/chialisp-oxide-macros/Cargo.toml', 'crates/chialisp-oxide-macros/src/lib.rs']) {
    assert(configuration.inputs.includes(required), `${label}: missing required Cargo input ${required}`);
  }
  for (const source of snapshot.sources) await checkRecordedHash(source.path, source.sha256);
  for (const [slug, exported] of Object.entries(snapshot.exports)) {
    assert.equal(exported.entry, `chialisp_oxide_puzzles::${slug.replaceAll('-', '_')}::puzzle`, `${label}: unexpected Cargo export ${slug}`);
    assert.equal(exported.source, `puzzles/${slug}/puzzle.rs`, `${label}: unexpected export source ${slug}`);
    assert(configuration.inputs.includes(exported.source), `${label}: export source missing from Cargo inputs`);
  }
}
await checkAuthoring(report.authoring, 'validation');
const slugs = new Set();
for (const puzzle of report.puzzles) {
  assert.match(puzzle.slug, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid puzzle path');
  assert(!slugs.has(puzzle.slug), `Duplicate puzzle ${puzzle.slug}`);
  slugs.add(puzzle.slug);
  assert(puzzle.validated, `${puzzle.slug}: validation failed`);
  assert.equal(await read(`puzzles/${puzzle.slug}/puzzle.rs`), puzzle.source, `${puzzle.slug}: Rust source changed; regenerate evidence`);
  assert.equal(await read(`puzzles/${puzzle.slug}/reference.clsp`), puzzle.reference, `${puzzle.slug}: reference changed; regenerate evidence`);
  const manifest = JSON.parse(await read(`puzzles/${puzzle.slug}/puzzle.json`));
  const dependencies = manifest.source_dependencies ?? [];
  assert(Array.isArray(puzzle.source_dependencies), `${puzzle.slug}: missing source dependency records`);
  assert.deepEqual(puzzle.source_dependencies.map(dependency => dependency.path).sort(), [...dependencies].sort(), `${puzzle.slug}: dependency manifest changed`);
  assert.equal(new Set(dependencies).size, dependencies.length, `${puzzle.slug}: duplicate source dependency`);
  for (const dependency of puzzle.source_dependencies) {
    const source = await recordedSource(dependency.path);
    assert.equal(source, dependency.source, `${puzzle.slug}/${dependency.path}: source dependency changed`);
    assert.equal(createHash('sha256').update(source).digest('hex'), dependency.sha256, `${puzzle.slug}/${dependency.path}: source dependency hash changed`);
  }
  assert.equal(puzzle.rue.path, `puzzles/${puzzle.slug}/puzzle.rue`, `${puzzle.slug}: Rue source identity`);
  assert.equal(await read(puzzle.rue.path), puzzle.rue.source, `${puzzle.slug}: stale Rue source`);
  await checkRecordedHash(puzzle.rue.path, puzzle.rue.sha256);
  assert.deepEqual(puzzle.rue.dependencies.map(item => item.path).sort(), [...(manifest.rue_dependencies ?? [])].sort(), `${puzzle.slug}: Rue imports changed`);
  for (const dependency of puzzle.rue.dependencies) {
    assert.equal(await recordedSource(dependency.path), dependency.source, `${puzzle.slug}: stale Rue dependency`);
    await checkRecordedHash(dependency.path, dependency.sha256);
  }
  assert.deepEqual(puzzle.cases.map(({ label, solution }) => ({ label, solution })), manifest.cases, `${puzzle.slug}: test vectors changed`);
  assert.equal(puzzle.security_scope, manifest.security_scope, `${puzzle.slug}: policy scope changed`);
  assert.equal(puzzle.title, manifest.title, `${puzzle.slug}: title changed`);
  assert.equal(puzzle.kind, manifest.kind, `${puzzle.slug}: result kind changed`);
  assert.deepEqual(puzzle.provenance, manifest.provenance, `${puzzle.slug}: provenance changed`);
  for (const [field, defaultValue] of [['environment', 'strict'], ['reference_mode', 'module']]) {
    assert.equal(puzzle[field] ?? defaultValue, manifest[field] ?? defaultValue, `${puzzle.slug}: ${field} contract changed`);
  }
  assert.equal(puzzle.artifacts.length, 5, `${puzzle.slug}: missing compiler path`);
  const paths = list => list.map(item => `${item.backend}:${item.optimize_lir}`).sort();
  const expectedPaths = ['chialisp:true', 'rue_source:false', 'rue_source:true', 'rust_mir:false', 'rust_mir:true'];
  assert.deepEqual(paths(puzzle.artifacts), expectedPaths, `${puzzle.slug}: incorrect compiler paths`);
  for (const artifact of puzzle.artifacts) {
    assert.equal(artifact.bytes, artifact.clvm_hex.length / 2, `${puzzle.slug}: wrong serialized size`);
    assert.match(artifact.tree_hash, /^[a-f0-9]{64}$/);
  }
  for (const test of puzzle.cases) {
    assert.equal(test.runs.length, 5, `${puzzle.slug}/${test.label}: missing execution`);
    assert.deepEqual(paths(test.runs), expectedPaths, `${puzzle.slug}/${test.label}: incorrect execution paths`);
    for (const run of test.runs) assert.deepEqual(run.execution.outcome, test.native, `${puzzle.slug}/${test.label}: outcomes disagree`);
  }
  assert(puzzle.simulator.every(scenario => scenario.passed), `${puzzle.slug}: failed ledger scenario`);
  if (puzzle.kind === 'coin_puzzle') {
    assert.deepEqual(puzzle.simulator_comparisons.map(run => `${run.backend}:${run.optimize_lir}`).sort(), ['chialisp:true', 'rue_source:true']);
    for (const run of puzzle.simulator_comparisons) assert.deepEqual(run.scenarios, puzzle.simulator, `${puzzle.slug}: ${run.backend} ledger behavior differs`);
  } else assert.deepEqual(puzzle.simulator_comparisons, [], `${puzzle.slug}: pure function claims ledger comparisons`);

}
const proof = JSON.parse(await read('corpus/proofs.json'));
assert.equal(proof.schema_version, 1);
assert.equal(proof.vm_failure_protocol, failureProtocol);
checkScalarProofReplays(proof);
const executionPaths = [
  'crates/chialisp-oxide-simulator/Cargo.toml', 'crates/chialisp-oxide-simulator/src/lib.rs',
  'crates/chialisp-oxide-simulator/rejection-protocol.json', 'scripts/ledger_rejections.py', 'www/scripts/ledger-rejections.mjs',
  'crates/chialisp-oxide-vm/Cargo.toml', 'crates/chialisp-oxide-vm/failure-protocol.json',
  'crates/chialisp-oxide-vm/src/lib.rs', 'scripts/vm_failures.py', 'www/scripts/vm-failures.mjs',
];
assert.deepEqual(proof.execution_sources.map(source => source.path), executionPaths.sort(), 'Scalar proof VM classifier/gate bindings changed');
for (const source of proof.execution_sources) await checkRecordedHash(source.path, source.sha256);
await checkAuthoring(proof.authoring, 'proof');
assert.equal(proof.authoring.entry, configuration.exports['bounded-counter'].entry, 'Proof selects another Cargo export');
const { entry: proofEntry, ...proofInputs } = proof.authoring;
assert.deepEqual(proofInputs, report.authoring, 'Validation and proof Cargo provenance differ');
for (const [fileKey, hashKey] of [['source', 'source_sha256'], ['ir', 'ir_sha256'], ['mutant_ir', 'mutant_ir_sha256']]) {
  await checkRecordedHash(proof[fileKey], proof[hashKey]);
}
for (const check of [...proof.checks, proof.mutation]) {
  await checkRecordedHash(check.specification, check.specification_sha256);
  await checkRecordedHash(check.query, check.query_sha256);
}
assert(proof.checks.length > 0 && proof.checks.every(check => check.result.status === 'proven'), 'Formal obligation missing or failed');
assert.equal(proof.mutation.result.status, 'refuted', 'Mutation was not detected');
const sourceInventory = JSON.parse(await read('corpus/inventory.json'));
assert.equal(tutorialExamples.schema_version, 1);
assert.equal(tutorialExamples.upstream.revision, sourceInventory.scope.tutorial_source_revision, 'Tutorial example pin differs from source inventory');
await checkRecordedHash(tutorialExamples.license.snapshot, tutorialExamples.license.sha256);
for (const page of tutorialExamples.pages) await checkRecordedHash(page.source.snapshot, page.source.sha256);
const snippets = [...tutorialExamples.examples.flatMap(example => [example, ...example.variants]), ...tutorialExamples.supporting_material];
for (const snippet of snippets) {
  const source = snippet.source;
  await checkRecordedHash(source.snapshot, source.file_sha256);
  const lines = (await recordedSource(source.snapshot)).match(/[^\n]*\n|[^\n]+$/g) ?? [];
  assert(Number.isInteger(source.start_line) && Number.isInteger(source.end_line) && source.start_line >= 1 && source.end_line >= source.start_line && source.end_line <= lines.length, `${snippet.id}: invalid source span`);
  const text = lines.slice(source.start_line - 1, source.end_line).join('');
  assert.equal(createHash('sha256').update(text).digest('hex'), source.sha256, `${snippet.id}: canonical source span changed`);
}
let canonicalPorts = 0;
for (const example of tutorialExamples.examples) {
  if (example.implementation_status === 'not_started') {
    assert.equal(example.validation_status, 'not_run', `${example.id}: unimplemented example has validation claims`);
    assert.deepEqual(example.local_examples, []);
    assert.deepEqual(example.evidence, []);
    assert.equal(example.source_adaptation, undefined, `${example.id}: unimplemented source adaptation`);
    continue;
  }
  assert.equal(example.implementation_status, 'implemented', `${example.id}: unknown implementation status`);
  assert.equal(example.validation_status, 'differential_only', `${example.id}: unsupported validation claim`);
  const adaptation = example.source_adaptation;
  assert(adaptation, `${example.id}: missing exact-source adaptation`);
  assert.equal(adaptation.operation, 'extract_single_quoted_run_expression');
  assert.equal(adaptation.source_span_sha256, example.source.sha256);
  const lines = (await recordedSource(example.source.snapshot)).match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const span = lines.slice(example.source.start_line - 1, example.source.end_line).join('');
  const expression = span.match(/^run '([^']+)'\n$/)?.[1];
  assert(expression, `${example.id}: source is not one exact run expression`);
  const puzzle = report.puzzles.find(puzzle => puzzle.slug === adaptation.slug);
  assert(puzzle?.validated, `${example.id}: canonical example has no validated puzzle`);
  assert.equal(puzzle.provenance.classification, 'canonical_tutorial_port');
  assert.equal(puzzle.provenance.example_id, example.id);
  assert.equal(puzzle.environment, 'ignored', `${example.id}: original environment behavior changed`);
  assert.equal(puzzle.reference_mode, 'expression', `${example.id}: canonical source must use raw-expression compilation`);
  const contract = example.validation_contract;
  assert(contract, `${example.id}: missing explicit original validation contract`);
  assert.equal(contract.abi, 'ignored_environment_nullary');
  assert.equal(contract.environment, puzzle.environment);
  assert.equal(contract.reference_mode, puzzle.reference_mode);
  assert.equal(contract.claim_scope, 'executed_differential_cases_only');
  assert.equal(contract.ledger, 'not_applicable_pure_function');
  assert(puzzle.cases.length > 0 && puzzle.cases.every(test => test.native.status === 'accepted' && test.native.output_hex === contract.expected_output_hex), `${example.id}: observed original output differs from its contract`);
  assert.equal(puzzle.kind, 'pure_function');
  assert.deepEqual(puzzle.simulator, [], `${example.id}: pure expression must not claim ledger validation`);
  assert.equal(adaptation.reference, `puzzles/${puzzle.slug}/reference.clsp`);
  assert.equal(adaptation.rust_source, `puzzles/${puzzle.slug}/puzzle.rs`);
  assert.equal(adaptation.manifest, `puzzles/${puzzle.slug}/puzzle.json`);
  assert.equal(adaptation.report, 'corpus/validation.json');
  for (const field of ['reference', 'rust_source', 'manifest', 'report']) {
    await checkRecordedHash(adaptation[field], adaptation[`${field}_sha256`]);
  }
  assert.equal(puzzle.reference, `${expression}\n`, `${example.id}: original expression was rewritten or wrapped`);
  assert(example.local_examples.includes(adaptation.reference) && example.local_examples.includes(adaptation.rust_source));
  assert(example.evidence.includes(adaptation.report));
  canonicalPorts++;
}
assert.equal(canonicalPorts, report.puzzles.filter(puzzle => puzzle.provenance.classification === 'canonical_tutorial_port').length, 'Untracked canonical-port claim in report');
console.log(`Evidence current: ${slugs.size} Cargo three-language comparisons, matching validation/proof input hashes, vectors/outcomes, ledger scenarios, and proof sources`);
console.log(`Tutorial sources current: ${tutorialExamples.examples.length} programs, ${canonicalPorts} exact canonical ports, pinned pages/license, and ${snippets.length} exact source spans`);

// Discovery and negative studies have independent offline freshness/claim gates.
for (const args of [['corpus/validate_published_puzzles.py'], ['scripts/check-error-study.py', '--check'], ['scripts/check-tree-proofs.py', '--check'], ['scripts/check-byte-proofs.py', '--check'], ['scripts/check-canonical-factories.py'], ['scripts/check-canonical-helpers.py'], ['scripts/check-runtime-frames.py'], ['experiments/tree-theorems/check-model.py', '--check-recorded', '--self-test']]) {
  process.stdout.write(execFileSync('python3', args, { cwd: rootPath, encoding: 'utf8' }));
}
