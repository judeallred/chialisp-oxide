import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { byteProofArtifacts, readByteProofArtifact } from '../src/lib/byte-proof-artifacts.mjs';

const report = JSON.parse(await readFile(new URL('../../corpus/byte-proofs.json', import.meta.url), 'utf8'));
const entries = byteProofArtifacts(report);
assert.equal(entries.length, 65, 'The publication contract is exactly 64 outputs plus one specification');
assert.equal(new Set(entries.map(entry => entry.basename)).size, 65);
const originalCwd = process.cwd();
const sourceRoot = fileURLToPath(new URL('../../', import.meta.url));
const actualBytes = new Map();
for (const entry of entries) {
  const actual = await readByteProofArtifact(entry);
  assert.deepEqual(actual, await readFile(path.join(sourceRoot, entry.path)));
  assert.equal(createHash('sha256').update(actual).digest('hex'), entry.sha256);
  actualBytes.set(entry.basename, actual);
}

let controls = 0;
const metadataMutations = [
  ['missing artifact', value => value.artifacts.pop()],
  ['duplicate artifact', value => { value.artifacts[1] = structuredClone(value.artifacts[0]); }],
  ['extra artifact', value => value.artifacts.push(structuredClone(value.artifacts[0]))],
  ['unknown model', value => { value.artifacts[0].path = 'proofs/atom-not-equal/unknown.ir.json'; }],
  ['wrong prefix', value => { value.artifacts[0].path = 'proofs/p2-conditions/original.ir.json'; }],
  ['parent traversal', value => { value.artifacts[0].path = 'proofs/atom-not-equal/../original.ir.json'; }],
  ['normalized alias', value => { value.artifacts[0].path = 'proofs/atom-not-equal/sub/../original.ir.json'; }],
  ['double separator', value => { value.artifacts[0].path = 'proofs/atom-not-equal//original.ir.json'; }],
  ['backslash path', value => { value.artifacts[0].path = 'proofs/atom-not-equal\\original.ir.json'; }],
  ['absolute path', value => { value.artifacts[0].path = '/proofs/atom-not-equal/original.ir.json'; }],
  ['encoded separator', value => { value.artifacts[0].path = 'proofs/atom-not-equal/%2foriginal.ir.json'; }],
  ['missing hash', value => { delete value.artifacts[0].sha256; }],
  ['malformed hash', value => { value.artifacts[0].sha256 = 'A'.repeat(64); }],
  ['missing specification', value => { delete value.specification; }],
  ['wrong specification', value => { value.specification.path = 'proofs/atom-not-equal/original.ir.json'; }],
  ['specification in model outputs', value => { value.artifacts[0] = structuredClone(value.specification); }],
];
for (const [label, mutate] of metadataMutations) {
  const bad = structuredClone(report);
  mutate(bad);
  assert.throws(() => byteProofArtifacts(bad), undefined, `Accepted ${label}`);
  controls++;
}

const temporary = await mkdtemp(path.join(os.tmpdir(), 'chialisp-oxide-byte-downloads-'));
const checkout = path.join(temporary, 'checkout');
const directory = path.join(checkout, 'proofs/atom-not-equal');
try {
  await mkdir(directory, { recursive: true });
  await mkdir(path.join(checkout, 'www'));
  await writeFile(path.join(checkout, 'www/package.json'), JSON.stringify({ name: 'chialisp-oxide-docs' }));
  for (const entry of entries) await writeFile(path.join(directory, entry.basename), actualBytes.get(entry.basename));
  process.chdir(checkout);
  const entry = entries.find(entry => entry.basename === 'original.ir.json');
  assert(entry);
  const filename = path.join(directory, entry.basename);
  const saved = actualBytes.get(entry.basename);

  await assert.rejects(() => readByteProofArtifact({ ...entry, sha256: '0'.repeat(64) }), /SHA-256/);
  controls++;
  await writeFile(filename, Buffer.concat([saved, Buffer.from('\nchanged\n')]));
  await assert.rejects(() => readByteProofArtifact(entry), /SHA-256/);
  controls++;
  await rm(filename);
  await assert.rejects(() => readByteProofArtifact(entry), { code: 'ENOENT' });
  controls++;
  await writeFile(filename, saved);

  const outsideFile = path.join(temporary, 'outside.ir.json');
  await copyFile(filename, outsideFile);
  await rm(filename);
  await symlink(outsideFile, filename);
  await assert.rejects(() => readByteProofArtifact(entry), /symlink/);
  controls++;
  await rm(filename);
  await symlink(path.join(directory, 'specification.json'), filename);
  await assert.rejects(() => readByteProofArtifact(entry), /symlink/);
  controls++;
  await rm(filename);
  await writeFile(filename, saved);

  const outsideDirectory = path.join(temporary, 'outside-directory');
  await rename(directory, outsideDirectory);
  await symlink(outsideDirectory, directory);
  await assert.rejects(() => readByteProofArtifact(entry), /symlink/);
  controls++;
} finally {
  process.chdir(originalCwd);
  await rm(temporary, { recursive: true, force: true });
}
console.log(`PASS: ${entries.length} exact byte-proof downloads and ${controls} metadata/path/hash/symlink corruption controls`);
