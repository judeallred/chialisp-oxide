import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { validateFiles, writeSearchIndex } from '../search-index.mjs';

const content = new TextEncoder().encode('export const options = () => {};');
const good = [{ path: 'pagefind.js', content }, { path: 'index/en.pf_index', content: new Uint8Array([0, 1, 255]) }];
let controls = 0;
for (const files of [
  undefined, [], [{ path: '../escape.js', content }], [{ path: '/absolute.js', content }],
  [{ path: 'C:/escape.js', content }], [{ path: 'index\\escape.js', content }],
  [{ path: 'index//empty.js', content }], [{ path: 'index/./alias.js', content }],
  [{ path: 'index/../alias.js', content }], [{ path: 'bad\0.js', content }],
  [{ path: 'empty.js', content: new Uint8Array() }], [{ path: 'string.js', content: 'text' }],
  [good[0], good[0]], [good[0], { path: 'PAGEFIND.js', content }],
  [{ path: 'a', content }, { path: 'a/b.js', content }],
  [{ path: 'A', content }, { path: 'a/b.js', content }],
  [{ path: 'A/b.js', content }, { path: 'a/c.js', content }],
  [{ path: 'a/B/c.js', content }, { path: 'a/b/d.js', content }],
]) {
  assert.throws(() => validateFiles(files, '/tmp/search-output'));
  controls++;
}

const temporary = await mkdtemp(path.join(os.tmpdir(), 'chialisp-oxide-search-'));
const directory = pathToFileURL(temporary + path.sep);
const output = path.join(temporary, 'pagefind');
const sentinel = path.join(output, 'sentinel');
const logger = { info() {} };
const fixture = overrides => {
  const calls = [];
  const index = {
    async addDirectory(options) {
      calls.push('addDirectory');
      assert.deepEqual(options, { path: temporary + path.sep });
      return { errors: [], page_count: 2 };
    },
    async getFiles() { calls.push('getFiles'); return { errors: [], files: good }; },
    async writeFiles() { assert.fail('Native disk writes must never be used'); },
  };
  return {
    calls,
    async createIndex() { calls.push('createIndex'); return { errors: [], index }; },
    async close() { calls.push('close'); },
    ...overrides,
  };
};

try {
  const service = fixture();
  service.close = async () => {
    // close is allowed to kill the native process: Node already owns and has
    // finished writing every returned asset, including binary bytes.
    for (const file of good) assert.deepEqual(await readFile(path.join(output, file.path)), Buffer.from(file.content));
    service.calls.push('close');
  };
  await writeSearchIndex(directory, logger, service);
  assert.deepEqual(service.calls, ['createIndex', 'addDirectory', 'getFiles', 'close']);
  controls++;

  const badResponses = [
    { create: {} }, { create: { errors: ['native failure'] } }, { create: { errors: [] } },
    { add: { errors: ['index failure'] } }, { add: { errors: [] } }, { add: { errors: [], page_count: 0 } },
    { get: {} }, { get: { errors: ['bundle failure'] } },
    { get: { errors: [], files: [good[0], { path: '../escape.js', content }] } },
    { get: { errors: [], files: [good[0], { path: 'empty.js', content: new Uint8Array() }] } },
    { get: { errors: [], files: [{ path: 'a', content }, { path: 'a/b.js', content }] } },
    { get: { errors: [], files: [{ path: 'A', content }, { path: 'a/b.js', content }] } },
    { get: { errors: [], files: [{ path: 'A/b.js', content }, { path: 'a/c.js', content }] } },
  ];
  for (const bad of badResponses) {
    await mkdir(output, { recursive: true });
    await writeFile(sentinel, 'unchanged');
    let closed = false;
    const index = {
      async addDirectory() { return bad.add ?? { errors: [], page_count: 2 }; },
      async getFiles() { return bad.get ?? { errors: [], files: good }; },
    };
    const failing = {
      async createIndex() { return bad.create ?? { errors: [], index }; },
      async close() { closed = true; },
    };
    await assert.rejects(writeSearchIndex(directory, logger, failing));
    assert.equal(closed, true, 'Failed indexing must close the service');
    assert.equal(await readFile(sentinel, 'utf8'), 'unchanged', 'Invalid bundles must be rejected before replacing output');
    controls++;
  }
} finally {
  await rm(temporary, { recursive: true, force: true });
}
console.log(`PASS search writer: ${controls} path/content/API/lifecycle controls; complete Node writes before service close`);
