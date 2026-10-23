// Independent Python admission checks retained evidence; this projection and
// its display checks add no compiler, native, VM or SDK execution credit.
import assert from 'node:assert/strict';
import { open } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

export const precommitControlsProtocol = 'chialisp-oxide-precommit-source-controls/v1';
const names = ['claim_original', 'claim_drop_age', 'registry_original', 'registry_drop_signature'];
const controls = ['minimum_age_original', 'minimum_age_removed', 'owner_signature_original', 'owner_signature_removed'];
const codes = ['AssertHeightRelativeFailed', null, 'BadAggregateSignature', null];
/**
 * @typedef {object} PrecommitControlRow
 * @property {string} mode
 * @property {string} name
 * @property {string} control
 * @property {string} module_tree_hash
 * @property {string} reference_tree_hash
 * @property {string|null} code
 * @property {boolean} passed
 */
export function projectPrecommitControls(report) {
  assert.equal(report.protocol, precommitControlsProtocol);
  assert.equal(report.status, 'actual_source_controls_captured');
  assert.equal(report.native.test_groups, 1);
  assert.equal(report.native.observations.length, 4);
  assert.deepEqual(report.native.observations.map(row => row.name), names);
  assert.equal(report.runs.length, 8);
  assert.equal(report.ledger.transaction_attempts, 8);
  assert.equal(report.ledger.runs.length, 8);
  /** @type {PrecommitControlRow[]} */
  const rows = report.ledger.runs.map((row, index) => {
    const mode = index < 4 ? 'off' : 'on', control = controls[index % 4], name = names[index % 4];
    assert.equal(row.mode, mode); assert.equal(row.name, name); assert.equal(row.control, control);
    assert.match(row.module_tree_hash, /^[0-9a-f]{64}$/);
    assert.match(row.reference_tree_hash, /^[0-9a-f]{64}$/);
    const run = report.runs[index];
    assert.equal(run.mode, mode); assert.equal(run.name, name);
    assert.equal(run.module_tree_hash, row.module_tree_hash);
    assert.equal(run.execution.outcome.status, 'accepted');
    assert.equal(run.execution.outcome.output_hex, report.native.observations[index % 4].output_hex);
    const code = codes[index % 4];
    assert.deepEqual(row.scenarios, [{name: `source control ${control}: one complete-spend submission`,
      passed: true, scope: 'ledger', rejections: code ? [{code, stage: 'consensus'}] : []}]);
    return {...row, code, passed: row.scenarios[0].passed};
  });
  return {report, protocol: report.protocol, rows,
    counts: {nativeGroups: 1, native: 4, vm: 8, sdk: 8, accepted: 4, rejected: 4}};
}
const maximum = 16 * 1024 * 1024;
async function boundedRead(filename) {
  const handle = await open(filename, 'r');
  try {
    const metadata = await handle.stat();
    assert(metadata.isFile() && metadata.size > 0 && metadata.size <= maximum, 'Bounded regular source packet required');
    const buffer = Buffer.alloc(metadata.size + 1);
    const {bytesRead} = await handle.read(buffer, 0, buffer.length, 0);
    assert.equal(bytesRead, metadata.size, 'Source packet changed during bounded read');
    return buffer.subarray(0, bytesRead);
  } finally { await handle.close(); }
}
/** @type {Promise<ReturnType<typeof projectPrecommitControls> & {bytes: Buffer}>|undefined} */
let cached;
export function readPrecommitControls() {
  return cached ??= (async () => {
    const root = await repository(), filename = path.join(root, 'corpus/precommit-source-controls/report.json');
    const bytes = await boundedRead(filename);
    const admitted = JSON.parse(execFileSync('python3', ['scripts/check-precommit.py', '--source-controls', filename, '--source-controls-only'],
      {cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: maximum}));
    assert.equal(admitted.status, 'validated_recorded_source_controls');
    assert.equal(admitted.native_test_groups, 1); assert.equal(admitted.native_output_observations, 4);
    assert.equal(admitted.primary_vm_observations, 8); assert.equal(admitted.sdk_transaction_attempts, 8);
    assert.deepEqual(await boundedRead(filename), bytes, 'Source packet changed during admission');
    const evidence = projectPrecommitControls(JSON.parse(bytes));
    assert.equal(evidence.report.scope, admitted.scope);
    return {...evidence, bytes};
  })();
}
