import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { projectPrecommitControls, readPrecommitControls, precommitControlsProtocol } from '../src/lib/precommit-source-controls.mjs';

const args = new Set(process.argv.slice(2));
assert([...args].every(arg => ['--constructed', '--built'].includes(arg)));
assert(!(args.has('--constructed') && args.has('--built')));
const report = JSON.parse(await readFile(new URL('../../corpus/validation.json', import.meta.url), 'utf8'));
const slugs = ['precommit-claim', 'precommit-registry'];
const selected = slugs.map(slug => {
  const puzzle = report.puzzles.find(row => row.slug === slug);
  assert(puzzle?.validated);
  const builds = [{ backend: 'rust_mir', optimize_lir: true, scenarios: puzzle.simulator }, ...puzzle.simulator_comparisons];
  assert.deepEqual(builds.map(row => [row.backend, row.optimize_lir]), [['rust_mir', true], ['chialisp', true], ['rue_source', true]]);
  for (const row of puzzle.cases) assert.equal(row.runs.length, 5);
  return { puzzle, builds, vectors: puzzle.cases.reduce((sum, row) => sum + row.runs.length, 0),
    groups: builds.reduce((sum, row) => sum + row.scenarios.length, 0) };
});
const visible = html => html.replace(/<[^>]*>/g, ' ').replace(/&#(?:x([0-9a-f]+)|([0-9]+));/gi,
  (_, hex, dec) => String.fromCodePoint(parseInt(hex ?? dec, hex ? 16 : 10)))
  .replaceAll('&amp;', '&').replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&quot;', '"')
  .replace(/\s+/g, ' ').replace(/\s+([:;,.])/g, '$1').trim();
const boundaries = ['attacker-first simulator ordering', 'not an actual mempool race',
  'naive attacker-first baseline uses a separate independently compiled Chialisp fixture',
  'fresh-precommit witness expects', 'AssertHeightRelativeFailed', 'DoubleSpend',
  'Scenario groups are not transaction-attempt counts', 'ordinary Rust and Clippy can accept',
  'not errors Rust makes impossible', 'first-committer-wins', 'not ready for use'];
const countText = row => `${row.puzzle.cases.length} named template vectors · ${row.vectors} compiled template VM observations · ${row.builds.length} ledger builds · ${row.groups} ledger scenario groups.`;
const escapeRegExp = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
function tagSnippet(html, tag, text) {
  const snippet = html.match(new RegExp(`<${tag}\\b[^>]*>${escapeRegExp(text)}<\\/${tag}>`))?.[0];
  assert(snippet, `Missing visible ${tag}: ${text}`);
  return snippet;
}
function checkDisplay(html) {
  const text = visible(html);
  for (const boundary of boundaries) assert(text.toLowerCase().includes(boundary.toLowerCase()), `Missing scope/witness: ${boundary}`);
  assert(html.includes('docs/precommit-research.md'), 'Missing pinned research link');
  for (const row of selected) {
    const section = html.match(new RegExp(`<section\\b[^>]*data-precommit-export="${row.puzzle.slug}"[^>]*>([\\s\\S]*?)<\\/section>`))?.[1];
    assert(section, `Missing export ${row.puzzle.slug}`);
    assert(visible(section).includes(countText(row)), `Wrong visible counts ${row.puzzle.slug}`);
    for (const build of row.builds) {
      const key = `${row.puzzle.slug}:${build.backend}:${build.optimize_lir}`;
      const detail = section.match(new RegExp(`<details\\b[^>]*data-precommit-build="${key}"[^>]*>([\\s\\S]*?)<\\/details>`))?.[1];
      assert(detail, `Missing build ${key}`);
      assert(visible(detail).includes(`${build.scenarios.length} ledger scenario groups`));
      const artifact = row.puzzle.artifacts.find(a => a.backend === build.backend && a.optimize_lir === build.optimize_lir);
      assert(visible(detail).includes(artifact.tree_hash), `Wrong module identity ${key}`);
      for (const scenario of build.scenarios) {
        assert(scenario.passed && scenario.scope === 'ledger');
        const match = detail.match(new RegExp(`<li\\b[^>]*data-precommit-scenario="${scenario.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"[^>]*>([\\s\\S]*?)<\\/li>`))?.[1];
        assert(match, `Missing scenario ${scenario.name}`);
        assert(visible(match).includes(`${scenario.name}: passed.`), `Wrong visible witness ${scenario.name}`);
        const codes = [...match.matchAll(/<code\b[^>]*>([\s\S]*?)<\/code>/g)].map(item => visible(item[1]));
        assert.deepEqual(codes, scenario.rejections.map(rejection => rejection.code), `Wrong visible outcomes ${scenario.name}`);
      }
    }
  }
}
// This synthetic display checks the reader and corruption controls only. It
// claims no native, compiler, VM or SDK execution and writes no evidence.
function fixture() {
  return `<main>${boundaries.join('; ')}<a href="https://github.com/judeallred/chialisp-oxide/blob/main/docs/precommit-research.md">research</a>${selected.map(row =>
    `<section data-precommit-export="${row.puzzle.slug}"><p>${countText(row)}</p>${row.builds.map(build => {
      const artifact = row.puzzle.artifacts.find(a => a.backend === build.backend && a.optimize_lir === build.optimize_lir);
      return `<details data-precommit-build="${row.puzzle.slug}:${build.backend}:${build.optimize_lir}"><summary>${build.scenarios.length} ledger scenario groups</summary><p>${artifact.tree_hash}</p><ul>${build.scenarios.map(scenario =>
        `<li data-precommit-scenario="${scenario.name}"><strong>${scenario.name}</strong>: passed. ${scenario.rejections.map(rejection => `<code>${rejection.code}</code>`).join(', ')}</li>`).join('')}</ul></details>`;
    }).join('')}</section>`).join('')}</main>`;
}
let corruptions = 0;
const controlNames = ['claim_original', 'claim_drop_age', 'registry_original', 'registry_drop_signature'];
const controlLabels = ['minimum_age_original', 'minimum_age_removed', 'owner_signature_original', 'owner_signature_removed'];
const controlCodes = ['AssertHeightRelativeFailed', null, 'BadAggregateSignature', null];
function controlFixture() {
  const runs = ['off', 'on'].flatMap(mode => controlNames.map(name => ({mode, name,
    module_tree_hash: 'ab'.repeat(32), execution: {outcome: {status: 'accepted', output_hex: '80'}}})));
  return {protocol: precommitControlsProtocol, status: 'actual_source_controls_captured',
    native: {test_groups: 1, observations: controlNames.map(name => ({name, status: 'accepted', output_hex: '80'}))}, runs,
    ledger: {transaction_attempts: 8, runs: runs.map((row, index) => ({mode: row.mode, name: row.name,
      control: controlLabels[index % 4], module_tree_hash: row.module_tree_hash, reference_tree_hash: 'cd'.repeat(32),
      scenarios: [{name: `source control ${controlLabels[index % 4]}: one complete-spend submission`,
        scope: 'ledger', passed: true, rejections: controlCodes[index % 4] ? [{stage: 'consensus', code: controlCodes[index % 4]}] : []}]}))}};
}
const sourceCounts = '1 native test group · 4 native output observations · 8 primary VM observations · 8 separate SDK submissions · 4 accepted mutant spends · 4 typed original-policy rejections.';
function checkControlsDisplay(html, evidence) {
  const section = html.match(/<section\b[^>]*data-precommit-source-controls(?:\s|=|>)[^>]*>([\s\S]*?)<\/section>/)?.[1];
  assert(section, 'Missing admitted source controls');
  assert(visible(section).includes(sourceCounts), 'Wrong visible source-control counts');
  assert(visible(section).includes(precommitControlsProtocol));
  assert(section.includes('reports/precommit-source-controls.json'));
  for (const row of evidence.rows) {
    const content = section.match(new RegExp(`<li\\b[^>]*data-precommit-control="${row.mode}:${row.name}"[^>]*>([\\s\\S]*?)<\\/li>`))?.[1];
    assert(content, `Missing source-control witness ${row.mode}/${row.name}`);
    const text = visible(content);
    assert(text.includes(`${row.mode} / ${row.name}: witness ${row.control}; scenario passed=true; actual SDK outcome ${row.code ?? 'accepted'}.`), 'Wrong visible control/outcome/pass relation');
    assert(text.includes(`Own module ${row.module_tree_hash}; independent counterpart module ${row.reference_tree_hash}.`), 'Wrong visible own/counterpart identities');
    if (row.code) assert(text.includes('full SDK simulator equality checked in memory'));
    else assert(text.includes("owner's 1-mojo receipt") && text.includes("treasury's 1,000-mojo payment"));
  }
}
function controlsHTML(evidence) {
  return `<section data-precommit-source-controls><p>${sourceCounts}</p>${evidence.rows.map(row =>
    `<li data-precommit-control="${row.mode}:${row.name}"><strong>${row.mode} / ${row.name}</strong>: witness <code>${row.control}</code>; scenario passed=true; actual SDK outcome <code>${row.code ?? 'accepted'}</code>.
    <p>Own module <code>${row.module_tree_hash}</code>; independent counterpart module <code>${row.reference_tree_hash}</code>.</p>${row.code ? 'full SDK simulator equality checked in memory' : "owner's 1-mojo receipt and treasury's 1,000-mojo payment"}</li>`).join('')}
    ${precommitControlsProtocol}<a href="reports/precommit-source-controls.json">packet</a></section>`;
}
function controlsCorruptions(html, evidence) {
  checkControlsDisplay(html, evidence);
  const section = html.match(/<section\b[^>]*data-precommit-source-controls(?:\s|=|>)[^>]*>[\s\S]*?<\/section>/)?.[0]; assert(section);
  const own = section.match(/Own module\s*<code\b[^>]*>[0-9a-f]+<\/code>/)?.[0]; assert(own);
  for (const [before, after] of [
    ['4 native output observations', '8 native output observations'], ['8 separate SDK submissions', '8 VM observations'],
    ['scenario passed=true', 'scenario passed=false'],
    ...[['minimum_age_original', 'minimum_age_removed'], ['AssertHeightRelativeFailed', 'accepted'],
      ['BadAggregateSignature', 'DoubleSpend'], ['accepted', 'BadAggregateSignature']].map(([from, to]) => {
      const snippet = tagSnippet(section, 'code', from); return [snippet, snippet.replace(`>${from}<`, `>${to}<`)];
    }),
    [own, own.replace(evidence.rows[0].module_tree_hash, '00')],
    ["owner's 1-mojo receipt", "owner's 2-mojo receipt"],
  ]) {
    assert(section.includes(before), `No-op source display corruption ${before}`);
    assert.throws(() => checkControlsDisplay(html.replace(section, section.replace(before, after)), evidence), `Escaped source display corruption ${before}`);
    corruptions++;
  }
}
function checkCorruptions(html) {
  checkDisplay(html);
  const policyBoundary = html.match(/not\s+errors\s+Rust\s+makes\s+impossible/)?.[0];
  assert(policyBoundary, 'Missing visible policy-error boundary');
  const evidenceStart = html.search(/<section\b[^>]*data-precommit-evidence(?:\s|=|>)/);
  const evidence = evidenceStart < 0 ? html : html.slice(evidenceStart);
  const first = selected[0];
  const artifact = first.puzzle.artifacts.find(a => a.backend === 'rust_mir' && a.optimize_lir);
  for (const [before, after] of [
    [countText(first), countText(first).replace('named template vectors', 'transaction attempts')],
    [`${first.groups} ledger scenario groups.`, `${first.groups + 1} ledger scenario groups.`],
    ['attacker-first simulator ordering', 'actual public mempool experiment'],
    ['fresh-precommit witness expects', 'fresh-precommit witness not checked'],
    ['naive attacker-first baseline uses a separate independently compiled Chialisp fixture',
      'naive attacker-first baseline executes three language implementations'],
    [policyBoundary, policyBoundary.replace(/^not\s+/, '')],
    [artifact.tree_hash, '00'.repeat(32)],
    ...[first.builds[0].scenarios[0].name].map(name => {
      const snippet = tagSnippet(evidence, 'strong', name); return [snippet, snippet.replace(name, 'an unrelated successful witness')];
    }),
    ['</strong>: passed.', '</strong>: failed.'],
    ...[['AssertHeightRelativeFailed', 'BadAggregateSignature'], ['DoubleSpend', 'AssertHeightRelativeFailed']].map(([from, to]) => {
      const snippet = tagSnippet(evidence, 'code', from); return [snippet, snippet.replace(`>${from}<`, `>${to}<`)];
    }),
  ]) {
    assert(html.includes(before), `No-op corruption: ${before}`);
    const bad = evidence.includes(before) ? html.replace(evidence, evidence.replace(before, after)) : html.replace(before, after);
    assert.notEqual(bad, html);
    assert.throws(() => checkDisplay(bad), `Escaped visible corruption: ${before}`);
    corruptions++;
  }
}
checkCorruptions(fixture());
const sourceFixture = controlFixture();
const syntheticControls = projectPrecommitControls(sourceFixture);
controlsCorruptions(controlsHTML(syntheticControls), syntheticControls);
for (const mutate of [p => {p.ledger.runs[0].scenarios[0].passed = false;},
  p => {p.ledger.runs[1].scenarios[0].rejections = [{stage: 'consensus', code: 'BadAggregateSignature'}];},
  p => {p.ledger.runs[0].scenarios[0].rejections = [];}, p => {p.runs[0].execution.outcome.output_hex = 'ff0180';}]) {
  const bad = structuredClone(sourceFixture); mutate(bad); assert.throws(() => projectPrecommitControls(bad)); corruptions++;
}
if (!args.has('--constructed')) {
  const html = await readFile(new URL('../dist/tutorials/precommit/index.html', import.meta.url), 'utf8');
  assert(Buffer.byteLength(html) <= 512 * 1024, 'Precommit source/evidence tutorial exceeds its 512KiB HTML budget');
  checkCorruptions(html);
  const evidence = await readPrecommitControls();
  controlsCorruptions(html, evidence);
  assert.deepEqual(await readFile(new URL('../dist/reports/precommit-source-controls.json', import.meta.url)), evidence.bytes,
    'Source-control endpoint must preserve exact admitted bytes');
  for (const slug of slugs) for (const source of ['puzzle.rs', 'reference.clsp', 'puzzle.rue']) {
    assert(html.includes(`puzzles/${slug}/${source}`), `Missing actual source ${slug}/${source}`);
  }
}
console.log(`PASS precommit template/ledger boundaries, exact module identities, visible witnesses/outcomes and ${corruptions} constructed display corruptions; ${args.has('--constructed') ? 'synthetic display only' : 'actual built tutorial checked'}. No new execution or proof credit.`);
