// Recorded presentation consistency; mandatory current-source Python checks live
// in check-evidence. This module never executes a compiler, VM, solver or ledger.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdir, realpath } from 'node:fs/promises';
import { compareCostCaptures, summarizeCosts, checkpointArtifact, readCostInputs } from './p2-costs.mjs';
import { compositionCheckpoint } from './runtime-checkpoints.mjs';

export const compositionArtifacts = Object.freeze([
  { basename: 'profile.json', path: 'corpus/runtime-composition-baseline.json', label: 'Reviewed composition-compiler transition' },
  { basename: 'costs.json', path: 'corpus/runtime-composition-costs.json', label: 'Composition-compiler fifteen bundle measurements' },
  { basename: 'controls.json', path: 'corpus/runtime-composition-controls.json', label: 'Composition-compiler native and VM mutation controls' },
]);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const parse = bytes => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
const canonical = value => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${canonical(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value).replace(/[\u007f-\uffff]/g, ch => `\\u${ch.charCodeAt(0).toString(16).padStart(4, '0')}`);
};
const fields = (value, keys) => assert.deepEqual(Object.keys(value).sort(), [...keys].sort(), 'Missing/unknown composition presentation fields');

export function compositionCostEvidence(inputs) {
  fields(inputs, ['validation', 'active-validation', 'append-profile', 'checkpoint', 'source-profile', 'before-costs', ...compositionArtifacts.map(row => row.basename)]);
  for (const [key, bytes] of Object.entries(inputs)) assert(bytes instanceof Uint8Array && bytes.length > 0 && bytes.length <= (['validation','active-validation','checkpoint'].includes(key) ? 16 : 4) * 1024 * 1024);
  const data = Object.fromEntries(Object.entries(inputs).map(([key, bytes]) => [key, parse(bytes)]));
  const profile = data['profile.json'], context = compositionCheckpoint(profile, inputs.checkpoint), after = profile.reviewed_after;
  fields(after, ['report_sha256','canonical_report_sha256','p2_sha256','protocol_sha256','raw_ir_sha256','metrics_sha256','controls_sha256','bundles','delta']);
  for (const key of Object.keys(after).filter(key => !['bundles','delta'].includes(key))) assert(/^[0-9a-f]{64}$/.test(after[key]));
  assert.equal(sha(inputs.validation), after.report_sha256, 'Composition validation capture bytes changed');
  assert.equal(sha(canonical(data.validation)), after.canonical_report_sha256);
  // The composed compiler capture is historical227. The active23 report has
  // different source/native metadata and its own outer admission authority.
  // Mandatory Python current checks run before this presentation-only reader.
  const append = data['append-profile'];
  fields(append, ['schema_version','protocol','status','scope','baseline','preserved_slugs','added_slug','source_transition','reviewed_after']);
  assert.equal(append.schema_version, 1);
  assert.equal(append.protocol, 'chialisp-oxide-p2dc-append/v1');
  assert.equal(append.status, 'reviewed_final_p2dc_append', 'Current23 append remains pending; historical captures cannot publish an active corpus');
  assert.equal(append.baseline.kind, 'unsigned_reviewed_content_addressed_checkpoint');
  assert.equal(append.baseline.report_sha256, sha(inputs.validation));
  assert.equal(append.baseline.authorizing_transition_path, 'corpus/runtime-composition-baseline.json');
  assert.equal(append.baseline.authorizing_transition_sha256, sha(inputs['profile.json']));
  assert.equal(sha(inputs['active-validation']), append.reviewed_after.report_sha256);
  assert.equal(sha(canonical(data['active-validation'])), append.reviewed_after.canonical_report_sha256);
  assert.deepEqual(data['active-validation'].puzzles.map(row => row.slug), [...append.preserved_slugs, append.added_slug]);
  assert.equal(append.added_slug, 'p2-delegated-conditions');
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
  const selected = report => {
    const rows = report.puzzles.filter(row => row.slug === 'p2-singleton');
    assert.equal(rows.length, 1, 'Expected exactly one pay-to-singleton component');
    return rows[0];
  };
  const puzzle = selected(data.validation), previous = selected(data.checkpoint), active = selected(data['active-validation']);
  assert.deepEqual(active.artifacts, puzzle.artifacts, 'Active23 changed the historical P2 compiler artifacts');
  assert.deepEqual(active.protocol_validation, puzzle.protocol_validation, 'Active23 changed the historical P2 bundles/receipts');
  assert.equal(active.source, puzzle.source, 'Active23 changed the retained P2 policy source');
  assert.equal(puzzle.slug, 'p2-singleton'); assert.equal(previous.slug, puzzle.slug);
  assert.equal(sha(canonical(puzzle)), after.p2_sha256);
  assert.equal(sha(canonical(puzzle.protocol_validation)), after.protocol_sha256);
  assert.equal(puzzle.source, previous.source, 'This compiler comparison must preserve the Rust policy source');
  const comparison = compareCostCaptures(previous, puzzle, data['before-costs'], data['costs.json']);
  const summary = summarizeCosts(comparison.rows);
  assert(summary.bytes < summary.beforeBytes && summary.rawVM < summary.beforeRawVM && summary.delta < 0, 'Composition measurements changed their recorded directions');
  const downloads = compositionArtifacts.map(row => ({ ...row, sha256: sha(inputs[row.basename]), bytes: inputs[row.basename].length, content: inputs[row.basename] }));
  return { ...comparison, summary, context, currentReportSHA: after.report_sha256,
    activeReportSHA: append.reviewed_after.report_sha256, downloads };
}
export async function readCompositionCostInputs(candidate) {
  return readCostInputs(candidate, [
    ['validation', 'corpus/checkpoints/runtime-composition-validation.json', 16 * 1024 * 1024],
    ['active-validation', 'corpus/validation.json', 16 * 1024 * 1024],
    ['append-profile', 'corpus/p2-delegated-conditions-append-baseline.json', 4 * 1024 * 1024],
    ['checkpoint', checkpointArtifact.path, 16 * 1024 * 1024],
    ['source-profile', 'corpus/p2-source-cost-baseline.json', 4 * 1024 * 1024],
    ['before-costs', 'corpus/p2-singleton-costs.json', 4 * 1024 * 1024],
    ...compositionArtifacts.map(row => [row.basename, row.path, 4 * 1024 * 1024]),
  ], 64 * 1024 * 1024);
}
export async function loadCompositionCostEvidence(candidate) { return compositionCostEvidence(await readCompositionCostInputs(candidate)); }

const fmt = value => new Intl.NumberFormat('en-US').format(value);
const plain = markup => markup.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const attr = (markup, name) => markup.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
function pair(markup, before, current) {
  assert(markup); assert.equal(attr(markup, 'data-before'), String(before)); assert.equal(attr(markup, 'data-current'), String(current));
  assert.equal(plain(markup), `${fmt(before)} → ${fmt(current)}`, 'Wrong visible composition cost');
}
export function checkCompositionDisplay(markup, evidence, { compact = false, base = '/chialisp-oxide/' } = {}) {
  const matches = [...markup.matchAll(/<section\b[^>]*data-composition-costs[^>]*>[\s\S]*?<\/section>/g)];
  assert.equal(matches.length, 1); const section = matches[0][0], text = plain(section);
  assert.equal(attr(section, 'data-composition-view'), compact ? 'compact' : 'full');
  assert.equal(plain(section.match(/<h2\b[^>]*>([\s\S]*?)<\/h2>/)?.[1] ?? ''), 'Source-only checkpoint → composed compiler checkpoint', 'Wrong visible composition checkpoint heading');
  const context = section.match(/<p\b[^>]*data-composition-context[^>]*>[\s\S]*?<\/p>/)?.[0];
  assert(context, 'Missing composition/source-only context');
  const visibleContext = plain(context);
  assert(visibleContext.includes(`unsigned reviewed source-only checkpoint SHA-256 ${evidence.context.reportSHA}`)
    && visibleContext.includes(`signed historical E20 anchor ${evidence.context.anchor.commit}`)
    && visibleContext.includes(`composed compiler validation SHA-256 ${evidence.currentReportSHA}`)
    && visibleContext.includes(`active corpus validation SHA-256 ${evidence.activeReportSHA}`), 'Wrong visible labelled composition/source-only/active identities');
  const commitAnchors = [...context.matchAll(/<a\b[^>]*>[\s\S]*?<\/a>/g)].map(row => row[0]);
  assert.equal(commitAnchors.length, 1, 'Expected one signed historical anchor');
  assert.equal(attr(commitAnchors[0], 'href'), `https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.anchor.commit}`);
  assert.equal(plain(commitAnchors[0]), evidence.context.anchor.commit, 'Wrong visible signed historical anchor commit');
  assert.deepEqual([...section.matchAll(/data-composition-summary="([^"]+)"/g)].map(row => row[1]), ['module','vm','joint'], 'Missing/duplicate composition summary metrics');
  for (const [key, before, current] of [['module', evidence.summary.beforeBytes, evidence.summary.bytes], ['vm', evidence.summary.beforeRawVM, evidence.summary.rawVM], ['joint', evidence.summary.beforeTotal, evidence.summary.total]]) { pair(section.match(new RegExp(`<strong\\b[^>]*data-composition-summary="${key}"[^>]*>[\\s\\S]*?</strong>`))?.[0], before, current);
    const label = { module: 'optimized Rust module bytes', vm: 'ordinary input · primary VM cost', joint: 'joint bundle · total conditions cost' }[key];
    const block = section.match(new RegExp(`<div\\b[^>]*class="[^"]*\\bmetric\\b[^"]*"[^>]*>\\s*<strong\\b[^>]*data-composition-summary="${key}"[^>]*>[\\s\\S]*?</strong>\\s*<span[^>]*>([\\s\\S]*?)</span>`));
    assert.equal(plain(block?.[1] ?? ''), label, 'Wrong visible composition summary unit/context');
  }
  assert(text.includes(`${fmt(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%)`));
  assert(text.includes(`${fmt(-evidence.summary.executionDelta)} fewer execution units`));
  assert(text.includes(`${fmt(-evidence.summary.byteDelta)} fewer charged bytes`));
  for (const scope of ['clvmr 0.21.0', 'clvmr 0.16.2', 'chia-consensus 0.36.1', '12,000', 'two-byte quote wrapper', 'not a universal cost bound', 'not additional admissions']) assert(text.includes(scope), `Missing composition context: ${scope}`);
  assert(section.includes(`href="${base}tutorials/transaction-costs/"`));
  if (compact) return;
  const matrix = section.match(/<table\b[^>]*data-composition-matrix[^>]*>[\s\S]*?<\/table>/)?.[0]; assert(matrix);
  assert.equal(plain(matrix.match(/<caption[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? ''), 'Source-only checkpoint → composed compiler checkpoint: total conditions costs');
  assert.deepEqual([...matrix.matchAll(/<th\b[^>]*scope="col"[^>]*>([\s\S]*?)<\/th>/g)].map(row => plain(row[1])), ['Build', 'Genesis', 'Joint collection', 'Lineage']);
  const rows = [...matrix.matchAll(/<tr\b[^>]*data-composition-build="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)]; assert.equal(rows.length, 5);
  for (const [index, row] of rows.entries()) {
    const expected = evidence.rows[index]; assert.equal(row[1], `${expected.backend}:${expected.optimize}`);
    assert.equal(plain(row[2].match(/<th\b[^>]*scope="row"[^>]*>([\s\S]*?)<\/th>/)?.[1] ?? ''), expected.label);
    const cells = [...row[2].matchAll(/<td\b[^>]*data-composition-phase="([^"]+)"[^>]*>[\s\S]*?<\/td>/g)]; assert.deepEqual(cells.map(cell => cell[1]), ['genesis','joint','lineage']);
    for (const [i, cell] of cells.entries()) pair(cell[0], expected.phases[i].before.total, expected.phases[i].current.total);
  }
  const downloads = [...section.matchAll(/<a\b[^>]*data-composition-download="([^"]+)"[^>]*>[\s\S]*?<\/a>/g)];
  assert.deepEqual(downloads.map(row => row[1]), evidence.downloads.map(row => row.basename));
  for (const artifact of evidence.downloads) {
    assert(section.includes(`href="${base}reports/runtime-composition/${artifact.basename}"`));
    assert(section.includes(artifact.sha256));
  }
}
export async function checkCompositionDownloads(directory, evidence) {
  directory = await realpath(directory);
  assert.deepEqual((await readdir(directory)).sort(), evidence.downloads.map(row => row.basename).sort());
  const inputs = await readCostInputs(directory, evidence.downloads.map(row => [row.basename, row.basename, 4 * 1024 * 1024]), 12 * 1024 * 1024);
  for (const artifact of evidence.downloads) assert.deepEqual(inputs[artifact.basename], Buffer.from(artifact.content));
}
