import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, rename, rm, stat, symlink, truncate, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acyclicProofArtifacts, acyclicProofLimits, acyclicProofSummary, acyclicWitnessBytes,
  checkAcyclicDisplay, loadAcyclicProofEvidence, readAcyclicProofArtifact } from '../src/lib/acyclic-proof-artifacts.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const originalCwd = process.cwd();
const actual = await loadAcyclicProofEvidence();
const { report, artifacts, sources } = actual;
assert.equal(artifacts.length, 76);
assert.equal(new Set(artifacts.map(entry => entry.basename)).size, 76);
assert.deepEqual(acyclicProofSummary(report), report.counts);
assert.deepEqual(actual.raw, await readFile(path.join(root, 'proofs/acyclic-scalar-source/result.json')));
for (const entry of artifacts) {
  assert.deepEqual(entry.bytes, await readFile(path.join(root, entry.path)));
  assert.equal(createHash('sha256').update(entry.bytes).digest('hex'), entry.sha256);
}
assert.equal(sources.size, 10, 'Actual current experiment source closure must be checked');

let metadataControls = 0;
const mutations = [
  ['missing export', value => value.exports.pop()],
  ['duplicate export', value => { value.exports[1] = structuredClone(value.exports[0]); }],
  ['reordered exports', value => value.exports.reverse()],
  ['missing mode', value => value.exports[0].builds.pop()],
  ['numeric mode', value => { value.exports[0].builds[0].optimize = 0; }],
  ['duplicate mode', value => { value.exports[0].builds[1].optimize = false; }],
  ['missing obligation', value => value.obligations.pop()],
  ['false model result', value => { value.obligations[0].expected = 'refuted'; }],
  ['unknown model', value => { value.obligations[0].verdict.model = 'default'; }],
  ['wrong source role', value => { value.exports[0].IR.path = value.exports[1].IR.path; }],
  ...['../outside.json', '/tmp/outside.json', 'ir/../ir/base-guarded.json', 'ir//base-guarded.json',
    'ir\\base-guarded.json', 'ir/%2fbase-guarded.json', 'ir/unknown.json'].map(name =>
    [`closed path ${name}`, value => { value.exports[0].IR.path = name; }]),
  ['malformed hash', value => { value.exports[0].IR.sha256 = 'A'.repeat(64); }],
  ['missing hash', value => { delete value.exports[0].IR.sha256; }],
  ['counterexample out of domain', value => { value.obligations[8].verdict.verification.counterexample.arguments[0].value = '256'; }],
  ['padded counterexample', value => { value.obligations[8].verdict.verification.counterexample.arguments[0].value = '00'; }],
  ['numeric counterexample', value => { value.obligations[8].verdict.verification.counterexample.arguments[0].value = 100; }],
  ['Boolean witness input', value => { value.obligations[8].witness.input = false; }],
];
for (const [label, mutate] of mutations) {
  const changed = structuredClone(report);
  mutate(changed);
  assert.notDeepEqual(changed, report, `Vacuous ${label}`);
  assert.throws(() => acyclicProofArtifacts(changed), undefined, `Accepted ${label}`);
  metadataControls++;
}
for (const mutate of [value => { value.counts.native_finite_cases = true; },
  value => { value.exports[0].native.pop(); }, value => { value.exports[0].builds[0].canonical.pop(); },
  value => { value.obligations[0].verdict.verification.nonvacuity = null; }]) {
  const changed = structuredClone(report); mutate(changed);
  assert.throws(() => acyclicProofSummary(changed)); metadataControls++;
}
assert.throws(() => acyclicWitnessBytes({ arguments: [{ kind: 'int', value: '025' }] }));
assert.equal(acyclicWitnessBytes({ arguments: [{ kind: 'int', value: '0' }] }).toString(),
  '{\n  "arguments": [\n    {\n      "kind": "int",\n      "value": "0"\n    }\n  ]\n}\n');

let fileControls = 0;
const temporary = await mkdtemp(path.join(os.tmpdir(), 'oxide-acyclic-downloads-'));
const checkout = path.join(temporary, 'checkout');
const proof = path.join(checkout, 'proofs/acyclic-scalar-source');
const rawPath = path.join(proof, 'result.json');
async function reset() {
  await rm(checkout, { recursive: true, force: true });
  await mkdir(path.join(checkout, 'www'), { recursive: true });
  await writeFile(path.join(checkout, 'www/package.json'), JSON.stringify({ name: 'chialisp-oxide-docs' }));
  await mkdir(proof, { recursive: true });
  await writeFile(rawPath, actual.raw);
  for (const entry of artifacts) {
    const target = path.join(checkout, entry.path); await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, entry.bytes);
  }
  for (const [relative, value] of sources) {
    const target = path.join(checkout, 'experiments/acyclic-scalar', relative);
    await mkdir(path.dirname(target), { recursive: true }); await writeFile(target, value);
  }
}
try {
  await reset(); process.chdir(checkout);
  await loadAcyclicProofEvidence(); // Own fixture positive before file corruptions.
  const entry = artifacts[0];
  const filename = path.join(checkout, entry.path);
  await assert.rejects(() => readAcyclicProofArtifact({ ...entry, sha256: '0'.repeat(64) }), /SHA-256/); fileControls++;
  await writeFile(filename, Buffer.concat([entry.bytes, Buffer.from('\nchanged')]));
  await assert.rejects(() => loadAcyclicProofEvidence(), /SHA-256/); fileControls++;
  await rm(filename); await assert.rejects(() => loadAcyclicProofEvidence(), { code: 'ENOENT' }); fileControls++;
  await writeFile(filename, entry.bytes);
  const outside = path.join(temporary, 'outside.json'); await copyFile(filename, outside);
  await rm(filename); await symlink(outside, filename);
  await assert.rejects(() => loadAcyclicProofEvidence(), /symlink/); fileControls++;
  await rm(filename); await writeFile(filename, entry.bytes);
  const oldDirectory = path.join(temporary, 'outside-ir'); await rename(path.dirname(filename), oldDirectory);
  await symlink(oldDirectory, path.dirname(filename));
  await assert.rejects(() => loadAcyclicProofEvidence(), /symlink/); fileControls++;
  await reset();
  await truncate(filename, acyclicProofLimits.artifact + 1);
  await assert.rejects(() => loadAcyclicProofEvidence(), /budget/); fileControls++;
  await reset(); await truncate(rawPath, acyclicProofLimits.report + 1);
  await assert.rejects(() => loadAcyclicProofEvidence(), /budget/); fileControls++;
  await reset();
  const witness = artifacts.find(entry => entry.relative.endsWith('.witness.json'));
  await writeFile(path.join(checkout, witness.path), Buffer.concat([witness.bytes, Buffer.from('\n')]));
  await assert.rejects(() => loadAcyclicProofEvidence(), /SHA-256|witness/); fileControls++;
  await reset();
  await writeFile(path.join(checkout, 'experiments/acyclic-scalar/src/base.rs'), sources.get('src/base.rs') + '\n');
  await assert.rejects(() => loadAcyclicProofEvidence(), /current source SHA-256/); fileControls++;
  await reset();
  await truncate(path.join(checkout, 'experiments/acyclic-scalar/src/base.rs'), acyclicProofLimits.source + 1);
  await assert.rejects(() => loadAcyclicProofEvidence(), /budget/); fileControls++;
  await reset();
  // Constructed resource control only: rebound hashes make individual reads
  // consistent so the aggregate budget, rather than a first hash mismatch, fires.
  const huge = Buffer.alloc(acyclicProofLimits.artifact, 0x20);
  const hash = createHash('sha256').update(huge).digest('hex');
  const expanded = structuredClone(report);
  const references = expanded.exports.flatMap(row => [row.IR, ...row.builds.map(build => build.retained_CLVM)]);
  for (const reference of references.slice(0, 5)) {
    reference.sha256 = hash; await writeFile(path.join(proof, reference.path), huge);
  }
  await writeFile(rawPath, JSON.stringify(expanded));
  await assert.rejects(() => loadAcyclicProofEvidence(), /budget/); fileControls++;
  await reset(); await loadAcyclicProofEvidence();
} finally {
  process.chdir(originalCwd); await rm(temporary, { recursive: true, force: true });
}

let displayControls = 0;
const page = path.join(root, 'www/dist/verification/acyclic-scalar/index.html');
if (await stat(page).catch(() => null)) {
  const markup = await readFile(page, 'utf8');
  const displayed = checkAcyclicDisplay(markup, report, artifacts, sources);
  assert.equal(displayed.downloads, 76);
  const corruptVisibleInput = (value, marker, input) => {
    const row = [...value.matchAll(/<tr\b[^>]*>[\s\S]*?<\/tr>/g)]
      .find(match => match[0].includes(marker))[0];
    const changed = row.replace(new RegExp(`(<code\\b[^>]*>)${input}(</code>)`),
      (_whole, first, last) => `${first}999${last}`);
    assert.notEqual(changed, row, 'Vacuous visible-input mutation');
    return value.replace(row, changed);
  };
  const corruptions = [
    ['scope', value => value.replace('No Rust/MIR/IR/CLVM preservation theorem', 'Compiler preservation proved')],
    ['count', value => value.replace(/(data-acyclic-count="native_finite_cases"[^>]*>)[\d,]+/, '$1999')],
    ['status', value => value.replace('data-acyclic-status="proven"', 'data-acyclic-status="refuted"')],
    ['obligation', value => value.replace('data-acyclic-obligation=', 'data-removed-obligation=')],
    ['mutation input', value => value.replace(/data-acyclic-input="\d+"/, 'data-acyclic-input="999"')],
    ['visible nonvacuity input', value => corruptVisibleInput(value,
      'data-acyclic-obligation="base::guarded/acceptance"',
      report.obligations[0].verdict.verification.nonvacuity.arguments[0].value)],
    ['visible mutation input', value => corruptVisibleInput(value,
      'data-acyclic-mutation="guard_mutant::guarded/acceptance"', report.obligations[8].witness.input)],
    ['source outcome', value => value.replace(/(data-acyclic-outcome="original"[^>]*>)[^<]+/, '$1unrelated result')],
    ['download path', value => value.replace(/(data-acyclic-download="[^"]+"[^>]*href=")[^"]+/, '$1/chialisp-oxide/reports/outside.json')],
    ['download hash', value => value.replace(artifacts[0].sha256, '0'.repeat(64))],
    ['source copy', value => value.replace('data-code="', 'data-code="changed-source')],
    ['page budget', value => value + 'x'.repeat(512 * 1024)],
  ];
  for (const [label, mutate] of corruptions) {
    const changed = mutate(markup); assert.notEqual(changed, markup, `Vacuous display ${label}`);
    assert.throws(() => checkAcyclicDisplay(changed, report, artifacts, sources), undefined, `Accepted display ${label}`);
    displayControls++;
  }
  for (const entry of artifacts) assert.deepEqual(await readFile(path.join(root, 'www/dist/reports/acyclic-scalar', entry.basename)), entry.bytes);
  assert.deepEqual(await readFile(path.join(root, 'www/dist/reports/acyclic-scalar.json')), actual.raw);
}
console.log(`PASS: 76 exact direct artifacts, 10 current source files, ${metadataControls} metadata, ${fileControls} file/budget, ${displayControls} actual-render controls; recorded checks only`);
