import assert from 'node:assert/strict';
import { readFileSync, readdirSync, lstatSync, mkdtempSync, mkdirSync, writeFileSync, unlinkSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { readStatefulEvidence, readStatefulDownloads, requireStatefulInputs, statefulInputPaths, statefulLedgerView, statefulMutationView, statefulScopes } from '../src/lib/stateful-evidence.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const parser = unified().use(rehypeParse);
const text = node => node.type === 'text' ? node.value : (node.children ?? []).map(text).join('');
const normalized = node => text(node).trim().replace(/\s+/g, ' ');
function find(node, predicate, parents = []) { return [...(predicate(node) ? [{ node, parents }] : []), ...(node.children ?? []).flatMap(child => find(child, predicate, [...parents, node]))]; }
function visible(node) {
  assert(!['script', 'style', 'template', 'noscript'].includes(node.tagName));
  const p = node.properties ?? {};
  assert(p.hidden === undefined && p.ariaHidden !== true && p.ariaHidden !== 'true');
  assert(!/display\s*:\s*none|visibility\s*:\s*hidden|opacity\s*:\s*0(?:[;\s]|$)/i.test(p.style ?? ''));
  assert(!(p.className ?? []).some(value => /^(?:sr-only|hidden|invisible)$/.test(value)));
}
function one(node, property, value = '') {
  const rows = find(node, child => child.properties?.[property] === value);
  assert.equal(rows.length, 1, `One ${property}=${value}`);
  for (const parent of [...rows[0].parents, rows[0].node]) visible(parent);
  return rows[0].node;
}
function fields(node, expected) {
  assert.equal(find(node, child => child.properties?.dataStatefulField !== undefined).length, expected.length, 'Exact visible field inventory');
  for (const item of expected) {
    assert.equal(normalized(one(node, 'dataStatefulField', item.id)), item.value, `Actual visible witness ${item.id}`);
    const located = find(node, child => child.properties?.dataStatefulField === item.id)[0];
    const wrapper = located.parents.at(-2);
    const label = wrapper?.children?.find(child => child.tagName === 'dt');
    assert(label, `Visible label for ${item.id}`); visible(label);
    assert.equal(normalized(label), item.label, `Metric/credit label for ${item.id}`);
  }
}
function chain(node, expected) {
  const shown = one(node, 'dataStatefulChain', expected.id);
  assert.equal(normalized(shown.children.find(child => child.tagName === 'summary')), `${expected.label}: ${expected.steps.length} accepted SDK steps`);
  fields(one(shown, 'dataStatefulChainHeader'), expected.fields);
  for (const [property, rows] of [['dataStatefulStep', expected.steps], ['dataStatefulRejection', expected.rejections]]) {
    assert.equal(find(shown, child => child.properties?.[property] !== undefined).length, rows.length);
    for (const row of rows) fields(one(shown, property, row.id), row.fields);
  }
}
const capturedControls = 'Actual Rust sources were changed by one exact recipe and compiled under both modes. Native and primary-VM output witnesses use fixture module fields; each SDK attempt instead funds its own actual compiled reveal. These counts are separate from base runs and from constructed IR compiler controls.';
function sourceControls(section, expected) {
  const shown = one(section, 'dataStatefulSourceControls');
  assert.equal(shown.properties.dataStatefulControlStatus, expected.status);
  assert.equal(expected.status, 'retained_actual_source_mutations');
  assert.equal(normalized(one(shown, 'dataStatefulControlScope')), capturedControls);
  assert.equal(find(shown, node => node.properties?.dataStatefulControl !== undefined).length, expected.controls.length);
  fields(one(shown, 'dataStatefulControlCounts'), expected.counts);
  for (const control of expected.controls) {
    const row = one(shown, 'dataStatefulControl', control.id);
    assert.equal(normalized(row.children.find(child => child.tagName === 'summary')), control.label);
    fields(one(row, 'dataStatefulRecipe'), control.fields);
    assert.equal(find(row, node => node.properties?.dataStatefulRole !== undefined).length, 2);
    for (const role of control.roles) fields(one(row, 'dataStatefulRole', role.id), role.fields);
  }
}
export function checkStatefulDisplay(ast, experiment, evidence) {
  const section = one(ast, 'dataStatefulEvidence', experiment);
  assert.equal(section.properties.dataStatefulStatus, evidence.status);
  const scopes = find(section, node => node.properties?.dataStatefulScope !== undefined);
  assert.deepEqual(scopes.map(row => { for (const parent of [...row.parents, row.node]) visible(parent); return normalized(row.node); }), evidence.scopes);
  sourceControls(section, evidence.sourceControls[experiment]);
  const boundary = one(section, 'dataStatefulBoundary');
  assert(boundary.properties.href.endsWith(`/experiments/${experiment === 'batch' ? 'continuation-batches' : 'spuds'}/evidence`));
  const selected = evidence[experiment];
  assert(selected, 'Complete captured evidence required');
  assert.equal(normalized(one(section, 'dataStatefulResult')), 'Retained complete three-mode source capture and SDK execution. These readers check retained consistency; they add no fresh execution credit.');
  fields(one(section, 'dataStatefulContext'), evidence.context);
  fields(one(section, 'dataStatefulCounts'), selected.counts);
  assert.equal(find(section, node => node.properties?.dataStatefulLane !== undefined).length, 3);
  for (const lane of selected.lanes) {
    const shown = one(section, 'dataStatefulLane', lane.id);
    assert.equal(normalized(shown.children.find(child => child.tagName === 'summary')), lane.label);
    fields(one(shown, 'dataStatefulLaneHeader'), lane.fields);
    if (experiment === 'batch') {
      assert.equal(find(shown, node => node.properties?.dataStatefulComparison !== undefined).length, 2);
      for (const comparison of lane.comparisons) {
        const row = one(shown, 'dataStatefulComparison', comparison.id);
        fields(one(row, 'dataStatefulDeltas'), comparison.fields);
        assert.equal(find(row, node => node.properties?.dataStatefulChain !== undefined).length, 2);
        for (const expected of comparison.chains) chain(row, expected);
      }
    } else chain(shown, lane.chain);
  }
}

const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const renderFields = rows => rows.map(row => `<div><dt>${escape(row.label)}</dt><dd><code data-stateful-field="${row.id}">${escape(row.value)}</code></dd></div>`).join('');
const renderChain = row => `<details data-stateful-chain="${row.id}"><summary>${row.label}: ${row.steps.length} accepted SDK steps</summary><div data-stateful-chain-header>${renderFields(row.fields)}</div>${row.steps.map(step => `<div data-stateful-step="${step.id}">${renderFields(step.fields)}</div>`).join('')}${row.rejections.map(rejection => `<div data-stateful-rejection="${rejection.id}">${renderFields(rejection.fields)}</div>`).join('')}</details>`;
function renderControls(controls) {
  return `<section data-stateful-source-controls data-stateful-control-status="${controls.status}"><p data-stateful-control-scope>${capturedControls}</p>${`<div data-stateful-control-counts>${renderFields(controls.counts)}</div>${controls.controls.map(control => `<details data-stateful-control="${control.id}"><summary>${control.label}</summary><div data-stateful-recipe>${renderFields(control.fields)}</div>${control.roles.map(role => `<div data-stateful-role="${role.id}">${renderFields(role.fields)}</div>`).join('')}</details>`).join('')}`}</section>`;
}
function render(experiment, evidence) {
  const selected = evidence[experiment];
  return `<section data-stateful-evidence="${experiment}" data-stateful-status="${evidence.status}">${evidence.scopes.map(scope => `<p data-stateful-scope>${escape(scope)}</p>`).join('')}<p data-stateful-result>Retained complete three-mode source capture and SDK execution. These readers check retained consistency; they add no fresh execution credit.</p><div data-stateful-context>${renderFields(evidence.context)}</div><div data-stateful-counts>${renderFields(selected.counts)}</div>${selected.lanes.map(lane => `<details data-stateful-lane="${lane.id}"><summary>${lane.label}</summary><div data-stateful-lane-header>${renderFields(lane.fields)}</div>${experiment === 'batch' ? lane.comparisons.map(c => `<section data-stateful-comparison="${c.id}"><div data-stateful-deltas>${renderFields(c.fields)}</div>${c.chains.map(renderChain).join('')}</section>`).join('') : renderChain(lane.chain)}</details>`).join('')}${renderControls(evidence.sourceControls[experiment])}<a data-stateful-boundary href="https://github.com/judeallred/chialisp-oxide/tree/main/experiments/${experiment === 'batch' ? 'continuation-batches' : 'spuds'}/evidence">Historical boundary</a></section>`;
}

// Constructed report/layout fixture only: no program is executed here and no
// Rust/native/SDK coverage is credited by these synthetic values.
const digest = 'ab'.repeat(32);
function fixtureChain(count, width) {
  const steps = Array.from({ length: count / width }, (_, i) => ({ height: i,
    removals: [{ coin_id: digest }], outputs: Array.from({ length: width }, () => ({ coin_id: digest })),
    measurement: { cost_per_byte: 12000, previous_transaction_height: i, generators: ['uncompressed', 'backrefs'].map(encoding => ({ encoding, generator_bytes: 1,
      budget_control: { exact_cost_succeeded: true, one_less_code: 'CostExceeded' },
      signature_controls: [{ signature: 'unrelated_nonempty', code: 'BadAggregateSignature' }, { signature: 'identity', code: 'BadAggregateSignature' }],
      metrics: { charged_bytes: 1, execution_cost: 1, condition_cost: 1, byte_cost: 12000, total_cost: 12002, signature_verified: true } })) } }));
  const control = (label, code) => ({ label, code, full_simulator_equal: true,
    policy_rejection_credit: code !== 'GeneratorRuntimeError',
    classification: code === 'GeneratorRuntimeError' ? 'inconclusive_runtime_umbrella' : 'typed_consensus_rejection' });
  return { count, batch_width: width, payout_total: count * 1000, terminal_successor: false,
    recipient_queue: Array.from({ length: count }, (_, i) => i.toString(16).padStart(64, '0')), steps,
    controls: [control('amount', 'AssertMyAmountFailed'), control('malformed', 'GeneratorRuntimeError')],
    whole_chain: Object.fromEntries(['uncompressed', 'backrefs'].map(encoding => [encoding,
      { charged_bytes: steps.length, execution_cost: steps.length, condition_cost: steps.length, byte_cost: steps.length * 12000, total_cost: steps.length * 12002 }])) };
}
const synthetic = { schema: 1, scope: 'three_mode_sdk_execution', network: 'TESTNET11',
  compiler_preservation_proof: false, standard_wallet_integration: false,
  simulator: 'constructed simulator label', consensus: 'constructed consensus label', vm: 'constructed VM label',
  reports: ['chialisp', 'rust_on', 'rust_off'].map(mode => ({ mode,
    artifact_blob_sha256: { batch_two: digest, spuds: digest }, module_hashes: { batch_two: digest, batch_one: digest, spuds: digest },
    batch_comparisons: [4, 8].map(count => { const one = fixtureChain(count, 1), two = fixtureChain(count, 2);
      return { count, batch_one: one, batch_two: two, batch_two_minus_batch_one: Object.fromEntries(['uncompressed', 'backrefs'].map(encoding => [encoding,
        Object.fromEntries(Object.keys(one.whole_chain[encoding]).map(metric => [metric, two.whole_chain[encoding][metric] - one.whole_chain[encoding][metric]]))])) }; }),
    spuds: { ...fixtureChain(3, 1), take_ages: [1, 4], claim_age: 5, timer_reset_checked: true, exact_six_condition_VM_outputs_checked: true } })) };
const packets = Object.fromEntries([['continuation-batches', 'batch_two'], ['spuds', 'spuds']].map(([experiment, kind]) => [experiment, { blobs: Object.fromEntries(synthetic.reports.map(row => [row.mode, row.artifact_blob_sha256[kind]])) }]));
const fixtureView = { status: 'retained_three_mode_execution', scopes: statefulScopes, ...statefulLedgerView(synthetic, packets) };
function fixtureMutations(experiment, names) {
  const records = [], cases = names.map((name, index) => {
    const variants = Object.fromEntries(['original', 'mutant'].map(role => {
      const native = role === 'original' && index === 0 ? { status: 'rejected' } : { status: 'accepted', output_hex: '80' };
      const primary_vm = Object.fromEntries(['rust_on', 'rust_off'].map(mode => {
        const umbrella = role === 'original' && index === 0;
        const typed = role === 'original' && index === 1;
        const rejected = { code: umbrella ? 'GeneratorRuntimeError' : 'AssertMyAmountFailed', full_simulator_equal: true,
          policy_rejection_credit: !umbrella, classification: umbrella ? 'inconclusive_runtime_umbrella' : 'typed_consensus_rejection' };
        records.push({ project: experiment, case: name, variant: role, mode, module_hash: digest, artifact_blob_sha256: digest,
          actual: { own_reveal_funding: { coin_id: digest, puzzle_hash: digest, amount: 4000 },
            submission: umbrella ? rejected : { status: typed ? 'consensus_rejected' : 'accepted', observation: typed ? rejected : { outputs: [{ coin_id: digest, puzzle_hash: digest, amount: 1000 }] } } } });
        return [mode, { outcome: native, ...(native.status === 'rejected' ? { failure: { kind: 'program', code: 'raise' } } : {}) }];
      }));
      return [role, { native, primary_vm, source_sha256: digest }];
    }));
    return { name, variants };
  });
  return { protocol: 'chialisp-oxide-stateful-mutations/v1', status: 'actual_source_mutations_captured', experiment,
    compiler_preservation: false, source_authentication: false, execution_authentication: false, cases,
    sdk: { protocol: 'chialisp-oxide-stateful-mutation-sdk/v1', status: 'passed', compiler_preservation: false, standard_wallet_integration: false, records } };
}
const mutationFixtures = {
  batch: fixtureMutations('continuation-batches', ['second_opening', 'amount', 'terminal', 'successor']),
  spuds: fixtureMutations('spuds', ['amount', 'age', 'nextstate', 'announcement', 'signature_binding']),
};
const recipes = Object.fromEntries(['second_opening', 'amount', 'terminal', 'successor', 'age', 'nextstate', 'announcement', 'signature_binding'].map(name => [name, { before: `assert_${name}();`, after: `let _ = ${name};`, occurrences: 1 }]));
fixtureView.sourceControls = Object.fromEntries(Object.entries(mutationFixtures).map(([kind, receipt]) => [kind, statefulMutationView(receipt, recipes)]));
let corruptions = 0;
// Required-path controls use only an owned temporary directory. Empty marker
// files exercise presence admission, never a source/VM/SDK success fixture.
const missingPacketRoot = mkdtempSync(path.join(os.tmpdir(), 'oxide-stateful-required-'));
try {
  assert.throws(() => requireStatefulInputs(missingPacketRoot)); corruptions++;
  for (const relative of statefulInputPaths) {
    const filename = path.join(missingPacketRoot, relative);
    mkdirSync(path.dirname(filename), { recursive: true }); writeFileSync(filename, '{}');
  }
  requireStatefulInputs(missingPacketRoot);
  for (const relative of statefulInputPaths) {
    const filename = path.join(missingPacketRoot, relative); unlinkSync(filename);
    assert.throws(() => requireStatefulInputs(missingPacketRoot)); corruptions++;
    writeFileSync(filename, '{}');
  }
} finally { rmSync(missingPacketRoot, { recursive: true, force: true }); }
for (const mutate of [
  report => { report.reports.pop(); },
  report => { report.reports[1].mode = 'chialisp'; },
  report => { report.reports[0].artifact_blob_sha256.spuds = '00'.repeat(32); },
  report => { report.reports[0].batch_comparisons[0].batch_two.whole_chain.uncompressed.total_cost++; },
  report => { report.reports[0].batch_comparisons[0].batch_two_minus_batch_one.backrefs.charged_bytes++; },
  report => { report.reports[0].batch_comparisons[0].batch_two.controls[1].policy_rejection_credit = true; },
  report => { report.reports[0].spuds.controls[0].full_simulator_equal = false; },
  report => { report.reports[0].spuds.steps[0].measurement.generators[0].metrics.signature_verified = false; },
]) { const report = structuredClone(synthetic); mutate(report); assert.throws(() => statefulLedgerView(report, packets)); corruptions++; }
for (const receipt of Object.values(mutationFixtures)) for (const mutate of [
  r => { r.cases.pop(); },
  r => { r.sdk.records[0].mode = 'chialisp'; },
  r => { r.sdk.records[1] = r.sdk.records[0]; },
  r => { r.cases[0].variants.original.primary_vm.rust_on.failure.code = 'cost_exceeded'; },
  r => { r.cases[0].variants.mutant.primary_vm.rust_on.outcome = { status: 'rejected' }; },
  r => { r.sdk.records[0].actual.submission.policy_rejection_credit = true; },
  r => { r.sdk.records[0].actual.submission.full_simulator_equal = false; },
  r => { r.sdk.records[0].module_hash = 'not a hash'; },
]) { const changed = structuredClone(receipt); mutate(changed); assert.throws(() => statefulMutationView(changed, recipes)); corruptions++; }
for (const experiment of ['batch', 'spuds']) {
  const ast = parser.parse(render(experiment, fixtureView));
  checkStatefulDisplay(ast, experiment, fixtureView);
  for (const mutate of [
    tree => { one(tree, 'dataStatefulLane', 'rust_off').properties.hidden = true; },
    tree => { one(one(tree, 'dataStatefulCounts'), 'dataStatefulField', 'typed').children = [{ type: 'text', value: 'wrong count' }]; },
    tree => { one(tree, 'dataStatefulContext').properties.style = 'display:none'; },
    tree => { find(tree, node => node.properties?.dataStatefulScope !== undefined)[0].node.children = [{ type: 'text', value: 'unbounded claim' }]; },
    tree => { find(tree, node => node.properties?.dataStatefulField?.endsWith('total_cost'))[0].node.children = [{ type: 'text', value: '0' }]; },
    tree => { find(tree, node => node.properties?.dataStatefulRejection !== undefined)[0].node.properties.ariaHidden = 'true'; },
    tree => { find(tree, node => node.tagName === 'dt')[0].node.children = [{ type: 'text', value: 'incorrect credit label' }]; },
    tree => { find(tree, node => node.properties?.dataStatefulControl !== undefined)[0].node.properties.hidden = true; },
    ...['source', 'module', 'blob', 'coin', 'funding', 'native', 'native-output', 'primary', 'primary-output', 'sdk', 'state', 'outputs'].map(id => tree => {
      const control = find(tree, node => node.properties?.dataStatefulControl !== undefined)[0].node;
      one(one(control, 'dataStatefulRole', 'mutant'), 'dataStatefulField', id).children = [{ type: 'text', value: 'altered source witness' }];
    }),
    tree => { one(one(find(tree, node => node.properties?.dataStatefulControl !== undefined)[0].node, 'dataStatefulRecipe'), 'dataStatefulField', 'after').children = [{ type: 'text', value: 'different mutation' }]; },
  ]) { const changed = structuredClone(ast); mutate(changed); assert.throws(() => checkStatefulDisplay(changed, experiment, fixtureView)); corruptions++; }
}
export function checkStatefulDownloadPayloads(catalog, read) {
  assert(catalog.downloads.length <= 4096);
  for (const file of catalog.downloads) {
    const raw = read(file.path);
    assert(raw instanceof Uint8Array && raw.length === file.bytes, `Missing or truncated stateful payload: ${file.path}`);
    assert.equal(createHash('sha256').update(raw).digest('hex'), file.sha256, `Stateful payload SHA-256: ${file.path}`);
    assert.deepEqual(raw, catalog.payloads.get(file.path), `Exact admitted stateful bytes: ${file.path}`);
  }
}

export async function checkBuiltStateful(dist) {
  const evidence = await readStatefulEvidence();
  const downloads = await readStatefulDownloads();
  const directory = path.join(dist, 'reports/stateful-data'), physical = [], stack = [[directory, 0]];
  let entries = 0;
  while (stack.length) {
    const [folder, depth] = stack.pop(); assert(depth < 64);
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      assert(++entries <= 16384 && !entry.isSymbolicLink(), 'Bounded regular physical download inventory');
      const filename = path.join(folder, entry.name);
      if (entry.isDirectory()) stack.push([filename, depth + 1]);
      else { assert(entry.isFile()); physical.push(path.relative(directory, filename).split(path.sep).join('/')); }
    }
  }
  assert.deepEqual(physical.sort(), downloads.downloads.map(file => file.path).sort(), 'Complete exact physical stateful download inventory');
  checkStatefulDownloadPayloads(downloads, relative => {
    const filename = path.join(directory, relative); assert(lstatSync(filename).size <= 32 * 1024 * 1024);
    return readFileSync(filename);
  });
  const summary = readFileSync(path.join(dist, 'reports/stateful.json'));
  assert(summary.length <= 2 * 1024 * 1024);
  assert.deepEqual(JSON.parse(summary.toString('utf8')), evidence, 'Downloaded stateful summary matches the exact visible values');
  for (const [experiment, route] of [['batch', 'continuations'], ['spuds', 'spuds']]) {
    const filename = path.join(dist, 'reference', route, 'index.html');
    const raw = readFileSync(filename); assert(raw.length <= 3 * 1024 * 1024);
    checkStatefulDisplay(parser.parse(raw.toString('utf8')), experiment, evidence);
    const ast = parser.parse(raw.toString('utf8'));
    assert(one(ast, 'dataStatefulDownload').properties.href.endsWith('/reports/stateful.json'));
    const packet = experiment === 'batch' ? 'batch' : 'spuds';
    const links = { 'sdk-report': 'ledger/report.json', 'sdk-receipt': 'ledger/receipt.json', 'source-receipt': `${packet}-success/receipt.json`, 'control-receipt': `${packet}-controls/receipt.json`, inventory: 'inventory.json' };
    assert.equal(find(ast, node => node.properties?.dataStatefulRaw !== undefined).length, Object.keys(links).length);
    for (const [id, file] of Object.entries(links)) assert(one(ast, 'dataStatefulRaw', id).properties.href.endsWith(`/reports/stateful-data/${file}`));
  }
}
const downloadFixture = await readStatefulDownloads();
checkStatefulDownloadPayloads(downloadFixture, name => downloadFixture.payloads.get(name));
for (const mutation of ['missing', 'changed']) {
  const corrupted = new Map(downloadFixture.payloads), selected = downloadFixture.downloads[0].path;
  if (mutation === 'missing') corrupted.delete(selected); else corrupted.set(selected, Buffer.from('changed download bytes'));
  assert.throws(() => checkStatefulDownloadPayloads(downloadFixture, name => corrupted.get(name))); corruptions++;
}
if (process.argv[2] === '--built') await checkBuiltStateful(path.resolve(root, 'www', process.argv[3] ?? 'dist'));
console.log(`PASS constructed stateful report/display controls: ${corruptions} corruptions; no constructed execution credit`);
