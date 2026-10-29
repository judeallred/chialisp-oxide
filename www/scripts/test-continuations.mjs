import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { readContinuationEvidence } from '../src/lib/continuations.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
for (const script of ['check_retained.py', 'check_ledger.py']) {
  execFileSync('python3', ['-B', `experiments/authenticated-continuations/${script}`],
    { cwd: root, timeout: 60_000, maxBuffer: 2 * 1024 * 1024 });
}
const view = await readContinuationEvidence();
const parser = unified().use(rehypeParse);
const text = node => node.type === 'text' ? node.value : (node.children ?? []).map(text).join('');
const normalized = node => text(node).trim().replace(/\s+/g, ' ');
function find(node, predicate, parents = []) {
  return [...(predicate(node) ? [{ node, parents }] : []), ...(node.children ?? []).flatMap(child => find(child, predicate, [...parents, node]))];
}
function visible(node) {
  assert(!['script', 'style', 'template', 'noscript'].includes(node.tagName));
  const p = node.properties ?? {};
  assert(p.hidden === undefined && p.ariaHidden !== 'true' && p.ariaHidden !== true, 'Witness is hidden');
  assert(!/display\s*:\s*none|visibility\s*:\s*hidden|opacity\s*:\s*0(?:[;\s]|$)/i.test(p.style ?? ''), 'Witness has hidden styling');
  assert(!(p.className ?? []).some(value => /^(?:sr-only|hidden|invisible)$/.test(value)), 'Witness has a hiding class');
}
function one(node, property, value) {
  const rows = find(node, child => child.properties?.[property] === value);
  assert.equal(rows.length, 1, `One visible ${property}=${value} required`);
  for (const parent of [...rows[0].parents, rows[0].node]) visible(parent);
  return rows[0].node;
}
function fields(node, values, property = 'dataContinuationField') {
  assert.equal(find(node, child => child.properties?.[property] !== undefined).length, values.length, 'Exact visible witness field inventory');
  for (const item of values) assert.equal(normalized(one(node, property, item.id)), item.value, `Visible witness changed: ${item.id}`);
}
export function checkContinuationDisplay(ast, evidence = view) {
  const sections = find(ast, node => node.properties?.dataContinuationsEvidence !== undefined);
  assert.equal(sections.length, 1, 'One mandatory finite continuation evidence section');
  const section = sections[0].node;
  fields(section, evidence.counts, 'dataContinuationCount');
  fields(section, evidence.context, 'dataContinuationContext');
  assert.equal(normalized(one(section, 'dataContinuationHost', '')), evidence.host, 'Historical capture host changed');
  const scopes = find(section, node => node.properties?.dataContinuationScope !== undefined);
  assert.deepEqual(scopes.map(row => { visible(row.node); return normalized(row.node); }), evidence.scopes, 'Exact bounded claim scopes');
  assert.equal(find(section, node => node.properties?.dataContinuationLane !== undefined).length, evidence.base.length);
  for (const lane of evidence.base) {
    const shown = one(section, 'dataContinuationLane', lane.id);
    assert.equal(normalized(shown.children.find(node => node.tagName === 'summary')), `${lane.label}: four accepted steps, three successor coins`);
    fields(one(shown, 'dataContinuationHeader', 'root'), lane.fields);
    assert.equal(find(shown, node => node.properties?.dataContinuationStep !== undefined).length, lane.steps.length);
    for (const step of lane.steps) fields(one(shown, 'dataContinuationStep', step.id), step.fields);
    assert.equal(find(shown, node => node.properties?.dataContinuationRejection !== undefined).length, lane.rejections.length);
    for (const rejection of lane.rejections) fields(one(shown, 'dataContinuationRejection', rejection.id), rejection.fields);
  }
  assert.equal(find(section, node => node.properties?.dataContinuationControl !== undefined).length, evidence.controls.length);
  for (const control of evidence.controls) {
    const shown = one(section, 'dataContinuationControl', control.id);
    assert.equal(normalized(shown.children.find(node => node.tagName === 'summary')), control.label);
    assert.equal(find(shown, node => node.properties?.dataContinuationRole !== undefined).length, control.roles.length);
    for (const role of control.roles) fields(one(shown, 'dataContinuationRole', role.id), role.fields);
  }
  assert(normalized(section).includes('CI builds and records its own current tools and fresh Linux observations; it does not claim those executables have the recorded macOS hashes.'));
  return section;
}
const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const renderFields = (items, property) => items.map(item => `<code ${property}="${escape(item.id)}">${escape(item.value)}</code>`).join('');
function render(evidence) {
  return `<section data-continuations-evidence><p>CI builds and records its own current tools and fresh Linux observations; it does not claim those executables have the recorded macOS hashes.</p>
    ${evidence.scopes.map(value => `<p data-continuation-scope>${escape(value)}</p>`).join('')}
    <code data-continuation-host>${evidence.host}</code>${renderFields(evidence.counts, 'data-continuation-count')}${renderFields(evidence.context, 'data-continuation-context')}
    ${evidence.base.map(lane => `<details data-continuation-lane="${lane.id}"><summary>${lane.label}: four accepted steps, three successor coins</summary><div data-continuation-header="root">${renderFields(lane.fields, 'data-continuation-field')}</div>${lane.steps.map(step => `<div data-continuation-step="${step.id}">${renderFields(step.fields, 'data-continuation-field')}</div>`).join('')}${lane.rejections.map(row => `<div data-continuation-rejection="${row.id}">${renderFields(row.fields, 'data-continuation-field')}</div>`).join('')}</details>`).join('')}
    ${evidence.controls.map(control => `<details data-continuation-control="${control.id}"><summary>${control.label}</summary>${control.roles.map(role => `<div data-continuation-role="${role.id}">${renderFields(role.fields, 'data-continuation-field')}</div>`).join('')}</details>`).join('')}</section>`;
}
const fixture = parser.parse(render(view));
checkContinuationDisplay(fixture);
let corruptions = 0;
const mutateText = node => { node.children = [{ type: 'text', value: 'changed visible witness' }]; };
const changes = [
  ast => mutateText(one(ast, 'dataContinuationCount', 'base-sdk')),
  ast => mutateText(one(ast, 'dataContinuationHost', '')),
  ast => mutateText(one(ast, 'dataContinuationContext', 'ledger-receipt')),
  ...['primary-vm', 'primary-failure', 'sdk', 'consensus', 'sdk-vm', 'network'].map(id => ast => mutateText(one(ast, 'dataContinuationContext', id))),
  ast => { one(ast, 'dataContinuationCount', 'source-vm').properties.hidden = true; },
  ast => { one(ast, 'dataContinuationCount', 'source-native').properties.style = 'display:none'; },
  ast => { const lane = one(ast, 'dataContinuationLane', 'rust_on'); mutateText(one(one(lane, 'dataContinuationStep', '0'), 'dataContinuationField', 'recipient')); },
  ast => { const lane = one(ast, 'dataContinuationLane', 'rust_off'); mutateText(one(one(lane, 'dataContinuationStep', '3'), 'dataContinuationField', 'successor')); },
  ast => { const lane = one(ast, 'dataContinuationLane', 'chialisp'); mutateText(one(one(lane, 'dataContinuationStep', '1'), 'dataContinuationField', 'uncompressed-cost')); },
  ast => { const lane = one(ast, 'dataContinuationLane', 'rust_on'); mutateText(one(one(lane, 'dataContinuationRejection', '2'), 'dataContinuationField', 'code')); },
  ...['native', 'native-output', 'primary', 'primary-output', 'sdk', 'recipient', 'fees', 'successor', 'module', 'coin'].map(id => ast => {
    const row = one(ast, 'dataContinuationControl', 'drop_amount:on'); mutateText(one(one(row, 'dataContinuationRole', 'mutant'), 'dataContinuationField', id));
  }),
  ast => { one(ast, 'dataContinuationControl', 'drop_terminal:off').properties.dataContinuationControl = 'other'; },
  ast => { const row = find(ast, node => node.properties?.dataContinuationScope !== undefined)[0].node; mutateText(row); },
];
for (const change of changes) { const ast = structuredClone(fixture); change(ast); assert.throws(() => checkContinuationDisplay(ast)); corruptions++; }
const args = process.argv.slice(2);
assert(args.length === 0 || args[0] === '--built' && args.length <= 2, 'Use --built [dist]');
export function checkBuiltContinuation(dist = path.resolve(root, 'www', 'dist')) {
  const file = path.resolve(dist, 'reference/continuations/index.html');
  assert(statSync(file).size <= 3 * 1024 * 1024, 'Bounded continuation built page');
  const bytes = readFileSync(file); assert(bytes.length <= 3 * 1024 * 1024);
  const ast = parser.parse(bytes.toString('utf8')); checkContinuationDisplay(ast);
  assert.equal(find(ast, node => node.tagName === 'h1').length, 1, 'Imported guide needs exactly one Starlight h1');
  assert(find(ast, node => node.tagName === 'a' && node.properties?.href === '/chialisp-oxide/reference/continuations/').length > 0, 'Mandatory sidebar link');
}
if (args[0] === '--built') checkBuiltContinuation(path.resolve(root, 'www', args[1] ?? 'dist'));
const packageJSON = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
assert(packageJSON.scripts['test:site'].includes('node scripts/check-site.mjs'), 'Existing mandatory site entrypoint required');
assert(readFileSync(new URL('./check-site.mjs', import.meta.url), 'utf8').includes('checkBuiltContinuation(root)'), 'Built continuation gate must be called by the mandatory site entrypoint');
console.log(`PASS continuation publication: complete portable packet admission, 108 base VM / 8 native + 16 source VM / 45 + 16 SDK lanes, visible coin/cost/outcome witnesses, ${corruptions} display corruptions${args.length ? ' and actual built page' : ''}`);
