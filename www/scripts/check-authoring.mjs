import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = file => readFile(new URL(`../../${file}`, import.meta.url), 'utf8');
const guide = await read('docs/llms.md');
const blocks = [...guide.matchAll(/^```rust\n([\s\S]*?)^```/gm)];
assert.equal(blocks.length, 1, 'Every authoring Rust example must have an explicit freshness binding');
assert.equal(blocks[0][1], await read('puzzles/signature-payment/puzzle.rs'), 'Authoring example differs from the real corpus export');
for (const [file, link] of [['README.md', 'docs/llms.md'], ['AGENTS.md', 'docs/llms.md'], ['CLAUDE.md', 'docs/llms.md']]) {
  assert((await read(file)).includes(`](${link})`), `${file}: missing authoring entry point`);
}
const toolchain = await read('rust-toolchain.toml');
assert(guide.includes(toolchain.match(/channel\s*=\s*"([^"]+)"/)[1]), 'Authoring toolchain is stale');
for (const text of ['WIP prototype', 'Not ready for use', 'cargo chialisp-oxide check', 'cargo chialisp-oxide build',
  'cargo check', 'cargo test', '#[chialisp_oxide::puzzle]', 'unpublished', 'full simulator equality',
  'inconclusive', 'compiler preservation', '/validation/inputs/', '/validation/outputs/', '/validation/review/']) {
  assert(guide.includes(text), `Missing authoring contract: ${text}`);
}
console.log('PASS authoring guide: exact corpus export, pinned toolchain, standard discovery entry points and policy/testing boundaries');
