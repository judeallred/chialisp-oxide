import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const root = new URL('../../', import.meta.url);
const raw = await readFile(new URL('experiments/spend-costs/results.json', root));
const evidence = JSON.parse(raw);
const page = await readFile(new URL('www/dist/tutorials/generator-costs/index.html', root), 'utf8');
const format = n => new Intl.NumberFormat('en-US').format(n);

function checkDisplay(html) {
  assert(html.includes(evidence.validation_report_sha256), 'Missing current corpus identity');
  assert.equal((html.match(/<table\b[^>]*data-generator-port=/g) ?? []).length, 2);
  for (const [slug, label] of [['p2-delegated-conditions', 'owner_signature_exact_payout'], ['p2-singleton', 'joint']]) {
    const table = html.match(new RegExp(`<table\\b[^>]*data-generator-port="${slug}"[^>]*>[\\s\\S]*?</table>`))?.[0];
    assert(table, 'Missing comparison table');
    const rows = [...table.matchAll(/<tr\b[^>]*data-generator-build="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];
    const expected = evidence.measurement.cases.filter(row => row.slug === slug && row.label === label);
    assert.equal(rows.length, 5);
    assert.deepEqual(rows.map(row => row[1]), expected.map(row => `${row.backend}:${row.optimize_lir}`));
    for (const [index, row] of rows.entries()) {
      const source = expected[index], plain = source.costs.generators[0], compressed = source.costs.generators[1];
      const buildName = `${({rust_mir:'Rust', chialisp:'Chialisp', rue_source:'Rue'})[source.backend]} / ${source.optimize_lir ? 'on' : 'off'}`;
      assert(row[2].includes(`>${buildName}</th>`), 'Visible build name differs');
      const values = {helper:source.costs.helper.total_cost, uncompressed:plain.metrics.total_cost,
        backrefs:compressed.metrics.total_cost, bytes:compressed.generator_bytes, execution:compressed.metrics.execution_cost};
      const cells = [...row[2].matchAll(/<td\b[^>]*data-generator-metric="([^"]+)"[^>]*data-value="([^"]+)"[^>]*>([^<]+)<\/td>/g)];
      assert.deepEqual(cells.map(cell => cell[1]), Object.keys(values));
      for (const cell of cells) {
        assert.equal(cell[2], String(values[cell[1]]), 'Metric attribute differs');
        assert.equal(cell[3].trim(), format(values[cell[1]]), 'Visible metric differs');
      }
    }
  }
  const boundary = html.match(/<p\b[^>]*data-generator-boundary[^>]*>([\s\S]*?)<\/p>/)?.[1];
  assert(boundary?.includes('helper skips signature verification')
    && boundary.includes('two-byte quote wrapper')
    && boundary.includes('signature verification enabled')
    && boundary.includes('not fee quotes or complete ledger admissions'), 'Missing measurement boundaries');
  assert(html.includes('href="/chialisp-oxide/reports/spend-costs.json"'), 'Missing exact download');
}

checkDisplay(page);
const component = page.match(/<section\b[^>]*data-generator-costs[^>]*>[\s\S]*?<\/section>/)?.[0];
assert(component, 'Missing generator comparison component');
checkDisplay(component);
const bytesCell = component.match(/<td\b[^>]*data-generator-metric="bytes"[^>]*data-value="500"[^>]*>500<\/td>/)?.[0];
assert(bytesCell, 'Missing visible 500-byte witness');
const changes = [
  [evidence.validation_report_sha256, '0'.repeat(64)],
  ['data-generator-build="rust_mir:true"', 'data-generator-build="rust_mir:1"'],
  [bytesCell, bytesCell.replace('>500<', '>501<')],
  ['data-generator-metric="backrefs"', 'data-generator-metric="helper"'],
  ['>Rust / on</th>', '>Rue / on</th>'],
  ['helper skips signature verification', 'helper verifies signatures'],
  ['two-byte quote wrapper', 'no quote wrapper'],
  ['not fee quotes or complete ledger admissions', 'complete ledger admissions'],
];
for (const [before, after] of changes) {
  assert(component.includes(before), `Missing control target: ${before}`);
  assert.throws(() => checkDisplay(component.replace(before, after)), undefined, before);
}
assert.deepEqual(await readFile(new URL('www/dist/reports/spend-costs.json', root)), raw, 'Published measurement bytes differ');
console.log(`Generator costs: actual ten-row HTML, exact download and ${changes.length} display corruption controls passed`);
