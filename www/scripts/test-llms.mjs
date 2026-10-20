import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderMarkdown, generateMarkdown, siteURL, warning, sha256 } from '../llms.mjs';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const fixture = content => `<html><head><script>OUTSIDE_SCRIPT</script></head><body><nav>OUTSIDE_NAV</nav><main><h1>Fixture title</h1><div class="sl-markdown-content">${content}</div></main></body></html>`;
const input = fixture(`<p>Rendered component <strong>value</strong>.</p><details><summary>Witness</summary><p>accepted · 4,819</p></details>
<pre data-language="rust"><code><div class="ec-line"><div class="code">fn puzzle() {</div></div><div class="ec-line"><div class="code">\n</div></div><div class="ec-line"><div class="code">    assert!(true);</div></div><div class="ec-line"><div class="code">}</div></div></code></pre>
<table><thead><tr><th>Build</th><th>Outcome</th></tr></thead><tbody><tr><td>Rust / on</td><td>program/raise</td></tr></tbody></table>
<p><a href="../reference/abi/#integers">ABI</a></p><script>INNER_SCRIPT</script><button>COPY_UI</button><span aria-hidden="true">HIDDEN_UI</span>`);
const { markdown } = await renderMarkdown(input, siteURL + 'validation/inputs/');
for (const value of [warning, '# Fixture title', 'Rendered component **value**', 'Witness', 'accepted · 4,819',
  '```rust\nfn puzzle() {\n\n    assert!(true);\n}\n```', '| Rust / on | program/raise |',
  siteURL + 'validation/reference/abi/#integers']) assert(markdown.includes(value), `Reading format lost ${value}`);
for (const value of ['OUTSIDE_SCRIPT', 'OUTSIDE_NAV', 'INNER_SCRIPT', 'COPY_UI', 'HIDDEN_UI', '<script', 'class=']) assert(!markdown.includes(value));
await assert.rejects(renderMarkdown('<main></main>', siteURL), /title\/content/);
await assert.rejects(renderMarkdown('<p>no main</p>', siteURL), /document main/);
await assert.rejects(renderMarkdown(fixture('<a href="javascript:alert(1)">bad</a>'), siteURL), /unsafe content link/);

const output = await generateMarkdown(root);
const read = file => readFile(path.join(root, file), 'utf8');
assert.equal(await read('llms.txt'), output.index, 'Curated index is stale');
assert.equal(await read('llms-full.txt'), output.full, 'Full Markdown is stale');
assert.equal(await read('llms-sitemap.xml'), output.sitemap, 'Markdown sitemap is stale');
assert.deepEqual(JSON.parse(await read('llms-manifest.json')), output.manifest, 'Markdown inventory is stale');
for (const page of output.pages) {
  assert.equal(await read(page.markdownPath), page.markdown, `${page.markdownPath}: stale reading format`);
  const html = await read(page.htmlPath);
  assert(html.includes('rel="alternate" type="text/markdown" href="' + new URL(page.markdownPath, siteURL).pathname + '"'), `${page.route}: missing Markdown alternative`);
  assert(html.includes('rel="describedby" href="/chialisp-oxide/llms.txt"'), `${page.route}: missing curated discovery`);
  assert(html.includes('rel="sitemap" type="application/xml" href="/chialisp-oxide/llms-sitemap.xml"'), `${page.route}: missing sitemap discovery`);
  assert(output.sitemap.includes(`<loc>${new URL(page.markdownPath, siteURL)}</loc>`));
  assert.equal(page.htmlSHA256, sha256(html));
  assert.equal(page.markdownSHA256, sha256(page.markdown));
  assert(output.full.includes(page.markdown));
}
const guide = await read('reference/agent-guide/index.md');
const signature = await readFile(new URL('../../puzzles/signature-payment/puzzle.rs', import.meta.url), 'utf8');
assert(guide.includes(signature.trimEnd()), 'Rendered repository guide must preserve the actual Rust source');
const cap = await read('validation/inputs/index.md');
const corpus = JSON.parse(await readFile(new URL('../../corpus/validation.json', import.meta.url), 'utf8'));
for (const slug of ['capped-payment', 'inner-puzzle']) {
  const puzzle = corpus.puzzles.find(item => item.slug === slug);
  for (const source of [puzzle.source, puzzle.reference, puzzle.rue.source]) assert(cap.includes(source.trimEnd()), `${slug}: Markdown lost a resolved language source`);
}
for (const value of ['program/raise', '4,819', '1,000', 'mismatched commitment rejects']) assert(cap.includes(value), `Markdown lost rendered evidence: ${value}`);
for (const artifact of corpus.puzzles.find(item => item.slug === 'capped-payment').artifacts.filter(item => item.backend === 'rust_mir')) {
  assert(cap.includes(artifact.disassembly.trimEnd()), 'Markdown lost the actual emitted decoder/body program');
}
assert(!output.full.includes('import ValidationCases from'), 'Raw MDX imports leaked into full text');
assert(!output.full.includes('<ValidationCases '), 'Unresolved evidence components leaked into full text');
// Deterministic rendering must remain sensitive to changed component outcomes.
assert.notEqual((await renderMarkdown(input.replace('4,819', '4,820'), siteURL + 'validation/inputs/')).markdown, markdown);
console.log(`PASS Markdown discovery: ${output.pages.length} exact rendered pages, ${Buffer.byteLength(output.full)} full-text bytes, live component/source preservation and malformed-input controls`);
