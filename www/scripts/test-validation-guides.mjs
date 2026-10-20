import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';

const report = JSON.parse(await readFile(new URL('../../corpus/validation.json', import.meta.url), 'utf8'));
const parser = unified().use(rehypeParse);
const text = node => node.type === 'text' ? node.value : (node.children ?? []).map(text).join('');
function all(node, predicate) { return [...(predicate(node) ? [node] : []), ...(node.children ?? []).flatMap(child => all(child, predicate))]; }
const one = (node, property, value) => {
  const rows = all(node, child => child.properties?.[property] === value);
  assert.equal(rows.length, 1, `Expected one ${property}=${value}`); return rows[0];
};
function checkSection(section, puzzle, labels, ledger = false) {
  assert.equal(section.properties.dataValidationLabels, JSON.stringify(labels));
  assert.equal(all(section, node => node.properties?.dataValidationCase !== undefined).length, labels.length);
  for (const label of labels) {
    const expected = puzzle.cases.find(row => row.label === label);
    assert(expected, `Unknown source case ${puzzle.slug}/${label}`);
    const displayed = one(section, 'dataValidationCase', label);
    assert.equal(text(displayed.children.find(node => node.tagName === 'summary')), `${label} · native ${expected.native.status}`);
    assert.equal(text(all(displayed, node => node.tagName === 'code')[0]), expected.solution, 'Whole environment changed');
    if (expected.native.output_hex) assert(all(displayed, node => node.tagName === 'code').some(node => text(node) === expected.native.output_hex), 'Accepted output bytes changed');
    assert.equal(all(displayed, node => node.properties?.dataValidationMode !== undefined).length, 5);
    for (const run of expected.runs) {
      const row = one(displayed, 'dataValidationMode', `${run.backend}:${run.optimize_lir}`);
      const cells = row.children.filter(node => node.tagName === 'td');
      const failure = run.execution.failure;
      assert.equal(text(cells[0]), run.execution.outcome.status + (failure ? ` · ${failure.kind}/${failure.code}` : ''), 'Typed outcome changed');
      assert.equal(text(cells[1]), run.execution.cost === null ? 'not recorded' : new Intl.NumberFormat('en-US').format(run.execution.cost), 'VM cost changed');
    }
  }
  const scenarios = all(section, node => node.properties?.dataValidationLedger !== undefined);
  assert.equal(scenarios.length, ledger ? puzzle.simulator.length : 0);
  if (ledger) for (const row of puzzle.simulator) {
    const shown = one(section, 'dataValidationLedger', row.name);
    assert.equal(text(shown), `${row.name} · ${row.passed ? 'passed' : 'FAILED'}${row.rejections.length ? ' · ' + row.rejections.map(item => item.code).join(', ') : ''}`);
  }
}
const expected = {
  inputs: [
    ['capped-payment', ['minimum', 'maximum', 'over cap', 'padded integer', 'too large', 'extra arguments', 'nested argument']],
    ['sha256tree', ['nil atom', 'one zero byte is not nil', 'padded one remains different bytes', 'proper three-element list', 'improper pair']],
    ['inner-puzzle', ['committed quoted constant returns it', 'mismatched commitment rejects', 'committed program that raises rejects', 'short commitment rejects', 'improper argument list rejects']],
  ],
  outputs: [
    ['signature-payment', ['message', 'short bytes', 'extra arguments'], true],
    ['timelocked-payment', ['empty', 'extra arguments'], true],
    ['split-payment', ['minimum', 'maximum', 'over cap', 'negative'], true],
  ],
  review: [['password-coin', ['valid_spend', 'unsigned_retarget', 'wrong_password', 'invalid_pair_destination', 'arbitrary_condition_atom']]],
};
let witnesses = 0;
let controls = 0;
for (const [page, examples] of Object.entries(expected)) {
  const html = await readFile(new URL(`../dist/validation/${page}/index.html`, import.meta.url), 'utf8');
  const ast = parser.parse(html);
  assert.equal(all(ast, node => node.properties?.dataValidationCases !== undefined).length, examples.length);
  for (const [slug, labels, ledger] of examples) {
    const section = one(ast, 'dataValidationCases', slug);
    const puzzle = report.puzzles.find(item => item.slug === slug);
    checkSection(section, puzzle, labels, ledger);
    witnesses += labels.length;
    // Each displayed source is bound independently to the report. Mutating a
    // status, environment, typed failure or cost must fail the display gate.
    for (const mutate of [
      clone => { one(clone, 'dataValidationCase', labels[0]).children.find(node => node.tagName === 'summary').children = [{type:'text',value:'invented outcome'}]; },
      clone => { all(one(clone, 'dataValidationCase', labels[0]), node => node.tagName === 'code')[0].children = [{type:'text',value:'invented environment'}]; },
      clone => { const row = all(clone, node => node.properties?.dataValidationMode !== undefined)[0]; row.children.filter(node => node.tagName === 'td')[0].children = [{type:'text',value:'accepted · wrong/code'}]; },
      clone => { const row = all(clone, node => node.properties?.dataValidationMode !== undefined)[0]; row.children.filter(node => node.tagName === 'td')[1].children = [{type:'text',value:'0'}]; },
    ]) { const clone = structuredClone(section); mutate(clone); assert.throws(() => checkSection(clone, puzzle, labels, ledger)); controls++; }
  }
}
console.log(`PASS validation guides: ${witnesses} real witnesses, ${witnesses * 5} VM rows, exact native/output/ledger observations and ${controls} display-corruption controls`);
