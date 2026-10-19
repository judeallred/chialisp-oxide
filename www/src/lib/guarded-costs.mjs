// Recorded presentation consistency; mandatory current-source Python checks live
// in check-evidence. This module never executes a compiler, VM, solver or ledger.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdir, realpath } from 'node:fs/promises';
import { compareCostCaptures, summarizeCosts, guardedCheckpoint, checkpointArtifact, readCostInputs } from './p2-costs.mjs';

export const guardedArtifacts = Object.freeze([
  { basename: 'profile.json', path: 'corpus/guarded-projection-baseline.json', label: 'Reviewed guarded-compiler transition' },
  { basename: 'costs.json', path: 'corpus/guarded-projection-costs.json', label: 'Guarded-compiler fifteen bundle measurements' },
  { basename: 'controls.json', path: 'corpus/guarded-projection-controls.json', label: 'Guarded-compiler native and VM mutation controls' },
]);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const parse = bytes => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
const canonical = value => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${canonical(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value).replace(/[\u007f-\uffff]/g, ch => `\\u${ch.charCodeAt(0).toString(16).padStart(4, '0')}`);
};
const fields = (value, keys) => assert.deepEqual(Object.keys(value).sort(), [...keys].sort(), 'Missing/unknown guarded presentation fields');

export function guardedCostEvidence(inputs) {
  fields(inputs, ['validation', 'checkpoint', 'source-profile', 'before-costs', ...guardedArtifacts.map(row => row.basename)]);
  for (const [key, bytes] of Object.entries(inputs)) assert(bytes instanceof Uint8Array && bytes.length > 0 && bytes.length <= (['validation','checkpoint'].includes(key) ? 16 : 4) * 1024 * 1024);
  const data = Object.fromEntries(Object.entries(inputs).map(([key, bytes]) => [key, parse(bytes)]));
  const profile = data['profile.json'], context = guardedCheckpoint(profile, inputs.checkpoint), after = profile.reviewed_after;
  fields(after, ['report_sha256','canonical_report_sha256','p2_sha256','protocol_sha256','raw_ir_sha256','metrics_sha256','controls_sha256','bundles','delta']);
  for (const key of Object.keys(after).filter(key => !['bundles','delta'].includes(key))) assert(/^[0-9a-f]{64}$/.test(after[key]));
  assert.equal(sha(inputs.validation), after.report_sha256, 'Guarded validation capture bytes changed');
  assert.equal(sha(canonical(data.validation)), after.canonical_report_sha256);
  assert.equal(sha(inputs['costs.json']), after.metrics_sha256);
  assert.equal(sha(inputs['controls.json']), after.controls_sha256);
  assert.equal(sha(inputs['source-profile']), profile.immutable_profiles['corpus/p2-source-cost-baseline.json']);
  fields(after.bundles, ['on','off']);
  for (const [name,mode] of [['on',true],['off',false]]) { fields(after.bundles[name],['manifest_sha256','optimize_lir']); assert(/^[0-9a-f]{64}$/.test(after.bundles[name].manifest_sha256) && after.bundles[name].optimize_lir === mode); }
  fields(after.delta,['leaf_count','inventory_sha256']); assert(Number.isSafeInteger(after.delta.leaf_count) && after.delta.leaf_count > 0 && /^[0-9a-f]{64}$/.test(after.delta.inventory_sha256));
  const source = data['source-profile'];
  assert.equal(source.status, 'reviewed_final_source_cost');
  assert.equal(source.reviewed_after.report_sha256, sha(inputs.checkpoint));
  assert.equal(source.reviewed_after.metrics_sha256, sha(inputs['before-costs']));
  assert.equal(source.reviewed_after.canonical_report_sha256, sha(canonical(data.checkpoint)));
  assert.deepEqual(data.validation.optimization_pipeline, profile.optimization_pipeline.after);
  assert.deepEqual(data.checkpoint.optimization_pipeline, profile.optimization_pipeline.before);
  const expected = structuredClone(profile.optimization_pipeline.before);
  expected.rust_mir.optimized.splice(expected.rust_mir.optimized.indexOf('runtime_frame_elision') + 1, 0, 'runtime_guarded_projection_forwarding');
  assert.deepEqual(profile.optimization_pipeline.after, expected);
  assert.equal(expected.rust_mir.optimized.length, 11);
  for (const report of [data.validation, data.checkpoint]) assert.equal(report.puzzles.length, 22);
  const puzzle = data.validation.puzzles.at(-1), previous = data.checkpoint.puzzles.at(-1);
  assert.equal(puzzle.slug, 'p2-singleton'); assert.equal(previous.slug, puzzle.slug);
  assert.equal(sha(canonical(puzzle)), after.p2_sha256);
  assert.equal(sha(canonical(puzzle.protocol_validation)), after.protocol_sha256);
  assert.equal(puzzle.source, previous.source, 'This compiler comparison must preserve the Rust policy source');
  const comparison = compareCostCaptures(previous, puzzle, data['before-costs'], data['costs.json']);
  const summary = summarizeCosts(comparison.rows);
  assert(summary.bytes < summary.beforeBytes && summary.rawVM < summary.beforeRawVM && summary.delta < 0, 'Guarded measurements changed their recorded directions');
  const downloads = guardedArtifacts.map(row => ({ ...row, sha256: sha(inputs[row.basename]), bytes: inputs[row.basename].length, content: inputs[row.basename] }));
  return { ...comparison, summary, context, currentReportSHA: after.report_sha256, downloads };
}
export async function readGuardedCostInputs(candidate) {
  return readCostInputs(candidate, [
    ['validation', 'corpus/validation.json', 16 * 1024 * 1024],
    ['checkpoint', checkpointArtifact.path, 16 * 1024 * 1024],
    ['source-profile', 'corpus/p2-source-cost-baseline.json', 4 * 1024 * 1024],
    ['before-costs', 'corpus/p2-singleton-costs.json', 4 * 1024 * 1024],
    ...guardedArtifacts.map(row => [row.basename, row.path, 4 * 1024 * 1024]),
  ], 48 * 1024 * 1024);
}
export async function loadGuardedCostEvidence(candidate) { return guardedCostEvidence(await readGuardedCostInputs(candidate)); }

const fmt = value => new Intl.NumberFormat('en-US').format(value);
const plain = markup => markup.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const attr = (markup, name) => markup.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
function pair(markup, before, current) {
  assert(markup); assert.equal(attr(markup, 'data-before'), String(before)); assert.equal(attr(markup, 'data-current'), String(current));
  assert.equal(plain(markup), `${fmt(before)} → ${fmt(current)}`, 'Wrong visible guarded cost');
}
export function checkGuardedDisplay(markup, evidence, { compact = false, base = '/chialisp-oxide/' } = {}) {
  const matches = [...markup.matchAll(/<section\b[^>]*data-guarded-costs[^>]*>[\s\S]*?<\/section>/g)];
  assert.equal(matches.length, 1); const section = matches[0][0], text = plain(section);
  assert.equal(attr(section, 'data-guarded-view'), compact ? 'compact' : 'full');
  assert.equal(plain(section.match(/<h2\b[^>]*>([\s\S]*?)<\/h2>/)?.[1] ?? ''), 'Source-only checkpoint → guarded compiler checkpoint', 'Wrong visible guarded checkpoint heading');
  const context = section.match(/<p\b[^>]*data-guarded-context[^>]*>[\s\S]*?<\/p>/)?.[0];
  assert(context, 'Missing guarded/source-only context');
  const visibleContext = plain(context);
  assert(visibleContext.includes(`signed source-only commit ${evidence.context.commit}`)
    && visibleContext.includes(`source-only validation SHA-256 ${evidence.context.reportSHA}`)
    && visibleContext.includes(`guarded compiler validation SHA-256 ${evidence.currentReportSHA}`), 'Wrong visible labelled guarded/source-only identities');
  const commitAnchors = [...context.matchAll(/<a\b[^>]*>[\s\S]*?<\/a>/g)].map(row => row[0]);
  assert.equal(commitAnchors.length, 1, 'Expected one signed checkpoint anchor');
  assert.equal(attr(commitAnchors[0], 'href'), `https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.commit}`);
  assert.equal(plain(commitAnchors[0]), evidence.context.commit, 'Wrong visible signed checkpoint commit');
  assert.deepEqual([...section.matchAll(/data-guarded-summary="([^"]+)"/g)].map(row => row[1]), ['module','vm','joint'], 'Missing/duplicate guarded summary metrics');
  for (const [key, before, current] of [['module', evidence.summary.beforeBytes, evidence.summary.bytes], ['vm', evidence.summary.beforeRawVM, evidence.summary.rawVM], ['joint', evidence.summary.beforeTotal, evidence.summary.total]]) { pair(section.match(new RegExp(`<strong\\b[^>]*data-guarded-summary="${key}"[^>]*>[\\s\\S]*?</strong>`))?.[0], before, current);
    const label = { module: 'optimized Rust module bytes', vm: 'ordinary input · primary VM cost', joint: 'joint bundle · total conditions cost' }[key];
    const block = section.match(new RegExp(`<div\\b[^>]*class="[^"]*\\bmetric\\b[^"]*"[^>]*>\\s*<strong\\b[^>]*data-guarded-summary="${key}"[^>]*>[\\s\\S]*?</strong>\\s*<span[^>]*>([\\s\\S]*?)</span>`));
    assert.equal(plain(block?.[1] ?? ''), label, 'Wrong visible guarded summary unit/context');
  }
  assert(text.includes(`${fmt(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%)`));
  assert(text.includes(`${fmt(-evidence.summary.executionDelta)} fewer execution units`));
  assert(text.includes(`${fmt(-evidence.summary.byteDelta)} fewer charged bytes`));
  for (const scope of ['clvmr 0.21.0', 'clvmr 0.16.2', 'chia-consensus 0.36.1', '12,000', 'two-byte quote wrapper', 'not a universal cost bound', 'not additional admissions']) assert(text.includes(scope), `Missing guarded context: ${scope}`);
  assert(section.includes(`href="${base}tutorials/transaction-costs/"`));
  if (compact) return;
  const matrix = section.match(/<table\b[^>]*data-guarded-matrix[^>]*>[\s\S]*?<\/table>/)?.[0]; assert(matrix);
  assert.equal(plain(matrix.match(/<caption[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? ''), 'Source-only checkpoint → guarded compiler checkpoint: total conditions costs');
  assert.deepEqual([...matrix.matchAll(/<th\b[^>]*scope="col"[^>]*>([\s\S]*?)<\/th>/g)].map(row => plain(row[1])), ['Build', 'Genesis', 'Joint collection', 'Lineage']);
  const rows = [...matrix.matchAll(/<tr\b[^>]*data-guarded-build="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)]; assert.equal(rows.length, 5);
  for (const [index, row] of rows.entries()) {
    const expected = evidence.rows[index]; assert.equal(row[1], `${expected.backend}:${expected.optimize}`);
    assert.equal(plain(row[2].match(/<th\b[^>]*scope="row"[^>]*>([\s\S]*?)<\/th>/)?.[1] ?? ''), expected.label);
    const cells = [...row[2].matchAll(/<td\b[^>]*data-guarded-phase="([^"]+)"[^>]*>[\s\S]*?<\/td>/g)]; assert.deepEqual(cells.map(cell => cell[1]), ['genesis','joint','lineage']);
    for (const [i, cell] of cells.entries()) pair(cell[0], expected.phases[i].before.total, expected.phases[i].current.total);
  }
  const downloads = [...section.matchAll(/<a\b[^>]*data-guarded-download="([^"]+)"[^>]*>[\s\S]*?<\/a>/g)];
  assert.deepEqual(downloads.map(row => row[1]), evidence.downloads.map(row => row.basename));
  for (const artifact of evidence.downloads) {
    assert(section.includes(`href="${base}reports/guarded-projection/${artifact.basename}"`));
    assert(section.includes(artifact.sha256));
  }
}
export async function checkGuardedDownloads(directory, evidence) {
  directory = await realpath(directory);
  assert.deepEqual((await readdir(directory)).sort(), evidence.downloads.map(row => row.basename).sort());
  const inputs = await readCostInputs(directory, evidence.downloads.map(row => [row.basename, row.basename, 4 * 1024 * 1024]), 12 * 1024 * 1024);
  for (const artifact of evidence.downloads) assert.deepEqual(inputs[artifact.basename], Buffer.from(artifact.content));
}
