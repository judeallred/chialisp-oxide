// Presentation consistency only. The mandatory Python recorded reader owns
// current-source/independent-recipe/artifact admission. This module executes no
// compiler/native-policy/VM/SDK programs.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstat, readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repository } from './p2-costs.mjs';

// Keep the Node-only reader at its real source location after Astro SSR bundling.
const { checkExecution, checkNativeOutcome } = await import(/* @vite-ignore */ pathToFileURL(
  path.join(await repository(), 'www/scripts/vm-failures.mjs'),
).href);

export const sourceControlsProtocol = 'chialisp-oxide-p2dc-source-controls/v1';
export const sourceControlsPath = 'corpus/p2dc-source-controls/report.json';
export const sourceControlNames = ['wrong_atom_domain', 'wrong_conditions_tail', 'reject_ignored_improper_tail'];
const sourcePath = 'puzzles/p2-delegated-conditions/puzzle.rs';
const sourceSHA = '8503a4c04ab10f3273b771e1fd378082b8b3b5230c5b708931818486046b89aa';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const closed = (value, fields, label) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), label);
  assert.deepEqual(Object.keys(value).sort(), [...fields].sort(), `${label}: missing/unknown fields`);
};
const signature = "pub fn puzzle<'a>(environment: &'a Tree<'a>) -> DelegatedConditions<'a> {\n";
const recipes = [
  { name: sourceControlNames[0], before: 'const ATOM_DOMAIN: [u8; 1] = [1];', after: 'const ATOM_DOMAIN: [u8; 1] = [2];', witness: 'named-key0-conditions0', distinguishing: 'accepted_output_differs' },
  { name: sourceControlNames[1], before: '                rest: conditions,', after: '                rest: environment,', witness: 'named-key0-conditions0', distinguishing: 'accepted_output_differs' },
  { name: sourceControlNames[2], before: signature, after: signature + '    assert!(matches!(environment, Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))));\n', witness: 'ignored-outer-tail3', distinguishing: 'conclusive_program_rejection' },
];
const labels = [
  ['Use the wrong atom hash prefix', 'Both prefixes are valid byte arrays. Rust cannot infer that an atom must use the atom-domain prefix; the returned signature message changes.'],
  ['Return the environment instead of the conditions', 'Both values have the same borrowed Tree type. Rust accepts either tail, but the second one returns the wrong complete tree.'],
  ['Require a proper outer tail', 'This assertion is valid Rust and compiles to a check. It rejects an input that the canonical module deliberately accepts because that tail is ignored.'],
];
const formatStrictTail = value => value.replace(
  '    assert!(matches!(environment, Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))));\n',
  '    assert!(matches!(\n        environment,\n        Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))\n    ));\n');

export function sourceControlOutcomeText(value, { vm = false } = {}) {
  const outcome = vm ? checkExecution(value) : checkNativeOutcome(value);
  if (outcome.status === 'accepted') return `accepted · output ${outcome.output_hex}`;
  return vm ? `rejected · ${value.failure.kind}/${value.failure.code}` : 'rejected · source policy panic';
}

export function delegatedSourceControlEvidence(report, inputs, puzzle) {
  assert.equal(puzzle?.slug, 'p2-delegated-conditions', 'Actual current23 component is required');
  assert.equal(puzzle.validated, true);
  assert.equal(puzzle.protocol_validation?.status, 'validated');
  assert.deepEqual(JSON.parse(inputs.reportBytes), report, 'Displayed report must match retained raw bytes');
  closed(report, ['schema_version','protocol','status','scope','context','prepared','native','bundles','runs','commands','files'], 'source-control report');
  assert.equal(report.schema_version, 1); assert.equal(report.protocol, sourceControlsProtocol);
  assert.equal(report.status, 'actual_source_controls_captured');
  assert.equal(typeof report.scope, 'string');
  for (const phrase of ['One actual native Rust test group', 'three independent source-policy observations', 'twelve retained primary VM witnesses', 'not execution authentication']) assert(report.scope.includes(phrase));
  assert.equal(sha(inputs.baselineSource), sourceSHA);
  assert.equal(inputs.recipe.schema_version, 1);
  assert.equal(inputs.recipe.protocol, 'chialisp-oxide-p2dc-source-mutations/v1');
  assert.equal(inputs.recipe.source, sourcePath); assert.equal(inputs.recipe.source_sha256, sourceSHA);
  assert.deepEqual(inputs.recipe.mutations, recipes, 'Only the three exact reviewed source recipes are displayed');
  assert.equal(report.native.native_test_groups, 1); assert.equal(report.native.native_policy_observations, 3);
  assert.equal(report.native.exit_code, 0); assert.equal(report.native.log, 'native.log');
  assert.equal(sha(inputs.nativeLog), report.native.log_sha256);
  const nativeText = Buffer.from(inputs.nativeLog).toString('utf8');
  assert.equal(nativeText.split('test result: ok. 1 passed; 0 failed;').length - 1, 1);
  const nativeRows = [...nativeText.matchAll(/P2DC_MUTATION_NATIVE (\{[^\n]+\})/g)].map(match => JSON.parse(match[1]));
  assert.equal(nativeRows.length, 3);
  assert.deepEqual(Object.keys(report.bundles).sort(), ['off','on']);
  for (const mode of ['off','on']) {
    const bundle = report.bundles[mode];
    assert.equal(bundle.manifest, `bundles/${mode}/manifest.json`);
    assert.match(bundle.manifest_sha256, /^[0-9a-f]{64}$/);
    assert.equal(bundle.recompiled.length, 4);
    assert.deepEqual(bundle.recompiled.map(row => row.entry).sort(), ['baseline',...sourceControlNames].map(name => `chialisp_oxide_p2dc_controls::${name}::puzzle`).sort());
  }
  assert.equal(report.runs.length, 12);
  assert.equal(report.prepared.witnesses.length, 3);
  assert.equal(report.commands.length, 22);
  const sourceFiles = new Map(report.prepared.source_files.map(row => [row.path, row.sha256]));
  assert.equal(sourceFiles.size, 9); assert.equal(report.prepared.source_files.length, 9);
  assert.equal(sourceFiles.get('src/baseline.rs'), sourceSHA);
  assert.equal(inputs.sources.baseline, Buffer.from(inputs.baselineSource).toString('utf8'));
  const rows = recipes.map((recipe, index) => {
    const witness = report.prepared.witnesses[index];
    closed(witness, ['name','witness','environment_hex','solution','baseline','mutant'], 'mutation witness');
    assert.equal(witness.name, recipe.name); assert.equal(witness.witness, recipe.witness);
    const independent = inputs.vectors.accepted.find(row => row.label === witness.witness);
    assert(independent, 'Witness must be one of the independently specified accepted inputs');
    assert.equal(witness.solution, independent.solution); assert.equal(witness.environment_hex, independent.environment_hex);
    assert.deepEqual(witness.baseline, { status: 'accepted', output_hex: independent.expected_output_hex });
    checkNativeOutcome(witness.mutant); assert.notDeepEqual(witness.baseline, witness.mutant);
    assert.equal(witness.mutant.status, index === 2 ? 'rejected' : 'accepted');
    assert.deepEqual(nativeRows[index], { name: recipe.name, witness: recipe.witness, baseline: witness.baseline, actual: witness.mutant });
    const original = inputs.sources.baseline;
    assert.equal(original.split(recipe.before).length - 1, 1, 'Unique source recipe anchor');
    const raw = original.replace(recipe.before, recipe.after);
    const changed = index === 2 ? formatStrictTail(raw) : raw;
    assert.equal(inputs.sources[recipe.name], changed, 'Exact source recipe and reviewed formatting relation');
    assert.equal(sourceFiles.get(`src/${recipe.name}.rs`), sha(changed));
    const current = puzzle.protocol_validation.raw_validation.cases.find(row => row.label === witness.witness);
    assert(current); assert.equal(current.solution, witness.solution); assert.deepEqual(current.native, witness.baseline);
    const modes = ['off','on'].map((mode, modeIndex) => {
      const runs = report.runs.slice(modeIndex * 6 + index * 2, modeIndex * 6 + index * 2 + 2);
      for (const [roleIndex, run] of runs.entries()) {
        closed(run, ['mode','name','witness','solution','artifact_sha256','execution'], 'VM witness');
        assert.equal(run.mode, mode); assert.equal(run.name, roleIndex ? recipe.name : 'baseline');
        assert.equal(run.witness, witness.witness); assert.equal(run.solution, witness.solution);
        assert.deepEqual(checkExecution(run.execution), roleIndex ? witness.mutant : witness.baseline);
        if (index === 2 && roleIndex) assert.deepEqual(run.execution.failure, { kind: 'program', code: 'raise' });
        else assert(run.execution.cost <= 1_000_000);
      }
      return { mode, baseline: runs[0].execution, mutant: runs[1].execution };
    });
    return { ...witness, title: labels[index][0], explanation: labels[index][1], before: recipe.before,
      after: index === 2 ? formatStrictTail(recipe.after) : recipe.after, native: nativeRows[index], modes };
  });
  return { rows, protocol: report.protocol, scope: report.scope, counts: { groups: 1, observations: 3, exports: 4, modes: 2, exportModes: 8, vm: 12 }, reportSHA: sha(inputs.reportBytes) };
}

async function boundedRead(file, limit) {
  const metadata = await lstat(file);
  assert(metadata.isFile() && !metadata.isSymbolicLink() && metadata.size > 0 && metadata.size <= limit, 'Bounded regular docs input required');
  const value = await readFile(file); assert(value.length > 0 && value.length <= limit); return value;
}
export async function readDelegatedSourceControls(puzzle, { sourceRoot } = {}) {
  const root = await repository(sourceRoot);
  if (!puzzle) return null; // A pending old22 corpus cannot promote this view.
  let reportBytes;
  try { reportBytes = await boundedRead(path.join(root, sourceControlsPath), 4 * 1024 * 1024); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  const base = path.join(root, 'corpus/p2dc-source-controls');
  const [baselineSource, recipeBytes, vectorBytes, nativeLog, ...sourceBytes] = await Promise.all([
    boundedRead(path.join(root, sourcePath), 64 * 1024),
    boundedRead(path.join(root, 'puzzles/p2-delegated-conditions/mutation-recipes.json'), 64 * 1024),
    boundedRead(path.join(root, 'puzzles/p2-delegated-conditions/independent-vectors.json'), 4 * 1024 * 1024),
    boundedRead(path.join(base, 'native.log'), 1024 * 1024),
    ...['baseline', ...sourceControlNames].map(name => boundedRead(path.join(base, `package/src/${name}.rs`), 64 * 1024)),
  ]);
  const inputs = { baselineSource, recipe: JSON.parse(recipeBytes), vectors: JSON.parse(vectorBytes), nativeLog, reportBytes,
    sources: Object.fromEntries(['baseline', ...sourceControlNames].map((name, index) => [name, sourceBytes[index].toString('utf8')])) };
  return delegatedSourceControlEvidence(JSON.parse(reportBytes), inputs, puzzle);
}

const entities = { amp: '&', lt: '<', gt: '>', apos: "'", quot: '"' };
const plain = value => value.replace(/<[^>]*>/g, ' ').replace(/&(?:#x([0-9a-f]+)|#([0-9]+)|(amp|lt|gt|apos|quot));/gi,
  (_, hex, decimal, name) => name ? entities[name.toLowerCase()] : String.fromCodePoint(Number.parseInt(hex ?? decimal, hex ? 16 : 10)),
).replace(/\s+/g, ' ').trim();
export function checkDelegatedSourceControlDisplay(markup, evidence) {
  const sections = [...markup.matchAll(/<section\b[^>]*data-p2dc-source-controls[^>]*>[\s\S]*?<\/section>/g)];
  assert.equal(sections.length, 1); const section = sections[0][0], text = plain(section);
  for (const [key, value] of Object.entries(evidence.counts)) assert(section.includes(`data-p2dc-controls-${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}="${value}"`));
  for (const phrase of ['1 native test group', '3 paired policy observations', '4 annotated Cargo exports', 'both optimization modes', '12 primary VM observations', 'valid Rust', 'raw policy counterexamples', 'not SDK admissions', 'not compiler preservation', 'not exhaustive security', '200 additional native', '1,000 additional VM', '60 SDK submissions']) assert(text.includes(phrase), `Missing mutation boundary: ${phrase}`);
  assert(text.includes(evidence.protocol)); assert(text.includes(evidence.reportSHA));
  assert.deepEqual([...section.matchAll(/data-p2dc-mutation="([^"]+)"/g)].map(match => match[1]), sourceControlNames);
  for (const row of evidence.rows) {
    const block = section.match(new RegExp(`<article\\b[^>]*data-p2dc-mutation="${row.name}"[^>]*>[\\s\\S]*?<\\/article>`))?.[0]; assert(block);
    const blockText = plain(block); assert(blockText.includes(row.title)); assert(blockText.includes(row.witness));
    const value = key => plain(block.match(new RegExp(`<[^>]+data-p2dc-${key}[^>]*>([\\s\\S]*?)<\\/[^>]+>`))?.[1] ?? '');
    assert.equal(value('solution'), row.solution);
    assert.equal(value('environment'), row.environment_hex);
    // ExpressiveCode can nest wrapper/header divs inside each recipe container.
    // Slice between our stable sibling markers rather than the first </div>.
    const recipeMarkers = [...block.matchAll(/<div\b[^>]*data-p2dc-recipe="([^"]+)"[^>]*>/g)];
    assert.deepEqual(recipeMarkers.map(marker => marker[1]), ['before','after']);
    const tableIndex = block.indexOf('<table', recipeMarkers[1].index + recipeMarkers[1][0].length);
    assert(tableIndex > recipeMarkers[1].index, 'Missing outcome table after recipe blocks');
    for (const [index, expected] of [row.before,row.after].entries()) {
      const marker = recipeMarkers[index];
      const snippet = block.slice(marker.index + marker[0].length, recipeMarkers[index + 1]?.index ?? tableIndex);
      const codeElements = [...snippet.matchAll(/<code\b[^>]*>([\s\S]*?)<\/code>/g)];
      assert.equal(codeElements.length, 1, 'Exactly one complete visible source snippet per recipe');
      const code = plain(codeElements[0][1].replace(/<[^>]*>/g, ''));
      assert.equal(code.replace(/\s+/g,' ').trim(), expected.replace(/\s+/g,' ').trim(), 'Visible source recipe changed');
    }
    const expected = [
      ['native-baseline', sourceControlOutcomeText(row.native.baseline)], ['native-mutant', sourceControlOutcomeText(row.native.actual)],
      ...row.modes.flatMap(mode => [[`${mode.mode}-baseline`,sourceControlOutcomeText(mode.baseline,{vm:true})],[`${mode.mode}-mutant`,sourceControlOutcomeText(mode.mutant,{vm:true})]]),
    ];
    for (const [role, outcome] of expected) {
      const cell = block.match(new RegExp(`<td\\b[^>]*data-p2dc-outcome="${role}"[^>]*>([\\s\\S]*?)<\\/td>`))?.[1]; assert(cell !== undefined);
      assert.equal(plain(cell), outcome, `Visible ${row.name}/${role} outcome changed`);
    }
  }
}
