// The Rust classifier owns this exhaustive table. Diagnostics are display text.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const table = JSON.parse(readFileSync(new URL('../../crates/chialisp-oxide-vm/failure-protocol.json', import.meta.url), 'utf8'));
assert.deepEqual(Object.keys(table).sort(), ['failures', 'protocol', 'schema_version']);
assert.equal(table.schema_version, 1);
assert.equal(table.protocol, 'clvmr-0.21.0/v1');
export const failureProtocol = table.protocol;
const kinds = new Set(['program', 'resource', 'infrastructure']);
const failures = new Map();
for (const row of table.failures) {
  assert.deepEqual(Object.keys(row).sort(), ['code', 'kind']);
  assert(kinds.has(row.kind) && /^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/.test(row.code) && !failures.has(row.code), 'Malformed VM failure protocol');
  failures.set(row.code, row.kind);
}
assert(failures.size > 0);
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const keys = (value, expected, label) => assert(object(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...expected].sort()), `${label}: inconsistent fields`);
const hex = (value) => typeof value === 'string' && /^(?:[0-9a-f]{2})+$/.test(value);

export function checkFailure(value, label = 'VM failure') {
  keys(value, ['kind', 'code'], label);
  assert(failures.has(value.code), `${label}: unknown failure code`);
  assert.equal(value.kind, failures.get(value.code), `${label}: inconsistent failure kind/code`);
  return value.kind;
}

export function checkNativeOutcome(value, label = 'native policy') {
  assert(object(value), `${label}: malformed native outcome`);
  if (value.status === 'accepted') {
    keys(value, ['status', 'output_hex'], label);
    assert(hex(value.output_hex), `${label}: malformed accepted output`);
  } else {
    keys(value, ['status'], label);
    assert.equal(value.status, 'rejected', `${label}: unknown native outcome`);
  }
  return value;
}

export function checkExecution(value, label = 'VM execution', { requireConclusive = true } = {}) {
  assert(object(value), `${label}: malformed execution envelope`);
  const expected = ['outcome', 'cost', 'error'];
  if (Object.hasOwn(value, 'failure')) expected.push('failure');
  keys(value, expected, label);
  assert(object(value.outcome), `${label}: malformed VM outcome`);
  if (value.outcome.status === 'accepted') {
    checkNativeOutcome(value.outcome, label);
    assert(Number.isSafeInteger(value.cost) && value.cost > 0 && value.error === null && !Object.hasOwn(value, 'failure'), `${label}: inconsistent successful execution metadata`);
  } else {
    keys(value.outcome, ['status'], label);
    assert(['rejected', 'inconclusive'].includes(value.outcome.status), `${label}: unknown VM outcome`);
    assert(value.cost === null && typeof value.error === 'string' && value.error.length > 0, `${label}: malformed failure diagnostics/cost`);
    const kind = checkFailure(value.failure, label);
    assert.equal(value.outcome.status === 'rejected', kind === 'program', `${label}: non-program failure cannot be program rejection`);
    assert(!requireConclusive || value.outcome.status !== 'inconclusive', `${label}: inconclusive execution is not validation evidence`);
  }
  return value.outcome;
}

export function checkReplay(value, label = 'VM replay', { expectedCode } = {}) {
  assert(object(value), `${label}: malformed replay`);
  if (value.status === 'accepted') {
    keys(value, ['status', 'output_hex', 'cost'], label);
    assert(hex(value.output_hex) && Number.isSafeInteger(value.cost) && value.cost > 0, `${label}: malformed accepted replay`);
    return { status: 'accepted', output_hex: value.output_hex };
  }
  keys(value, ['status', 'error', 'failure'], label);
  assert.equal(value.status, 'rejected', `${label}: unknown/inconclusive replay outcome`);
  assert(typeof value.error === 'string' && value.error.length > 0, `${label}: missing replay diagnostic`);
  assert.equal(checkFailure(value.failure, label), 'program', `${label}: non-program replay rejection`);
  if (expectedCode !== undefined) assert.equal(value.failure.code, expectedCode, `${label}: unexpected program failure code`);
  return { status: 'rejected' };
}

export function checkCorpusExecutions(report) {
  assert.equal(report.vm_failure_protocol, failureProtocol, 'Missing/unsupported VM failure protocol');
  for (const puzzle of report.puzzles) {
    for (const test of puzzle.cases) {
      const label = `${puzzle.slug}/${test.label}`;
      checkNativeOutcome(test.native, label);
      for (const run of test.runs) checkExecution(run.execution, `${label}/${run.backend}/${run.optimize_lir}`);
    }
    for (const factory of puzzle.factory_checks ?? []) {
      for (const run of factory.application_runs) checkExecution(run.execution, `${puzzle.slug}/factory application`);
    }
  }
}

function replayRows(rows, label, expectedStatus, expectedCode) {
  assert.equal(rows.length, 2, `${label}: missing replay mode`);
  assert.deepEqual(rows.map(row => row.optimize).sort(), [false, true], `${label}: invalid replay modes`);
  for (const row of rows) {
    checkReplay(row.outcome, label, { expectedCode });
    if (expectedStatus !== undefined) assert.equal(row.outcome.status, expectedStatus, label);
  }
}

export function checkScalarProofReplays(proof) {
  assert.equal(proof.vm_failure_protocol, failureProtocol, 'Scalar proof failure protocol');
  for (const check of proof.checks) replayRows(check.clvm_nonvacuity_replay, `scalar/${check.name}`, 'accepted');
  replayRows(proof.mutation.original_clvm_replay, 'scalar/original mutation witness', 'rejected', 'raise');
  replayRows(proof.mutation.mutant_clvm_replay, 'scalar/mutant witness', 'accepted');
}

export function checkTreeProofReplays(proof) {
  assert.equal(proof.vm_failure_protocol, failureProtocol, 'Structural proof failure protocol');
  function walk(value) {
    if (!object(value) && !Array.isArray(value)) return;
    for (const [key, child] of Object.entries(value)) {
      if (key === 'native_original') checkNativeOutcome(child, 'structural/native');
      else if (key === 'canonical_reference') checkReplay(child, 'structural/reference');
      else if (['rust_original', 'mutant_ir', 'guarded', 'unguarded'].includes(key) && Array.isArray(child)) replayRows(child, `structural/${key}`);
      walk(child);
    }
  }
  walk(proof);
}

export function testFailureProtocol() {
  let controls = 0;
  const reject = (value) => { assert.throws(() => checkExecution(value)); controls++; };
  const base = { outcome: { status: 'rejected' }, cost: null, error: 'Raise(spoofed)', failure: { kind: 'program', code: 'raise' } };
  for (const [code, kind] of failures) {
    const value = { ...base, outcome: { status: kind === 'program' ? 'rejected' : 'inconclusive' }, failure: { kind, code } };
    checkExecution(value, code, { requireConclusive: false });
    if (kind !== 'program') reject(value);
  }
  reject({ ...base, failure: { kind: 'resource', code: 'cost_exceeded' } });
  reject({ ...base, failure: { kind: 'infrastructure', code: 'internal_error' } });
  reject({ ...base, failure: { kind: 'program', code: 'cost_exceeded' } });
  reject({ ...base, failure: { kind: 'program', code: 'unknown_code' } });
  reject({ ...base, failure: null });
  const missing = structuredClone(base); delete missing.failure; reject(missing);
  const accepted = { outcome: { status: 'accepted', output_hex: '80' }, cost: 1, error: null };
  checkExecution(accepted);
  reject({ ...accepted, failure: null });
  reject({ ...accepted, failure: base.failure });
  assert.throws(() => checkReplay({ status: 'rejected', error: 'Raise(spoofed)', failure: { kind: 'resource', code: 'cost_exceeded' } })); controls++;
  assert.throws(() => checkReplay({ status: 'rejected', error: 'Raise(spoofed)', failure: { kind: 'program', code: 'path_into_atom' } }, 'assertion', { expectedCode: 'raise' })); controls++;
  assert.throws(() => checkReplay({ status: 'rejected' })); controls++;
  // A projection may discard metadata only after these current records pass.
  for (const failure of [undefined, { kind: 'resource', code: 'cost_exceeded' }, { kind: 'program', code: 'unknown_code' }]) {
    const execution = structuredClone(base);
    if (failure === undefined) delete execution.failure; else execution.failure = failure;
    const corpus = { vm_failure_protocol: failureProtocol, puzzles: [{ slug: 'control', cases: [{ label: 'reject', native: { status: 'rejected' }, runs: [{ backend: 'rust_mir', optimize_lir: true, execution }] }] }] };
    assert.throws(() => checkCorpusExecutions(corpus)); controls++;
  }
  const acceptedRows = [false, true].map(optimize => ({ optimize, outcome: { status: 'accepted', output_hex: '80', cost: 1 } }));
  const rejectedRows = [false, true].map(optimize => ({ optimize, outcome: { status: 'rejected', error: 'Raise(spoofed)', failure: { kind: 'program', code: 'raise' } } }));
  const scalar = { vm_failure_protocol: failureProtocol, checks: [{ name: 'control', clvm_nonvacuity_replay: acceptedRows }], mutation: { original_clvm_replay: rejectedRows, mutant_clvm_replay: acceptedRows } };
  checkScalarProofReplays(scalar);
  for (const mutation of ['missing', 'unknown', 'resource']) {
    const corrupt = structuredClone(scalar);
    const replay = corrupt.mutation.original_clvm_replay[0].outcome;
    if (mutation === 'missing') delete replay.failure;
    else if (mutation === 'unknown') replay.failure.code = 'future_code';
    else replay.failure = { kind: 'resource', code: 'cost_exceeded' };
    assert.throws(() => checkScalarProofReplays(corrupt)); controls++;
  }
  for (const key of ['canonical_reference', 'rust_original', 'mutant_ir', 'guarded', 'unguarded']) {
    for (const mutation of ['missing', 'unknown', 'resource']) {
      const replay = structuredClone(rejectedRows[0].outcome);
      if (mutation === 'missing') delete replay.failure;
      else if (mutation === 'unknown') replay.failure.code = 'future_code';
      else replay.failure = { kind: 'resource', code: 'cost_exceeded' };
      const child = key === 'canonical_reference' ? replay : [{ optimize: false, outcome: replay }, rejectedRows[1]];
      const corrupt = { vm_failure_protocol: failureProtocol, optimization: { control: { replay: { [key]: child } } } };
      assert.throws(() => checkTreeProofReplays(corrupt)); controls++;
    }
  }
  return controls;
}
