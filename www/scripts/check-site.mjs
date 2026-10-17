import { readFile, readdir, stat } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { failureProtocol, checkCorpusExecutions, checkExecution, checkNativeOutcome, checkReplay, checkScalarProofReplays, checkTreeProofReplays, testFailureProtocol } from './vm-failures.mjs';
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
// A successful index command can still leave empty generated runtime assets.
// Check the actual browser files as well as executing search in Node below.
const searchAssets = built.filter(file => file.startsWith(path.join(root, 'pagefind') + path.sep)
  && /\.(?:js|css|pagefind)$/.test(file));
assert(searchAssets.length > 0, 'Missing built search runtime');
for (const file of searchAssets) {
  assert((await stat(file)).size > 0, `Empty generated search asset: ${path.relative(root, file)}`);
}
const cache = new Map();
const html = async file => {
  if (!cache.has(file)) cache.set(file, await readFile(file, 'utf8'));
  return cache.get(file);
};

let checkedLinks = 0;
for (const file of pages) {
  const text = await html(file);
  assert.match(text, /<html[^>]*lang="en"/, `${file}: missing document language`);
  assert.match(text, /<site-search(?:\s|>)/, `${file}: missing the rendered search control`);
  assert(text.includes('data-open-modal'), `${file}: missing the stock search modal trigger`);
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

for (const [name, source] of [
  ...['validation', 'proofs', 'tree-proofs', 'byte-proofs', 'inventory', 'tutorial-examples', 'published-puzzles', 'error-study'].map(name => [name, `corpus/${name}.json`]),
  ['tree-theorems', 'experiments/tree-theorems/results.json'],
]) {
  const report = JSON.parse(await readFile(path.join(root, `reports/${name}.json`), 'utf8'));
  assert.equal(report.schema_version, 1, `${name}: report schema`);
  assert.deepEqual(report, JSON.parse(await readFile(new URL(`../../${source}`, import.meta.url), 'utf8')), `${name}: published report differs from current repository evidence`);
}
const validation = JSON.parse(await readFile(path.join(root, 'reports/validation.json'), 'utf8'));
const proofs = JSON.parse(await readFile(path.join(root, 'reports/proofs.json'), 'utf8'));
const treeProofs = JSON.parse(await readFile(path.join(root, 'reports/tree-proofs.json'), 'utf8'));
const byteProofs = JSON.parse(await readFile(path.join(root, 'reports/byte-proofs.json'), 'utf8'));
checkCorpusExecutions(validation);
checkLedgerReport(validation);
console.log(`Consensus rejection controls: ${testLedgerProtocol()} passed`);
checkScalarProofReplays(proofs);
checkTreeProofReplays(treeProofs);
console.log(`Typed failure controls: ${testFailureProtocol()} passed`);
const formal = await html(path.join(root, 'verification/formal/index.html'));
const algorithmEvidence = JSON.parse(await readFile(path.join(root, 'reports/tree-theorems.json'), 'utf8'));
assert.equal(algorithmEvidence.expected_theorem_types, 21);
assert.equal(algorithmEvidence.finite_bridge.cases, 85);
assert.equal(algorithmEvidence.finite_bridge.recorded_vm_observations, 425);
assert.equal(algorithmEvidence.finite_bridge.vm_replays_executed, 425);
assert.equal(algorithmEvidence.finite_bridge.artifacts.length, 10);
assert.equal(algorithmEvidence.mutation_controls, 38);
for (const label of ['Lean finite-tree algorithm theorems', '85', '425', 'freshness-only check', 'not extracted from Rust', 'compiler-preservation proof']) {
  assert(formal.includes(label), `Missing algorithm-model evidence/scope label: ${label}`);
}
for (const theorem of algorithmEvidence.theorems) assert(formal.includes(theorem.name), `Missing recorded theorem audit: ${theorem.name}`);
for (const filename of ['TreeTheorems.lean', 'ProofObligations.lean', 'check-model.py']) {
  assert(formal.includes(`href="https://github.com/judeallred/chialisp-oxide/blob/main/experiments/tree-theorems/${filename}"`), `Missing Lean source link: ${filename}`);
  assert(formal.includes(algorithmEvidence.inputs.find(input => input.path === filename).sha256), `Missing recorded Lean source hash: ${filename}`);
}
assert(formal.includes(`href="${base}reports/tree-theorems.json"`), 'Missing algorithm evidence download');
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
const compilerTrust = await html(path.join(root, 'reference/compiler-trust/index.html'));
const compilerTrustText = decodeAttribute(compilerTrust.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
for (const claim of ['built-in host target', 'byte-identical copies', 'binary dependency information', 'positive control', 'not an atomic filesystem snapshot', 'not authenticated binary attestations', 'not a general plugin sandbox']) {
  assert(compilerTrustText.includes(claim), `Compiler trust guide misses its checked boundary: ${claim}`);
}
const compilerTrustCopies = [...compilerTrust.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
assert(compilerTrustCopies.some(source => source.includes('cargo test --locked -p chialisp-oxide-driver --test toolchain')), 'Compiler trust guide misses its executable native/wrapper controls');
const sourceDataDownloads = [
  { slug: 'prefix-list', path: 'puzzles/prefix-list/generated-cases.json', basename: 'prefix-list-generated-cases.json', rows: 1183 },
  { slug: 'atom-not-equal', path: 'puzzles/atom-not-equal/generated-cases.json', basename: 'atom-not-equal-generated-cases.json', rows: 2557 },
  { slug: 'insertion-sort-by-keys', path: 'puzzles/insertion-sort-by-keys/generated-cases.json', basename: 'insertion-sort-by-keys-generated-cases.json', rows: 285 },
];
const sourceDataDirectory = path.join(root, 'reports/source-data');
for (const download of sourceDataDownloads) {
  const puzzle = validation.puzzles.find(puzzle => puzzle.slug === download.slug);
  const records = puzzle.source_dependencies.filter(dependency => dependency.path === download.path);
  assert.equal(records.length, 1, `${download.path}: source data must have exactly one validated dependency`);
  const dependency = records[0];
  const source = await readFile(path.resolve(root, '..', '..', download.path));
  const published = await readFile(path.join(sourceDataDirectory, download.basename));
  assert(source.length > 0 && source.length <= 2 * 1024 * 1024, 'Source-data download exceeds the reviewed bound');
  assert.equal(source.toString('utf8'), dependency.source, 'Source-data report differs from actual source');
  assert.equal(createHash('sha256').update(source).digest('hex'), dependency.sha256, 'Source-data hash differs');
  assert.deepEqual(published, source, 'Published generated vectors differ from the original bytes');
  assert.equal(JSON.parse(published.toString('utf8')).length, download.rows, 'Generated-vector download lost cases');
  for (const route of [`lab/${download.slug}`, `tutorials/${download.slug}`]) {
    const filename = path.join(root, route, 'index.html');
    const page = await html(filename);
    assert((await readFile(filename)).length <= 512 * 1024, `${route}: generated-data page exceeds its 512KiB HTML budget`);
    assert(page.includes(`href="${base}reports/source-data/${download.basename}"`), `${route}: missing exact generated-vector download`);
    assert(page.includes(`data-source-download="${download.basename}"`) && page.includes(dependency.sha256), `${route}: source download lacks its provenance marker/hash`);
    const copies = [...page.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
    assert(!copies.includes(normalizedCode(dependency.source)), `${route}: generated vectors remain embedded in a source-copy payload`);
  }
}
assert.deepEqual((await readdir(sourceDataDirectory)).sort(), sourceDataDownloads.map(entry => entry.basename).sort(), 'Unexpected/missing generated-source data download');
let checkedSourceCopies = 0;
for (const puzzle of validation.puzzles) {
  const detail = await html(path.join(root, `lab/${puzzle.slug}/index.html`));
  for (const language of ['chialisp', 'rust', 'rue']) assert(detail.includes(`data-language="${language}"`), `${puzzle.slug}: missing ${language} side-by-side source column`);
  assert(detail.includes('Serialized-byte diffs'), `${puzzle.slug}: missing compiled-output comparison`);
  assert(detail.includes(validation.authoring.exports[puzzle.slug].entry), `${puzzle.slug}: missing actual Cargo export identity`);
  const copied = [...detail.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
  for (const [label, source] of [['Rust', puzzle.source], ['Chialisp', puzzle.reference], ['Rue', puzzle.rue.source], ...[...puzzle.source_dependencies, ...puzzle.rue.dependencies].filter(dependency => !sourceDataDownloads.some(download => download.path === dependency.path)).map(dependency => [dependency.path, dependency.source])]) {
    assert(copied.includes(normalizedCode(source)), `${puzzle.slug}: copy payload changed ${label} source`);
    checkedSourceCopies++;
  }
  for (const dependency of [...puzzle.source_dependencies, ...puzzle.rue.dependencies]) {
    assert(detail.includes(dependency.sha256), `${puzzle.slug}: missing source dependency provenance`);
    assert(detail.includes(`data-source-dependency="${dependency.path}"`), `${puzzle.slug}: missing expandable dependency source`);
    if (dependency.path.endsWith('.json')) {
      const drawer = [...detail.matchAll(/<details\b[^>]*>/g)].find(match => match[0].includes(`data-source-dependency="${dependency.path}"`))?.[0];
      assert(drawer?.includes('data-source-language="json"'), `${puzzle.slug}: JSON dependency must use the JSON code language`);
    }
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
  if (puzzle.provenance.classification === 'canonical_helper_port') {
    assert.equal(puzzle.kind, 'pure_function', `${puzzle.slug}: helper classification must preserve its evidence scope`);
    const helperProfiles = {
      'last-list-element': { environment: 'raw', source: 'The raw wrapper passes the entire environment as the single last_inner argument.', adapter: 'a raw module adapter: it passes the complete environment as one argument.' },
      'prefix-list': { environment: 'strict', source: 'The strict wrapper passes exactly two input trees to prefix: the list and an arbitrary tail.', adapter: 'a strict two-argument module adapter: it passes the list and arbitrary tail as separate arguments.' },
      'atom-not-equal': { environment: 'strict', source: 'The strict wrapper passes exactly two atom arguments to the published != helper.', adapter: 'a strict two-atom-argument module adapter: pair operands and malformed argument lists reject.' },
      'insertion-sort-by-keys': { environment: 'strict', source: 'The strict wrapper passes exactly two input trees to insertion_sort_by_keys: the existing sequence and insertions.', adapter: 'a strict two-argument module adapter: it preserves key comparisons and permissive base cases.' },
    };
    const helperProfile = helperProfiles[puzzle.slug];
    assert(helperProfile, `${puzzle.slug}: helper adapter requires an explicit rendering contract`);
    assert.equal(puzzle.environment ?? 'strict', helperProfile.environment);
    for (const label of [helperProfile.source, helperProfile.adapter]) assert(detail.includes(label), `${puzzle.slug}: incorrect helper invocation contract`);
    assert(detail.includes(`href="${base}tutorials/${puzzle.slug}/"`), `${puzzle.slug}: missing its own helper tutorial link`);
    if (puzzle.slug === 'prefix-list') {
      assert(detail.includes('over bounded list spines, arbitrary tail trees and malformed argument lists'), 'Prefix sweep must describe tree arguments, not scalar boundaries');
      assert(!detail.includes('the entire environment as the single last_inner argument'), 'Prefix adapter must not inherit the raw last_inner contract');
      assert(puzzle.source_dependencies.some(dependency => dependency.path === 'puzzles/prefix-list/generated-cases.json'), 'Prefix generated cases must remain available in the source drawer');
    }
    if (puzzle.slug === 'atom-not-equal') {
      assert(detail.includes('over exact atom bytes, lengths, padding and malformed argument lists'), 'Byte sweep must describe the exact atom contract');
      assert(!detail.includes('the entire environment as the single last_inner argument'), 'Byte adapter must not inherit the raw last_inner contract');
      assert(puzzle.source_dependencies.some(dependency => dependency.path === 'puzzles/atom-not-equal/generated-cases.json'), 'Byte generated cases must remain available in the source drawer');
    }
    assert.deepEqual(puzzle.simulator, []);
    assert.equal(puzzle.module_identity, undefined, 'Helper adapters must not acquire a published standalone-module identity');
    for (const label of ['published helper adapter', 'Original helper + module adapter', 'explicit source wrapper', 'no published module-hash compatibility claim']) {
      assert(detail.includes(label), `${puzzle.slug}: missing adapter provenance label ${label}`);
    }
    assert(!detail.includes('This original teaching fixture'), `${puzzle.slug}: published helper misclassified as original teaching`);
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
const listTutorial = await html(path.join(root, 'tutorials/last-list-element/index.html'));
const listPuzzle = validation.puzzles.find(puzzle => puzzle.slug === 'last-list-element');
assert.equal(listPuzzle?.provenance.classification, 'canonical_helper_port', 'Missing executed published list-helper adapter');
const listCopies = [...listTutorial.matchAll(/\bdata-code="([^"]*)"/g)]
  .map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
for (const language of ['chialisp', 'rust', 'rue']) {
  assert(listTutorial.includes(`data-language="${language}"`), `List tutorial misses ${language} comparison`);
}
for (const source of [listPuzzle.source, listPuzzle.reference, listPuzzle.rue.source]) {
  assert(listCopies.includes(normalizedCode(source)), 'List tutorial copy differs from executed source');
  checkedSourceCopies++;
}
for (const scope of ['lines 8–13', 'Apache-2.0', 'module adapter', 'no published module-hash claim', '0x0000', 'host bound', 'reject all runtime function tables']) {
  assert(listTutorial.includes(scope), `List tutorial misses scope: ${scope}`);
}
const byteTutorial = await html(path.join(root, 'tutorials/atom-not-equal/index.html'));
const bytePuzzle = validation.puzzles.find(puzzle => puzzle.slug === 'atom-not-equal');
assert.equal(bytePuzzle?.provenance.classification, 'canonical_helper_port');
assert.equal(bytePuzzle.cases.length, 32);
assert.equal(bytePuzzle.generated_cases, 2557);
const byteCopies = [...byteTutorial.matchAll(/\bdata-code="([^"]*)"/g)]
  .map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
for (const language of ['chialisp', 'rust', 'rue']) assert(byteTutorial.includes(`data-language="${language}"`), `Byte tutorial misses ${language} comparison`);
for (const source of [bytePuzzle.source, bytePuzzle.reference, bytePuzzle.rue.source]) {
  assert(byteCopies.includes(normalizedCode(source)), 'Byte tutorial copy differs from executed source');
  checkedSourceCopies++;
}
for (const scope of ['8,192', 'PartialEq', 'strict two-argument wrapper', 'no trailing newline', 'false result is successful evaluation', 'does not emit spend conditions', 'constant-time', 'compiler preservation']) {
  assert(byteTutorial.includes(scope), `Byte tutorial misses scope: ${scope}`);
}
const byteGuide = await html(path.join(root, 'verification/byte-proofs/index.html'));
const byteModelIds = ['original', 'operator_eq', 'self_inequality', 'constant_true', 'constant_false', 'swap_operands', 'loose_environment', 'fixed_length_narrowing'];
const byteQueryIds = ['abi_nonvacuity', 'acceptance_nonvacuity', 'true_nonvacuity', 'false_nonvacuity', 'counterexample'];
const byteSourceControls = byteModelIds.slice(1, 6);
const bytePrefix = 'proofs/atom-not-equal/';
const byteFiles = byteModelIds.flatMap(name => [`${name}.ir.json`, ...byteQueryIds.map(query => `${name}-${query}.smt2`), `${name}-off.clvm.hex`, `${name}-on.clvm.hex`]).sort();
assert.equal(byteFiles.length, 64);
assert.equal(byteProofs.vm_failure_protocol, failureProtocol);
assert.equal(byteProofs.scope.kind, 'source_derived_structural_boolean_ir_model');
const { entry: byteEntry, ...byteInputs } = byteProofs.authoring;
assert.equal(byteEntry, validation.authoring.exports['atom-not-equal'].entry);
assert.deepEqual(byteInputs, validation.authoring, 'Byte proof/validation Cargo inputs differ');
assert.deepEqual(byteProofs.models.map(model => model.id), byteModelIds, 'Byte model coverage/order');
assert.deepEqual([...byteGuide.matchAll(/data-byte-proof-model="([^"]+)"/g)].map(match => match[1]), byteModelIds, 'Displayed byte model coverage/order');
assert.deepEqual(byteProofs.source_lineage.controls.map(control => control.id), byteSourceControls, 'Actual source control coverage');
assert.deepEqual(bytePuzzle.artifacts.map(artifact => [artifact.backend, artifact.optimize_lir]), [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]], 'Byte proof needs exactly five independently compiled language modes');
assert.deepEqual(byteProofs.artifacts.map(artifact => artifact.path), byteFiles.map(name => bytePrefix + name), 'Byte proof artifact set/order');
assert.equal(byteProofs.specification.path, `${bytePrefix}specification.json`);
const byteDownloads = [...byteProofs.artifacts, byteProofs.specification];
assert.equal(byteDownloads.length, 65);
for (const artifact of byteDownloads) {
  assert.match(artifact.sha256, /^[0-9a-f]{64}$/, 'Byte proof artifact hash');
  const basename = path.basename(artifact.path);
  assert.equal(artifact.path, bytePrefix + basename, 'Byte artifact path must be confined');
  const published = await readFile(path.join(root, 'reports/byte-proofs', basename));
  const source = await readFile(new URL(`../../${artifact.path}`, import.meta.url));
  assert.deepEqual(published, source, `${basename}: published byte artifact differs from source`);
  assert.equal(createHash('sha256').update(published).digest('hex'), artifact.sha256, `${basename}: byte artifact hash`);
  assert(byteGuide.includes(`href="${base}reports/byte-proofs/${basename}"`) && byteGuide.includes(artifact.sha256), `${basename}: missing byte-proof download/hash`);
}
assert.deepEqual((await readdir(path.join(root, 'reports/byte-proofs'))).sort(), [...byteFiles, 'specification.json'].sort(), 'Unexpected/missing byte-proof downloads');
const byteGuideCopies = [...byteGuide.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
assert(byteGuideCopies.includes(normalizedCode(bytePuzzle.source)), 'Byte proof guide changed actual Rust source copy');
checkedSourceCopies++;
assert.equal(byteProofs.source_lineage.base.source, 'puzzles/atom-not-equal/puzzle.rs');
assert(byteGuide.includes(`data-byte-proof-source="${byteProofs.source_lineage.base.source}"`), 'Missing byte-proof source identity');
assert.equal(createHash('sha256').update(await readFile(new URL(`../../${byteProofs.source_lineage.base.source}`, import.meta.url))).digest('hex'), byteProofs.source_lineage.base.sha256);
for (const control of byteProofs.source_lineage.controls) {
  assert.equal(control.source, `experiments/byte-proofs/src/${control.id}.rs`);
  assert.equal(control.entry, `chialisp_oxide_byte_proofs::${control.id}::puzzle`);
  assert.equal(control.role, control.id === 'swap_operands' ? 'positive_equivalence_control' : 'negative_mutant');
  assert.deepEqual(byteProofs.control_authoring.exports[control.id], { entry: control.entry, source: control.source });
  const actual = await readFile(new URL(`../../${control.source}`, import.meta.url));
  assert.equal(createHash('sha256').update(actual).digest('hex'), control.source_sha256, `${control.id}: source-control bytes`);
  assert.equal(createHash('sha256').update(control.patch.text).digest('hex'), control.patch.sha256, `${control.id}: source-control patch`);
  assert(byteGuideCopies.includes(normalizedCode(control.patch.text)), `${control.id}: copied source patch changed`);
  assert(byteGuide.includes(`href="https://github.com/judeallred/chialisp-oxide/blob/main/${control.source}"`), `${control.id}: missing actual source link`);
  checkedSourceCopies++;
}
const byteBoolean = value => ({ status: 'accepted', output_hex: value ? '01' : '80' });
function byteTree(value, budget = { nodes: 20_000, bytes: 1_048_576 }, depth = 0) {
  assert(value && typeof value === 'object' && !Array.isArray(value) && --budget.nodes >= 0 && depth <= 128, 'Malformed/oversized byte witness tree');
  if (value.kind === 'atom') {
    assert.deepEqual(Object.keys(value).sort(), ['hex', 'kind']);
    assert(typeof value.hex === 'string' && /^(?:[0-9a-f]{2})*$/.test(value.hex), 'Malformed byte witness atom');
    budget.bytes -= value.hex.length / 2;
    assert(budget.bytes >= 0, 'Byte witness exceeds finite replay byte bound');
    return Buffer.from(value.hex, 'hex');
  }
  assert.equal(value.kind, 'pair');
  assert.deepEqual(Object.keys(value).sort(), ['first', 'kind', 'rest']);
  return [byteTree(value.first, budget, depth + 1), byteTree(value.rest, budget, depth + 1)];
}
function byteArguments(environment) {
  if (!Array.isArray(environment) || !Buffer.isBuffer(environment[0]) || !Array.isArray(environment[1])) return null;
  const [right, tail] = environment[1];
  return Buffer.isBuffer(right) && Buffer.isBuffer(tail) && tail.length === 0 ? [environment[0], right] : null;
}
let byteGroups = 0, byteSourceGroups = 0, byteNativeControls = 0, byteOriginalRuns = 0, byteModeledRuns = 0, byteRetainedRuns = 0;
for (const model of byteProofs.models) {
  const actualSource = ['original', ...byteSourceControls].includes(model.id);
  const proven = ['original', 'swap_operands'].includes(model.id);
  assert.equal(model.origin, actualSource ? 'actual_cargo_source' : 'synthetic_abi_ir_control');
  assert.equal(model.result.status, proven ? 'proven' : 'refuted');
  assert.equal(model.ir, `${bytePrefix}${model.id}.ir.json`);
  assert.deepEqual(model.queries, Object.fromEntries(byteQueryIds.map(query => [query, `${bytePrefix}${model.id}-${query}.smt2`])));
  assert.deepEqual(model.artifacts.map(artifact => artifact.optimize), [false, true], `${model.id}: missing compiled mode`);
  for (const artifact of model.artifacts) {
    assert.equal(typeof artifact.optimize, 'boolean');
    assert(Number.isSafeInteger(artifact.bytes) && artifact.bytes > 0 && /^(?:[0-9a-f]{2})+$/.test(artifact.clvm_hex));
    assert.equal(artifact.clvm_hex.length / 2, artifact.bytes);
    assert.match(artifact.tree_hash, /^[0-9a-f]{64}$/);
    const mode = artifact.optimize ? 'on' : 'off';
    assert.equal(await readFile(path.join(root, `reports/byte-proofs/${model.id}-${mode}.clvm.hex`), 'utf8'), artifact.clvm_hex + '\n');
  }
  const roles = proven ? ['abi_nonvacuity', 'acceptance_nonvacuity', 'true_witness', 'false_witness'] : ['counterexample'];
  const seen = [];
  for (const group of model.replays) {
    assert(group.roles.length > 0);
    for (const role of group.roles) {
      assert(roles.includes(role), `${model.id}: unexpected witness role`);
      assert.deepEqual(group.witness, model.result[role], `${model.id}: disconnected solver witness`);
      seen.push(role);
    }
    const environment = byteTree(group.witness.environment);
    const args = byteArguments(environment);
    const desired = args ? byteBoolean(!args[0].equals(args[1])) : { status: 'rejected' };
    assert.deepEqual(group.expected_original, desired);
    assert.deepEqual(checkNativeOutcome(group.original.native), desired);
    assert.deepEqual(group.original.runs.map(run => [run.backend, run.optimize_lir]), bytePuzzle.artifacts.map(artifact => [artifact.backend, artifact.optimize_lir]));
    for (const run of group.original.runs) assert.deepEqual(checkExecution(run.execution), desired);
    let wanted = desired;
    if (byteSourceControls.includes(model.id)) {
      assert(args, `${model.id}: native source control requires actual atom arguments`);
      wanted = byteBoolean(({ operator_eq: args[0].equals(args[1]), self_inequality: false, constant_true: true, constant_false: false, swap_operands: !args[1].equals(args[0]) })[model.id]);
      assert.deepEqual(checkNativeOutcome(group.native_control), wanted);
      byteNativeControls++;
    } else {
      assert.equal(group.native_control, null, 'Synthetic/original IR cannot claim mutant native execution');
      if (model.id === 'fixed_length_narrowing') {
        assert(args?.some(value => value.length !== 2));
        wanted = { status: 'rejected' };
      } else if (model.id === 'loose_environment') {
        assert.equal(desired.status, 'rejected');
        assert(Array.isArray(environment) && Buffer.isBuffer(environment[0]) && Array.isArray(environment[1]) && Buffer.isBuffer(environment[1][0]));
        wanted = byteBoolean(!environment[0].equals(environment[1][0]));
      }
    }
    assert.deepEqual(group.expected_modeled, wanted);
    if (proven) assert.deepEqual(wanted, desired); else assert.notDeepEqual(wanted, desired);
    assert.deepEqual(group.modeled.map(run => run.optimize), [false, true], `${model.id}: missing replay mode`);
    for (const run of group.modeled) {
      assert.equal(typeof run.optimize, 'boolean');
      assert.deepEqual(checkReplay(run.outcome), wanted);
      assert.deepEqual(checkExecution(run.retained_execution), wanted);
      assert.equal(run.artifact, `${bytePrefix}${model.id}-${run.optimize ? 'on' : 'off'}.clvm.hex`);
      if (wanted.status === 'accepted') assert.equal(run.outcome.cost, run.retained_execution.cost);
      else assert.deepEqual(run.outcome.failure, run.retained_execution.failure);
      byteModeledRuns++; byteRetainedRuns++;
    }
    byteGroups++; byteOriginalRuns += group.original.runs.length;
    if (actualSource) byteSourceGroups++;
  }
  assert.deepEqual(seen.sort(), roles.sort(), `${model.id}: missing/duplicate solver witness replay`);
  const modelTag = [...byteGuide.matchAll(/<details\b[^>]*>/g)].find(match => match[0].includes(`data-byte-proof-model="${model.id}"`))?.[0];
  assert(modelTag?.includes(`data-byte-proof-origin="${model.origin}"`) && modelTag.includes(`data-byte-proof-status="${model.result.status}"`), `${model.id}: missing displayed model classification`);
  assert.equal([...byteGuide.matchAll(new RegExp(`data-byte-proof-replay="${model.id}"`, 'g'))].length, model.replays.length, `${model.id}: missing displayed witness row`);
}
const byteVmRuns = byteOriginalRuns + byteModeledRuns + byteRetainedRuns;
for (const label of [`${byteGroups} replay groups`, `${byteSourceGroups} actual-source groups`, `${byteVmRuns} VM executions`, `${byteOriginalRuns} original-language runs`, `${byteModeledRuns} internal modeled-IR runs`, `${byteRetainedRuns} retained-file runs`, `${byteNativeControls} additional native source-control calls`, 'data-byte-proof-evidence', 'data-byte-proof-downloads', 'Download 65', 'not prove rustc extraction', 'standard-library primitive bridge', 'installed compiler and standard library remain trusted']) {
  assert(byteGuide.includes(label), `Byte proof guide misses measured evidence/scope: ${label}`);
}
for (const route of ['reports/byte-proofs.json', 'tutorials/atom-not-equal/', 'verification/error-study/', 'verification/formal/', 'reference/supported-rust/']) assert(byteGuide.includes(`href="${base}${route}"`), `Byte proof guide misses link: ${route}`);
assert(byteGuide.includes('href="https://github.com/judeallred/chialisp-oxide/blob/main/docs/toolchain-trust.md"'), 'Missing explicit compiler/stdlib trust notes');
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
  for (const term of ['recovery', 'canonical', 'AGG_SIG_ME', 'CoinId', 'from_mojos', 'create_coin', 'chialisp_oxide', 'impossible', 'currying', 'p2_conditions', 'equivalence', 'rightmost', 'recursion', 'last_inner', 'elision', 'prefix', 'Lean', 'inequality', 'nonvacuity', 'sysroot', 'insertion_sort_by_keys', 'bitvectors']) {
    const search = await pagefind.search(term);
    assert(search.results.length > 0, `No production search results for ${term}`);
    const result = await search.results[0].data();
    assert(result.url.startsWith(base), `Search result escapes base: ${result.url}`);
    if (term === 'rightmost') {
      const matches = await Promise.all(search.results.slice(0, 10).map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/recursive-trees/`), 'Recursive tutorial is not discoverable in search');
    }
    if (term === 'last_inner') {
      const matches = await Promise.all(search.results.slice(0, 10).map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/last-list-element/`), 'Published helper tutorial is not discoverable in search');
    }
    if (term === 'elision') {
      const matches = await Promise.all(search.results.slice(0, 10).map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/recursive-trees/` || row.url === `${base}verification/method/`), 'Runtime-frame documentation is not discoverable in search');
    }
    if (term === 'prefix') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      for (const route of ['tutorials/prefix-list/', 'lab/prefix-list/']) {
        assert(matches.some(row => row.url === `${base}${route}`), `Prefix source/lesson is not discoverable in search: ${route}`);
      }
    }
    if (term === 'Lean') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}verification/formal/`), 'Lean evidence is not discoverable in search');
    }
    if (term === 'inequality') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      for (const route of ['tutorials/atom-not-equal/', 'lab/atom-not-equal/']) {
        assert(matches.some(row => row.url === `${base}${route}`), `Byte source/lesson is not discoverable in search: ${route}`);
      }
    }
    if (term === 'nonvacuity') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}verification/byte-proofs/`), 'Source-derived byte proof guide is not discoverable in search');
    }
    if (term === 'sysroot') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}reference/compiler-trust/`), 'Compiler trust and standard-library boundary is not discoverable in search');
    }
    if (term === 'bitvectors') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}verification/formal/`), 'Fixed-width mask models are not discoverable in search');
    }
    if (term === 'insertion_sort_by_keys') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      for (const route of ['tutorials/insertion-sort-by-keys/', 'lab/insertion-sort-by-keys/']) {
        assert(matches.some(row => row.url === `${base}${route}`), `Published byte-order helper is not discoverable in search: ${route}`);
      }
    }
    console.log(`Search ${JSON.stringify(term)}: ${search.results.length} results; first ${result.url}`);
  }
  await pagefind.destroy();
} finally {
  globalThis.fetch = originalFetch;
}

console.log(`PASS: ${pages.length} pages, ${checkedLinks} local links/assets/fragments, ${checkedSourceCopies} original source-copy payloads, 9 JSON reports, ${structuralBasenames.length} exact structural downloads, ${byteDownloads.length} exact byte-proof downloads, ${sourceDataDownloads.length} exact source-data downloads with ${sourceDataDownloads.length * 2} 512KiB page budgets, ${byteVmRuns} typed byte-proof VM observations, and live Pagefind queries`);
