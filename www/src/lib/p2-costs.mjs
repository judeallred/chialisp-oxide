// Recorded presentation checks, separate from execution and the full Python
// source/native/protocol gate required by check-evidence before every build.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { open, readFile, readdir, realpath } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { compositionCheckpoint } from './runtime-checkpoints.mjs';

export const costArtifacts = Object.freeze([
  { basename: 'profile.json', path: 'corpus/p2-source-cost-baseline.json', label: 'Reviewed source-cost transition' },
  { basename: 'before-p2.json', path: 'corpus/p2-source-cost-before-p2.json', label: 'Complete previous P2 record' },
  { basename: 'before-costs.json', path: 'corpus/p2-source-cost-before-metrics.json', label: 'Previous fifteen bundle measurements' },
  { basename: 'before-controls.json', path: 'corpus/p2-source-cost-before-controls.json', label: 'Previous source-mutation recipes' },
  { basename: 'costs.json', path: 'corpus/p2-singleton-costs.json', label: 'Source-only checkpoint bundle measurements' },
  { basename: 'controls.json', path: 'corpus/p2-source-cost-controls.json', label: 'Source-only checkpoint native and VM controls' },
]);
export const costPhases = Object.freeze(['genesis', 'joint', 'lineage']);
const modes = [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]];
const sourcePath = 'puzzles/p2-singleton/puzzle.rs';
const beforeSource = '1ef0b592106b0d497d21b2f28c8c675e5a714a647142a4bc9c377be5f586d7f9';
const afterSource = '72140f55999d8bdfe431372b4f380973e91be19ef6364a546255ddcba9b6f281';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const isDigest = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const fields = (value, names, label) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), `${label}: expected object`);
  assert.deepEqual(Object.keys(value).sort(), [...names].sort(), `${label}: missing/unknown fields`);
};
const integer = (value, label) => assert(Number.isSafeInteger(value) && value >= 0, `${label}: expected nonnegative safe integer`);
const hexLength = (value, label) => {
  assert(typeof value === 'string' && /^(?:[0-9a-f]{2})+$/.test(value), `${label}: expected exact hex bytes`);
  return value.length / 2;
};
// Match Python's canonical JSON encoding for the independently pinned object
// digests. This is presentation consistency, not a second migration engine.
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${canonical(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value).replace(/[\u007f-\uffff]/g, ch => `\\u${ch.charCodeAt(0).toString(16).padStart(4, '0')}`);
}
const parse = bytes => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
const label = (backend, optimize) => `${backend === 'rust_mir' ? 'Rust' : backend === 'rue_source' ? 'Rue' : 'Chialisp'} / ${optimize ? 'on' : 'off'}`;

function measured(row, group, phase, height) {
  const value = row[phase];
  assert(value && typeof value === 'object', 'Missing accepted-bundle cost phase');
  const numbers = ['previous_transaction_height', 'max_cost', 'total_conditions_cost', 'execution_cost', 'condition_cost',
    'cost_per_byte', 'charged_generator_bytes_pinned_source_rule', 'excluded_quote_wrapper_bytes', 'generator_bytes_calculated',
    'generator_uncompressed_bytes', 'generator_backrefs_bytes', 'observed_byte_cost_component', 'total_reveal_bytes', 'total_solution_bytes'];
  numbers.forEach(key => integer(value[key], key));
  assert.equal(value.previous_transaction_height, height, 'Wrong measured phase height');
  assert.equal(value.max_cost, 5_500_000_000);
  assert.equal(value.cost_per_byte, 12_000, 'This snapshot uses the pinned TESTNET11 byte charge');
  assert.equal(value.excluded_quote_wrapper_bytes, 2, 'Pinned helper excludes exactly the quote wrapper');
  assert.equal(value.generator_uncompressed_bytes, hexLength(value.generator_uncompressed_hex, 'Uncompressed generator'));
  assert.equal(value.generator_bytes_calculated, value.generator_uncompressed_bytes);
  assert.equal(value.generator_backrefs_bytes, hexLength(value.generator_backrefs_hex, 'Measured backreference serialization'));
  assert.equal(value.charged_generator_bytes_pinned_source_rule, value.generator_uncompressed_bytes - 2);
  assert.equal(value.observed_byte_cost_component, value.charged_generator_bytes_pinned_source_rule * 12_000);
  assert.equal(value.total_conditions_cost, value.execution_cost + value.condition_cost + value.observed_byte_cost_component);
  assert(value.total_conditions_cost <= value.max_cost, 'Measured bundle exceeds its cost budget');
  assert.equal(value.quote_rule_source, 'chia-consensus0.36.1/src/spendbundle_conditions.rs: const QUOTE_BYTES: usize = 2; calculate_generator_length - QUOTE_BYTES');
  assert.equal(value.signature_scope, 'This conditions API skips signature verification. These exact successful bundles were separately submitted to the SDK Simulator with signature verification; this metric call is not an additional admission.');
  assert.equal(value.backrefs_scope, 'Serialized and measured only; no compressed-generator cost or admission is claimed. The conditions API above charges its own uncompressed length rule.');
  const spends = group.lifecycle[phase].spends;
  assert(Array.isArray(value.per_spend) && value.per_spend.length === spends.length);
  assert.deepEqual(value.per_spend.map(spend => spend.coin_id).sort(), spends.map(spend => spend.coin.id).sort(), 'Measured input coins differ from the separately admitted bundle');
  assert.equal(new Set(value.per_spend.map(spend => spend.coin_id)).size, spends.length);
  let execution = 0, conditions = 0;
  for (const spend of value.per_spend) {
    fields(spend, ['coin_id', 'execution_cost', 'condition_cost'], 'Per-spend metric');
    integer(spend.execution_cost, 'Spend execution cost'); integer(spend.condition_cost, 'Spend condition cost');
    execution += spend.execution_cost; conditions += spend.condition_cost;
  }
  assert.equal(execution, value.execution_cost); assert.equal(conditions, value.condition_cost);
  assert.equal(value.total_reveal_bytes, spends.reduce((sum, spend) => sum + hexLength(spend.reveal_hex, 'Admitted reveal'), 0));
  assert.equal(value.total_solution_bytes, spends.reduce((sum, spend) => sum + hexLength(spend.solution_hex, 'Admitted solution'), 0));
  return { total: value.total_conditions_cost, execution: value.execution_cost, conditions: value.condition_cost,
    byteCost: value.observed_byte_cost_component, chargedBytes: value.charged_generator_bytes_pinned_source_rule,
    generatorBytes: value.generator_uncompressed_bytes, spends: spends.length };
}

/** Shared checked numeric presentation only; not a migration or execution gate. */
export function compareCostCaptures(previous, puzzle, before, now, { allowRustOff = false } = {}) {
  assert.equal(typeof allowRustOff, 'boolean');
  const receipts = checkProtocolComponent(puzzle); checkProtocolComponent(previous);
  const header = { VM: 'clvmr 0.16.2', consensus: 'chia-consensus 0.36.1', network: 'TESTNET11',
    scope: 'Pinned consensus conditions/generator metrics for separately admitted exact canonical composition bundles. Neither primary raw clvmr0.21 costs nor universal block/mainnet performance.' };
  for (const metrics of [before, now]) {
    fields(metrics, [...Object.keys(header), 'observations'], 'Cost capture');
    assert.deepEqual(Object.fromEntries(Object.keys(header).map(key => [key, metrics[key]])), header);
    assert.deepEqual(metrics.observations.map(row => [row.backend, row.optimize_lir]), modes);
  }
  const ordinaryBefore = previous.cases.find(row => row.label === 'ordinary five32-byte fields');
  const ordinary = puzzle.cases.find(row => row.label === ordinaryBefore?.label);
  assert(ordinary && ordinaryBefore, 'Missing exact raw VM witness');
  const rows = modes.map(([backend, optimize], index) => {
    const artifact = puzzle.artifacts[index], oldArtifact = previous.artifacts[index];
    const phaseRows = costPhases.map((phase, height) => {
      const old = measured(before.observations[index], previous.protocol_validation.observations[index], phase, height);
      const current = measured(now.observations[index], puzzle.protocol_validation.observations[index], phase, height);
      assert.equal(current.conditions, old.conditions, 'Source refactor changed the condition charge');
      if (backend !== 'rust_mir' || (!allowRustOff && !optimize) || phase !== 'joint') assert.deepEqual(current, old, 'Changed unreviewed language/phase costs');
      return { phase, before: old, current, delta: current.total - old.total };
    });
    const run = ordinary.runs[index], oldRun = ordinaryBefore.runs[index];
    assert.deepEqual([run.backend, run.optimize_lir], [backend, optimize]);
    assert.deepEqual(run.execution.outcome, oldRun.execution.outcome);
    assert.equal(run.execution.outcome.status, 'accepted');
    integer(run.execution.cost, 'Raw VM cost'); integer(oldRun.execution.cost, 'Previous raw VM cost');
    assert.deepEqual([artifact.backend, artifact.optimize_lir], [backend, optimize]);
    assert.equal(hexLength(artifact.clvm_hex, 'Current module'), artifact.bytes);
    assert.equal(hexLength(oldArtifact.clvm_hex, 'Previous module'), oldArtifact.bytes);
    return { backend, optimize, label: label(backend, optimize), beforeBytes: oldArtifact.bytes, bytes: artifact.bytes,
      beforeRawVM: oldRun.execution.cost, rawVM: run.execution.cost, phases: phaseRows };
  });
  return { rows, receipts, header, witness: ordinary.label };
}

export function summarizeCosts(rows) {
  const rust = rows[1], joint = rust.phases[1];
  return { beforeBytes: rust.beforeBytes, bytes: rust.bytes, beforeRawVM: rust.beforeRawVM, rawVM: rust.rawVM,
    beforeTotal: joint.before.total, total: joint.current.total, delta: joint.delta,
    percent: (-joint.delta / joint.before.total * 100).toFixed(2), byteDelta: joint.current.chargedBytes - joint.before.chargedBytes,
    executionDelta: joint.current.execution - joint.before.execution, conditionDelta: joint.current.conditions - joint.before.conditions };
}

export const checkpointArtifact = Object.freeze({ basename: 'validation.json', path: 'corpus/checkpoints/p2-source-cost-validation.json', label: 'Complete source-only checkpoint validation' });
/** Root-reviewed metadata context; the mandatory Python gate checks the transition. */
export function guardedCheckpoint(profile, bytes) {
  fields(profile, ['schema_version','protocol','status','scope','baseline','immutable_profiles','optimization_pipeline','reviewed_after'], 'Guarded compiler profile');
  assert.equal(profile.schema_version, 1);
  assert.equal(profile.protocol, 'chialisp-oxide-runtime-guarded-projection/v1');
  assert.equal(profile.status, 'reviewed_final_guarded_projection', 'Guarded checkpoint requires final root capture/review; pending profiles cannot publish');
  assert(profile.reviewed_after && isDigest(profile.reviewed_after.report_sha256), 'Missing reviewed guarded capture');
  const baseline = profile.baseline;
  fields(baseline, ['repository','commit','tree_sha1','report_git_blob_sha1','report_path','checkpoint_path','report_sha256','canonical_report_sha256'], 'Signed source-only checkpoint');
  assert.equal(baseline.repository, 'https://github.com/judeallred/chialisp-oxide');
  for (const key of ['commit','tree_sha1','report_git_blob_sha1']) assert(typeof baseline[key] === 'string' && /^[0-9a-f]{40}$/.test(baseline[key]), 'Missing actual signed source-only checkpoint identity');
  assert.equal(baseline.report_path, 'corpus/validation.json'); assert.equal(baseline.checkpoint_path, checkpointArtifact.path);
  assert.equal(baseline.report_sha256, '1a19cca4b2f1d840bf49c3119305f8872ff03e4ea91c5f74e93ad57ef4ad8b03');
  assert.equal(baseline.canonical_report_sha256, '1f8819d8c2ddab219b1277e2550381378c3e7b2a9e5933830b1bbdfe96370943');
  assert.equal(digest(bytes), baseline.report_sha256, 'Source-only checkpoint bytes changed');
  assert.equal(digest(canonical(parse(bytes))), baseline.canonical_report_sha256);
  return { label: 'Source-only checkpoint', commit: baseline.commit, tree: baseline.tree_sha1, reportSHA: baseline.report_sha256 };
}

/** Exact historical checkpoint in; checked presentation rows out. No subprocess. */
export function p2CostEvidence(inputs) {
  fields(inputs, ['validation', 'composition-profile', ...costArtifacts.map(row => row.basename)], 'Cost input set');
  const parsed = Object.fromEntries(Object.entries(inputs).map(([key, bytes]) => {
    assert(bytes instanceof Uint8Array && bytes.byteLength > 0 && bytes.byteLength <= (key === 'validation' ? 16 : 4) * 1024 * 1024, 'Cost input exceeds its byte bound');
    return [key, parse(bytes)];
  }));
  const profile = parsed['profile.json'];
  assert.equal(profile.protocol, 'chialisp-oxide-p2-source-cost/v1');
  assert.equal(profile.schema_version, 1);
  assert.equal(profile.status, 'reviewed_final_source_cost', 'Source-cost lesson requires final root capture/review; pending profiles cannot publish');
  assert.equal(profile.baseline.commit, 'e20f6d126aedbf76a8c179701af654ca96550074');
  assert.deepEqual(profile.source_transition, { path: sourcePath, before_sha256: beforeSource, after_sha256: afterSource, changed_main_authoring_inputs: [sourcePath] });
  const after = profile.reviewed_after;
  fields(after, ['report_sha256', 'canonical_report_sha256', 'p2_sha256', 'protocol_sha256', 'raw_ir_sha256', 'metrics_sha256', 'controls_sha256', 'bundles'], 'Final cost capture');
  for (const key of Object.keys(after).filter(key => key !== 'bundles')) assert(isDigest(after[key]), 'Missing final cost capture hash');
  fields(after.bundles, ['on', 'off'], 'Captured cost bundle modes');
  for (const [key, mode] of [['on', true], ['off', false]]) {
    fields(after.bundles[key], ['manifest_sha256', 'optimize_lir'], 'Captured cost mode');
    assert(isDigest(after.bundles[key].manifest_sha256) && after.bundles[key].optimize_lir === mode);
  }
  for (const [key, basename] of [['p2', 'before-p2.json'], ['metrics', 'before-costs.json'], ['controls', 'before-controls.json']]) {
    assert.equal(profile.retained[key].path, costArtifacts.find(row => row.basename === basename).path);
    assert.equal(digest(inputs[basename]), profile.retained[key].sha256, 'Previous cost input bytes changed');
  }
  assert.equal(digest(inputs.validation), after.report_sha256, 'Current validation bytes changed');
  assert.equal(digest(canonical(parsed.validation)), after.canonical_report_sha256, 'Current canonical report identity changed');
  assert.equal(digest(inputs['costs.json']), after.metrics_sha256, 'Current cost capture bytes changed');
  assert.equal(digest(inputs['controls.json']), after.controls_sha256, 'Current source-control bytes changed');
  const report = parsed.validation;
  assert(Array.isArray(report.puzzles) && report.puzzles.length === 22);
  const current = report.puzzles.filter(row => row.slug === 'p2-singleton');
  assert.equal(current.length, 1); const puzzle = current[0], previous = parsed['before-p2.json'];
  assert.equal(digest(canonical(puzzle)), after.p2_sha256);
  assert.equal(digest(canonical(puzzle.protocol_validation)), after.protocol_sha256);
  assert.equal(digest(puzzle.source), afterSource); assert.equal(digest(previous.source), beforeSource);
  const { rows, receipts, header, witness } = compareCostCaptures(previous, puzzle, parsed['before-costs.json'], parsed['costs.json'], { allowRustOff: true });
  const rust = rows[1], joint = rust.phases[1];
  assert(rust.bytes < rust.beforeBytes && rust.rawVM > rust.beforeRawVM && joint.delta < 0,
    'The reviewed source-only tradeoff must retain its measured directions');
  const summary = summarizeCosts(rows);
  const context = compositionCheckpoint(parsed['composition-profile'], inputs.validation);
  const downloads = costArtifacts.map(row => ({ ...row, sha256: digest(inputs[row.basename]), bytes: inputs[row.basename].byteLength, content: inputs[row.basename] }));
  downloads.push({ ...checkpointArtifact, sha256: digest(inputs.validation), bytes: inputs.validation.byteLength, content: inputs.validation });
  return { rows, summary, downloads, baseline: profile.baseline, context, receipts, header, witness };
}

export async function repository(candidate) {
  if (candidate) return realpath(candidate);
  for (const root of [fileURLToPath(new URL('../../../', import.meta.url)), process.cwd(), path.dirname(process.cwd())]) {
    const manifest = await readFile(path.join(root, 'www/package.json'), 'utf8').then(JSON.parse).catch(() => null);
    if (manifest?.name === 'chialisp-oxide-docs') return realpath(root);
  }
  throw new Error('Cost downloads must be built from this checkout or www');
}
async function boundedRead(root, relative, limit) {
  const filename = path.join(root, relative);
  assert.equal(await realpath(filename), filename, 'Cost inputs cannot follow symlinks');
  assert.equal(await realpath(path.dirname(filename)), path.dirname(filename));
  const file = await open(filename, 'r');
  try {
    const stat = await file.stat(); assert(stat.isFile() && stat.size > 0 && stat.size <= limit);
    const buffer = Buffer.alloc(limit + 1); let length = 0;
    while (length < buffer.length) {
      const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
      if (!bytesRead) break; length += bytesRead;
    }
    assert(length === stat.size && length <= limit, 'Cost input changed during bounded read');
    return Buffer.from(buffer.subarray(0, length));
  } finally { await file.close(); }
}
export async function readCostInputs(candidate, entries, maxTotal = 32 * 1024 * 1024) {
  const root = await repository(candidate), inputs = {}; let remaining = maxTotal;
  for (const [key, relative, limit] of entries) {
    const bytes = await boundedRead(root, relative, Math.min(limit, remaining));
    remaining -= bytes.byteLength; inputs[key] = bytes;
  }
  return inputs;
}
export async function readP2CostInputs(candidate) {
  return readCostInputs(candidate, [
    ['validation', checkpointArtifact.path, 16 * 1024 * 1024],
    ['composition-profile', 'corpus/runtime-composition-baseline.json', 4 * 1024 * 1024],
    ...costArtifacts.map(row => [row.basename, row.path, 4 * 1024 * 1024]),
  ]);
}
export async function loadP2CostEvidence(candidate) { return p2CostEvidence(await readP2CostInputs(candidate)); }

const format = value => new Intl.NumberFormat('en-US').format(value);
const plain = markup => markup.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const attribute = (markup, name) => markup.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
function displayedPair(markup, before, current, label) {
  assert(markup, `Missing ${label}`);
  assert.equal(attribute(markup, 'data-before'), String(before), `Wrong ${label} previous attribute`);
  assert.equal(attribute(markup, 'data-current'), String(current), `Wrong ${label} current attribute`);
  assert.equal(plain(markup), `${format(before)} → ${format(current)}`, `Wrong visible ${label}`);
}
/** Check actual built markup, or explicitly labeled synthetic metadata controls. */
export function checkCostDisplay(markup, evidence, { compact = false, base = '/chialisp-oxide/' } = {}) {
  assert.equal([...markup.matchAll(/<section\b[^>]*data-transaction-costs[^>]*>/g)].length, 1, 'Expected exactly one complete-bundle section');
  const section = markup.match(/<section\b[^>]*data-transaction-costs[^>]*>[\s\S]*?<\/section>/)?.[0];
  assert(section, 'Missing measured complete-bundle section');
  assert.equal(attribute(section, 'data-cost-view'), compact ? 'compact' : 'full');
  assert.deepEqual([...section.matchAll(/data-cost-summary="([^"]+)"/g)].map(row => row[1]), ['module-bytes','raw-vm','joint-total'], 'Missing/duplicate historical summary metrics');
  for (const [key, before, now] of [['module-bytes', evidence.summary.beforeBytes, evidence.summary.bytes],
    ['raw-vm', evidence.summary.beforeRawVM, evidence.summary.rawVM], ['joint-total', evidence.summary.beforeTotal, evidence.summary.total]]) {
    displayedPair(section.match(new RegExp(`<strong\\b[^>]*data-cost-summary="${key}"[^>]*>[\\s\\S]*?</strong>`))?.[0], before, now, key);
    const label = { 'module-bytes': 'optimized Rust module bytes', 'raw-vm': 'ordinary input · primary VM cost increases', 'joint-total': 'joint bundle · total conditions cost decreases' }[key];
    const block = section.match(new RegExp(`<div\\b[^>]*class="[^"]*\\bmetric\\b[^"]*"[^>]*>\\s*<strong\\b[^>]*data-cost-summary="${key}"[^>]*>[\\s\\S]*?</strong>\\s*<span[^>]*>([\\s\\S]*?)</span>`));
    assert.equal(plain(block?.[1] ?? ''), label, 'Wrong visible historical summary unit/context');
  }
  assert.equal(attribute(section, 'data-cost-delta'), String(evidence.summary.delta));
  const text = plain(section);
  assert(!/\bcurrent\b/i.test(text), 'Historical cost section must not use ambiguous Current labels');
  const context = section.match(/<p\b[^>]*data-cost-checkpoint[^>]*>[\s\S]*?<\/p>/)?.[0];
  assert(context, 'Missing source-only checkpoint context');
  const visibleContext = plain(context);
  assert(visibleContext.includes('Unsigned reviewed source-only checkpoint') && visibleContext.includes(`signed historical E20 anchor ${evidence.context.anchor.commit}`)
    && visibleContext.includes(`validation SHA-256 ${evidence.context.reportSHA}`), 'Wrong visible source-only checkpoint/commit/report identity');
  const commitAnchors = [...context.matchAll(/<a\b[^>]*>[\s\S]*?<\/a>/g)].map(row => row[0]);
  assert.equal(commitAnchors.length, 1, 'Expected one signed checkpoint anchor');
  assert.equal(attribute(commitAnchors[0], 'href'), `https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.anchor.commit}`);
  assert.equal(plain(commitAnchors[0]), evidence.context.anchor.commit, 'Wrong visible signed historical anchor commit');
  assert(text.includes(`${format(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%)`));
  assert(text.includes(`execution component increases by ${format(evidence.summary.executionDelta)}`));
  assert(text.includes(`charged generator shrinks by ${format(-evidence.summary.byteDelta)} bytes`));
  for (const phrase of ['primary VM cost increases', 'total conditions cost decreases', 'not a universal performance bound',
    'clvmr 0.21.0', 'clvmr 0.16.2', 'chia-consensus 0.36.1', 'TESTNET11', 'do not add the raw VM table']) assert(text.includes(phrase), `Missing cost scope: ${phrase}`);
  if (compact) {
    assert(section.includes(`href="${base}tutorials/transaction-costs/"`));
    return;
  }
  const matrix = section.match(/<table\b[^>]*data-cost-matrix[^>]*>[\s\S]*?<\/table>/)?.[0];
  const joint = section.match(/<table\b[^>]*data-cost-joint[^>]*>[\s\S]*?<\/table>/)?.[0];
  const raw = section.match(/<table\b[^>]*data-cost-raw[^>]*>[\s\S]*?<\/table>/)?.[0];
  assert(matrix && joint && raw, 'Missing measured cost tables');
  const layouts = [
    [matrix, 'data-cost-matrix', 'data-cost-build', ['Build', 'Genesis', 'Joint collection', 'Lineage'],
      'Before → source-only checkpoint total conditions cost: all five builds and three accepted phases'],
    [joint, 'data-cost-joint', 'data-cost-breakdown', ['Build', 'Execution', 'Conditions', 'Charged bytes', 'Byte charge', 'Total'],
      'Source-only checkpoint joint collection: the three cost components and their sum'],
    [raw, 'data-cost-raw', 'data-cost-raw-build', ['Build', 'Serialized module bytes', 'Raw VM execution cost'],
      `Separate module and primary-VM comparison: before → source-only checkpoint for “${evidence.witness}”`],
  ];
  for (const [table, marker, rowMarker, columns, caption] of layouts) {
    assert.equal([...section.matchAll(new RegExp(`<table\\b[^>]*${marker}(?:\\s|=|>)[\\s\\S]*?</table>`, 'g'))].length, 1, 'Expected one measured table');
    assert.deepEqual([...table.matchAll(/<th\b[^>]*scope="col"[^>]*>([\s\S]*?)<\/th>/g)].map(match => plain(match[1])), columns, 'Wrong visible cost column labels');
    assert.equal(plain(table.match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? ''), caption, 'Wrong visible cost table caption');
    const rows = [...table.matchAll(new RegExp(`<tr\\b[^>]*${rowMarker}="([^"]+)"[^>]*>([\\s\\S]*?)</tr>`, 'g'))];
    assert.equal(rows.length, evidence.rows.length);
    for (const [index, row] of rows.entries()) {
      const labels = [...row[2].matchAll(/<th\b[^>]*scope="row"[^>]*>([\s\S]*?)<\/th>/g)];
      assert.equal(labels.length, 1, 'Expected one visible build label per row');
      assert.equal(plain(labels[0][1]), evidence.rows[index].label, 'Wrong visible cost build label');
    }
  }
  const expectedModes = evidence.rows.map(row => `${row.backend}:${row.optimize}`);
  for (const [table, attr] of [[matrix, 'data-cost-build'], [joint, 'data-cost-breakdown'], [raw, 'data-cost-raw-build']]) {
    assert.deepEqual([...table.matchAll(new RegExp(`${attr}="([^"]+)"`, 'g'))].map(match => match[1]), expectedModes, 'Missing/reordered measured builds');
  }
  for (const row of evidence.rows) {
    const mode = `${row.backend}:${row.optimize}`;
    const matrixRow = matrix.match(new RegExp(`<tr\\b[^>]*data-cost-build="${mode}"[^>]*>[\\s\\S]*?</tr>`))?.[0];
    assert.deepEqual([...matrixRow.matchAll(/data-cost-phase="([^"]+)"/g)].map(match => match[1]), costPhases);
    for (const phase of row.phases) displayedPair(matrixRow.match(new RegExp(`<td\\b[^>]*data-cost-phase="${phase.phase}"[^>]*>[\\s\\S]*?</td>`))?.[0], phase.before.total, phase.current.total, `${mode}/${phase.phase}`);
    const breakdown = joint.match(new RegExp(`<tr\\b[^>]*data-cost-breakdown="${mode}"[^>]*>[\\s\\S]*?</tr>`))?.[0];
    const keys = ['execution', 'conditions', 'chargedBytes', 'byteCost', 'total'];
    assert.deepEqual([...breakdown.matchAll(/data-cost-metric="([^"]+)"/g)].map(match => match[1]), keys);
    for (const key of keys) {
      const cell = breakdown.match(new RegExp(`<td\\b[^>]*data-cost-metric="${key}"[^>]*>[\\s\\S]*?</td>`))?.[0];
      assert.equal(attribute(cell, 'data-value'), String(row.phases[1].current[key]));
      assert.equal(plain(cell), format(row.phases[1].current[key]), `Wrong visible current ${mode}/${key}`);
    }
    const rawRow = raw.match(new RegExp(`<tr\\b[^>]*data-cost-raw-build="${mode}"[^>]*>[\\s\\S]*?</tr>`))?.[0];
    displayedPair(rawRow.match(/<td\b[^>]*data-cost-module[^>]*>[\s\S]*?<\/td>/)?.[0], row.beforeBytes, row.bytes, `${mode} module bytes`);
    displayedPair(rawRow.match(/<td\b[^>]*data-cost-execution[^>]*>[\s\S]*?<\/td>/)?.[0], row.beforeRawVM, row.rawVM, `${mode} raw VM`);
  }
  for (const phrase of ['12,000 cost units per charged generator byte', 'two-byte quote wrapper', 'no compressed-generator cost is claimed',
    'conditions API skips signature verification', 'not additional admissions', 'do not authenticate execution or prove compiler preservation', 'host counterparts, not new Rust ports']) assert(text.includes(phrase), `Missing complete-bundle boundary: ${phrase}`);
  assert(text.includes(`${evidence.receipts.attempts} submissions`) && text.includes(`${evidence.receipts.accepted} accepted`) && text.includes(`${evidence.receipts.rejected} typed rejections`));
  assert.deepEqual([...section.matchAll(/data-cost-download="([^"]+)"/g)].map(match => match[1]), evidence.downloads.map(row => row.basename));
  for (const artifact of evidence.downloads) {
    assert(section.includes(`href="${base}reports/p2-source-cost/${artifact.basename}"`), 'Missing exact cost download link');
    const items = [...section.matchAll(/<li\b[^>]*>[\s\S]*?<\/li>/g)].map(match => match[0]).filter(item => item.includes(`data-cost-download="${artifact.basename}"`));
    assert.equal(items.length, 1, 'Duplicate cost download row'); const item = items[0];
    assert(item && item.includes(artifact.sha256) && plain(item).includes(`${format(artifact.bytes)} bytes`), 'Wrong cost download bytes/hash label');
  }
}

export async function checkCostDownloads(directory, evidence) {
  directory = await realpath(directory);
  assert.deepEqual((await readdir(directory)).sort(), evidence.downloads.map(row => row.basename).sort(), 'Unexpected or missing cost downloads');
  for (const artifact of evidence.downloads) {
    const bytes = await boundedRead(directory, artifact.basename, (artifact.basename === 'validation.json' ? 16 : 4) * 1024 * 1024);
    assert.deepEqual(bytes, Buffer.from(artifact.content), 'Published cost download must preserve exact raw bytes');
  }
}

// Keep the existing Node-only protocol reader at its real source location.
// Astro relocates bundled modules into dist/.prerender; its relative fs URLs
// must not be interpreted against that generated directory.
const { checkProtocolComponent, historicalOptimizationReport } = await import(/* @vite-ignore */ pathToFileURL(
  path.join(await repository(), 'www/scripts/p2-protocol.mjs'),
).href);
export { historicalOptimizationReport };
