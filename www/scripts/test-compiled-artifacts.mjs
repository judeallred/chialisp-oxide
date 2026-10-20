import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { artifactFile, preview, previewLimit } from '../src/lib/compiled-artifacts.mjs';

const report = JSON.parse(await readFile(new URL('../../corpus/validation.json',import.meta.url),'utf8'));
const directory = new URL('../dist/reports/compiled/',import.meta.url);
const parser = unified().use(rehypeParse);
function copies(node) {
  return [...(typeof node.properties?.dataCode === 'string' ? [node.properties.dataCode.replaceAll('\x7f','\n')] : []),
    ...(node.children ?? []).flatMap(copies)];
}
const expected = [];
let prefixes = 0;
for (const puzzle of report.puzzles) {
  const html = await readFile(new URL(`../dist/lab/${puzzle.slug}/index.html`,import.meta.url),'utf8');
  const copied = copies(parser.parse(html));
  for (const artifact of puzzle.artifacts) {
    for (const [extension,value] of [['clvm.txt',artifact.disassembly],['clvm.hex',artifact.clvm_hex]]) {
      const file = artifactFile(puzzle.slug,artifact,extension);
      expected.push(file);
      assert.equal(await readFile(new URL(file,directory),'utf8'),value+'\n','Download differs from exact recorded artifact');
      assert(html.includes(`href="/chialisp-oxide/reports/compiled/${file}"`));
      const normalized = preview(value).trim();
      assert(copied.includes(normalized),`${file}: compiled preview differs`);
      if (value.length > previewLimit) { assert(html.includes(`${previewLimit} characters`)); prefixes++; }
    }
    assert.equal(Buffer.from(artifact.clvm_hex,'hex').length,artifact.bytes);
  }
}
assert.deepEqual((await readdir(directory)).sort(),expected.sort(),'Unexpected or missing compiled downloads');
for (const bad of ['../escape','Uppercase','a/b']) assert.throws(()=>artifactFile(bad,report.puzzles[0].artifacts[0],'clvm.hex'));
assert.equal(preview('x'.repeat(previewLimit+1)),'x'.repeat(previewLimit));
assert.equal(preview('complete'),'complete');
console.log(`PASS compiled display: ${expected.length} exact full downloads, bounded copy payloads, ${prefixes} labeled prefixes and path controls; human source comparisons remain complete`);
