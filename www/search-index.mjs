import assert from 'node:assert/strict';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as pagefind from 'pagefind';

// Pagefind 1.5.2 can acknowledge native disk writes before flushing them; its
// close() then kills the service. Use the public in-memory API and Node writes:
// https://github.com/Pagefind/pagefind/issues/1271
// https://pagefind.app/docs/node-api/#indexgetfiles
function response(result, action) {
  assert(result && Array.isArray(result.errors), `${action}: missing Pagefind error array`);
  assert.equal(result.errors.length, 0, `${action}: ${result.errors.join('; ')}`);
  return result;
}

export function validateFiles(files, outputDirectory) {
  assert(Array.isArray(files) && files.length > 0, 'Pagefind returned no files');
  const output = path.resolve(outputDirectory);
  const names = new Set();
  const validated = files.map(file => {
    assert(file && typeof file.path === 'string' && file.path.length > 0, 'Invalid Pagefind file path');
    const parts = file.path.split('/');
    assert(parts.every(part => /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(part)), `Unsafe Pagefind path: ${file.path}`);
    const destination = path.resolve(output, ...parts);
    const relative = path.relative(output, destination);
    assert(relative && relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative), `Pagefind path escapes output: ${file.path}`);
    // Also reject aliases on case-insensitive filesystems.
    const name = file.path.toLowerCase();
    assert(!names.has(name), `Duplicate Pagefind path: ${file.path}`);
    names.add(name);
    assert(file.content instanceof Uint8Array && file.content.byteLength > 0, `Empty or invalid Pagefind file: ${file.path}`);
    return { destination, content: file.content };
  });
  const directories = new Map();
  for (const file of files) {
    const parts = file.path.split('/');
    for (let index = 1; index < parts.length; index++) {
      const directory = parts.slice(0, index).join('/');
      const name = directory.toLowerCase();
      assert(!names.has(name), `Pagefind file/directory collision: ${file.path}`);
      assert(!directories.has(name) || directories.get(name) === directory, `Pagefind directory case alias: ${file.path}`);
      directories.set(name, directory);
    }
  }
  return validated;
}

export async function writeSearchIndex(directory, logger, service = pagefind) {
  const site = fileURLToPath(directory);
  const output = path.join(site, 'pagefind');
  try {
    const { index } = response(await service.createIndex(), 'createIndex');
    assert(index && typeof index.addDirectory === 'function' && typeof index.getFiles === 'function', 'Pagefind returned an invalid index');
    // Keep Starlight's default indexing options: no forced language, selectors,
    // custom records or URL rewrite. The stock client supplies BASE_URL.
    const { page_count } = response(await index.addDirectory({ path: site }), 'addDirectory');
    assert(Number.isInteger(page_count) && page_count > 0, 'Pagefind indexed no HTML pages');
    const { files } = response(await index.getFiles(), 'getFiles');
    const validated = validateFiles(files, output);
    // Validate the complete bundle before replacing any existing generated files.
    await rm(output, { recursive: true, force: true });
    for (const file of validated) {
      await mkdir(path.dirname(file.destination), { recursive: true });
      await writeFile(file.destination, file.content);
    }
    logger.info(`Indexed ${page_count} HTML pages; wrote ${validated.length} complete search assets.`);
  } finally {
    await service.close();
  }
}

export default function searchIndex() {
  return {
    name: 'chialisp-oxide:search-index',
    hooks: {
      'astro:build:done': ({ dir, logger }) => writeSearchIndex(dir, logger.fork('search-index')),
    },
  };
}
