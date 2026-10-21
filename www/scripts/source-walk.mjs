// Independent Node source discovery. No generated Cargo target tree is evidence.
import { opendir, lstat } from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';

const defaults = Object.freeze({ depth: 64, directoryEntries: 4096, sources: 4096, directories: 4096, entries: 16384 });
// UTF-8 byte order agrees with Python's Unicode scalar order for valid paths;
// JavaScript's default UTF-16 order differs for supplementary characters.
export const compareSourcePaths = (a, b) => Buffer.compare(Buffer.from(a, 'utf8'), Buffer.from(b, 'utf8'));

export async function sourceFiles(root, directories, { suffixes = ['.rs'], names = [], recursive = true, limits = {}, openDirectory = opendir } = {}) {
  const bounds = { ...defaults, ...limits };
  assert(Object.keys(bounds).length === Object.keys(defaults).length, 'Unknown source walk limit');
  for (const [key, maximum] of Object.entries(defaults)) {
    assert(Number.isInteger(bounds[key]) && bounds[key] >= 0 && bounds[key] <= maximum, 'Invalid source walk limit');
  }
  root = path.resolve(root);
  if (typeof directories === 'string') directories = [directories];
  const found = new Set();
  let directoryCount = 0, entryCount = 0;
  function reserveDirectory() {
    assert(directoryCount < bounds.directories, 'Source directory count exceeded');
    directoryCount++;
  }
  async function visit(directory, depth, reserved = false) {
    assert(depth <= bounds.depth, 'Source directory depth exceeded');
    if (!reserved) reserveDirectory();
    let stream;
    try {
      stream = await openDirectory(directory, { bufferSize: 8 });
    } catch (error) {
      if (error.code === 'ENOENT' && depth === 0) return;
      throw error;
    }
    const entries = [];
    // Async Dir closes on completion or abrupt exit. Charge before retaining
    // each entry; never use readdir's unbounded complete-directory array.
    for await (const entry of stream) {
      assert(entries.length < bounds.directoryEntries, 'Source directory entry count exceeded');
      assert(entryCount < bounds.entries, 'Source aggregate entry count exceeded');
      entryCount++;
      entries.push(entry);
    }
    const hasManifest = entries.some(entry => entry.name === 'Cargo.toml' && entry.isFile());
    for (const entry of entries) {
      const filename = path.join(directory, entry.name);
      if (entry.name === 'target' && hasManifest && (entry.isDirectory() || entry.isSymbolicLink())) continue;
      assert(!entry.isSymbolicLink(), 'Source discovery rejects non-cache child symlinks');
      if (entry.isDirectory()) {
        if (recursive) await visit(filename, depth + 1);
      } else if (entry.isFile() && (suffixes === null || suffixes.some(suffix => entry.name.endsWith(suffix)) || names.includes(entry.name))) {
        const relative = path.relative(root, filename).split(path.sep).join('/');
        if (!found.has(relative)) {
          assert(found.size < bounds.sources, 'Source file count exceeded');
          found.add(relative);
        }
      }
    }
  }
  for (const directory of directories) {
    assert(typeof directory === 'string' && directory && !directory.includes('\\')
      && Buffer.byteLength(directory) <= 4096 && directory.split('/').length <= 64
      && !/[\x00-\x1f]/.test(directory) && !path.posix.isAbsolute(directory)
      && directory.split('/').every(part => part && part !== '.' && part !== '..'),
    'Source root must be a normalized relative path');
    reserveDirectory();
    let start = root, missing = false;
    for (const part of directory.split('/')) {
      start = path.join(start, part);
      let metadata;
      try { metadata = await lstat(start); }
      catch (error) { if (error.code === 'ENOENT') { missing = true; break; } throw error; }
      assert(metadata.isDirectory() && !metadata.isSymbolicLink(), 'Source root cannot contain symlinks or non-directories');
    }
    if (!missing) await visit(start, 0, true);
  }
  return [...found].sort(compareSourcePaths);
}
