import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { MAX_SOURCE_DATA_BYTES, sourceDataArtifact, sourceDataArtifacts, readSourceDataArtifact } from '../src/lib/source-data.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const report = JSON.parse(await readFile(path.join(root, 'corpus/validation.json'), 'utf8'));
const artifacts = sourceDataArtifacts(report);
assert.deepEqual(artifacts.map(entry => entry.basename), ['prefix-list-generated-cases.json', 'atom-not-equal-generated-cases.json', 'insertion-sort-by-keys-generated-cases.json']);
assert.equal(artifacts.length, 3);
const actualBytes = new Map();
for (const entry of artifacts) {
  const bytes = await readSourceDataArtifact(entry);
  const dependency = report.puzzles.flatMap(puzzle => puzzle.source_dependencies).find(dependency => dependency.path === entry.path);
  assert.deepEqual(bytes, await readFile(path.join(root, entry.path)));
  assert.equal(bytes.toString('utf8'), dependency.source);
  assert.equal(bytes.length, entry.bytes);
  assert.equal(JSON.parse(bytes).length, entry.rows);
  actualBytes.set(entry.path, bytes);
}
assert.equal(sourceDataArtifact('puzzles/other/generated-cases.json'), undefined);
assert.equal(sourceDataArtifact('puzzles/prefix-list/../prefix-list/generated-cases.json'), undefined);

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const owner = value => value.puzzles.find(puzzle => puzzle.slug === 'prefix-list');
const dependency = value => owner(value).source_dependencies.find(row => row.path === artifacts[0].path);
const setSource = (value, source) => {
  dependency(value).source = source;
  dependency(value).sha256 = hash(source);
};
let controls = 0;
const mutations = [
  ['missing fixture', value => { value.puzzles = value.puzzles.filter(puzzle => puzzle.slug !== 'prefix-list'); }],
  ['duplicate fixture', value => value.puzzles.push(structuredClone(owner(value)))],
  ['missing dependency', value => { owner(value).source_dependencies = []; }],
  ['duplicate dependency', value => owner(value).source_dependencies.push(structuredClone(dependency(value)))],
  ['wrong owner', value => value.puzzles[0].source_dependencies.push(structuredClone(dependency(value)))],
  ['parent path', value => { dependency(value).path = 'puzzles/prefix-list/../prefix-list/generated-cases.json'; }],
  ['absolute path', value => { dependency(value).path = '/' + dependency(value).path; }],
  ['backslash path', value => { dependency(value).path = 'puzzles\\prefix-list\\generated-cases.json'; }],
  ['unknown dataset', value => { dependency(value).path = 'puzzles/prefix-list/other.json'; }],
  ['missing source', value => { delete dependency(value).source; }],
  ['malformed hash', value => { dependency(value).sha256 = 'F'.repeat(64); }],
  ['source/hash disagreement', value => { dependency(value).source += '\n'; }],
  ['malformed JSON with matching hash', value => setSource(value, '[invalid')],
  ['non-array JSON', value => setSource(value, '{}')],
  ['oversized source', value => setSource(value, ' '.repeat(MAX_SOURCE_DATA_BYTES + 1))],
  ['non-UTF8 string in valid JSON', value => { setSource(value, '["\ud800"]'); owner(value).generated_cases = 1; }],
  ['changed generated count', value => { owner(value).generated_cases += 1; }],
  ['Boolean generated count', value => { owner(value).generated_cases = true; }],
];
for (const [label, mutate] of mutations) {
  const bad = structuredClone(report);
  mutate(bad);
  assert.notDeepEqual(bad, report, `No-op control: ${label}`);
  assert.throws(() => sourceDataArtifacts(bad), undefined, `Accepted ${label}`);
  controls++;
}

const cwd = process.cwd();
const temporary = await mkdtemp(path.join(os.tmpdir(), 'chialisp-oxide-source-data-'));
const checkout = path.join(temporary, 'checkout');
try {
  await mkdir(path.join(checkout, 'www'), { recursive: true });
  await writeFile(path.join(checkout, 'www/package.json'), JSON.stringify({ name: 'chialisp-oxide-docs' }));
  for (const entry of artifacts) {
    await mkdir(path.dirname(path.join(checkout, entry.path)), { recursive: true });
    await writeFile(path.join(checkout, entry.path), actualBytes.get(entry.path));
  }
  process.chdir(checkout);
  const entry = artifacts[0];
  const filename = path.join(checkout, entry.path);
  const saved = actualBytes.get(entry.path);
  const reject = async (label, mutation, pattern) => {
    assert.notDeepEqual(mutation, entry, `No-op entry control: ${label}`);
    await assert.rejects(() => readSourceDataArtifact(mutation), pattern, label);
    controls++;
  };
  await reject('unknown path', { ...entry, path: 'Cargo.toml' }, /allowlisted/);
  await reject('wrong basename', { ...entry, basename: 'other.json' }, /allowlisted/);
  await reject('oversized declared bytes', { ...entry, bytes: MAX_SOURCE_DATA_BYTES + 1 }, /bounded/);
  await reject('invalid rows', { ...entry, rows: 0 }, /bounded/);
  await reject('missing hash', { ...entry, sha256: undefined }, /SHA-256/);
  await reject('wrong hash', { ...entry, sha256: '0'.repeat(64) }, /SHA-256/);
  await reject('wrong byte count', { ...entry, bytes: entry.bytes - 1 }, /size/);
  await reject('wrong row count', { ...entry, rows: entry.rows + 1 }, /row count/);

  await rm(filename);
  await assert.rejects(() => readSourceDataArtifact(entry), { code: 'ENOENT' }); controls++;
  const changed = Buffer.from(saved); changed[changed.length - 1] ^= 1;
  await writeFile(filename, changed);
  await assert.rejects(() => readSourceDataArtifact(entry), /SHA-256/); controls++;
  await writeFile(filename, saved.subarray(1));
  await assert.rejects(() => readSourceDataArtifact(entry), /size/); controls++;
  await writeFile(filename, Buffer.alloc(MAX_SOURCE_DATA_BYTES + 1));
  await assert.rejects(() => readSourceDataArtifact(entry), /size/); controls++;
  await writeFile(filename, Buffer.from([0xff]));
  await assert.rejects(() => readSourceDataArtifact({ ...entry, bytes: 1, sha256: hash(Buffer.from([0xff])), rows: 1 })); controls++;
  await writeFile(filename, '{}');
  await assert.rejects(() => readSourceDataArtifact({ ...entry, bytes: 2, sha256: hash('{}'), rows: 1 }), /row count/); controls++;
  await writeFile(filename, saved);

  const outside = path.join(temporary, 'outside.json');
  await writeFile(outside, saved);
  await rm(filename); await symlink(outside, filename);
  await assert.rejects(() => readSourceDataArtifact(entry), /symlink/); controls++;
  await rm(filename); await symlink(path.join(checkout, artifacts[1].path), filename);
  await assert.rejects(() => readSourceDataArtifact(entry), /symlink/); controls++;
  await rm(filename); await writeFile(filename, saved);
  const folder = path.dirname(filename);
  const elsewhere = path.join(temporary, 'outside-directory');
  await rename(folder, elsewhere); await symlink(elsewhere, folder);
  await assert.rejects(() => readSourceDataArtifact(entry), /symlink/); controls++;
  await rm(folder); await rename(elsewhere, folder);

  // The inclusive allocation bound has a valid boundary case; the next byte
  // fails above. This is a test-only temporary file, never a corpus edit.
  const boundary = '[{}]' + ' '.repeat(MAX_SOURCE_DATA_BYTES - 4);
  await writeFile(filename, boundary);
  const bound = { ...entry, bytes: MAX_SOURCE_DATA_BYTES, sha256: hash(boundary), rows: 1 };
  assert.equal((await readSourceDataArtifact(bound)).length, MAX_SOURCE_DATA_BYTES);
  const boundedRecord = structuredClone(report);
  setSource(boundedRecord, boundary); owner(boundedRecord).generated_cases = 1;
  assert.equal(sourceDataArtifacts(boundedRecord)[0].bytes, MAX_SOURCE_DATA_BYTES);
} finally {
  process.chdir(cwd);
  await rm(temporary, { recursive: true, force: true });
}
console.log(`PASS: ${artifacts.length} exact generated-data downloads (${artifacts.reduce((total, entry) => total + entry.bytes, 0)} bytes), inclusive 2 MiB boundary and ${controls} record/path/hash/size/symlink controls`);
