import { readdir } from 'node:fs/promises';
import path from 'node:path';

// Retained source packets may contain local build directories. Never enter
// them while enumerating published pages or assets, even if traversal fails.
const ignored = new Set(['target', '.git', 'node_modules']);

export async function siteFiles(directory, readDirectory = readdir, prefix = '') {
  const result = [];
  for (const entry of await readDirectory(directory, { withFileTypes: true })) {
    const relative = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) {
      if (!ignored.has(entry.name)) result.push(...await siteFiles(path.join(directory, entry.name), readDirectory, relative));
    } else if (entry.isFile()) result.push(relative);
  }
  return result.sort();
}
