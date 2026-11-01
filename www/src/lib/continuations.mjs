import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

const digest = raw => createHash('sha256').update(raw).digest('hex');
export const formatContinuation = value => typeof value === 'number' ? value.toLocaleString('en-US') : String(value);
const field = (id, label, value) => ({ id, label, value: formatContinuation(value) });
const output = value => value.status === 'accepted' ? value.output_hex : 'native policy panic';
const vmOutcome = execution => execution.outcome.status === 'accepted' ? 'accepted' : `${execution.outcome.status} · ${execution.failure.kind}/${execution.failure.code}`;
const sdkOutcome = value => value.accepted ? 'accepted' : value.negative.code === 'GeneratorRuntimeError'
  ? 'GeneratorRuntimeError · inconclusive; no policy rejection credit' : value.negative.code;
export const continuationScopes = [
  'WIP prototype. Not ready for real funds.',
  'Original ordered-payout teaching policy; not a generic continuation SDK.',
  'Only batch size one has complete SDK measurements.',
  'This historical simulator cursor is persisted and reparsed in one process while the simulator remains in memory.',
  'This historical simulator packet records no live Testnet11 continuation spend.',
  'Runtime umbrellas remain inconclusive as policy-rejection evidence.',
  'Source/file consistency is not execution authentication or compiler preservation.',
  'Conditions-helper calls skip BLS; actual generator runs verify BLS separately.',
  'Liveness, inclusion, finality and cross-block atomicity are not guaranteed.',
];

/** @returns {Promise<{fields: ContinuationField[], steps: ContinuationRow[], scopes: string[]}>} */
export async function readLiveContinuationEvidence() {
  const filename = path.join(await repository(), 'experiments/authenticated-continuations/live-evidence/report.json');
  assert(statSync(filename).isFile() && statSync(filename).size <= 1024 * 1024, 'Bounded live continuation summary');
  const bytes = readFileSync(filename); assert(bytes.length <= 1024 * 1024);
  const { summary } = JSON.parse(bytes.toString('utf8'));
  assert(summary.status === 'completed_trial_observed' && summary.network === 'testnet11');
  assert(summary.chain_authentication === false && summary.finality === false && summary.compiler_preservation === false);
  assert(summary.steps.length === 4 && summary.funding_coin.amount === 4000);
  assert(summary.counts.provider_observed_spends === 4 && summary.counts.retained_http_push_attempts === 5
    && summary.counts.unique_prepared_bundles === 4 && summary.counts.success_acknowledgements === 4);
  assert(summary.fees.prepared_bundles_mojos === 0 && summary.original_ambiguous_response.availability === 'not_retained');
  const hash = value => { assert(typeof value === 'string' && /^[0-9a-f]{64}$/.test(value)); return value; };
  return {
    fields: [field('spends', 'Provider-observed policy spends', 4), field('paid', 'Paid back to the test wallet (mojos)', 4000),
      field('attempts', 'HTTP pushes / distinct bundles / success acknowledgements', '5 / 4 / 4'),
      field('fees', 'Prepared policy-bundle fees (mojos; funding excluded)', 0),
      field('module', 'Retained Rust-on module tree hash', hash(summary.selected_module_hash)),
      field('funding', 'Initial 4,000-mojo coin', hash(summary.funding_coin.coin_id)),
      field('birth', 'Funding confirmation height', summary.funding_confirmed_height)],
    steps: summary.steps.map((step, index) => {
      assert(step.index === index && Number.isInteger(step.spent_height) && step.payout.amount === 1000);
      assert(index === 3 ? step.successor === null : step.successor.amount === 3000 - index * 1000);
      return { id: String(index), fields: [field('height', 'Spent at height', step.spent_height),
        field('bundle', 'Prepared bundle ID', hash(step.bundle_id)), field('payout', '1,000-mojo payout coin', hash(step.payout.coin_id)),
        field('successor', 'Continuation successor', step.successor ? hash(step.successor.coin_id) : 'none — terminal payout')] };
    }),
    scopes: ['WIP Testnet11 trial using the retained Rust-on batch-one artifact; not a reusable SDK or shared Rust CLI/GUI.',
      'Five HTTP push attempts concern four bundle IDs. The first response body was not retained; its receipt remains an inconclusive gap.',
      'One explicit identical-bundle retry and process restart are retained separately; default submission never retries automatically.',
      'Provider observations and source hashes do not authenticate inclusion, submission causality, binaries or finality.',
      'Prepared continuation bundles have zero fees. Funding fees and containing chain-bundle fees are not measured.'],
  };
}

/** @typedef {{id: string, label: string, value: string}} ContinuationField */
/** @typedef {{id: string, fields: ContinuationField[]}} ContinuationRow */
/** @typedef {{id: string, label: string, fields: ContinuationField[], steps: ContinuationRow[], rejections: ContinuationRow[]}} ContinuationLane */
/** @typedef {{id: string, label: string, roles: ContinuationRow[]}} ContinuationControl */
/** @returns {Promise<{host: string, scopes: string[], counts: ContinuationField[], context: ContinuationField[], base: ContinuationLane[], controls: ContinuationControl[]}>} */
export async function readContinuationEvidence() {
  const directory = path.join(await repository(), 'experiments/authenticated-continuations');
  const read = name => {
    const filename = path.join(directory, name);
    assert(statSync(filename).isFile() && statSync(filename).size <= 8 * 1024 * 1024, `Bounded continuation input ${name}`);
    const raw = readFileSync(filename); assert(raw.length <= 8 * 1024 * 1024); return raw;
  };
  const json = name => JSON.parse(read(name).toString('utf8'));
  const capture = json('evidence/receipt.json');
  const ledger = json('ledger/evidence/base-report.json');
  const sourceLedger = json('ledger/evidence/controls-report.json');
  const source = json('ledger/evidence/source-controls.json');
  const vm = json('evidence/report.json');
  assert.deepEqual(ledger.counts, { accepted_submissions: 12, runtime_umbrella_submissions: 12, scenario_runs: 3,
    successor_coins: 9, total_sdk_attempts: 45, typed_rejected_submissions: 21 });
  assert.deepEqual(sourceLedger.counts, { accepted_submissions: 10, runtime_umbrella_submissions: 4,
    selected_control_builds: 8, total_sdk_attempts: 16, typed_rejected_submissions: 2 });
  assert.deepEqual(source.counts, { native_observations: 8, primary_vm_observations: 16, sdk_attempts: 16 });
  assert.equal(vm.cases, 36); assert.equal(Object.keys(vm.programs).length, 3);
  const hosts = capture.compiler.split('\n').filter(line => line.startsWith('host: ')).map(line => line.slice(6));
  assert.deepEqual(hosts, ['aarch64-apple-darwin']);
  const counts = [
    field('base-vm', 'Base primary-VM observations', vm.cases * Object.keys(vm.programs).length),
    field('source-native', 'Actual-source native observations', source.counts.native_observations),
    field('source-vm', 'Actual-source primary-VM observations', source.counts.primary_vm_observations),
    field('base-scenarios', 'Base SDK scenario runs', ledger.counts.scenario_runs),
    field('base-sdk', 'Base SDK submissions', ledger.counts.total_sdk_attempts),
    field('base-accepted', 'Base accepted submissions', ledger.counts.accepted_submissions),
    field('base-typed', 'Base exact typed rejections', ledger.counts.typed_rejected_submissions),
    field('base-umbrellas', 'Base inconclusive runtime umbrellas', ledger.counts.runtime_umbrella_submissions),
    field('base-successors', 'Actual base successor coins', ledger.counts.successor_coins),
    field('source-sdk', 'Separate source-control SDK submissions', sourceLedger.counts.total_sdk_attempts),
    field('source-accepted', 'Source-control accepted submissions', sourceLedger.counts.accepted_submissions),
    field('source-typed', 'Source-control exact typed rejections', sourceLedger.counts.typed_rejected_submissions),
    field('source-umbrellas', 'Source-control inconclusive runtime umbrellas', sourceLedger.counts.runtime_umbrella_submissions),
  ];
  const labels = { rust_on: 'Rust on', rust_off: 'Rust off', chialisp: 'Independently compiled Chialisp' };
  const base = ledger.lanes.map(lane => ({ id: lane.name, label: labels[lane.name], fields: [
    field('module', 'Actual selected module tree hash', lane.module_hash), field('bytes', 'Serialized module bytes', lane.module_bytes),
    field('fees', 'Whole-chain fees in mojos', lane.total_fees),
    ...Object.entries(lane.chain_totals).flatMap(([encoding, metrics]) => [field(`${encoding}-bytes`, `${encoding} chain charged bytes`, metrics.charged_bytes), field(`${encoding}-cost`, `${encoding} chain total cost`, metrics.total_cost)]),
  ], steps: lane.steps.map(step => {
    const generators = Object.fromEntries(step.costs.generators.map(row => [row.encoding, row.metrics]));
    return { id: String(step.index), fields: [field('height', 'Previous transaction height', step.previous_transaction_height),
      field('coin', 'Actual spent coin ID', step.coin.coin_id), field('recipient', 'Committed payout puzzle hash', step.payout.puzzle_hash),
      field('amount', 'Payout in mojos', step.payout.amount), field('successor', 'Actual successor coin ID', step.successor?.coin_id ?? 'none'),
      field('fees', 'Step fees in mojos', step.fees),
      ...Object.entries(generators).flatMap(([encoding, metrics]) => [field(`${encoding}-bytes`, `${encoding} charged bytes`, metrics.charged_bytes), field(`${encoding}-cost`, `${encoding} total cost`, metrics.total_cost)]),
    ] };
  }), rejections: lane.typed_rejections.map((row, index) => ({ id: String(index), fields: [field('label', 'Submission', row.label), field('code', 'Exact consensus code', row.code), field('state', 'Rejected-state check', row.full_simulator_equal ? 'full simulator equality checked in memory' : 'FAILED')] })) }));
  const controls = sourceLedger.controls.map(row => ({ id: `${row.kind}:${row.mode}`, label: `${row.kind} · Rust ${row.mode}`,
    roles: ['original', 'mutant'].map(role => {
      const observed = row[role];
      const native = source.native.find(value => value.kind === row.kind && value.role === role).observed;
      const primary = source.primary_vm.find(value => value.kind === row.kind && value.role === role && value.mode === row.mode).execution;
      return { id: role, fields: [field('module', 'Own actual module tree hash', observed.module_hash), field('coin', 'Own funded coin ID', observed.coin.coin_id),
        field('native', 'Native policy observation', native.status === 'accepted' ? 'accepted' : 'native policy panic'),
        field('native-output', 'Native output bytes', output(native)), field('primary', 'Primary-VM typed observation', vmOutcome(primary)),
        field('primary-output', 'Primary-VM output bytes', primary.outcome.output_hex ?? 'none'), field('sdk', 'Actual SDK observation', sdkOutcome(observed)),
        field('recipient', 'Actual payout puzzle hash', observed.payout?.puzzle_hash ?? 'not spent'),
        field('successor', 'Actual successor puzzle hash', observed.accepted ? observed.successor?.puzzle_hash ?? 'none' : 'not spent'),
        field('fees', 'Actual fees in mojos', observed.accepted ? observed.fees : 'not spent') ] };
    }) }));
  return { host: hosts[0], scopes: continuationScopes, counts, base, controls,
    context: [field('primary-vm', 'Primary VM and flags', capture.primary_vm), field('primary-failure', 'Primary typed-failure protocol', 'clvmr-0.21.0/v1'), field('sdk', 'SDK simulator', ledger.sdk), field('consensus', 'SDK consensus version', ledger.consensus), field('sdk-vm', 'SDK execution VM', ledger.vm), field('network', 'Simulator network constants', ledger.network), field('spike-receipt', 'Retained ABI/VM receipt SHA-256', digest(read('evidence/receipt.json'))), field('ledger-receipt', 'Retained SDK/source-control receipt SHA-256', digest(read('ledger/evidence/receipt.json')))] };
}
