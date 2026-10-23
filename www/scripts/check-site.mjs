import { readFile, readdir, stat } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { failureProtocol, checkCorpusExecutions, checkExecution, checkNativeOutcome, checkReplay, checkScalarProofReplays, checkTreeProofReplays, testFailureProtocol } from './vm-failures.mjs';
import { checkLedgerReport, testLedgerProtocol } from './ledger-rejections.mjs';
import { checkProtocolReport, checkProtocolDisplay, historicalOptimizationReport } from './p2-protocol.mjs';
import { loadAcyclicProofEvidence, checkAcyclicDisplay } from '../src/lib/acyclic-proof-artifacts.mjs';
import { loadP2CostEvidence } from '../src/lib/p2-costs.mjs';
import { loadCompositionCostEvidence } from '../src/lib/composition-costs.mjs';
import { checkDelegatedReport, checkDelegatedDisplay } from './p2dc-protocol.mjs';
import { checkBoomerangReport, checkBoomerangDisplay } from './boomerang-protocol.mjs';
import { readBoomerangControls, checkBoomerangControlsDisplay, controlPacketRoot } from '../src/lib/boomerang-source-controls.mjs';
import { siteFiles } from '../src/lib/site-files.mjs';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const base = '/chialisp-oxide/';
const origin = 'https://judeallred.github.io';
const compositionCosts = await loadCompositionCostEvidence();
const historicalCosts = await loadP2CostEvidence();
const sourceCheckpointBytes = await readFile(path.join(root, 'reports/p2-source-cost/validation.json'));
assert.equal(createHash('sha256').update(sourceCheckpointBytes).digest('hex'), historicalCosts.context.reportSHA);
const sourceCheckpoint = JSON.parse(sourceCheckpointBytes.toString('utf8'));
const compositionRaw = [
  ['profile.json','corpus/runtime-composition-baseline.json'],
  ['costs.json','corpus/runtime-composition-costs.json'],
  ['controls.json','corpus/runtime-composition-controls.json'],
];
for (const [name, source] of compositionRaw) assert.deepEqual(await readFile(path.join(root, 'reports/runtime-composition', name)), await readFile(new URL(`../../${source}`, import.meta.url)), 'Composed compiler raw download must equal the current reviewed source bytes');
const historicalGuardedPins = { 'profile.json':'66dc316cf447bd123b11bb33c3a4568a7be28f64d31b7947ebedf9ed3e614b54', 'costs.json':'878307b1082c5e6ae95ae8ffb5c5d31a1efb3c985dea50b378206a6c215934d4', 'controls.json':'c73b3cbde014d8ff6caf05a37bfacc939af5820c86ca58fb4944e3387828ba93' };
for (const [name, pin] of Object.entries(historicalGuardedPins)) assert.equal(createHash('sha256').update(await readFile(path.join(root,'reports/guarded-projection',name))).digest('hex'), pin, 'Historical guarded candidate bytes changed');
assert.equal(JSON.parse(await readFile(path.join(root,'reports/guarded-projection/profile.json'),'utf8')).status, 'pending_signed_checkpoint_and_capture_review', 'Historical guarded v1 was never promoted');


const built = (await siteFiles(root)).map(file => path.join(root, file));
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
  assert.equal([...text.matchAll(/<h1(?:\s|>)/g)].length, 1, `${file}: expected exactly one page h1`);
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

const publishedReports = [
  ...['validation', 'proofs', 'tree-proofs', 'byte-proofs', 'inventory', 'tutorial-examples', 'published-puzzles', 'error-study'].map(name => [name, `corpus/${name}.json`]),
  ['tree-theorems', 'experiments/tree-theorems/results.json'],
  ['cost-optimization-baseline', 'corpus/cost-optimization-baseline.json'],
  ['helper-inlining-baseline', 'corpus/helper-inlining-baseline.json'],
  ['runtime-refinement-baseline', 'corpus/runtime-refinement-baseline.json'],
  ['integer-canonicality-proof', 'corpus/integer-canonicality-proof.json'],
  ['p2-singleton-append-baseline', 'corpus/p2-singleton-append-baseline.json'],
  ['acyclic-scalar', 'proofs/acyclic-scalar-source/result.json'],
  ['spend-costs', 'experiments/spend-costs/results.json'],
];
for (const [name, source] of publishedReports) {
  const report = JSON.parse(await readFile(path.join(root, `reports/${name}.json`), 'utf8'));
  assert.equal(report.schema_version, 1, `${name}: report schema`);
  assert.deepEqual(report, JSON.parse(await readFile(new URL(`../../${source}`, import.meta.url), 'utf8')), `${name}: published report differs from current repository evidence`);
  if (['validation', 'cost-optimization-baseline', 'helper-inlining-baseline', 'runtime-refinement-baseline', 'integer-canonicality-proof', 'p2-singleton-append-baseline', 'acyclic-scalar', 'spend-costs'].includes(name)) {
    assert.deepEqual(await readFile(path.join(root, `reports/${name}.json`)), await readFile(new URL(`../../${source}`, import.meta.url)), `${name}: download must preserve exact source bytes for its recorded SHA-256`);
  }
}
const validation = JSON.parse(await readFile(path.join(root, 'reports/validation.json'), 'utf8'));
const proofs = JSON.parse(await readFile(path.join(root, 'reports/proofs.json'), 'utf8'));
const treeProofs = JSON.parse(await readFile(path.join(root, 'reports/tree-proofs.json'), 'utf8'));
const byteProofs = JSON.parse(await readFile(path.join(root, 'reports/byte-proofs.json'), 'utf8'));
checkCorpusExecutions(validation);
checkLedgerReport(validation);
const dedicatedCounts = checkProtocolReport(validation);
const delegatedCounts = checkDelegatedReport(validation);
const boomerangCounts = checkBoomerangReport(validation);
const optimizationValidation = historicalOptimizationReport(validation);
console.log(`Consensus rejection controls: ${testLedgerProtocol()} passed`);
checkScalarProofReplays(proofs);
checkTreeProofReplays(treeProofs);
console.log(`Typed failure controls: ${testFailureProtocol()} passed`);
const formal = await html(path.join(root, 'verification/formal/index.html'));
const acyclic = await loadAcyclicProofEvidence();
const acyclicGuide = await html(path.join(root, 'verification/acyclic-scalar/index.html'));
const acyclicDisplay = checkAcyclicDisplay(acyclicGuide, acyclic.report, acyclic.artifacts, acyclic.sources);
for (const artifact of acyclic.artifacts) {
  assert.deepEqual(await readFile(path.join(root, 'reports/acyclic-scalar', artifact.basename)), artifact.bytes,
    `${artifact.basename}: acyclic download differs from the checked retained artifact`);
}
assert.deepEqual((await readdir(path.join(root, 'reports/acyclic-scalar'))).sort(),
  acyclic.artifacts.map(artifact => artifact.basename).sort(), 'Unexpected/missing acyclic downloads');
assert(formal.includes(`href="${base}verification/acyclic-scalar/"`), 'Formal guide misses actual-source scalar lesson');
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
const plainText = value => decodeAttribute(value.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim();
const attribute = (tag, name) => tag.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
function checkSolutionPreviews(markup, values, label) {
  const decodedMarkup = decodeAttribute(markup);
  const previews = [...markup.matchAll(/<code\b([^>]*)>([\s\S]*?)<\/code>/g)]
    .filter(match => attribute(match[1], 'class')?.split(/\s+/).includes('solution'));
  assert.equal(previews.length, values.length, `${label}: missing/extra solution previews`);
  for (const [index, value] of values.entries()) {
    const [, tag, contents] = previews[index];
    const truncated = value.length > 512;
    const expected = truncated ? `${value.slice(0, 384)} … ${value.slice(-64)}` : value;
    assert(!/<[^>]+>/.test(contents), `${label}/${index}: solution preview must remain escaped text`);
    assert.equal(decodeAttribute(contents), expected, `${label}/${index}: changed preview prefix/suffix or exact input`);
    assert(/\bdata-pagefind-ignore(?:=|\s|$)/.test(tag), `${label}/${index}: raw input must not pollute search`);
    if (truncated) {
      assert.equal(attribute(tag, 'data-solution-preview'), String(value.length), `${label}/${index}: missing/wrong original length`);
      assert(markup.includes(`Preview of ${new Intl.NumberFormat('en-US').format(value.length)} characters. The validation JSON retains the exact input.`), `${label}/${index}: truncation/full-report scope is missing`);
      assert(!decodedMarkup.includes(value), `${label}/${index}: full long input bypassed its bounded preview`);
    } else {
      assert(!/\bdata-solution-preview\b/.test(tag), `${label}/${index}: short input is incorrectly marked truncated`);
    }
  }
}
// Challenge the checker itself: boundary values are accepted, while changed
// prefix/suffix/length, missing rows and hidden complete inputs must fail.
const escapePreview = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const previewFixture = value => `<code class="solution" data-pagefind-ignore${value.length > 512 ? ` data-solution-preview="${value.length}"` : ''}>${escapePreview(value.length > 512 ? `${value.slice(0, 384)} … ${value.slice(-64)}` : value)}</code>${value.length > 512 ? `<small>Preview of ${new Intl.NumberFormat('en-US').format(value.length)} characters. The validation JSON retains the exact input.</small>` : ''}`;
for (const value of ['', '<&>', 'x'.repeat(512), 'x'.repeat(513), 'x'.repeat(384) + 'middle'.repeat(12) + 'z'.repeat(64)]) checkSolutionPreviews(previewFixture(value), [value], 'preview positive control');
const longPreview = 'p'.repeat(384) + 'hidden-middle'.repeat(6) + 's'.repeat(64);
const goodPreview = previewFixture(longPreview);
let previewControls = 0;
for (const [name, bad] of [
  ['prefix', goodPreview.replace('p'.repeat(384), 'q' + 'p'.repeat(383))],
  ['suffix', goodPreview.replace('s'.repeat(64), 's'.repeat(63) + 't')],
  ['original length', goodPreview.replace(`data-solution-preview="${longPreview.length}"`, `data-solution-preview="${longPreview.length + 1}"`)],
  ['missing length', goodPreview.replace(` data-solution-preview="${longPreview.length}"`, '')],
  ['missing search exclusion', goodPreview.replace(' data-pagefind-ignore', '')],
  ['hidden full input', goodPreview + `<span>${longPreview}</span>`],
  ['missing row', ''],
  ['duplicate row', goodPreview + goodPreview],
]) {
  assert.notEqual(bad, goodPreview, `No-op preview control: ${name}`);
  assert.throws(() => checkSolutionPreviews(bad, [longPreview], name), undefined, `Accepted corrupt preview: ${name}`);
  previewControls++;
}
// Expressive Code normalizes surrounding/trailing whitespace, but wrapping
// must not add newlines or otherwise rewrite the copied program text.
const normalizedCode = source => source.replaceAll('\t', '  ').split(/\r?\n/).map(line => line.trimEnd()).join('\n').trim();
let checkedSourceCopies = 0;
for (const [route, source, expectedBlocks] of [
  ['reference/optional-lists', 'docs/optional-lists.md', 2],
  ['reference/agent-guide', 'docs/llms.md', 1],
]) {
  const page = await html(path.join(root, route, 'index.html'));
  const guide = await readFile(new URL(`../../${source}`, import.meta.url), 'utf8');
  const blocks = [...guide.matchAll(/^```rust\n([\s\S]*?)^```/gm)].map(match => normalizedCode(match[1]));
  assert.equal(blocks.length, expectedBlocks, `${route}: changed shared Rust example count`);
  const copies = [...page.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
  for (const block of blocks) {
    assert(copies.includes(block), `${route}: rendered Rust copy differs from the shared guide`);
    checkedSourceCopies++;
  }
}
const costPredecessorBytes = await readFile(path.join(root, 'reports/cost-optimization-baseline.json'));
assert.equal(createHash('sha256').update(costPredecessorBytes).digest('hex'), '484168ef0aad4c53808ff37f5300288bb022dde58748e34db03e61b4f5834308', 'Historical cost baseline bytes changed');
const costPredecessor = JSON.parse(costPredecessorBytes.toString('utf8'));
const costHelperBytes = await readFile(path.join(root, 'reports/helper-inlining-baseline.json'));
assert.equal(createHash('sha256').update(costHelperBytes).digest('hex'), '0b7326f83065ab1bc186a1e9b0d3b5c581fa7cc3110845cc4b6db70a39b5da52', 'Historical helper-inlining baseline bytes changed');
const costHelper = JSON.parse(costHelperBytes.toString('utf8'));
const costBaseline = JSON.parse(await readFile(path.join(root, 'reports/runtime-refinement-baseline.json'), 'utf8'));
const costBefore = costHelper.reviewed_after.puzzles;
const costReview = costBaseline.reviewed_after;
assert.equal(costPredecessor.protocol, 'chialisp-oxide-cost-migration/v1');
assert.equal(costPredecessor.reviewed_after.status, 'reviewed_final');
assert.equal(costHelper.protocol, 'chialisp-oxide-helper-inlining/v1');
assert.equal(costHelper.baseline.commit, '405792113242aed8d94d3dc4805ed68fc667b3d1');
assert.equal(costHelper.baseline.report_sha256, 'edaeebaebe368b72bb471c37b0fd608d1000f5a83b1f8defc9c8ec6eb04b468d');
assert.deepEqual(costHelper.predecessor, { path: 'corpus/cost-optimization-baseline.json', sha256: createHash('sha256').update(costPredecessorBytes).digest('hex') });
assert.equal(costPredecessor.reviewed_after.report_sha256, costHelper.baseline.report_sha256);
assert.deepEqual(costHelper.optimization_pipeline.before, costPredecessor.reviewed_after.optimization_pipeline);
assert.equal(costHelper.reviewed_after.status, 'reviewed_final');
assert.equal(costBaseline.protocol, 'chialisp-oxide-runtime-refinement/v1');
assert.equal(costBaseline.baseline.repository, 'https://github.com/judeallred/chialisp-oxide');
assert.equal(costBaseline.baseline.commit, 'c87957e2d28c832ec76551aa0c5e16e31f973b2f');
assert.equal(costBaseline.baseline.report_sha256, 'c3448703f6912d94900b2d012aabb610f1f2efc174f19699094fd43229f4e1ac');
assert.equal(costBaseline.baseline.report_git_blob_sha1, '38c14f911eb6462044fd8bf0d0b11d6404457614');
assert.deepEqual(costBaseline.predecessor, { path: 'corpus/helper-inlining-baseline.json', sha256: createHash('sha256').update(costHelperBytes).digest('hex') });
assert.equal(costHelper.reviewed_after.report_sha256, costBaseline.baseline.report_sha256);
assert.deepEqual(costBaseline.optimization_pipeline.before, costHelper.reviewed_after.optimization_pipeline);
assert.deepEqual(costBaseline.optimization_pipeline.after, sourceCheckpoint.optimization_pipeline);
assert.deepEqual(costHelper.rue_local_patches.before, [108, 109, 110, 111, 112, 113].map(number => `https://github.com/xch-dev/rue/issues/${number}`));
assert.equal(costHelper.rue_local_patches.status, 'reviewed');
assert.deepEqual(costHelper.rue_local_patches.added, ['https://github.com/xch-dev/rue/issues/114']);
assert(!costHelper.rue_local_patches.before.includes(costHelper.rue_local_patches.added[0]));
assert.deepEqual(validation.rue_local_patches, [...costHelper.rue_local_patches.before, ...costHelper.rue_local_patches.added]);
// Pending profiles never substitute scratch measurements for a publication.
assert.equal(costReview.status, 'reviewed_final', 'Optimization measurements must be final reviewed evidence');
assert.equal(costReview.report_path, 'corpus/validation.json');
const appendProfile = JSON.parse(await readFile(path.join(root, 'reports/p2-singleton-append-baseline.json'), 'utf8'));
assert.equal(appendProfile.reviewed_after.status, 'reviewed_final_current22', 'P2 append remains pending');
const sourceCostProfile = JSON.parse(await readFile(path.join(root, 'reports/p2-source-cost/profile.json'), 'utf8'));
assert.equal(sourceCostProfile.status, 'reviewed_final_source_cost', 'P2 source-cost capture remains pending');
assert.equal(sourceCostProfile.baseline.report_sha256, appendProfile.reviewed_after.report_sha256, 'Source-cost must retain the signed current22 append report');
assert.equal(sourceCostProfile.reviewed_after.report_sha256, createHash('sha256').update(sourceCheckpointBytes).digest('hex'), 'Historical source-only checkpoint report is stale');
assert.equal(compositionCosts.context.reportSHA, sourceCostProfile.reviewed_after.report_sha256);
assert.equal(compositionCosts.currentReportSHA, createHash('sha256').update(await readFile(path.join(root, 'reports/p2dc-append/before-validation.json'))).digest('hex'));
assert.equal(compositionCosts.activeReportSHA, createHash('sha256').update(await readFile(path.join(root, 'reports/validation.json'))).digest('hex'));
assert.equal(costReview.report_sha256, appendProfile.baseline.report_sha256, 'Historical optimization report differs from signed append baseline');
assert.deepEqual(costReview.optimization_pipeline, sourceCheckpoint.optimization_pipeline);
assert.equal(costBefore.length, 21, 'The signed c879 baseline must retain its entire reviewed corpus');
// The immutable earliest cost snapshot owns the Cargo authoring metadata;
// helper-inlining and runtime-refinement profiles retain observation slots.
assert.deepEqual(costPredecessor.authoring, optimizationValidation.authoring, 'Optimization comparisons changed their Cargo source inputs');
assert.deepEqual(costBefore.map(row => row.slug), optimizationValidation.puzzles.map(row => row.slug), 'Missing/extra/reordered optimization puzzle');
assert.deepEqual(costReview.puzzles.map(row => row.slug), costBefore.map(row => row.slug), 'Missing/extra/reordered reviewed optimization row');
const safeMetric = (value, minimum, label) => assert(Number.isSafeInteger(value) && value >= minimum, `${label}: expected a safe integer >= ${minimum}`);
const costArtifact = (record, label) => {
  assert.equal(record.backend, 'rust_mir', `${label}: expected actual Rust artifact`);
  assert.equal(record.optimize_lir, true, `${label}: expected exact optimized mode`);
  safeMetric(record.bytes, 1, `${label} bytes`);
  assert.match(record.clvm_hex, /^(?:[0-9a-f]{2})+$/, `${label}: malformed artifact bytes`);
  assert.equal(record.bytes, record.clvm_hex.length / 2, `${label}: artifact size differs from serialized program`);
  assert.match(record.tree_hash, /^[0-9a-f]{64}$/, `${label}: malformed tree hash`);
  return { backend: record.backend, optimize_lir: record.optimize_lir, bytes: record.bytes, clvm_hex: record.clvm_hex, tree_hash: record.tree_hash };
};
const costRows = [];
for (const [index, before] of costBefore.entries()) {
  const puzzle = optimizationValidation.puzzles[index];
  const reviewed = costReview.puzzles[index];
  const source = costPredecessor.authoring.exports[before.slug]?.source;
  assert.equal(source, `puzzles/${before.slug}/puzzle.rs`, `${before.slug}: unreviewed cost source path`);
  const sourceBytes = await readFile(new URL(`../../${source}`, import.meta.url));
  const sourceHash = createHash('sha256').update(sourceBytes).digest('hex');
  assert.equal(puzzle.source, sourceBytes.toString('utf8'), `${before.slug}: cost row uses stale or different source`);
  assert.equal(costPredecessor.authoring.sources.find(row => row.path === source)?.sha256, sourceHash, `${before.slug}: cost input hash changed`);
  const afterArtifacts = puzzle.artifacts.filter(artifact => artifact.backend === 'rust_mir' && artifact.optimize_lir === true);
  assert.equal(afterArtifacts.length, 1, `${before.slug}: missing/duplicate optimized Rust artifact`);
  const after = afterArtifacts[0];
  costArtifact(before.artifact, `${before.slug} before`);
  assert.deepEqual(costArtifact(reviewed.artifact, `${before.slug} reviewed`), costArtifact(after, `${before.slug} current`), `${before.slug}: current artifact differs from final review`);
  for (const records of [before.cases, reviewed.cases]) {
    assert.deepEqual(records.map(row => row.label), puzzle.cases.map(row => row.label), `${before.slug}: missing/extra/reordered measured input`);
    for (const [caseIndex, record] of records.entries()) {
      assert.equal(record.solution_sha256, createHash('sha256').update(puzzle.cases[caseIndex].solution).digest('hex'), `${before.slug}/${record.label}: comparison input changed`);
      checkExecution(record.execution);
    }
  }
  for (const [caseIndex, record] of reviewed.cases.entries()) {
    const runs = puzzle.cases[caseIndex].runs.filter(run => run.backend === 'rust_mir' && run.optimize_lir === true);
    assert.equal(runs.length, 1, `${before.slug}/${record.label}: missing/duplicate actual measurement`);
    assert.deepEqual(record.execution, runs[0].execution, `${before.slug}/${record.label}: execution differs from final review`);
  }
  const sampleIndex = before.cases.findIndex(row => row.execution.outcome.status === 'accepted');
  assert(sampleIndex >= 0, `${before.slug}: missing accepted before input`);
  const sample = before.cases[sampleIndex];
  const current = reviewed.cases[sampleIndex].execution;
  assert.equal(current.outcome.status, 'accepted', `${before.slug}: displayed input changed acceptance`);
  assert.equal(current.outcome.output_hex, sample.execution.outcome.output_hex, `${before.slug}: displayed output bytes changed`);
  assert.deepEqual(puzzle.cases[sampleIndex].native, current.outcome, `${before.slug}: displayed VM/native outcome differs`);
  for (const [label, value] of [['before', sample.execution.cost], ['now', current.cost]]) safeMetric(value, 0, `${before.slug}/${label} cost`);
  costRows.push({ slug: puzzle.slug, title: puzzle.title, label: sample.label, values: [before.artifact.bytes, after.bytes, sample.execution.cost, current.cost] });
}
const acceptedCostDeltas = costBefore.flatMap((before, index) => before.cases.flatMap((sample, caseIndex) => {
  if (sample.execution.outcome.status !== 'accepted') return [];
  const after = costReview.puzzles[index].cases[caseIndex].execution;
  assert.deepEqual(after.outcome, sample.execution.outcome, `${before.slug}/${sample.label}: accepted-cost summary changes outcome`);
  safeMetric(sample.execution.cost, 0, `${before.slug}/${sample.label} before cost`);
  safeMetric(after.cost, 0, `${before.slug}/${sample.label} current cost`);
  return [after.cost - sample.execution.cost];
}));
const optimizationFilename = path.join(root, 'reference/optimization/index.html');
const optimizationGuide = await html(optimizationFilename);
assert((await readFile(optimizationFilename)).length <= 512 * 1024, 'Optimization guide exceeds its 512KiB HTML budget');
const optimizationText = plainText(optimizationGuide);
for (const label of ['Successful execution gives useful facts', 'only facts valid on every successful incoming branch', 'A rejecting branch has no successful continuation', 'A callee starts a fresh scope', 'removing the earlier guard', 'an unused call, SHA computation or checked projection still runs', 'not averages or a universal performance bound', 'no general compiler-preservation proof']) assert(optimizationText.includes(label), `Optimization guide loses its claim boundary: ${label}`);
for (const label of ['Recursive functions keep their calls', 'Every argument becomes an ordered lexical binding', 'an unused argument that rejects, overflows, hashes or diverges', 'Callee locals receive fresh identities', 'Only typed compiler-limit failures', 'compilation retries the previous optimized program', 'Type errors and other compiler errors remain errors', 'Public optimize() results', 'serialized and compiled again']) assert(optimizationText.includes(label), `Helper-inlining guide loses its strictness or serialization boundary: ${label}`);
for (const route of ['reports/runtime-refinement-baseline.json', 'reports/helper-inlining-baseline.json', 'reports/cost-optimization-baseline.json', 'reports/integer-canonicality-proof.json', 'reports/validation.json']) assert(optimizationGuide.includes(`href="${base}${route}"`), `Optimization guide misses evidence download: ${route}`);
assert(optimizationGuide.includes(`href="${costBaseline.baseline.repository}/commit/${costBaseline.baseline.commit}"`), 'Optimization guide lost its exact signed baseline source link');
const summaries = [...optimizationGuide.matchAll(/<p\b[^>]*data-cost-summary[^>]*>/g)];
assert.equal(summaries.length, 1, 'Missing/duplicate accepted-cost summary');
for (const [name, expected] of Object.entries({ accepted: acceptedCostDeltas.length, lower: acceptedCostDeltas.filter(delta => delta < 0).length, equal: acceptedCostDeltas.filter(delta => delta === 0).length, higher: acceptedCostDeltas.filter(delta => delta > 0).length })) {
  assert(summaries[0][0].includes(`data-${name}="${expected}"`), `Accepted-cost summary differs from complete measurements: ${name}`);
}
const canonicality = JSON.parse(await readFile(path.join(root, 'reports/integer-canonicality-proof.json'), 'utf8'));
assert.equal(canonicality.protocol, 'chialisp-oxide-integer-canonicality/v1');
assert.equal(canonicality.baseline.queries.length, 14);
assert.equal(canonicality.model_source_mutations.length, 3);
assert(optimizationGuide.includes('data-integer-canonicality="chialisp-oxide-integer-canonicality/v1"'), 'Missing recorded canonicality evidence');
assert(optimizationText.includes(canonicality.solver.version), 'Canonicality solver metadata not displayed');
assert(optimizationText.includes(canonicality.scope.decoder_source_binding), 'Canonicality source binding overstated');
for (const claim of ['Refine checked integer intervals', 'Decode canonical integers without repeating the body', 'final operation remains checked', 'budget exhaustion skips this optional pass', 'not compiler mutations', 'without running Z3', 'does not prove actual CLVM opcodes', 'complete transaction', 'generator encoding', 'exact transaction costs need their own simulator or consensus measurement']) {
  assert(optimizationText.includes(claim), `Interval/decoder guide loses its execution or model scope: ${claim}`);
}
for (const name of ['equivalence', 'valid-nil', 'valid-positive-128', 'valid-negative-129', 'reject-00', 'reject-0001', 'reject-ffff', 'reject-ff80', 'reject-007f00']) {
  const query = canonicality.baseline.queries.find(query => query.name === name);
  assert(query && query.expected_status === query.observed_status, `Canonicality display lacks retained query: ${name}`);
}
const counterSource = validation.puzzles.find(puzzle => puzzle.slug === 'bounded-counter');
assert(counterSource, 'Missing measured counter source');
const optimizationCopies = [...optimizationGuide.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
for (const [language, source] of [['Rust', counterSource.source], ['Chialisp', counterSource.reference], ['Rue', counterSource.rue.source]]) {
  assert(optimizationCopies.includes(normalizedCode(source)), `Optimization tutorial copy payload differs from executed ${language} counter source`);
  checkedSourceCopies++;
}
function checkCostRows(markup, rows) {
  const table = [...markup.matchAll(/<table\b[^>]*>([\s\S]*?)<\/table>/g)]
    .filter(match => plainText(match[1].match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? '') === 'Optimized Rust before and after interval and decoder refinement: first accepted named input per puzzle');
  assert.equal(table.length, 1, 'Missing/duplicate optimization measurement table');
  const headers = [...table[0][1].matchAll(/<th\b[^>]*scope="col"[^>]*>([\s\S]*?)<\/th>/g)].map(match => plainText(match[1]));
  assert.deepEqual(headers, ['Puzzle / input', 'Before bytes', 'Now bytes', 'Before VM cost', 'Now VM cost'], 'Optimization table columns changed');
  const records = [...table[0][1].matchAll(/<tr\b[^>]*data-cost-puzzle="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];
  assert.deepEqual(records.map(row => row[1]), rows.map(row => row.slug), 'Missing/extra/reordered displayed cost row');
  for (const [index, row] of rows.entries()) {
    const markup = records[index][2];
    const headers = [...markup.matchAll(/<th\b[^>]*scope="row"[^>]*>([\s\S]*?)<\/th>/g)];
    assert.equal(headers.length, 1, `${row.slug}: missing cost row header`);
    assert.equal(plainText(headers[0][1]), `${row.title} ${row.label}`, `${row.slug}: displayed puzzle/input changed`);
    assert(headers[0][1].includes(`href="${base}lab/${row.slug}/"`), `${row.slug}: missing complete source/measurement link`);
    const values = [...markup.matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)].map(match => plainText(match[1]));
    assert.deepEqual(values, row.values.map(value => new Intl.NumberFormat('en-US').format(value)), `${row.slug}: displayed byte sizes or costs differ from actual observations`);
  }
  return records;
}
const costMarkupRows = checkCostRows(optimizationGuide, costRows);
// Check the rendered-row assertion against actual non-noop corruptions; these
// do not substitute a mock renderer or execute a second set of VM programs.
let costDisplayControls = 0;
const firstCostRow = costMarkupRows[0][0];
const changedCostCells = [...firstCostRow.matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)];
for (const [label, bad] of [
  ['missing row', optimizationGuide.replace(firstCostRow, '')],
  ['duplicate row', optimizationGuide.replace(firstCostRow, firstCostRow + firstCostRow)],
  ['row order', optimizationGuide.replace(firstCostRow, '__COST_ROW__').replace(costMarkupRows[1][0], firstCostRow).replace('__COST_ROW__', costMarkupRows[1][0])],
  ['input label', optimizationGuide.replace(firstCostRow, firstCostRow.replace(/<small\b[^>]*>[\s\S]*?<\/small>/, '<small>changed input</small>'))],
  ['source link', optimizationGuide.replace(firstCostRow, firstCostRow.replace(`href="${base}lab/${costRows[0].slug}/"`, `href="${base}lab/password-coin/"`))],
  ...changedCostCells.map((cell, index) => [`metric ${index}`, optimizationGuide.replace(firstCostRow, firstCostRow.replace(cell[0], `<td>${new Intl.NumberFormat('en-US').format(costRows[0].values[index] + 1)}</td>`))]),
]) {
  assert.notEqual(bad, optimizationGuide, `No-op cost display control: ${label}`);
  assert.throws(() => checkCostRows(bad, costRows), undefined, `Accepted corrupt optimization display: ${label}`);
  costDisplayControls++;
}
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
  { slug: 'sha256tree', path: 'puzzles/sha256tree/generated-cases.json', basename: 'sha256tree-generated-cases.json', rows: 275 },
  { slug: 'p2-singleton', path: 'puzzles/p2-singleton/vectors.json', basename: 'p2-singleton-vectors.json', rows: 178, manifest: true },
  { slug: 'p2-singleton', path: 'puzzles/p2-singleton/generated-cases.json', basename: 'p2-singleton-generated-cases.json', rows: 128 },
  { slug: 'password-coin', path: 'puzzles/password-coin/generated-cases.json', basename: 'password-coin-generated-cases.json', rows: 256 },
  { slug: 'p2-delegated-conditions', tutorial: 'delegated-conditions', path: 'puzzles/p2-delegated-conditions/independent-vectors.json', basename: 'p2-delegated-conditions-independent-vectors.json', rows: 200, independent: true },
];
const sourceDataDirectory = path.join(root, 'reports/source-data');
const sourceBudgetRoutes = new Set(['homepage']);
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
  const parsedSourceData = JSON.parse(published.toString('utf8'));
  assert.equal(Array.isArray(parsedSourceData) ? parsedSourceData.length : download.independent ? parsedSourceData.accepted.length + parsedSourceData.rejected.length : parsedSourceData.named.length + parsedSourceData.generated.length, download.rows, 'Vector download lost recipes');
  if (download.manifest) {
    assert.deepEqual(parsedSourceData.named.map(({label,solution}) => ({label,solution})), puzzle.cases.map(({label,solution}) => ({label,solution})), 'Relation manifest changes exact recorded named inputs');
    assert.equal(parsedSourceData.checks.status, 'expected_relation_codec_and_hash_arithmetic_no_VM_invocation');
  }
  else if(download.independent) assert.equal(puzzle.generated_cases, download.rows - puzzle.cases.length + 2059, 'Independent vector replays must not double-count the named inputs');
  else assert.equal(puzzle.generated_cases, download.rows + (['sha256tree', 'password-coin', 'p2-singleton'].includes(download.slug) ? 2059 : 0), `${download.slug}: inherited raw inputs were dropped or counted in the download`);
  for (const route of [`lab/${download.slug}`, `tutorials/${download.tutorial ?? download.slug}`]) {
    const filename = path.join(root, route, 'index.html');
    const page = await html(filename);
    sourceBudgetRoutes.add(route);
    const pageLimit = route === 'lab/p2-singleton' ? 1024 * 1024 : 512 * 1024;
    assert((await readFile(filename)).length <= pageLimit, `${route}: source-data page exceeds its reviewed HTML budget`);
    assert(page.includes(`href="${base}reports/source-data/${download.basename}"`), `${route}: missing exact generated-vector download`);
    assert(page.includes(`data-source-download="${download.basename}"`) && page.includes(dependency.sha256), `${route}: source download lacks its provenance marker/hash`);
    const copies = [...page.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
    assert(!copies.includes(normalizedCode(dependency.source)), `${route}: generated vectors remain embedded in a source-copy payload`);
  }
}
assert.deepEqual((await readdir(sourceDataDirectory)).sort(), sourceDataDownloads.map(entry => entry.basename).sort(), 'Unexpected/missing generated-source data download');
for (const puzzle of validation.puzzles) {
  const detail = await html(path.join(root, `lab/${puzzle.slug}/index.html`));
  const detailText = decodeAttribute(detail.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
  checkSolutionPreviews(detail, [
    ...puzzle.cases.map(test => test.solution),
    ...(puzzle.resource_controls?.cases ?? []).map(test => test.solution),
    ...(puzzle.curried_instances?.cases ?? []).map(test => test.solution),
    ...(puzzle.curried_examples ?? []).flatMap(example => example.cases.map(test => test.solution)),
    ...(puzzle.curried_instances?.ledger ?? []).flatMap(comparison => comparison.attempts.map(attempt => attempt.solution)),
  ], `${puzzle.slug}: exact report input previews`);
  const comparisonBudget = new Intl.NumberFormat('en-US').format(puzzle.validation_max_cost ?? 1_000_000);
  assert(detailText.includes(`Every build uses a ceiling of ${comparisonBudget} cost units per input`), `${puzzle.slug}: missing actual comparison budget`);
  const resourceSection = detail.match(/<section[^>]*aria-label="Lower-budget resource controls"[^>]*>([\s\S]*?)<\/section>/)?.[1];
  if (puzzle.resource_controls) {
    assert(resourceSection, `${puzzle.slug}: missing separate resource controls`);
    const controls = puzzle.resource_controls;
    const observations = controls.cases.reduce((count, test) => count + test.runs.length, 0);
    const resourceText = decodeAttribute(resourceSection.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ');
    assert(resourceText.includes(`${observations} observations below are separate controls, excluded from the named and generated coverage totals`), `${puzzle.slug}: resource controls inflate passing coverage`);
    assert(resourceText.includes(`${new Intl.NumberFormat('en-US').format(controls.max_cost)} cost ceiling`), `${puzzle.slug}: wrong resource-control budget`);
    const displayedStatuses = [...resourceSection.matchAll(/<span\b[^>]*class="outcome[^\"]*"[^>]*>([a-z_]+)<\/span>/g)].map(match => match[1]);
    assert.deepEqual(displayedStatuses, controls.cases.flatMap(test => [test.native.status, ...test.runs.map(run => run.execution.outcome.status)]), `${puzzle.slug}: displayed resource outcomes differ from actual typed observations`);
    for (const test of controls.cases) assert(resourceText.includes(test.label), `${puzzle.slug}: missing resource-control case ${test.label}`);
    assert(resourceText.includes('inconclusive / cost_exceeded'), `${puzzle.slug}: resource failure must not become policy rejection`);
  } else {
    assert.equal(resourceSection, undefined, `${puzzle.slug}: invented resource evidence`);
  }
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
    const curryRelation = puzzle.factory_checks.some(row => row.relation === 'standard_curry');
    assert(puzzle.factory_checks.every(row => (row.relation === 'standard_curry') === curryRelation), `${puzzle.slug}: mixed factory relations`);
    const labels = curryRelation
      ? ['Environment contract: strict typed arguments.', 'returns a program', 'Apply the produced programs', 'Expected program', 'uncurried environment']
      : ['Environment contract: raw tree.', 'returns a program', 'Published module identity', 'Apply the produced programs', 'Expected payload'];
    for (const label of labels) {
      assert(detail.includes(label), `${puzzle.slug}: missing factory contract/evidence label ${label}`);
    }
    assert(puzzle.factory_checks.length > 0 && (curryRelation || puzzle.module_identity), `${puzzle.slug}: missing factory evidence`);
    assert.deepEqual(puzzle.simulator, [], 'A factory must not claim direct ledger scenarios');
    if (curryRelation) {
      assert.equal(puzzle.provenance.classification, 'original teaching implementation', `${puzzle.slug}: a curry factory is not a canonical module port`);
      assert.equal(puzzle.module_identity, undefined, `${puzzle.slug}: a curry factory claims no published module identity`);
      for (const row of puzzle.factory_checks) {
        assert(row.application_runs.length > 0 && row.application_runs.every(run => run.expected_outcome && JSON.stringify(run.execution.outcome) === JSON.stringify(run.expected_outcome)), `${puzzle.slug}: curry applications must carry and match their independent expectation`);
      }
    } else {
      for (const backend of ['rust_mir', 'chialisp', 'rue_source']) {
        const artifact = puzzle.artifacts.find(row => row.backend === backend && row.optimize_lir);
        assert.equal(artifact.clvm_hex, puzzle.module_identity.upstream_hex, `${backend}: canonical factory bytes must match`);
        assert.equal(artifact.tree_hash, puzzle.module_identity.upstream_tree_hash, `${backend}: canonical factory hash must match`);
      }
      const tutorial = await html(path.join(root, 'tutorials/p2-conditions/index.html'));
      assert(tutorial.includes(puzzle.module_identity.upstream_tree_hash), 'Tutorial must bind the published module identity');
    }
    const applications = puzzle.factory_checks.reduce((n, row) => n + row.application_runs.length, 0);
    assert(detail.includes(`${applications} second-stage VM executions`), 'Factory application count must come from the report');
  }
  if (puzzle.provenance.classification === 'canonical_helper_port') {
    assert.equal(puzzle.kind, 'pure_function', `${puzzle.slug}: helper classification must preserve its evidence scope`);
    const helperProfiles = {
      'last-list-element': { environment: 'raw', source: 'The raw wrapper passes the entire environment as the single last_inner argument.', adapter: 'a raw module adapter: it passes the complete environment as one argument.' },
      'prefix-list': { environment: 'strict', source: 'The strict wrapper passes exactly two input trees to prefix: the list and an arbitrary tail.', adapter: 'a strict two-argument module adapter: it passes the list and arbitrary tail as separate arguments.' },
      'atom-not-equal': { environment: 'strict', source: 'The strict wrapper passes exactly two atom arguments to the published != helper.', adapter: 'a strict two-atom-argument module adapter: pair operands and malformed argument lists reject.' },
      'insertion-sort-by-keys': { environment: 'strict', source: 'The strict wrapper passes exactly two input trees to insertion_sort_by_keys: the existing sequence and insertions.', adapter: 'a strict two-argument module adapter: it preserves key comparisons and permissive base cases.' },
      'sha256tree': { environment: 'raw', source: 'The raw wrapper passes the entire environment as the single sha256tree argument.', adapter: 'a raw module adapter: it hashes the complete environment as one tree without numeric or list decoding.' },
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
// Password modules are reusable raw templates. Fixed commitments and actual
// spend attempts have their own evidence contract and do not inflate ordinary
// Dedicated component receipts remain separate from ordinary scenario groups.
const p2Puzzle = validation.puzzles.find(puzzle => puzzle.slug === 'p2-singleton');
assert(p2Puzzle, 'Missing actual P2 component');
const p2Detail = await html(path.join(root, 'lab/p2-singleton/index.html'));
const p2Lesson = await html(path.join(root, 'tutorials/p2-singleton/index.html'));
assert.deepEqual(checkProtocolDisplay(p2Detail, p2Puzzle), dedicatedCounts);
assert.deepEqual(checkProtocolDisplay(p2Lesson, p2Puzzle), dedicatedCounts);
assert.deepEqual(JSON.parse(await readFile(path.join(root, 'reports/p2-singleton.json'), 'utf8')), p2Puzzle.protocol_validation, 'Derived P2 endpoint must equal the exact embedded field');
const delegatedPuzzle = validation.puzzles.find(puzzle => puzzle.slug === 'p2-delegated-conditions');
assert(delegatedPuzzle,'Missing actual current delegated-conditions component');
assert.deepEqual(JSON.parse(await readFile(path.join(root,'reports/p2-delegated-conditions.json'),'utf8')),delegatedPuzzle.protocol_validation,'Derived delegated endpoint must equal the exact embedded field');
for(const route of ['tutorials/delegated-conditions','lab/p2-delegated-conditions'])checkDelegatedDisplay(await html(path.join(root,route,'index.html')),delegatedPuzzle);
const boomerangPuzzle = validation.puzzles.find(puzzle => puzzle.slug === 'boomerang');
assert(boomerangPuzzle, 'Missing actual boomerang capture');
const boomerangControls=await readBoomerangControls();assert(boomerangControls,'Actual admitted source-control packet required');
for (const route of ['tutorials/boomerang','lab/boomerang']) {
  const filename=path.join(root,route,'index.html'), markup=await html(filename);
  assert.deepEqual(checkBoomerangDisplay(markup,boomerangPuzzle),boomerangCounts);
  assert.deepEqual(checkBoomerangControlsDisplay(markup,boomerangControls),boomerangControls.counts);
  assert((await readFile(filename)).length<=512*1024,`${route}: source/evidence page exceeds its 512KiB HTML budget`);
  assert(markup.includes('WIP prototype.')&&markup.includes('Not ready for use.'),'Missing boomerang prototype warning');
  const copies=[...markup.matchAll(/\bdata-code="([^"]*)"/g)].map(match=>decodeAttribute(match[1]).replaceAll('\x7f','\n'));
  for(const source of [boomerangPuzzle.source,boomerangPuzzle.reference,boomerangPuzzle.rue.source])assert(copies.includes(normalizedCode(source)),`${route}: exact Rust/Chialisp/Rue source-copy changed`);
  // preserveIndent keeps the recipe's leading and internal source indentation;
  // Expressive Code omits only its terminating newline from the copy payload.
  for(const control of boomerangControls.controls.slice(1))for(const edit of control.edits)for(const source of [edit.before,edit.after])assert(copies.includes(source.trimEnd()),`${route}: actual compiled source-mutation copy changed`);
}
for(const file of ['report.json',...boomerangControls.report.files.map(row=>row.path)])assert.deepEqual(
  await readFile(path.join(root,'reports/boomerang-source-controls',file)),await readFile(path.join(controlPacketRoot,file)),
  'Published boomerang source-control packet bytes changed');
assert.deepEqual(JSON.parse(await readFile(path.join(root,'reports/boomerang.json'),'utf8')),boomerangPuzzle.coin_instances,'Derived boomerang endpoint must equal the exact embedded field');
const boomerangDownloads=boomerangPuzzle.coin_instances.examples[0].instances.map(({artifact})=>{
  const filename=`boomerang--${artifact.backend}--${artifact.optimize_lir?'on':'off'}.clvm.hex`;
  return [filename,artifact.clvm_hex+'\n'];
});
assert.deepEqual((await readdir(path.join(root,'reports/boomerang'))).sort(),boomerangDownloads.map(([filename])=>filename).sort());
for(const [filename,expected]of boomerangDownloads)assert.equal(await readFile(path.join(root,'reports/boomerang',filename),'utf8'),expected,'Committed boomerang reveal download changed');
const outerDownloads=[['profile.json','corpus/p2-delegated-conditions-append-baseline.json'],['before-validation.json','corpus/checkpoints/runtime-composition-validation.json'],['p2-costs.json','corpus/p2dc-append-existing-p2-costs.json'],['p2-controls.json','corpus/p2dc-append-existing-p2-controls.json']];
for(const [basename,source]of outerDownloads)assert.deepEqual(await readFile(path.join(root,'reports/p2dc-append',basename)),await readFile(new URL(`../../${source}`,import.meta.url)),'Outer raw download bytes changed');
assert.deepEqual((await readdir(path.join(root,'reports/p2dc-append'))).sort(),outerDownloads.map(([name])=>name).sort());

const p2Display = p2Detail.match(/<section\b[^>]*data-protocol-validation[^>]*>[\s\S]*?<\/section>/)[0];
let protocolDisplayControls = 0;
for (const mutate of [
  text => text.replace('data-protocol-attempts="110"', 'data-protocol-attempts="109"'),
  text => text.replace('data-protocol-accepted="15"', 'data-protocol-accepted="95"'),
  text => text.replace('data-protocol-rejected="95"', 'data-protocol-rejected="94"'),
  text => text.replace('data-protocol-mode="rust_mir:false"', 'data-protocol-mode="rust_mir:true"'),
  text => text.replace('data-protocol-rejection="AssertPuzzleAnnouncementFailed"', 'data-protocol-rejection="CostExceeded"'),
  text => text.replace('data-protocol-receipt="genesis"', 'data-protocol-receipt="synthetic"'),
  text => text.replace('host counterparts, not Rust ports', 'Rust ports'),
]) {
  checkProtocolDisplay(p2Display, p2Puzzle); // positive normalization first
  const bad = mutate(p2Display);
  assert.notEqual(bad, p2Display, 'No-op P2 display corruption');
  assert.throws(() => checkProtocolDisplay(bad, p2Puzzle));
  protocolDisplayControls++;
}

// ledger scenario counts or turn a runtime umbrella into semantic rejection.
const passwordRows = validation.puzzles.filter(puzzle => puzzle.slug === 'password-coin');
assert.equal(passwordRows.length, 1, 'Missing/duplicate executed password module');
const passwordPuzzle = passwordRows[0];
const passwordDetail = await html(path.join(root, 'lab/password-coin/index.html'));
const passwordTutorial = await html(path.join(root, 'tutorials/password-coin/index.html'));
const passwordText = plainText(passwordDetail);
const passwordLessonText = plainText(passwordTutorial);
const instance = passwordPuzzle.curried_instances;
assert(instance && instance.protocol === 'chialisp-oxide-password-instances/v1', 'Missing dedicated fixed-commitment evidence');
assert.deepEqual(passwordPuzzle.simulator, [], 'Raw password template must not claim locked-coin ledger groups');
assert.deepEqual(passwordPuzzle.simulator_comparisons, [], 'Raw template must keep instance spends separate');
assert.equal(passwordPuzzle.environment, 'raw');
assert.equal(passwordPuzzle.reference_mode ?? 'module', 'module');
assert.equal(instance.scheme, '(a (q . M) (c (q . H) 1))');
assert.equal(instance.constructor, 'private standard-tree constructor using clvmr 0.21.0; no clvm-utils call');
assert.equal(instance.source_role, 'algorithm-reference provenance, not an executed library dependency');
assert.equal(instance.fixed_argument, 'a0' + createHash('sha256').update('abc').digest('hex'), 'The fixed public commitment changed');
const passwordModes = [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]];
const checkPasswordModes = (rows, label) => {
  assert(rows.every(row => typeof row.optimize_lir === 'boolean'), `${label}: nonboolean mode`);
  assert.deepEqual(rows.map(row => [row.backend, row.optimize_lir]), passwordModes, `${label}: missing/duplicate/reordered mode`);
};
checkPasswordModes(passwordPuzzle.artifacts, 'Password raw modules');
checkPasswordModes(instance.artifacts.map(row => row.artifact), 'Fixed instances');
assert.equal(instance.artifacts.length, 5);
for (const [index, record] of instance.artifacts.entries()) {
  const module = passwordPuzzle.artifacts[index];
  assert.equal(record.module_tree_hash, module.tree_hash, 'Instance is detached from its own language module');
  assert.equal(record.artifact.clvm_hex, `ff02ffff01${module.clvm_hex}ffff04ffff01${instance.fixed_argument}ff018080`, 'Fixed instance does not quote its exact module and argument');
  assert.equal(record.artifact.bytes, record.artifact.clvm_hex.length / 2);
  assert.notEqual(record.artifact.tree_hash, module.tree_hash, 'Module/instance identities were conflated');
}
const instanceSectionMatches = [...passwordDetail.matchAll(/<section\b[^>]*data-instance-protocol="([^"]+)"[^>]*>([\s\S]*?)<\/section>/g)];
assert.equal(instanceSectionMatches.length, 1, 'Missing/extra fixed-instance section');
assert.equal(instanceSectionMatches[0][1], instance.protocol);
const instanceSection = instanceSectionMatches[0][2];
const instanceText = plainText(instanceSection);
const numberMarker = (name, expected) => {
  const matches = [...instanceSection.matchAll(new RegExp(`\\b${name}="([^"]*)"`, 'g'))];
  assert.equal(matches.length, 1, `Missing/duplicate instance marker ${name}`);
  assert.equal(matches[0][1], String(expected), `${name}: rendered count differs from actual observations`);
};
assert.equal(instance.cases.length, 48);
const fixedRuns = instance.cases.reduce((count, test) => count + test.runs.length, 0);
assert.equal(fixedRuns, 240);
numberMarker('data-instance-case-count', 48);
numberMarker('data-instance-vm-count', fixedRuns);
const buildRows = [...instanceSection.matchAll(/<tr\b[^>]*data-instance-build="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];
assert.deepEqual(buildRows.map(row => row[1]), passwordModes.map(([backend, optimize]) => `${backend}/${optimize}`));
for (const [index, row] of buildRows.entries()) {
  const record = instance.artifacts[index];
  assert(row[2].includes(record.module_tree_hash) && row[2].includes(record.artifact.tree_hash), 'Displayed module/instance identity differs from the record');
  assert(plainText(row[2]).includes(new Intl.NumberFormat('en-US').format(record.artifact.bytes)), 'Displayed instance byte count differs');
}
const fixedTable = [...instanceSection.matchAll(/<table\b[^>]*>([\s\S]*?)<\/table>/g)]
  .find(match => plainText(match[1].match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? '') === 'Actual instance solutions, native outcomes and five compiled builds')?.[1];
assert(fixedTable, 'Missing fixed-hash input/outcome matrix');
const fixedRows = [...fixedTable.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)].slice(1);
assert.equal(fixedRows.length, 48);
for (const [index, test] of instance.cases.entries()) {
  checkNativeOutcome(test.native, `fixed-H/${test.label}`);
  checkPasswordModes(test.runs, `fixed-H/${test.label}`);
  const cells = [...fixedRows[index][1].matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)].map(match => plainText(match[1]));
  assert.equal(cells.length, 6);
  assert.equal(cells[0], test.native.status, 'Displayed fixed-H native result changed');
  for (const [mode, run] of test.runs.entries()) {
    assert.deepEqual(checkExecution(run.execution), test.native, 'Fixed-H VM/native outcome differs');
    if (run.execution.cost !== null) assert(run.execution.cost <= 1_000_000);
    assert.equal(cells[mode + 1], `${run.execution.outcome.status} ${run.execution.cost === null ? 'no successful cost' : `${new Intl.NumberFormat('en-US').format(run.execution.cost)} cost`}`, 'Displayed fixed-H VM result/cost changed');
  }
}
assert.equal(instance.varying_arguments.length, 3);
const varyingRuns = instance.varying_arguments.reduce((count, row) => count + row.runs.length, 0);
assert.equal(varyingRuns, 15);
numberMarker('data-instance-varying-count', 3);
numberMarker('data-instance-varying-vm-count', varyingRuns);
const varyingTable = [...instanceSection.matchAll(/<table\b[^>]*>([\s\S]*?)<\/table>/g)]
  .find(match => plainText(match[1].match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/)?.[1] ?? '') === 'Nonvacuous controls for the host curry constructor')?.[1];
assert(varyingTable, 'Missing nonvacuous varying-H matrix');
const varyingRows = [...varyingTable.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)].slice(1);
assert.equal(varyingRows.length, 3);
for (const [index, control] of instance.varying_arguments.entries()) {
  assert.equal(control.password_hex, ['', '616263', '0000'][index], 'Varying-H controls must keep their three distinct byte messages');
  assert.equal(control.argument_hex, 'a0' + createHash('sha256').update(Buffer.from(control.password_hex, 'hex')).digest('hex'));
  assert.equal(control.native.status, 'accepted');
  checkNativeOutcome(control.native);
  checkPasswordModes(control.runs.map(run => run.artifact), 'Varying-H artifacts');
  for (const [mode, run] of control.runs.entries()) {
    assert.equal(run.module_tree_hash, passwordPuzzle.artifacts[mode].tree_hash);
    assert.equal(run.artifact.clvm_hex, `ff02ffff01${passwordPuzzle.artifacts[mode].clvm_hex}ffff04ffff01${control.argument_hex}ff018080`);
    assert.deepEqual(checkExecution(run.execution), control.native, 'Varying-H output differs');
  }
  const cells = [...varyingRows[index][1].matchAll(/<t[hd]\b[^>]*>([\s\S]*?)<\/t[hd]>/g)].map(match => plainText(match[1]));
  assert.deepEqual(cells, [control.password_hex || 'empty atom', control.argument_hex, '5 / 5 builds'], 'Displayed varying-H bytes/results differ from the actual controls');
}
const ledgerModes = [['rust_mir', true], ['chialisp', true], ['rue_source', true]];
assert(instance.ledger.every(row => typeof row.optimize_lir === 'boolean'));
assert.deepEqual(instance.ledger.map(row => [row.backend, row.optimize_lir]), ledgerModes);
const instanceAttempts = instance.ledger.flatMap(row => row.attempts);
assert.equal(instanceAttempts.length, 21);
const acceptedInstances = instanceAttempts.filter(row => row.consensus.status === 'accepted').length;
const semanticInstances = instanceAttempts.filter(row => row.consensus.semantic_rejection_credit).length;
const umbrellaInstances = instanceAttempts.filter(row => row.consensus.status === 'runtime_umbrella').length;
assert.equal(acceptedInstances, 6);
assert.equal(semanticInstances, 12);
assert.equal(umbrellaInstances, 3);
for (const [name, count] of [['data-instance-submissions', 21], ['data-instance-accepted', 6], ['data-instance-semantic-rejections', 12], ['data-instance-umbrellas', 3]]) numberMarker(name, count);
const consensusCodes = [null, null, 'GeneratorRuntimeError', 'InvalidPuzzleHash', 'InvalidCoinAmount', 'InvalidCondition', 'WrongPuzzleHash'];
for (const comparison of instance.ledger) {
  assert.equal(comparison.transaction_attempts, 7);
  assert.equal(comparison.attempts.length, 7);
  const section = instanceSection.match(new RegExp(`<details[^>]*data-instance-ledger="${comparison.backend}"[^>]*>([\\s\\S]*?)</details>`))?.[1];
  assert(section, `Missing ${comparison.backend} instance submissions`);
  for (const version of ['chia-sdk-test 0.36.0', 'chia-consensus 0.36.1', 'clvmr 0.16.2', 'TESTNET11', 'clvmr 0.21.0']) assert(plainText(section).includes(version), 'Instance primary/consensus VM versions were conflated');
  const rows = [...section.matchAll(/<tr\b([^>]*data-instance-attempt="[^"]+"[^>]*)>([\s\S]*?)<\/tr>/g)];
  assert.equal(rows.length, 7);
  for (const [index, attempt] of comparison.attempts.entries()) {
    const status = index < 2 ? 'accepted' : index === 2 ? 'runtime_umbrella' : 'consensus_rejected';
    assert.equal(typeof attempt.consensus.semantic_rejection_credit, 'boolean');
    assert.equal(attempt.consensus.semantic_rejection_credit, index >= 3, 'Umbrella or accepted spend received semantic rejection credit');
    assert.equal(attempt.consensus.status, status);
    assert.equal(attempt.consensus.code, consensusCodes[index]);
    assert.equal(typeof attempt.full_state_unchanged, 'boolean');
    assert.equal(attempt.full_state_unchanged, index >= 2);
    if (index >= 2) assert.deepEqual(attempt.after, attempt.before, 'Rejected instance changed recorded state');
    const observed = checkExecution(attempt.vm_021);
    if (index === 2) assert.deepEqual(attempt.vm_021.failure, { kind: 'program', code: 'raise' });
    else assert.equal(observed.status, 'accepted', 'Malformed conditions/reveal must evaluate before consensus');
    assert.equal(decodeAttribute(attribute(rows[index][1], 'data-instance-attempt')), attempt.label);
    assert.equal(attribute(rows[index][1], 'data-instance-result'), status);
    const cells = [...rows[index][2].matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)].map(match => plainText(match[1]));
    assert.equal(cells.length, 4);
    assert.equal(cells[0], `${observed.status} ${attempt.vm_021.cost === null ? attempt.vm_021.failure.code : `${new Intl.NumberFormat('en-US').format(attempt.vm_021.cost)} cost`}`);
    assert.equal(cells[1], `${status}${consensusCodes[index] ? ` ${consensusCodes[index]}` : ''}${index === 2 ? ' Observation only; no semantic rejection credit.' : ''}`);
    assert.equal(cells[2], attempt.full_state_unchanged ? 'unchanged' : 'spend applied');
    assert.equal(cells[3], attempt.reveal_tree_hash);
    assert(rows[index][2].includes(attempt.coin_id), 'Instance row lost its actual input coin id');
  }
  assert.equal(comparison.attempts[0].coin_id, comparison.attempts[1].coin_id, 'Unsigned retargeting must begin with the identical coin');
  assert.notEqual(comparison.attempts[6].reveal_tree_hash, comparison.instance_tree_hash, 'Wrong-reveal control did not change identity');
}
for (const label of ['separate from the report’s ordinary ledger scenario-group totals', 'no semantic rejection credit', 'not an executed library dependency', 'does not claim that the reference library’s builder executed']) assert(instanceText.includes(label), `Instance evidence loses claim boundary: ${label}`);
assert(passwordText.includes('raw module alone receives no password-locked spend credit'), 'Raw template is incorrectly credited with locked-coin spends');
for (const page of [passwordDetail, passwordTutorial]) {
  assert(page.includes('Original Chialisp module') && page.includes('no source wrapper'), 'Password source must be labeled as an unchanged complete module');
  const copies = [...page.matchAll(/\bdata-code="([^"]*)"/g)].map(match => decodeAttribute(match[1]).replaceAll('\x7f', '\n'));
  for (const source of [passwordPuzzle.source, passwordPuzzle.reference, passwordPuzzle.rue.source]) assert(copies.includes(normalizedCode(source)), 'Password source copy differs from the actual executed program');
}
for (const scope of ['trusted expected hash', 'Its measured compiled size is 31 bytes', 'WrongPuzzleHash', 'Both spends succeed', 'Resource exhaustion is inconclusive', 'current formal models reject SHA', 'do not prove cryptographic security', 'compiler preservation', 'general typed instance-authoring workflow']) assert(passwordLessonText.toLowerCase().includes(scope.toLowerCase()), `Password lesson misses its commitment/evidence boundary: ${scope}`);
const passwordSample = instance.cases.find(test => test.label === 'derived tail: valid_spend');
assert(passwordSample, 'Missing actual fixed-instance cost example');
const reviewedPassword = costReview.puzzles.find(row => row.slug === 'password-coin');
const reviewedFixed = reviewedPassword?.curried_instances;
assert(reviewedFixed, 'Final cost transition misses fixed-password measurements');
// The cost review owns the historical21 instance, not the current23 compiler output.
const historicalPassword = optimizationValidation.puzzles.find(row => row.slug === 'password-coin');
const historicalFixed = historicalPassword?.curried_instances;
assert(historicalFixed, 'Historical cost checkpoint misses fixed-password evidence');
const reviewedFixedArtifacts = historicalFixed.artifacts.filter(row => row.artifact.backend === 'rust_mir' && row.artifact.optimize_lir === true);
assert.equal(reviewedFixedArtifacts.length, 1, 'Missing/duplicate historical optimized fixed instance');
const reviewedFixedArtifact = reviewedFixedArtifacts[0];
assert.equal(reviewedFixed.artifact.module_tree_hash, reviewedPassword.artifact.tree_hash, 'Reviewed instance uses another module');
assert.deepEqual(costArtifact(reviewedFixed.artifact.artifact, 'reviewed password instance'), costArtifact(reviewedFixedArtifact.artifact, 'historical password instance'), 'Reviewed fixed instance differs from its actual artifact');
assert.deepEqual(reviewedFixed.cases.map(row => row.label), historicalFixed.cases.map(row => row.label), 'Missing/extra/reordered final fixed-instance measurement');
for (const [index, sample] of reviewedFixed.cases.entries()) {
  const actual = historicalFixed.cases[index];
  assert.equal(sample.solution_sha256, createHash('sha256').update(actual.solution).digest('hex'), 'Reviewed fixed-instance input changed');
  const executions = actual.runs.filter(row => row.backend === 'rust_mir' && row.optimize_lir === true);
  assert.equal(executions.length, 1, 'Missing/duplicate fixed-instance measurement');
  checkExecution(sample.execution);
  assert.deepEqual(sample.execution, executions[0].execution, 'Fixed-instance observation differs from final cost review');
}
const reviewedPasswordSample = reviewedFixed.cases.filter(test => test.label === 'derived tail: valid_spend');
assert.equal(reviewedPasswordSample.length, 1, 'Missing/duplicate final fixed-instance sample pin');
safeMetric(reviewedPasswordSample[0].execution.cost, 1, 'Final password fixed-instance cost');
// The lesson separately displays the current module and its actual curried sample.
const currentPasswordArtifacts = passwordPuzzle.artifacts.filter(row => row.backend === 'rust_mir' && row.optimize_lir === true);
const currentPasswordRuns = passwordSample.runs.filter(row => row.backend === 'rust_mir' && row.optimize_lir === true);
assert.equal(currentPasswordArtifacts.length, 1, 'Missing/duplicate current password module');
assert.equal(currentPasswordRuns.length, 1, 'Missing/duplicate current password sample');
const currentPasswordArtifact = costArtifact(currentPasswordArtifacts[0], 'current password module');
const currentPasswordExecution = currentPasswordRuns[0].execution;
assert.equal(checkExecution(currentPasswordExecution).status, 'accepted', 'Current password sample must succeed');
safeMetric(currentPasswordExecution.cost, 1, 'Current password fixed-instance cost');
const measured = [
  ['rust_mir', currentPasswordArtifact.bytes, currentPasswordExecution.cost],
  ['chialisp', 31, 1321],
  ['rue_source', 213, 3937],
].map(([backend, bytes, cost]) => {
  const artifact = passwordPuzzle.artifacts.find(row => row.backend === backend && row.optimize_lir === true);
  const run = passwordSample.runs.find(row => row.backend === backend && row.optimize_lir === true);
  assert.equal(artifact?.bytes, bytes, `${backend}: reviewed module-size measurement changed`);
  assert.equal(run?.execution.cost, cost, `${backend}: reviewed fixed-instance cost measurement changed`);
  return { bytes: new Intl.NumberFormat('en-US').format(bytes), cost: new Intl.NumberFormat('en-US').format(cost) };
});
assert(passwordLessonText.includes(`The optimized Rust module is ${measured[0].bytes} bytes, compared with ${measured[1].bytes} for Chialisp and ${measured[2].bytes} for optimized Rue.`), 'Lesson module-size comparison differs from the executed artifacts');
assert(passwordLessonText.includes(`The named valid curried spend costs ${measured[0].cost} VM units in optimized Rust, ${measured[1].cost} in Chialisp, and ${measured[2].cost} in optimized Rue.`), 'Lesson cost comparison differs from the actual named instance');
for (const scope of ['not general cost guarantees', 'exclude overall consensus condition and bundle costs']) assert(passwordLessonText.includes(scope), `Password measurements lose their finite scope: ${scope}`);

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
const introIds = new Set(['chialisp-example:intro-list', 'chialisp-example:intro-add', 'chialisp-example:intro-conditional']);
const moduleIds = new Set(['chialisp-example:module-add', 'chialisp-example:module-times-ten', 'chialisp-example:module-factorial', 'chialisp-example:currying-multiply']);
const implementedExamples = [];
for (const example of tutorialExamples.examples) {
  assert(coverage.includes(example.title), `Missing canonical example: ${example.id}`);
  if (example.implementation_status === 'implemented') {
    implementedExamples.push(example.id);
    assert.equal(example.validation_status, 'differential_only');
    const puzzle = validation.puzzles.find(puzzle => puzzle.slug === example.source_adaptation?.slug);
    assert(puzzle?.validated, `${example.id}: missing canonical validation`);
    const detail = await html(path.join(root, `lab/${puzzle.slug}/index.html`));
    assert.equal(puzzle.provenance.classification, 'canonical_tutorial_port');
    assert(detail.includes('no source wrapper'), `${example.id}: original source must remain unwrapped`);
    if (introIds.has(example.id)) {
      assert.equal(example.source_adaptation.operation, 'extract_single_quoted_run_expression');
      assert(detail.includes('Original Chialisp expression'), `${example.id}: original expression must be labeled`);
      assert(detail.includes('Environment contract: explicitly ignored.'), `${example.id}: missing original environment contract`);
      assert.equal(puzzle.environment, 'ignored');
      assert.equal(puzzle.reference_mode, 'expression');
      assert.equal(example.validation_contract.abi, 'ignored_environment_nullary');
      assert.equal(example.validation_contract.environment, 'ignored');
      assert.equal(example.validation_contract.reference_mode, 'expression');
    } else if (moduleIds.has(example.id)) {
      assert.equal(example.source_adaptation.operation, 'extract_complete_chialisp_module');
      assert(detail.includes('Original Chialisp module'), `${example.id}: original module must be labeled`);
      assert(detail.includes('Environment contract: strict typed arguments.'), `${example.id}: missing strict typed ABI contract`);
      assert(detail.includes('Declared divergences from the original'), `${example.id}: declared divergences must be visible`);
      assert.equal(puzzle.environment ?? 'strict', 'strict');
      assert.equal(puzzle.reference_mode ?? 'module', 'module');
      assert.equal(example.validation_contract.abi, 'strict_typed_i64_list');
      assert.equal(example.validation_contract.claim_scope, 'executed_differential_cases_with_declared_original_divergences_only');
      assert.equal(example.validation_contract.compiler_preservation, 'not_claimed');
      const divergent = puzzle.cases.filter(test => test.declared_divergence);
      assert.equal(example.validation_contract.declared_divergences, divergent.length, `${example.id}: declared divergence count differs from the report`);
      assert(divergent.length > 0 && divergent.every(test => test.declared_divergence.backends.every(backend => ['chialisp', 'rue_source'].includes(backend))), `${example.id}: declared divergences may only bind Chialisp or Rue`);
      for (const test of puzzle.cases) {
        for (const run of test.runs) {
          const bound = test.declared_divergence && test.declared_divergence.backends.includes(run.backend) ? test.declared_divergence.outcome : test.native;
          assert.deepEqual(run.execution.outcome, bound, `${example.id}/${test.label}/${run.backend}: run differs from its bound expectation`);
        }
      }
      assert.deepEqual(puzzle.simulator, [], 'Tutorial module has no ledger scenario credit');
    } else {
      assert.equal(example.id, 'chialisp-example:password-coin', 'An unreviewed canonical example was promoted');
      assert.equal(puzzle.slug, 'password-coin');
      assert.equal(example.source_adaptation.operation, 'extract_complete_chialisp_module');
      assert.deepEqual(example.validation_contract, {
        abi: 'raw_one_tree', environment: 'raw', reference_mode: 'module',
        claim_scope: 'executed_raw_differential_cases_and_separate_fixed_instance_observations_only',
        instance_protocol: 'chialisp-oxide-password-instances/v1',
        ledger: 'separate_fixed_commitment_instance_observations_only',
        hash_proof: 'not_claimed', compiler_preservation: 'not_claimed',
        whole_page_completed: false, runtime_umbrella_semantic_rejection_credit: false,
      }, 'Password module/instance promotion has a changed claim contract');
      assert(detail.includes('Original Chialisp module') && detail.includes('Environment contract: raw tree.'), 'Password promotion needs its original complete-module/raw ABI labels');
      assert.equal(puzzle.environment, 'raw');
      assert.equal(puzzle.reference_mode ?? 'module', 'module');
      assert.equal(puzzle.provenance.source_adaptation.kind, 'none');
      assert(!Object.hasOwn(puzzle, 'module_identity'), 'Tutorial module must not invent a published artifact identity');
      assert.deepEqual(puzzle.simulator, [], 'Raw tutorial module has no locked-coin scenario credit');
      assert.equal(puzzle.curried_instances?.protocol, example.validation_contract.instance_protocol);
    }
  } else {
    assert.equal(example.implementation_status, 'not_started');
    assert.equal(example.validation_status, 'not_run', 'Canonical source inventory is not executed evidence');
  }
}
assert.deepEqual(implementedExamples.sort(), [...introIds, ...moduleIds, 'chialisp-example:password-coin'].sort(), 'Only the three closed Intro expressions, the three Using Modules programs and the exact password module are promoted');
assert(built.includes(path.join(root, 'tutorials/canonical-intro/index.html')), 'Missing canonical Intro tutorial');
assert(built.includes(path.join(root, 'tutorials/canonical-modules/index.html')), 'Missing canonical Using Modules tutorial');
assert(built.includes(path.join(root, 'tutorials/delegated-puzzle/index.html')), 'Missing delegated-puzzle tutorial');
const delegatedPage = await html(path.join(root, 'tutorials/delegated-puzzle/index.html'));
for (const needle of ['delegated-puzzle', 'apply', 'opcode 2', 'fingerprint', 'authorization', 'inner-puzzle', 'sha256tree.clib']) {
  assert(delegatedPage.includes(needle), `delegated-puzzle tutorial missing ${needle}`);
}
const innerLab = await html(path.join(root, 'lab/inner-puzzle/index.html'));
for (const needle of ['data-curried-examples="2"', 'id="curry"', 'committed']) assert(innerLab.includes(needle), `inner-puzzle lab page missing ${needle}`);
assert(built.includes(path.join(root, 'tutorials/curry-hash/index.html')), 'Missing curry-hash tutorial');
const curryPage = await html(path.join(root, 'tutorials/curry-hash/index.html'));
for (const needle of ['curry-hash', 'curry-and-treehash.clib', 'curried_puzzle_hash', 'clvm-utils', 'currying-multiply', '(a (q 18 2 5) (c (q . 2) 1))', 'curry-factory', 'second VM execution']) {
  assert(curryPage.includes(needle), `curry-hash tutorial missing ${needle}`);
}
const curryLab = await html(path.join(root, 'lab/currying-multiply/index.html'));
for (const needle of ['data-curried-examples="2"', 'data-curried-argument="2"', 'data-curried-argument="5"', '(a (q 18 2 5) (c (q . 2) 1))', 'id="curry"']) {
  assert(curryLab.includes(needle), `currying-multiply lab page missing ${needle}`);
}
const modulesPage = await html(path.join(root, 'tutorials/canonical-modules/index.html'));
for (const needle of ['module-add', 'module-times-ten', 'module-factorial', 'declared_divergence', 'i64']) {
  assert(modulesPage.includes(needle), `Using Modules tutorial missing ${needle}`);
}
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
assert(studyPage.includes('E0308') && studyPage.includes('E0425') && studyPage.includes('E0061'), 'Missing captured Rust diagnostic references');
assert(study.studies.filter(example => example.category === 'runtime_policy_control').length === 2, 'Expected two negative controls');
for (const needle of ['not a compiler guarantee', 'overflow checks', 'E0061']) assert(studyPage.includes(needle), `error study page missing ${needle}`);
const statusPage = await html(path.join(root, 'reference/status/index.html'));
assert(statusPage.includes(`${validation.puzzles.length} annotated exports`), 'Status page export count must match the validation report');
const namedVectors = validation.puzzles.reduce((n, puzzle) => n + puzzle.cases.length, 0);
assert(statusPage.includes(`${namedVectors} named`), 'Status page named-vector count must match the validation report');
assert(statusPage.includes(`${study.studies.length === 11 ? 'eleven' : study.studies.length} studies`), 'Status page study count must match the error-study report');
for (const needle of ['not ready for real funds', 'roadmap-history.md', 'Next work']) assert(statusPage.includes(needle), `status page missing ${needle}`);
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
  for (const [term, route] of [
    ['validate inputs commitments', 'validation/inputs/'],
    ['validate conditions authority', 'validation/outputs/'],
    ['review adversarial', 'validation/review/'],
    ['puzzle authoring agents', 'reference/agent-guide/'],
    ['lifecycle simulator journal', 'reference/lifecycle/'],
  ]) {
    const result = await pagefind.search(term);
    const matches = await Promise.all(result.results.map(row => row.data()));
    assert(matches.some(row => row.url === `${base}${route}`), `${route}: guide is not discoverable in the actual Pagefind index`);
  }
  for (const term of ['recovery', 'canonical', 'AGG_SIG_ME', 'CoinId', 'from_mojos', 'create_coin', 'chialisp_oxide', 'impossible', 'currying', 'p2_conditions', 'equivalence', 'rightmost', 'recursion', 'last_inner', 'elision', 'prefix', 'Lean', 'inequality', 'nonvacuity', 'sysroot', 'insertion_sort_by_keys', 'bitvectors', 'sha256tree', 'launcher', 'sha256_parts', 'password', 'cost', 'facts', 'helper inlining', 'intervals', 'canonical integers', 'pay-to-singleton', 'lineage', 'reciprocal announcements', 'acyclic scalar', 'unused initializer', 'transaction costs', 'charged generator', 'guarded projections', 'wire pairs', 'unary frames', 'delegated conditions', 'owner key commitment', 'arbitrary tail', 'policy mistakes', 'atom hash prefix', 'proper outer tail', 'compressed generators', 'boomerang']) {
    const search = await pagefind.search(term);
    assert(search.results.length > 0, `No production search results for ${term}`);
    const result = await search.results[0].data();
    assert(result.url.startsWith(base), `Search result escapes base: ${result.url}`);
    if (term === 'compressed generators') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/generator-costs/`), 'Generator-cost tutorial is not discoverable');
    }
    if (term === 'pay-to-singleton') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/p2-singleton/`), 'P2 tutorial is not discoverable');
    }
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
    if (term === 'sha256tree' || term === 'launcher' || term === 'sha256_parts') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      const routes = term === 'sha256_parts' ? ['reference/hashing/']
        : term === 'sha256tree' ? ['tutorials/sha256tree/', 'lab/sha256tree/']
        : ['tutorials/singleton-launcher/', 'lab/singleton-launcher/'];
      for (const route of routes) assert(matches.some(row => row.url === `${base}${route}`), `Hash documentation is not discoverable in production search: ${route}`);
    }
    if (term === 'password' || term === 'currying') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      for (const route of ['tutorials/password-coin/', 'lab/password-coin/']) assert(matches.some(row => row.url === `${base}${route}`), `Password module/instance documentation is not discoverable for ${term}: ${route}`);
    }
    if (term === 'cost' || term === 'facts' || term === 'helper inlining' || term === 'intervals' || term === 'canonical integers') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}reference/optimization/`), `Measured optimization and execution-success facts are not discoverable for ${term}`);
    }
    if (term === 'transaction costs' || term === 'charged generator') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/transaction-costs/`),
        `${term}: missing transaction-cost lesson from actual built search results`);
    }
    if (term === 'wire pairs' || term === 'unary frames') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      if (term === 'wire pairs') assert(matches.some(row => row.url === `${base}reference/abi/` || row.url === `${base}reference/supported-rust/`), 'Wire-pair scope is missing from actual built search results');
      if (term === 'unary frames') assert(matches.some(row => row.url === `${base}tutorials/guarded-projections/`), 'Composed runtime-frame guide is missing from actual built search results');
    }
    if (term === 'guarded projections') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}tutorials/guarded-projections/`), 'Guarded projection lesson is missing from actual built search results');
    }
    if (term === 'boomerang') {
      const matches=await Promise.all(search.results.map(row=>row.data()));
      for(const route of ['tutorials/boomerang/','lab/boomerang/'])assert(matches.some(row=>row.url===`${base}${route}`),'Boomerang lesson and committed-instance evidence must be searchable');
    }
    if (['delegated conditions','owner key commitment','arbitrary tail'].includes(term)) {
      const matches=await Promise.all(search.results.map(row=>row.data()));
      assert(matches.some(row=>row.url===`${base}tutorials/delegated-conditions/`),`${term}: missing delegated-conditions lesson from actual index`);
    }
    if (['policy mistakes', 'atom hash prefix', 'proper outer tail'].includes(term)) {
      const matches = await Promise.all(search.results.map(row => row.data()));
      for (const route of ['tutorials/delegated-conditions/', 'lab/p2-delegated-conditions/']) {
        assert(matches.some(row => row.url === `${base}${route}`), `${term}: missing actual source-policy counterexamples in ${route}`);
      }
    }
    if (term === 'acyclic scalar' || term === 'unused initializer') {
      const matches = await Promise.all(search.results.map(row => row.data()));
      assert(matches.some(row => row.url === `${base}verification/acyclic-scalar/`),
        `Actual-source scalar helper experiment is not discoverable for ${term}`);
    }
    console.log(`Search ${JSON.stringify(term)}: ${search.results.length} results; first ${result.url}`);
  }
  await pagefind.destroy();
} finally {
  globalThis.fetch = originalFetch;
}

console.log(`PASS: ${pages.length} pages, ${checkedLinks} local links/assets/fragments, ${checkedSourceCopies} original source-copy payloads, ${publishedReports.length} JSON reports, ${structuralBasenames.length} exact structural downloads, ${byteDownloads.length} exact byte-proof downloads, ${acyclicDisplay.downloads} exact acyclic downloads with ${acyclicDisplay.sourceCopies} source/spec copies, ${sourceDataDownloads.length} exact source-data downloads with ${sourceBudgetRoutes.size} source-page budgets (512KiB except the P2lab1MiB), ${byteVmRuns} typed byte-proof VM observations, exact password module/instance counts and typed rows, ${dedicatedCounts.attempts} separate P2 singleton and ${delegatedCounts.attempts} delegated-conditions actual recorded submissions with ${protocolDisplayControls} display corruption controls, ${costRows.length} before/after optimization rows with ${costDisplayControls} display corruption controls, ${previewControls} bounded-preview corruption controls, and live Pagefind queries`);
