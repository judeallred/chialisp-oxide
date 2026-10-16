import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { pageSize, roleOptions, selectSources } from '../src/components/published/catalog.mjs';

const catalog = JSON.parse(await readFile(new URL('../../corpus/published-puzzles.json', import.meta.url), 'utf8'));
const sources = catalog.sources;
const all = selectSources(sources, { role: 'all' });
assert.equal(all.total, sources.length);
assert.equal(all.entries.length, pageSize);
assert.equal(new Set(sources.map(source => source.id)).size, sources.length, 'Source IDs must be unique');

// Walk every page: filtering must precede pagination, with no omitted/repeated
// records even when the final page is only partly filled.
const paginated = [];
for (let page = 1; page <= all.pages; page++) {
  const selection = selectSources(sources, { role: 'all' }, page);
  assert(selection.entries.length <= pageSize);
  assert.equal(selection.start, (page - 1) * pageSize + 1);
  assert.equal(selection.end, Math.min(page * pageSize, sources.length));
  paginated.push(...selection.entries.map(source => source.id));
}
assert.deepEqual(paginated, sources.map(source => source.id));

for (const [language, count] of Object.entries(catalog.summary.by_language)) {
  const selection = selectSources(sources, { role: 'all', language });
  assert.equal(selection.total, count, `Language count: ${language}`);
  assert(selection.entries.every(source => source.language === language));
}
let roleTotal = 0;
for (const role of roleOptions) {
  const count = role.kinds.reduce((sum, kind) => sum + (catalog.summary.by_artifact_kind[kind] ?? 0), 0);
  const selection = selectSources(sources, { role: role.value });
  assert.equal(selection.total, count, `Role count: ${role.value}`);
  assert(selection.entries.every(source => role.kinds.includes(source.artifact_kind)));
  roleTotal += count;
}
assert.equal(roleTotal, sources.length, 'Source-role groups must partition the snapshot');

const rustPrograms = selectSources(sources, { language: 'rust', role: 'programs' });
const rustBuilders = selectSources(sources, { language: 'rust', role: 'builders' });
assert.equal(rustPrograms.total, 12, 'Current annotated Rust puzzle sources');
assert.equal(rustBuilders.total, 65, 'Current Rust host puzzle builders');
assert(rustPrograms.entries.every(source => source.artifact_kind === 'rust_to_clvm_puzzle_source'));
assert(rustBuilders.entries.every(source => source.artifact_kind === 'rust_puzzle_builder'));

const sample = sources.find(source => source.path.toLowerCase().includes('singleton') && source.language === 'chialisp');
assert(sample);
const combined = selectSources(sources, {
  role: 'all', language: sample.language, repository: sample.repository,
  query: `  SINGLETON   ${sample.classification.toUpperCase()} `,
});
assert(combined.total > 0, 'Combined metadata search must preserve matches');
assert(combined.entries.some(source => source.id === sample.id));
assert(combined.entries.every(source => source.repository === sample.repository && source.language === sample.language));
assert(combined.entries.every(source => source.path.toLowerCase().includes('singleton')));
assert(combined.entries.every(source => source.classification === sample.classification));

const empty = selectSources(sources, { query: 'NO_SUCH_SOURCE_7a2' });
assert.equal(empty.total, 0); assert.equal(empty.start, 0); assert.equal(empty.end, 0);
assert.equal(empty.pages, 1); assert.deepEqual(empty.entries, []);
for (const invalid of [-100, 0, Number.NaN, Infinity, -Infinity]) {
  assert.equal(selectSources(sources, { role: 'all' }, invalid).page, 1);
}
assert.equal(selectSources(sources, { role: 'all' }, 1e6).page, all.pages);
assert.equal(selectSources(sources, { role: 'all' }, 2.9).page, 2);
assert.equal(selectSources(sources, { role: 'unknown-role' }).total, 0);

console.log(`PASS catalog: ${sources.length} records partitioned across ${all.pages} bounded pages; all language/role counts, combined metadata search, empty/invalid-page controls, and ${rustPrograms.total} Rust→CLVM vs ${rustBuilders.total} host-builder sources`);
