import { readFile, readdir, stat } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const base = '/chialisp-oxide/';
const origin = 'https://judeallred.github.io';

async function files(directory) {
  const contents = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(contents.map(entry => entry.isDirectory() ? files(path.join(directory, entry.name)) : [path.join(directory, entry.name)]))).flat();
}

const built = await files(root);
const pages = built.filter(file => file.endsWith('.html'));
assert(pages.length >= 30, 'Expected the full documentation and puzzle corpus');
const cache = new Map();
const html = async file => {
  if (!cache.has(file)) cache.set(file, await readFile(file, 'utf8'));
  return cache.get(file);
};

let checkedLinks = 0;
for (const file of pages) {
  const text = await html(file);
  assert.match(text, /<html[^>]*lang="en"/, `${file}: missing document language`);
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

for (const name of ['validation', 'proofs', 'inventory']) {
  const report = JSON.parse(await readFile(path.join(root, `reports/${name}.json`), 'utf8'));
  assert.equal(report.schema_version, 1, `${name}: report schema`);
}

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
  for (const term of ['recovery', 'canonical', 'AGG_SIG_ME']) {
    const search = await pagefind.search(term);
    assert(search.results.length > 0, `No production search results for ${term}`);
    const result = await search.results[0].data();
    assert(result.url.startsWith(base), `Search result escapes base: ${result.url}`);
    console.log(`Search ${JSON.stringify(term)}: ${search.results.length} results; first ${result.url}`);
  }
  await pagefind.destroy();
} finally {
  globalThis.fetch = originalFetch;
}

console.log(`PASS: ${pages.length} pages, ${checkedLinks} local links/assets/fragments, 3 JSON reports, and live Pagefind queries`);
