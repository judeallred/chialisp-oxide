import { readFile, readdir, stat } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { failureProtocol, checkCorpusExecutions, checkExecution, checkScalarProofReplays, checkTreeProofReplays, testFailureProtocol } from './vm-failures.mjs';
import { checkLedgerReport, testLedgerProtocol } from './ledger-rejections.mjs';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const base = '/chialisp-oxide/';
const origin = 'https://judeallred.github.io';

async function files(directory) {
  const contents = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(contents.map(entry => entry.isDirectory() ? files(path.join(directory, entry.name)) : [path.join(directory, entry.name)]))).flat();
}

const built = await files(root);
const pages = built.filter(file => file.endsWith('.html'));
assert(pages.length >= 31, 'Expected the full documentation and puzzle corpus');
const cache = new Map();
const html = async file => {
  if (!cache.has(file)) cache.set(file, await readFile(file, 'utf8'));
  return cache.get(file);
};

let checkedLinks = 0;
for (const file of pages) {
  const text = await html(file);
  assert.match(text, /<html[^>]*lang="en"/, `${file}: missing document language`);
  assert(text.includes('aria-label="Prototype status"'), `${file}: missing prominent prototype status banner`);
  assert(text.includes('WIP prototype. Not ready for use. Do not use for real funds or production puzzles.'), `${file}: missing explicit prototype warning`);
  const url = new URL(base + path.relative(root, file).replace(/index\.html$/, ''), origin);
  for (const match of text.matchAll(/\b(?:href|src)="([^"]+)"/g)) {
    const raw = match[1].replaceAll('&amp;', '&');
    if (/^(?:data:|mailto:|tel:|javascript:)/.test(raw)) continue;
    const target = new URL(raw, url);
    if (target.origin !== origin) continue;
    assert(target.pathname.startsWith(base), `${file}: URL escapes the Pages base: ${raw}`);
    const relative = decodeURIComponent(target.pathname.slice(base.length));
    let destination = path.join(root, relative);
    assert(destination.startsWith(root), `Path escapes build directory: ${raw}`);
    // Astro emits its special error route as 404.html even with trailingSlash.
    if (relative === '404/') destination = path.join(root, '404.html');
    else if (target.pathname.endsWith('/')) destination = path.join(destination, 'index.html');
    const info = await stat(destination).catch(() => null);
    assert(info?.isFile(), `${file}: missing local target ${raw}`);
    if (target.hash && destination.endsWith('.html')) {
      const fragment = decodeURIComponent(target.hash.slice(1));
      const ids = new Set([...((await html(destination)).matchAll(/\bid="([^"]+)"/g))].map(m => m[1]));
      assert(ids.has(fragment), `${file}: missing fragment ${raw}`);
    }
    checkedLinks++;
  }
}

for (const name of ['validation', 'proofs', 'tree-proofs', 'inventory', 'tutorial-examples', 'published-puzzles', 'error-study']) {
  const report = JSON.parse(await readFile(path.join(root, `reports/${name}.json`), 'utf8'));
  assert.equal(report.schema_version, 1, `${name}: report schema`);
  assert.deepEqual(report, JSON.parse(await readFile(new URL(`../../corpus/${name}.json`, import.meta.url), 'utf8')), `${name}: published report differs from current repository evidence`);
}
const validation = JSON.parse(await readFile(path.join(root, 'reports/validation.json'), 'utf8'));
const proofs = JSON.parse(await readFile(path.join(root, 'reports/proofs.json'), 'utf8'));
const treeProofs = JSON.parse(await readFile(path.join(root, 'reports/tree-proofs.json'), 'utf8'));
checkCorpusExecutions(validation);
checkLedgerReport(validation);
console.log(`Consensus rejection controls: ${testLedgerProtocol()} passed`);
checkScalarProofReplays(proofs);
checkTreeProofReplays(treeProofs);
console.log(`Typed failure controls: ${testFailureProtocol()} passed`);
const formal = await html(path.join(root, 'verification/formal/index.html'));
const structuralTutorial = await html(path.join(root, 'tutorials/p2-conditions/index.html'));
const structuralDirectory = path.join(root, 'reports/structural');
const structuralBasenames = [];
assert.equal(treeProofs.artifacts.length, 21, 'Expected the explicitly allowlisted structural proof artifacts');
for (const artifact of treeProofs.artifacts) {
  assert.match(artifact.path, /^proofs\/p2-conditions\/[a-z][a-z0-9-]*(?:\.ir\.json|\.smt2)$/, 'Unexpected structural source path');
  const basename = path.basename(artifact.path);
  structuralBasenames.push(basename);
  const published = await readFile(path.join(structuralDirectory, basename));
  const source = await readFile(new URL(`../../${artifact.path}`, import.meta.url));
  assert.deepEqual(published, source, `${basename}: published structural artifact bytes differ from repository`);
  for (const bytes of [published, source]) {
    assert.equal(createHash('sha256').update(bytes).digest('hex'), artifact.sha256, `${basename}: structural artifact hash mismatch`);
  }
  const link = `href="${base}reports/structural/${basename}"`;
  assert(formal.includes(link) && structuralTutorial.includes(link), `${basename}: missing direct proof download link`);
}
assert.deepEqual((await readdir(structuralDirectory)).sort(), structuralBasenames.sort(), 'Unexpected or missing published structural artifact');
for (const label of ['Before/after IR equivalence', 'Unsafe guard removal: refuted', 'Exact bytes and hash']) {
  assert(formal.includes(label), `Missing actual optimization evidence: ${label}`);
}
assert.equal(treeProofs.optimization.equivalence.status, 'proven');
assert.equal(treeProofs.optimization.proof.status, 'proven');
assert.equal(treeProofs.optimization.control.result.status, 'refuted');
assert.equal(treeProofs.optimization.control.detected, true);
assert.equal(validation.authoring.mode, 'cargo_library', 'Published validation must use real Cargo authoring');
const { entry: proofEntry, ...proofInputs } = proofs.authoring;
assert.equal(proofEntry, validation.authoring.exports['bounded-counter'].entry, 'Published proof has the wrong Cargo export');
assert.deepEqual(proofInputs, validation.authoring, 'Published proof/validation Cargo inputs differ');
const decodeAttribute = value => value.replace(/&(?:#x([0-9a-f]+)|#([0-9]+)|(amp|lt|gt|quot|apos));/gi,
  (_, hex, decimal, named) => hex || decimal
    ? String.fromCodePoint(Number.parseInt(hex ?? decimal, hex ? 16 : 10))
    : ({ amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" })[named.toLowerCase()]);
// Expressive Code normalizes surrounding/trailing whitespace, but wrapping
// must not add newlines or otherwise rewrite the copied program text.
const normalizedCode = source => source.replaceAll('\t', '  ').split(/\r?\n/).map(line => line.trimEnd()).join('\n').trim();
let checkedSourceCopies = 0;
for (const puzzle of validation.puzzles) {
  const detail = await html(path.join(root, `lab/${puzzle.slug}/index.html`));
  for (const language of ['chialisp', 'rust', 'rue']) assert(detail.includes(`data-language="${language}"`), `${puzzle.slug}: missing ${language} side-by-side source column`);
  assert(detail.includes('Serialized-byte diffs'), `${puzzle.slug}: missing compiled-output comparison`);
  assert(detail.includes(validation.authoring.exports[puzzle.slug].entry), `${puzzle.slug}: missing actual Cargo export identity`);
  const copied = [...detail.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
  for (const [label, source] of [['Rust', puzzle.source], ['Chialisp', puzzle.reference], ['Rue', puzzle.rue.source], ...[...puzzle.source_dependencies, ...puzzle.rue.dependencies].map(dependency => [dependency.path, dependency.source])]) {
    assert(copied.includes(normalizedCode(source)), `${puzzle.slug}: copy payload changed ${label} source`);
    checkedSourceCopies++;
  }
  for (const dependency of [...puzzle.source_dependencies, ...puzzle.rue.dependencies]) {
    assert(detail.includes(dependency.sha256), `${puzzle.slug}: missing source dependency provenance`);
    assert(detail.includes(`data-source-dependency="${dependency.path}"`), `${puzzle.slug}: missing expandable dependency source`);
    if (dependency.path.endsWith('/fixture_keys.rs')) {
      assert(detail.includes('Public fixture key bytes'), `${puzzle.slug}: fixture keys must be labeled separately`);
    }
  }
  if (puzzle.simulator.length) {
    assert(detail.includes(validation.simulator.rejection_protocol), `${puzzle.slug}: missing consensus protocol metadata`);
    assert(detail.includes('scenario runs, not transaction counts'), `${puzzle.slug}: grouped ledger count must be labeled`);
    for (const run of [{ backend: 'rust_mir', scenarios: puzzle.simulator }, ...puzzle.simulator_comparisons]) {
      const section = detail.match(new RegExp(`<details[^>]*data-ledger-language="${run.backend}"[^>]*>([\\s\\S]*?)</details>`))?.[1];
      assert(section, `${puzzle.slug}: missing ${run.backend} ledger observations`);
      for (const scenario of run.scenarios) assert(section.includes(`data-ledger-scenario="${scenario.name}"`), `${puzzle.slug}: missing named consensus scenario`);
      const expected = run.scenarios.flatMap(scenario => scenario.rejections.map(row => row.code)).sort();
      const actual = [...section.matchAll(/data-ledger-rejection="([A-Za-z0-9]+)"/g)].map(match => match[1]).sort();
      assert.deepEqual(actual, expected, `${puzzle.slug}/${run.backend}: displayed consensus codes differ from the report`);
    }
  }
  if (puzzle.kind === 'program_factory') {
    for (const label of ['Environment contract: raw tree.', 'returns a program', 'Published module identity', 'Apply the produced programs']) {
      assert(detail.includes(label), `${puzzle.slug}: missing factory contract/evidence label ${label}`);
    }
    assert(puzzle.factory_checks.length > 0 && puzzle.module_identity, `${puzzle.slug}: missing factory evidence`);
    assert.deepEqual(puzzle.simulator, [], 'A factory must not claim direct ledger scenarios');
    for (const backend of ['rust_mir', 'chialisp', 'rue_source']) {
      const artifact = puzzle.artifacts.find(row => row.backend === backend && row.optimize_lir);
      assert.equal(artifact.clvm_hex, puzzle.module_identity.upstream_hex, `${backend}: canonical factory bytes must match`);
      assert.equal(artifact.tree_hash, puzzle.module_identity.upstream_tree_hash, `${backend}: canonical factory hash must match`);
    }
    const applications = puzzle.factory_checks.reduce((n, row) => n + row.application_runs.length, 0);
    assert(detail.includes(`${applications} second-stage VM executions`), 'Factory application count must come from the report');
    const tutorial = await html(path.join(root, 'tutorials/p2-conditions/index.html'));
    assert(tutorial.includes(puzzle.module_identity.upstream_tree_hash), 'Tutorial must bind the published module identity');
  }
}
const recursiveTutorial = await html(path.join(root, 'tutorials/recursive-trees/index.html'));
const recursivePuzzle = validation.puzzles.find(puzzle => puzzle.slug === 'rightmost-tree');
assert(recursivePuzzle, 'Missing executed recursive-tree corpus fixture');
assert.equal(recursivePuzzle.kind, 'pure_function');
assert.equal(recursivePuzzle.environment, 'raw');
assert.deepEqual(recursivePuzzle.simulator, [], 'Tree helper must not claim ledger scenarios');
for (const language of ['chialisp', 'rust', 'rue']) {
  assert(recursiveTutorial.includes(`data-language="${language}"`), `Recursive tutorial misses ${language} comparison`);
}
const recursiveCopies = [...recursiveTutorial.matchAll(/\bdata-code="([^"]*)"/g)]
  .map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
for (const source of [recursivePuzzle.source, recursivePuzzle.reference, recursivePuzzle.rue.source]) {
  assert(recursiveCopies.includes(normalizedCode(source)), 'Recursive tutorial copy differs from executed source');
  checkedSourceCopies++;
}
for (const scope of ['version-4 typed IR', 'finite VM budget', 'proper list ends in nil', 'not a port of a published protocol module']) {
  assert(recursiveTutorial.includes(scope), `Recursive tutorial misses scope: ${scope}`);
}
const sdk = await html(path.join(root, 'tutorials/typed-sdk/index.html'));
assert(sdk.includes('newtype_swap.rs') && sdk.includes('newtype_time_swap.rs'), 'Missing executable SDK diagnostic examples');
for (const source of ['examples/cargo-puzzles/Cargo.toml', 'examples/cargo-puzzles/src/lib.rs', 'examples/cargo-puzzles/build.rs', 'crates/chialisp-oxide/src/lib.rs', 'crates/chialisp-oxide/src/domain.rs']) {
  assert(sdk.includes(source), `Missing actual Cargo tutorial source: ${source}`);
}
assert(sdk.includes('chialisp-oxide-puzzles') && sdk.includes('chialisp_oxide'), 'SDK tutorial must identify the real Cargo corpus');
const quickstart = await html(path.join(root, 'start/quickstart/index.html'));
for (const name of ['cargo-chialisp-oxide', 'chialisp-oxide-driver', 'cargo chialisp-oxide build', 'chialisp_oxide::puzzle', 'target/chialisp-oxide/artifacts/']) {
  assert(quickstart.includes(name), `Quickstart missing the current package, command, or output name: ${name}`);
}
const tutorialExamples = JSON.parse(await readFile(path.join(root, 'reports/tutorial-examples.json'), 'utf8'));
const coverage = await html(path.join(root, 'coverage/index.html'));
for (const example of tutorialExamples.examples) {
  assert(coverage.includes(example.title), `Missing canonical example: ${example.id}`);
  if (example.implementation_status === 'implemented') {
    assert.equal(example.validation_status, 'differential_only');
    const puzzle = validation.puzzles.find(puzzle => puzzle.slug === example.source_adaptation?.slug);
    assert(puzzle?.validated, `${example.id}: missing canonical validation`);
    const detail = await html(path.join(root, `lab/${puzzle.slug}/index.html`));
    assert(detail.includes('Original Chialisp expression') && detail.includes('no source wrapper'), `${example.id}: original source must be labeled`);
    assert(detail.includes('Environment contract: explicitly ignored.'), `${example.id}: missing original environment contract`);
    assert.equal(puzzle.environment, 'ignored');
    assert.equal(puzzle.reference_mode, 'expression');
  } else {
    assert.equal(example.implementation_status, 'not_started');
    assert.equal(example.validation_status, 'not_run', 'Canonical source inventory is not executed evidence');
  }
}
assert(built.includes(path.join(root, 'tutorials/canonical-intro/index.html')), 'Missing canonical Intro tutorial');
const homepage = await html(path.join(root, 'index.html'));
const consensusChecks = validation.puzzles.reduce((sum, puzzle) => sum
  + puzzle.simulator.reduce((n, scenario) => n + scenario.rejections.length, 0)
  + puzzle.simulator_comparisons.reduce((n, run) => n + run.scenarios.reduce((m, scenario) => m + scenario.rejections.length, 0), 0), 0);
assert(homepage.includes('ledger scenario runs') && !homepage.includes('ledger executions'), 'Homepage ledger count must describe grouped scenarios');
assert(homepage.includes(`${consensusChecks} exact consensus rejection checks`), 'Homepage rejection-check count must come from the report');
assert(homepage.includes('Errors that Rust makes impossible'), 'Missing dedicated static-error section');
const study = JSON.parse(await readFile(path.join(root, 'reports/error-study.json'), 'utf8'));
assert.equal(study.vm_failure_protocol, failureProtocol, 'Published error-study failure protocol');
for (const example of study.studies) {
  for (const language of ['chialisp', 'rue']) {
    for (const mode of example[language]) {
      if (mode.status === 'compiled') {
        for (const observation of [...mode.weak_suite, mode.missed_witness]) checkExecution(observation.execution, `published error study/${example.id}/${language}`);
      }
    }
  }
}
const studyPage = await html(path.join(root, 'verification/error-study/index.html'));
for (const example of study.studies) {
  assert(studyPage.includes(example.title), `${example.id}: missing executable error study`);
  if (example.category === 'rust_typecheck') assert(homepage.includes(example.title), `${example.id}: missing actual diagnostic summary`);
}
assert(studyPage.includes('E0308') && studyPage.includes('E0425'), 'Missing captured Rust diagnostic references');
const functionality = await html(path.join(root, 'reference/chialisp-functionality/index.html'));
assert(functionality.includes('proof') && functionality.includes('recursion'), 'Missing explicit functionality and parity gaps');
const catalog = await html(path.join(root, 'published/index.html'));
const published = JSON.parse(await readFile(path.join(root, 'reports/published-puzzles.json'), 'utf8'));
assert(catalog.includes('Published') && catalog.includes('not'), 'Missing source catalog scope');
assert(published.scope.not_internet_exhaustive, 'Source discovery must retain its bounded search scope');
const heroTitle = homepage.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/)?.[1].replace(/<[^>]*>/g, '');
assert.match(heroTitle ?? '', /WIP prototype.*not ready for use/i, 'Homepage must lead with the prototype status');

// Execute the real built Pagefind WebAssembly index in Node. Its fetches are
// served from dist; no server, browser, network access, or fake search required.
const originalFetch = globalThis.fetch;
globalThis.fetch = async input => {
  const target = new URL(typeof input === 'string' ? input : input.url ?? input.toString(), origin);
  assert.equal(target.origin, origin, 'Pagefind must fetch only its own static index');
  assert(target.pathname.startsWith(base), 'Pagefind asset escapes the Pages base');
  const content = await readFile(path.join(root, decodeURIComponent(target.pathname.slice(base.length))));
  return new Response(content, { headers: { 'Content-Type': target.pathname.endsWith('.wasm') ? 'application/wasm' : 'application/octet-stream' } });
};
try {
  const pagefind = await import(pathToFileURL(path.join(root, 'pagefind/pagefind.js')).href);
  await pagefind.options({ basePath: `${origin}${base}pagefind/`, baseUrl: base, language: 'en' });
for (const term of ['recovery', 'canonical', 'AGG_SIG_ME', 'CoinId', 'from_mojos', 'create_coin', 'chialisp_oxide', 'impossible', 'currying', 'p2_conditions', 'equivalence', 'rightmost', 'recursion']) {
    const search = await pagefind.search(term);
    assert(search.results.length > 0, `No production search results for ${term}`);
    const result = await search.results[0].data();
    assert(result.url.startsWith(base), `Search result escapes base: ${result.url}`);
    if (term === 'rightmost') {
      const matches = await Promise.all(search.results.slice(0, 10).map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/recursive-trees/`), 'Recursive tutorial is not discoverable in search');
    }
    console.log(`Search ${JSON.stringify(term)}: ${search.results.length} results; first ${result.url}`);
  }
  await pagefind.destroy();
} finally {
  globalThis.fetch = originalFetch;
}

console.log(`PASS: ${pages.length} pages, ${checkedLinks} local links/assets/fragments, ${checkedSourceCopies} original source-copy payloads, 7 JSON reports, ${structuralBasenames.length} exact structural downloads, and live Pagefind queries`);
