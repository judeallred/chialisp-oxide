import assert from 'node:assert/strict';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { execFileSync } from 'node:child_process';
import { lstatSync, readFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

const base = 'experiments/standard-tutorial/evidence';
const digest = raw => createHash('sha256').update(raw).digest('hex');
const bounded = filename => {
  const info = lstatSync(filename);
  assert(info.isFile() && !info.isSymbolicLink() && info.nlink === 1 && info.size <= 16 * 1024 * 1024, 'Bounded regular standard evidence file');
  const raw = readFileSync(filename);
  assert.equal(raw.length, info.size);
  return raw;
};
export const standardTutorialScopes = [
  'WIP teaching policy. Not ready for real funds or production.',
  'Five actual module builds, each curried with its chosen synthetic key before direct simulator fixture funding. No wallet funding transaction or live network is recorded.',
  '80 SDK submissions and 85 retained primary observations have separate scopes; repeats add no unique coverage.',
  '45 exact typed consensus rejections are separate from 20 GeneratorRuntimeError umbrellas. Umbrellas add no semantic rejection credit.',
  'The public hidden payment fixture requires no synthetic signature. A real hidden policy must enforce its own intended authorization.',
  'The recorded checker performs no SDK execution or BLS verification and does not authenticate execution, installed binaries or compiler preservation.',
];
export async function readStandardTutorialEvidence({ required = true } = {}) {
  const root = await repository();
  const receipt = path.join(root, base, 'receipt.json');
  if (!required && !existsSync(receipt)) return { status: 'pending_current_capture', scope: standardTutorialScopes, downloads: [] };
  const before = bounded(receipt);
  const raw = execFileSync('python3', ['-B', path.join(root, 'experiments/standard-tutorial/check.py'), '--packet', path.join(root, base), '--current'], { cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 1024 * 1024 });
  const summary = JSON.parse(raw);
  assert.equal(summary.status, 'recorded_standard_tutorial_consistency_checked');
  assert.equal(summary.current_source_checked, true);
  assert.equal(summary.execution_credit, false);
  assert.equal(summary.bls_verification, false);
  assert.deepEqual(summary.counts, { builds: 5, scenario_groups: 15, direct_fixture_funding_coins: 15, transaction_attempts: 80, accepted: 15, typed_consensus_rejected: 45, runtime_umbrellas: 20 });
  assert.deepEqual(summary.primary_observations, { total: 85, accepted: 65, typed_program_rejected: 20 });
  assert(before.equals(bounded(receipt)), 'Retained receipt changed during recorded reader');
  assert(Array.isArray(summary.downloads) && summary.downloads.length <= 2048);
  const seen = new Set();
  for (const row of summary.downloads) {
    assert(typeof row.path === 'string' && row.path.split('/').every(p => p && p !== '.' && p !== '..') && !row.path.includes('\\') && !path.isAbsolute(row.path) && !seen.has(row.path), 'Unique confined standard download');
    seen.add(row.path);
    assert(Number.isSafeInteger(row.bytes) && row.bytes >= 0 && row.bytes <= 16 * 1024 * 1024 && /^[0-9a-f]{64}$/.test(row.sha256));
    let current = path.join(root, base);
    for (const part of row.path.split('/')) { current = path.join(current, part); assert(!lstatSync(current).isSymbolicLink()); }
    const bytes = bounded(current);
    assert.equal(bytes.length, row.bytes); assert.equal(digest(bytes), row.sha256);
  }
  return { ...summary, source: base, scope: standardTutorialScopes };
}
export async function readStandardTutorialDownloads({ required = true } = {}) {
  const evidence = await readStandardTutorialEvidence({ required });
  const root = await repository();
  return { evidence, downloads: evidence.downloads, payloads: new Map(evidence.downloads.map(row => [row.path, bounded(path.join(root, base, row.path))])) };
}
// This is a bounded DOM/text consistency gate, not computed browser layout.
const standardDisplayParser = unified().use(rehypeParse);
const standardDisplayLimits = { bytes: 16 * 1024 * 1024, nodes: 250_000, depth: 256 };
const standardInertTags = new Set(['script', 'style', 'template', 'noscript', 'svg']);
function standardNodeVisible(node) {
  const properties = node.properties ?? {};
  const hiddenStyle = /(?:^|;)\s*(?:display\s*:\s*none|(?:content-)?visibility\s*:\s*(?:hidden|collapse)|opacity\s*:\s*(?:0+(?:\.0*)?|\.0+)%?)(?:\s*!important)?\s*(?:;|$)/i;
  return !standardInertTags.has(node.tagName)
    && properties.hidden === undefined && properties.inert === undefined
    && properties.ariaHidden !== true && String(properties.ariaHidden).toLowerCase() !== 'true'
    && !hiddenStyle.test(properties.style ?? '')
    && !(properties.className ?? []).some(value => /^(?:sr-only|hidden|invisible)$/.test(value));
}
function standardDisplayNodes(html) {
  assert(typeof html === 'string' && Buffer.byteLength(html, 'utf8') <= standardDisplayLimits.bytes, 'Bounded actual standard HTML');
  const root = standardDisplayParser.parse(html);
  const rows = [], pending = [{ node: root, depth: 0, visible: true }];
  let scheduled = 1;
  while (pending.length) {
    const entry = pending.pop();
    if (entry.exit !== undefined) { rows[entry.exit].end = rows.length; continue; }
    assert(entry.depth <= standardDisplayLimits.depth, 'Standard display DOM depth limit');
    const index = rows.length;
    const row = { node: entry.node, visible: entry.visible && standardNodeVisible(entry.node), index, end: index + 1 };
    rows.push(row);
    const children = entry.node.children ?? [];
    const content = entry.node.content;
    scheduled += children.length + (content ? 1 : 0);
    assert(scheduled <= standardDisplayLimits.nodes, 'Standard display DOM node limit');
    pending.push({ exit: index });
    // Template contents also count toward inventory/work, but remain inert.
    if (content) pending.push({ node: content, depth: entry.depth + 1, visible: row.visible });
    for (let i = children.length - 1; i >= 0; i--) pending.push({ node: children[i], depth: entry.depth + 1, visible: row.visible });
  }
  return rows;
}
function standardVisibleText(rows, root) {
  const parts = [];
  for (let index = root.index; index < root.end; index++) {
    const row = rows[index];
    if (row.visible && row.node.type === 'text') parts.push(row.node.value);
  }
  return parts.join('').trim().replace(/\s+/g, ' ');
}
export function checkStandardTutorialDisplay(html, evidence) {
  assert.equal(evidence.status, 'recorded_standard_tutorial_consistency_checked');
  const rows = standardDisplayNodes(html), sections = [];
  for (const row of rows) if (row.node.properties?.dataStandardSdk !== undefined) {
    assert.equal(sections.length, 0, 'One current recorded standard SDK section'); sections.push(row);
  }
  assert.equal(sections.length, 1, 'One current recorded standard SDK section');
  const section = sections[0];
  assert.equal(section.node.tagName, 'section'); assert.equal(section.node.properties.dataStandardSdk, 'current');
  assert(section.visible, 'Standard SDK section and ancestors must be visible');
  const counts = { sdk_attempts: 80, sdk_accepted: 15, sdk_typed_rejected: 45, sdk_runtime_umbrellas: 20, fixture_coins: 15, primary_observations: 85 };
  const cells = [];
  for (let index = section.index + 1; index < section.end; index++) if (rows[index].node.properties?.dataStandardCount !== undefined) {
    assert(cells.length < Object.keys(counts).length, 'Exact standard count inventory'); cells.push(rows[index]);
  }
  assert.equal(cells.length, Object.keys(counts).length, 'Exact standard count inventory');
  for (const [name, n] of Object.entries(counts)) {
    const matching = cells.filter(row => row.node.properties.dataStandardCount === name);
    assert.equal(matching.length, 1, 'Exact bound standard count ' + name);
    const cell = matching[0];
    assert.equal(cell.node.tagName, 'dd'); assert.equal(cell.node.properties.dataValue, String(n));
    assert(cell.visible, 'Standard count and ancestors must be visible: ' + name);
    assert.equal(standardVisibleText(rows, cell), new Intl.NumberFormat('en-US').format(n), 'Visible standard count ' + name);
  }
  const visible = standardVisibleText(rows, section);
  for (const text of ['Recorded simulator observations', 'GeneratorRuntimeError', 'no semantic rejection credit', 'no BLS verification', 'No live network']) assert(visible.includes(text), 'Visible standard scope: ' + text);
  return { visible_counts: Object.keys(counts).length, execution_credit: false };
}
