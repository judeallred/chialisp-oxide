// Actual temporary-directory controls; no compiler/VM/provenance execution.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, opendir, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { sourceFiles } from './source-walk.mjs';

async function fixture(t) {
  const root = await mkdtemp(path.join(tmpdir(), 'oxide-source-walk-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const files = ['pkg/Cargo.toml', 'pkg/src/lib.rs', 'pkg/src/target/real.rs', 'pkg/plain/target/real.rs',
    'pkg/tests/fixtures/consumer/Cargo.toml', 'pkg/tests/fixtures/consumer/src/lib.rs',
    'pkg/target/deps/cache.rs', 'pkg/tests/fixtures/consumer/target/deps/cache.rs', 'pkg/notes.txt'];
  for (const name of files) {
    await mkdir(path.dirname(path.join(root, name)), { recursive: true });
    await writeFile(path.join(root, name), 'temporary fixture\n');
  }
  const caches = ['pkg/target', 'pkg/tests/fixtures/consumer/target'].map(name => path.join(root, name));
  const guarded = (directory, options) => {
    assert(!caches.some(cache => directory === cache || directory.startsWith(cache + path.sep)), 'entered a Cargo build cache');
    assert.equal(options.bufferSize, 8);
    return opendir(directory, options);
  };
  return { root, guarded };
}

test('Cargo caches are never opened; ordinary target modules and Python inventory agree', async t => {
  const { root, guarded } = await fixture(t);
  const result = await sourceFiles(root, ['pkg'], { openDirectory: guarded });
  assert.deepEqual(result, ['pkg/plain/target/real.rs', 'pkg/src/lib.rs', 'pkg/src/target/real.rs', 'pkg/tests/fixtures/consumer/src/lib.rs']);
  for (const name of ['\uE000.rs', '\u{10000}.rs']) await writeFile(path.join(root, 'pkg/src', name), 'Unicode source path');
  const scripts = fileURLToPath(new URL('../../scripts', import.meta.url));
  for (const suffixes of [['.rs'], null]) {
    const node = await sourceFiles(root, ['pkg'], { suffixes, openDirectory: guarded });
    const python = execFileSync('python3', ['-B', '-c',
      'import json,sys;from source_walk import source_files;v=json.load(sys.stdin);print(json.dumps(source_files(v["root"],["pkg"],suffixes=None if v["suffixes"] is None else tuple(v["suffixes"]))))'],
    { input: JSON.stringify({ root, suffixes }), encoding: 'utf8', env: { ...process.env, PYTHONPATH: scripts } });
    assert.deepEqual(node, JSON.parse(python));
    if (suffixes === null) assert(node.includes('pkg/notes.txt'));
  }
});

test('non-Cargo target permission errors and vanished children propagate', async t => {
  const { root, guarded } = await fixture(t);
  for (const code of ['EACCES', 'ENOENT']) {
    const blocked = (directory, options) => {
      if (directory === path.join(root, 'pkg/plain/target')) throw Object.assign(new Error('constructed failure'), { code });
      return guarded(directory, options);
    };
    await assert.rejects(sourceFiles(root, 'pkg', { openDirectory: blocked }), { code });
  }
  assert.deepEqual(await sourceFiles(root, 'missing'), []);
});

test('regular Cargo cache links are excluded and other source symlinks fail closed', async t => {
  const { root, guarded } = await fixture(t);
  await mkdir(path.join(root, 'pkg/name-only/Cargo.toml'), { recursive: true });
  await mkdir(path.join(root, 'pkg/name-only/target'));
  await writeFile(path.join(root, 'pkg/name-only/target/module.rs'), 'source');
  await writeFile(path.join(root, 'outside.rs'), 'outside');
  const result = await sourceFiles(root, 'pkg', { openDirectory: guarded });
  assert(result.includes('pkg/name-only/target/module.rs'));
  for (const [name, target] of [['alias.rs', 'outside.rs'], ['alias-directory', 'pkg/src'], ['broken', 'missing-source']]) {
    const link = path.join(root, 'pkg', name);
    await symlink(path.join(root, target), link);
    try {
      await assert.rejects(sourceFiles(root, 'pkg', { openDirectory: guarded }), /child symlinks/);
      if (name === 'alias-directory') {
        for (const relative of ['pkg/alias-directory', 'pkg/alias-directory/target']) await assert.rejects(sourceFiles(root, relative), /symlinks/);
      }
    } finally { await rm(link); }
  }
  const cache = path.join(root, 'pkg/cache-link');
  await mkdir(cache); await writeFile(path.join(cache, 'Cargo.toml'), 'regular Cargo manifest');
  const cacheGuard = (directory, options) => { assert.notEqual(directory, path.join(cache, 'target')); return guarded(directory, options); };
  for (const target of ['pkg/src', 'missing-cache']) {
    await symlink(path.join(root, target), path.join(cache, 'target'));
    try { assert.deepEqual(await sourceFiles(root, 'pkg', { openDirectory: cacheGuard }), result); }
    finally { await rm(path.join(cache, 'target')); }
  }
  await mkdir(path.join(root, 'pkg/linked-manifest/target'), { recursive: true });
  await symlink(path.join(root, 'pkg/Cargo.toml'), path.join(root, 'pkg/linked-manifest/Cargo.toml'));
  await writeFile(path.join(root, 'pkg/linked-manifest/target/module.rs'), 'source');
  await assert.rejects(sourceFiles(root, 'pkg', { openDirectory: guarded }), /child symlinks/);
});

test('depth, per-directory, aggregate directory/entry and source limits are enforced', async t => {
  const { root } = await fixture(t);
  await mkdir(path.join(root, 'tight'));
  await writeFile(path.join(root, 'tight/a.rs'), 'source');
  await writeFile(path.join(root, 'tight/b.rs'), 'source');
  for (const limits of [{ sources: 1 }, { directoryEntries: 1 }, { entries: 1 }]) {
    await assert.rejects(sourceFiles(root, 'tight', { limits }));
  }
  await mkdir(path.join(root, 'tree/child/grandchild'), { recursive: true });
  for (const limits of [{ depth: 1 }, { directories: 1 }]) await assert.rejects(sourceFiles(root, 'tree', { limits }));
  assert.deepEqual(await sourceFiles(root, 'tree', { limits: { depth: 2, directories: 3, entries: 2 } }), []);
  await assert.rejects(sourceFiles(root, 'tight', { limits: { sources: 4097 } }));
  for (const relative of ['../pkg', '/pkg', 'pkg//src', 'pkg/./src', 'pkg\\src']) await assert.rejects(sourceFiles(root, relative));
});

test('stream stops at the first excess entry and closes instead of buffering a whole directory', async t => {
  const { root } = await fixture(t);
  await mkdir(path.join(root, 'virtual'));
  const seen = []; let closed = false;
  async function* entries() {
    try {
      for (let index = 0; index < 3; index++) {
        seen.push(index); assert(index <= 1, 'consumed beyond first excess entry'); yield {};
      }
    } finally { closed = true; }
  }
  await assert.rejects(sourceFiles(root, 'virtual', { limits: { directoryEntries: 1 }, openDirectory: async () => entries() }), /directory entry count/);
  assert.deepEqual(seen, [0, 1]); assert(closed);
});

test('budgets aggregate across starting roots and shallow inventory stays shallow', async t => {
  const { root } = await fixture(t);
  for (const directory of ['left', 'right']) {
    await mkdir(path.join(root, directory)); await writeFile(path.join(root, directory, 'file.rs'), 'source');
  }
  const limits = { directories: 2, entries: 2, sources: 2, depth: 0 };
  assert.deepEqual(await sourceFiles(root, ['left', 'right'], { limits }), ['left/file.rs', 'right/file.rs']);
  for (const lower of [{ directories: 1 }, { entries: 1 }, { sources: 1 }]) {
    await assert.rejects(sourceFiles(root, ['left', 'right'], { limits: { ...limits, ...lower } }));
  }
  await assert.rejects(sourceFiles(root, ['missing', 'also-missing'], { limits: { directories: 1 } }));
  await mkdir(path.join(root, 'shallow/nested'), { recursive: true });
  await writeFile(path.join(root, 'shallow/direct.py'), 'source');
  await writeFile(path.join(root, 'shallow/nested/hidden.py'), 'outside shallow inventory');
  const directOnly = (directory, options) => { assert.equal(directory, path.join(root, 'shallow')); return opendir(directory, options); };
  assert.deepEqual(await sourceFiles(root, 'shallow', { suffixes: ['.py'], recursive: false, openDirectory: directOnly }), ['shallow/direct.py']);
});
