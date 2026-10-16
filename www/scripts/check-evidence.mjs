import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const root = new URL('../../', import.meta.url);
const read = async relative => readFile(new URL(relative, root), 'utf8');
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
for (const [fileKey, hashKey] of [['source', 'source_sha256'], ['ir', 'ir_sha256']]) {
  assert(!proof[fileKey].includes('..'), 'Invalid proof source path');
  assert.equal(createHash('sha256').update(await read(proof[fileKey])).digest('hex'), proof[hashKey], 'Formal report source changed; regenerate proof evidence');
}
assert(proof.checks.length > 0 && proof.checks.every(check => check.result.status === 'proven'), 'Formal obligation missing or failed');
assert.equal(proof.mutation.result.status, 'refuted', 'Mutation was not detected');
console.log(`Evidence current: ${slugs.size} compiled source pairs, matching vectors/outcomes, passing ledger scenarios, and hashed proof sources`);
