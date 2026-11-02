import assert from 'node:assert/strict';
import { readFileSync, lstatSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const modes = ['chialisp', 'rust_on', 'rust_off'];
const labels = { chialisp: 'Independently compiled Chialisp', rust_on: 'Rust on', rust_off: 'Rust off' };
const encodings = ['uncompressed', 'backrefs'];
const metrics = ['charged_bytes', 'execution_cost', 'condition_cost', 'byte_cost', 'total_cost'];
const metricLabels = { charged_bytes: 'charged generator bytes', execution_cost: 'execution cost', condition_cost: 'condition cost', byte_cost: 'byte cost', total_cost: 'total cost' };
const integer = n => assert(Number.isSafeInteger(n) && n >= 0, 'Nonnegative safe measured integer');
const hash = h => { assert(typeof h === 'string' && /^[0-9a-f]{64}$/.test(h), 'SHA-256/tree hash'); return h; };
const field = (id, label, value) => ({ id, label, value: typeof value === 'number' ? value.toLocaleString('en-US') : String(value) });
function bytes(filename, limit = 32 * 1024 * 1024) {
  const stat = lstatSync(filename); assert(stat.isFile() && !stat.isSymbolicLink() && stat.size <= limit, 'Bounded regular evidence file');
  const raw = readFileSync(filename); assert(raw.length <= limit); return raw;
}
function confined(root, relative) {
  assert(typeof relative === 'string' && relative.length < 4096 && !relative.includes('\\') && !path.isAbsolute(relative));
  assert(relative.split('/').every(part => part && part !== '.' && part !== '..'), 'Confined packet path');
  const filename = path.join(root, relative);
  let current = root;
  for (const part of relative.split('/')) { current = path.join(current, part); assert(!lstatSync(current).isSymbolicLink(), 'No packet symlink'); }
  return filename;
}
export const statefulScopes = [
  'WIP original teaching policies. Not ready for use or real funds.',
  'Three separate modes: independently compiled Chialisp, Rust on and Rust off. Repeated captures add no distinct coverage.',
  'Exact typed consensus rejections and inconclusive GeneratorRuntimeError umbrellas are counted separately; every rejected submission checks full simulator equality in memory.',
  'Actual-source mutants are a separate evidence lane. Native observations, primary-VM observations and SDK attempts are never interchangeable.',
  'Spuds has no wallet integration or live-network lane. Its top-up and batch inflation controls use quoted composition fixtures.',
  'Finite simulator executions and source/file consistency do not authenticate execution, establish compiler preservation, or guarantee inclusion, finality or liveness.',
];

function admitInventory(root, receipt, sourceRoot) {
  assert(receipt.files && Object.keys(receipt.files).length > 0 && Object.keys(receipt.files).length <= 4096);
  const programs = new Map();
  let remaining = 256 * 1024 * 1024;
  for (const [relative, digest] of Object.entries(receipt.files)) {
    const filename = confined(root, relative);
    const size = lstatSync(filename).size; assert(size <= remaining, 'Aggregate packet read budget'); remaining -= size;
    const raw = bytes(filename, Math.min(32 * 1024 * 1024, size)); assert.equal(sha(raw), hash(digest), 'Complete packet byte identity');
    if (relative.endsWith('.clvm.hex')) {
      const text = raw.toString('utf8').trim(); assert(/^(?:[0-9a-f]{2})+$/.test(text), 'Exact serialized program hex');
      const decoded = Buffer.from(text, 'hex'); programs.set(sha(raw), { blob: sha(decoded), length: decoded.length });
    }
  }
  if (sourceRoot) {
    assert(receipt.source_files && Object.keys(receipt.source_files).length > 0 && Object.keys(receipt.source_files).length <= 4096, 'Complete declared current source window');
    for (const [relative, digest] of Object.entries(receipt.source_files)) assert.equal(sha(bytes(confined(sourceRoot, relative))), hash(digest), 'Current declared source changed; recapture required');
  }
  return programs;
}

/** Retained packet reader only: this grants no fresh execution credit. */
export function admitStatefulSuccess(root, experiment, sourceRoot) {
  const filename = path.join(root, 'receipt.json');
  const receipt = JSON.parse(bytes(filename).toString('utf8'));
  assert(receipt.protocol === 'chialisp-oxide-stateful-success/v1' && receipt.status === 'successful_source_capture' && receipt.experiment === experiment);
  assert(receipt.compiler_preservation === false && receipt.execution_authentication === false && receipt.source_authentication === false && receipt.sdk_spends === 0);
  assert(receipt.vm.status === 'passed' && receipt.vm.mode_scope === 'three_mode');
  assert.deepEqual(Object.keys(receipt.vm.programs).sort(), [...modes].sort(), 'Exact three VM modes');
  const programs = admitInventory(root, receipt, sourceRoot);
  assert.deepEqual(JSON.parse(bytes(path.join(root, 'vm/report.json')).toString('utf8')), receipt.vm);
  const blobs = Object.fromEntries(modes.map(mode => {
    const digest = hash(receipt.vm.programs[mode].program_hex_sha256);
    assert(programs.has(digest), 'Executed VM module must occur in the admitted packet');
    assert.equal(programs.get(digest).length, receipt.vm.programs[mode].serialized_bytes);
    return [mode, programs.get(digest).blob];
  }));
  return { receipt, blobs, receiptSHA: sha(bytes(filename)) };
}

const mutationNames = {
  'continuation-batches': ['second_opening', 'amount', 'terminal', 'successor'],
  spuds: ['amount', 'age', 'nextstate', 'announcement', 'signature_binding'],
};

/** Distinct actual-source lane; native rows are counted once per variant. */
export function statefulMutationView(receipt, recipes = {}) {
  const names = mutationNames[receipt.experiment]; assert(names);
  assert(receipt.protocol === 'chialisp-oxide-stateful-mutations/v1' && receipt.status === 'actual_source_mutations_captured');
  assert(receipt.compiler_preservation === false && receipt.execution_authentication === false && receipt.source_authentication === false);
  assert.deepEqual(receipt.cases.map(row => row.name).sort(), [...names].sort(), 'Complete exact actual-source mutant set');
  assert(receipt.sdk.protocol === 'chialisp-oxide-stateful-mutation-sdk/v1' && receipt.sdk.status === 'passed' && receipt.sdk.compiler_preservation === false && receipt.sdk.standard_wallet_integration === false);
  assert.equal(receipt.sdk.records.length, names.length * 4);
  let accepted = 0, typed = 0, umbrellas = 0;
  const controls = receipt.cases.flatMap(control => {
    assert.deepEqual(Object.keys(control.variants).sort(), ['mutant', 'original']);
    const recipe = recipes[control.name];
    if (recipe) assert(recipe.occurrences === 1 && typeof recipe.before === 'string' && typeof recipe.after === 'string');
    return ['rust_on', 'rust_off'].map(mode => {
      const roles = ['original', 'mutant'].map(role => {
        const variant = control.variants[role];
        assert(['accepted', 'rejected'].includes(variant.native.status));
        assert.deepEqual(Object.keys(variant.primary_vm).sort(), ['rust_off', 'rust_on']);
        const vm = variant.primary_vm[mode];
        assert.deepEqual(vm.outcome, variant.native, 'Actual-source native and VM exact witness');
        if (vm.outcome.status === 'rejected') assert(vm.failure.kind === 'program' && vm.failure.code === 'raise', 'Typed source policy rejection');
        const matches = receipt.sdk.records.filter(row => row.project === receipt.experiment && row.case === control.name && row.variant === role && row.mode === mode);
        assert.equal(matches.length, 1, 'Unique own-reveal SDK observation for every source variant/mode');
        const row = matches[0], actual = row.actual, submission = actual.submission;
        const observed = submission.observation ?? submission;
        const isAccepted = submission.status === 'accepted';
        let outcome;
        if (isAccepted) { accepted++; outcome = 'accepted'; assert(Array.isArray(observed.outputs) && observed.outputs.length > 0); }
        else {
          const umbrella = observed.code === 'GeneratorRuntimeError';
          assert(observed.full_simulator_equal === true && observed.policy_rejection_credit === !umbrella);
          assert.equal(observed.classification, umbrella ? 'inconclusive_runtime_umbrella' : 'typed_consensus_rejection');
          if (umbrella) umbrellas++; else typed++;
          outcome = umbrella ? 'GeneratorRuntimeError · inconclusive; no policy rejection credit' : `typed consensus rejection · ${observed.code}`;
        }
        const coin = actual.own_reveal_funding;
        return { id: role, fields: [field('source', 'Actual source SHA-256', hash(variant.source_sha256)),
          field('module', 'Own actual module tree hash', hash(row.module_hash)), field('blob', 'Own serialized module SHA-256', hash(row.artifact_blob_sha256)),
          field('coin', 'Own reveal funded coin ID', hash(coin.coin_id)), field('funding', 'Own reveal funding puzzle hash', hash(coin.puzzle_hash)),
          field('native', 'Native source observation', variant.native.status === 'accepted' ? 'accepted' : 'native policy panic'),
          field('native-output', 'Native output bytes (fixture module field)', variant.native.output_hex ?? 'none'),
          field('primary', 'Primary-VM observation', vm.outcome.status === 'accepted' ? 'accepted' : `${vm.failure.kind}/${vm.failure.code}`),
          field('primary-output', 'Primary-VM output bytes (fixture module field)', vm.outcome.output_hex ?? 'none'),
          field('sdk', 'Own-reveal SDK observation', outcome), field('state', 'Rejected-state check', isAccepted ? 'accepted; exact complete outputs checked' : 'full simulator equality checked in memory'),
          field('outputs', 'Actual SDK output coins · puzzle hash · amount', isAccepted ? observed.outputs.map(c => `${hash(c.coin_id)} · ${hash(c.puzzle_hash)} · ${c.amount}`).join(' / ') : 'not spent')] };
      });
      return { id: `${control.name}:${mode}`, label: `${control.name} · ${labels[mode]}`, fields: recipe ? [field('before', 'Original Rust source recipe (whitespace collapsed)', recipe.before.trim().replace(/\s+/g, ' ')), field('after', 'Changed Rust source recipe · one occurrence (whitespace collapsed)', recipe.after.trim().replace(/\s+/g, ' '))] : [], roles };
    });
  });
  return { status: 'retained_actual_source_mutations', counts: [field('native', 'Separate actual-source native observations', names.length * 2), field('primary', 'Separate actual-source primary-VM observations', names.length * 4), field('sdk', 'Separate actual-source SDK submissions', names.length * 4), field('accepted', 'Source-control accepted SDK submissions', accepted), field('typed', 'Source-control exact typed SDK rejections', typed), field('umbrellas', 'Source-control inconclusive SDK runtime umbrellas', umbrellas)], controls };
}

function readMutations(root, experiment, sourceRoot) {
  const filename = path.join(root, 'receipt.json');
  const receipt = JSON.parse(bytes(filename).toString('utf8'));
  assert.equal(receipt.experiment, experiment);
  admitInventory(root, receipt, sourceRoot);
  assert.deepEqual(JSON.parse(bytes(path.join(root, 'sdk-report.json')).toString('utf8')), receipt.sdk);
  const recipes = Object.fromEntries(mutationNames[experiment].map(name => [name, JSON.parse(bytes(confined(root, `${name}/recipe.json`)).toString('utf8'))]));
  return statefulMutationView(receipt, recipes);
}

function metricFields(value, prefix = '', delta = false) {
  return encodings.flatMap(encoding => metrics.map(metric => {
    const number = value[encoding][metric];
    if (delta) assert(Number.isSafeInteger(number), 'Signed measured delta'); else integer(number);
    return field(`${prefix}${encoding}-${metric}`, `${encoding} ${metricLabels[metric]}${delta ? ' · batch two minus batch one' : ''}`, delta && number > 0 ? `+${number.toLocaleString('en-US')} (regression)` : number);
  }));
}
function controls(rows) {
  assert(Array.isArray(rows) && rows.length <= 64);
  return rows.map((row, index) => {
    assert(row.full_simulator_equal === true);
    const umbrella = row.code === 'GeneratorRuntimeError';
    assert.equal(row.policy_rejection_credit, !umbrella);
    assert.equal(row.classification, umbrella ? 'inconclusive_runtime_umbrella' : 'typed_consensus_rejection');
    return { id: String(index), fields: [field('label', 'Submitted control', row.label), field('code', 'Exact SDK consensus code', row.code), field('classification', 'Credit scope', umbrella ? 'inconclusive runtime umbrella; no policy rejection credit' : 'typed consensus rejection'), field('state', 'State after rejection', 'full simulator equality checked in memory')] };
  });
}
function chain(value, id, label, width, count, queue = true) {
  assert(value.steps.length === count / width);
  if (queue) assert(value.batch_width === width && value.count === count && value.payout_total === count * 1000 && value.terminal_successor === false);
  const sums = Object.fromEntries(encodings.map(encoding => [encoding, Object.fromEntries(metrics.map(metric => [metric, 0]))]));
  const steps = value.steps.map((step, index) => {
    integer(step.height); assert(step.removals.length >= 1 && step.outputs.length >= width);
    assert(step.measurement.cost_per_byte === 12000 && step.measurement.previous_transaction_height === step.height, 'Pinned byte charge and actual previous transaction height');
    const observed = {};
    assert.deepEqual(step.measurement.generators.map(row => row.encoding), encodings);
    for (const generator of step.measurement.generators) {
      assert(generator.metrics.signature_verified === true && generator.budget_control.exact_cost_succeeded === true && generator.budget_control.one_less_code === 'CostExceeded');
      assert(generator.generator_bytes === generator.metrics.charged_bytes, 'Actual generator charges every serialized byte');
      assert(generator.metrics.byte_cost === generator.metrics.charged_bytes * 12000 && generator.metrics.total_cost === generator.metrics.execution_cost + generator.metrics.condition_cost + generator.metrics.byte_cost, 'Complete charged generator cost');
      assert(generator.signature_controls.some(row => row.signature === 'unrelated_nonempty' && row.code === 'BadAggregateSignature'), 'Actual unrelated-signature control');
      if (!queue) assert(generator.signature_controls.some(row => row.signature === 'identity' && row.code === 'BadAggregateSignature'), 'Actual signed Spuds identity-signature control');
      observed[generator.encoding] = generator.metrics;
      for (const metric of metrics) { integer(generator.metrics[metric]); sums[generator.encoding][metric] += generator.metrics[metric]; }
    }
    return { id: String(index), fields: [field('height', 'Previous transaction height', step.height), field('inputs', 'Actual removed coin IDs', step.removals.map(coin => hash(coin.coin_id)).join(' · ')), field('outputs', 'Complete actual output coin IDs', step.outputs.map(coin => hash(coin.coin_id)).join(' · ')), ...metricFields(observed)] };
  });
  for (const encoding of encodings) for (const metric of metrics) assert.equal(value.whole_chain[encoding][metric], sums[encoding][metric], 'Whole-chain sum must match every measured step');
  return { id, label, fields: [field('accepted', 'Accepted SDK submissions', steps.length), ...metricFields(value.whole_chain)], steps, rejections: controls(value.controls) };
}

/** Normalize actual report values; never manufacture a three-mode success. */
export function statefulLedgerView(report, packets) {
  assert(report.schema === 1 && report.scope === 'three_mode_sdk_execution' && report.network === 'TESTNET11');
  assert(report.compiler_preservation_proof === false && report.standard_wallet_integration === false);
  assert.deepEqual(report.reports.map(row => row.mode), modes, 'Exact ordered three SDK modes');
  const batch = [], spuds = [];
  for (const lane of report.reports) {
    for (const [kind, experiment] of [['batch_two', 'continuation-batches'], ['spuds', 'spuds']]) assert.equal(hash(lane.artifact_blob_sha256[kind]), packets[experiment].blobs[lane.mode], 'SDK must execute the actual source-capture artifact');
    assert.deepEqual(lane.batch_comparisons.map(row => row.count), [4, 8]);
    const comparisons = lane.batch_comparisons.map(row => {
      assert.deepEqual(row.batch_one.recipient_queue, row.batch_two.recipient_queue, 'Same ordered pair-distinct queue');
      const chains = [chain(row.batch_one, 'one', 'Immutable batch-one baseline', 1, row.count), chain(row.batch_two, 'two', 'Batch-two policy', 2, row.count)];
      for (const encoding of encodings) for (const metric of metrics) assert.equal(row.batch_two_minus_batch_one[encoding][metric], row.batch_two.whole_chain[encoding][metric] - row.batch_one.whole_chain[encoding][metric], 'Signed measured comparison');
      return { id: String(row.count), label: `${row.count} recipients`, fields: metricFields(row.batch_two_minus_batch_one, '', true), chains };
    });
    batch.push({ id: lane.mode, label: labels[lane.mode], fields: [field('module', 'Batch-two module tree hash', hash(lane.module_hashes.batch_two)), field('baseline', 'Immutable batch-one module tree hash', hash(lane.module_hashes.batch_one))], comparisons });
    const s = lane.spuds;
    assert.deepEqual(s.take_ages, [1, 4]); assert(s.claim_age === 5 && s.timer_reset_checked === true && s.exact_six_condition_VM_outputs_checked === true && s.steps.length === 3);
    // Spuds has a fixed three-step take/take/claim chain, rather than a queue.
    spuds.push({ id: lane.mode, label: labels[lane.mode], fields: [field('module', 'Spuds module tree hash', hash(lane.module_hashes.spuds)), field('ages', 'Actual take ages / claim age', '1, 4 / 5'), field('reset', 'Successor timer reset and exact six-condition VM output', 'checked')], chain: chain(s, 'spuds', 'Take, take, claim', 1, 3, false) });
  }
  const counts = rows => {
    const chains = rows.flatMap(row => row.comparisons ? row.comparisons.flatMap(c => c.chains) : [row.chain]);
    const rejections = chains.flatMap(c => c.rejections);
    const umbrellas = rejections.filter(r => r.fields.find(f => f.id === 'code').value === 'GeneratorRuntimeError').length;
    const accepted = chains.reduce((n, c) => n + c.steps.length, 0);
    return [field('accepted', 'Accepted base SDK submissions', accepted), field('typed', 'Exact typed rejected base SDK submissions', rejections.length - umbrellas), field('umbrellas', 'Inconclusive base runtime umbrellas', umbrellas), field('attempts', 'Total base SDK submissions', accepted + rejections.length)];
  };
  return { batch: { lanes: batch, counts: counts(batch) }, spuds: { lanes: spuds, counts: counts(spuds) }, context: [field('sdk', 'SDK simulator', report.simulator), field('consensus', 'Consensus implementation', report.consensus), field('vm', 'SDK VM', report.vm), field('network', 'Simulator constants', report.network), field('byte-rule', 'Pinned charged-generator rule', '12,000 cost per full serialized generator byte. The separate helper estimate excludes the two-byte quote prefix.'), field('measurement', 'Measurement scope', 'Actual uncompressed and backreference generator runs verify BLS; conditions-helper calls skip BLS and are separate. Generator bytes are not whole spend-bundle wire bytes.')] };
}

export const statefulInputPaths = ['continuation-batches', 'spuds'].flatMap(experiment => ['success-evidence', 'source-controls'].map(lane => `experiments/${experiment}/${lane}/receipt.json`)).concat(['experiments/stateful-ledger/evidence/report.json', 'experiments/stateful-ledger/evidence/receipt.json']);
export function requireStatefulInputs(root) {
  for (const relative of statefulInputPaths) assert(existsSync(path.join(root, relative)), `Required complete stateful evidence input: ${relative}`);
}

export async function readStatefulEvidence() {
  const root = await repository();
  requireStatefulInputs(root);
  const ledgerPath = path.join(root, 'experiments/stateful-ledger/evidence/report.json');
  const packets = Object.fromEntries(['continuation-batches', 'spuds'].map(experiment => [experiment, admitStatefulSuccess(path.join(root, 'experiments', experiment, 'success-evidence'), experiment, root)]));
  const ledgerRoot = path.dirname(ledgerPath);
  const ledgerReceipt = JSON.parse(bytes(path.join(ledgerRoot, 'receipt.json')).toString('utf8'));
  assert(ledgerReceipt.protocol === 'chialisp-oxide-stateful-ledger-capture/v1' && ledgerReceipt.status === 'successful_stateful_ledger_capture');
  admitInventory(ledgerRoot, ledgerReceipt, root);
  const reportBytes = bytes(ledgerPath); assert.equal(sha(reportBytes), hash(ledgerReceipt.report_sha256), 'Actual SDK report bytes bound to capture receipt');
  const report = JSON.parse(reportBytes.toString('utf8'));
  const view = statefulLedgerView(report, packets);
  assert.deepEqual(ledgerReceipt.counts, { accepted_sdk: 63, typed_rejected_sdk: 108, runtime_umbrella_sdk: 15, sdk_attempts: 186 }, 'Closed three-mode base SDK counts');
  const sourceControls = Object.fromEntries([['batch', 'continuation-batches'], ['spuds', 'spuds']].map(([kind, experiment]) => [kind, readMutations(path.join(root, 'experiments', experiment, 'source-controls'), experiment, root)]));
  return { status: 'retained_three_mode_execution', scopes: statefulScopes, ...view, sourceControls, context: [...view.context, ...Object.entries(packets).flatMap(([experiment, packet]) => [field(`${experiment}-receipt`, `${experiment} success receipt SHA-256`, packet.receiptSHA), field(`${experiment}-native`, `${experiment} native tests (separate host scope)`, packet.receipt.native_tests), field(`${experiment}-vm`, `${experiment} base primary-VM observations (separate from SDK)`, packet.receipt.vm.cases * modes.length)])] };
}

const downloadPackets = {
  'batch-success': 'experiments/continuation-batches/success-evidence',
  'batch-controls': 'experiments/continuation-batches/source-controls',
  'spuds-success': 'experiments/spuds/success-evidence',
  'spuds-controls': 'experiments/spuds/source-controls',
  ledger: 'experiments/stateful-ledger/evidence',
};
let downloadPromise;
/** Exact canonical packet bytes. Generated inventory excludes its own digest. */
export function readStatefulDownloads() {
  downloadPromise ??= (async () => {
    await readStatefulEvidence();
    const root = await repository(), payloads = new Map(), files = [];
    let remaining = 64 * 1024 * 1024;
    for (const [packet, directory] of Object.entries(downloadPackets)) {
      const receipt = JSON.parse(bytes(confined(root, `${directory}/receipt.json`)).toString('utf8'));
      assert(!Object.hasOwn(receipt.files, 'receipt.json'), 'Own receipt is not a self-hashed member');
      for (const relative of ['receipt.json', ...Object.keys(receipt.files).sort()]) {
        assert(files.length < 4096, 'Aggregate physical download file bound');
        const source = `${directory}/${relative}`, filename = confined(root, source);
        const size = lstatSync(filename).size;
        assert(size <= remaining, 'Precharged aggregate physical download bytes'); remaining -= size;
        const raw = bytes(filename, Math.min(size, 32 * 1024 * 1024));
        const digest = sha(raw);
        if (relative !== 'receipt.json') assert.equal(digest, hash(receipt.files[relative]), 'Download differs from admitted canonical packet');
        const path = `${packet}/${relative}`;
        assert(!payloads.has(path), 'Unique physical download path');
        files.push({ path, source, bytes: raw.length, sha256: digest }); payloads.set(path, raw);
      }
    }
    files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
    const inventoryBytes = Buffer.from(JSON.stringify({ schema: 1, protocol: 'chialisp-oxide-stateful-downloads/v1', scope: 'Exact retained canonical packet bytes; inventory excludes its own digest. Consistency is not execution authentication or compiler preservation.', files }, null, 2) + '\n');
    assert(inventoryBytes.length <= 2 * 1024 * 1024 && inventoryBytes.length <= remaining, 'Bounded generated download inventory');
    payloads.set('inventory.json', inventoryBytes);
    return { downloads: [...files, { path: 'inventory.json', source: null, bytes: inventoryBytes.length, sha256: sha(inventoryBytes) }], payloads, inventoryBytes };
  })();
  return downloadPromise;
}
