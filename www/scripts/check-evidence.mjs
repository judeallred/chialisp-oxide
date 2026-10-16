import { readFile, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

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
assert.equal(report.schema_version, 1);
assert(report.puzzles.length > 0, 'A successful validation report is required');
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
  assert.deepEqual(puzzle.cases.map(({ label, solution }) => ({ label, solution })), manifest.cases, `${puzzle.slug}: test vectors changed`);
  assert.equal(puzzle.security_scope, manifest.security_scope, `${puzzle.slug}: policy scope changed`);
  assert.equal(puzzle.title, manifest.title, `${puzzle.slug}: title changed`);
  assert.equal(puzzle.kind, manifest.kind, `${puzzle.slug}: result kind changed`);
  assert.deepEqual(puzzle.provenance, manifest.provenance, `${puzzle.slug}: provenance changed`);
  assert.equal(puzzle.artifacts.length, 3, `${puzzle.slug}: missing compiler path`);
  const paths = list => list.map(item => `${item.backend}:${item.optimize_lir}`).sort();
  const expectedPaths = ['chialisp:true', 'rust_mir:false', 'rust_mir:true'];
  assert.deepEqual(paths(puzzle.artifacts), expectedPaths, `${puzzle.slug}: incorrect compiler paths`);
  for (const artifact of puzzle.artifacts) {
    assert.equal(artifact.bytes, artifact.clvm_hex.length / 2, `${puzzle.slug}: wrong serialized size`);
    assert.match(artifact.tree_hash, /^[a-f0-9]{64}$/);
  }
  for (const test of puzzle.cases) {
    assert.equal(test.runs.length, 3, `${puzzle.slug}/${test.label}: missing execution`);
    assert.deepEqual(paths(test.runs), expectedPaths, `${puzzle.slug}/${test.label}: incorrect execution paths`);
    for (const run of test.runs) assert.deepEqual(run.execution.outcome, test.native, `${puzzle.slug}/${test.label}: outcomes disagree`);
  }
  assert(puzzle.simulator.every(scenario => scenario.passed), `${puzzle.slug}: failed ledger scenario`);
}
const proof = JSON.parse(await read('corpus/proofs.json'));
assert.equal(proof.schema_version, 1);
for (const [fileKey, hashKey] of [['source', 'source_sha256'], ['ir', 'ir_sha256'], ['mutant_ir', 'mutant_ir_sha256']]) {
  await checkRecordedHash(proof[fileKey], proof[hashKey]);
}
for (const check of [...proof.checks, proof.mutation]) {
  await checkRecordedHash(check.specification, check.specification_sha256);
  await checkRecordedHash(check.query, check.query_sha256);
}
assert(proof.checks.length > 0 && proof.checks.every(check => check.result.status === 'proven'), 'Formal obligation missing or failed');
assert.equal(proof.mutation.result.status, 'refuted', 'Mutation was not detected');
console.log(`Evidence current: ${slugs.size} compiled source pairs, matching vectors/outcomes, passing ledger scenarios, and hashed proof sources`);
