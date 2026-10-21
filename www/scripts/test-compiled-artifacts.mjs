import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { artifactFile, preview, previewLimit } from '../src/lib/compiled-artifacts.mjs';
import { checkArtifactDiffDisplay } from './artifact-diff-display.mjs';

const report = JSON.parse(await readFile(new URL('../../corpus/validation.json',import.meta.url),'utf8'));
const directory = new URL('../dist/reports/compiled/',import.meta.url);
const parser = unified().use(rehypeParse);
function copies(node) {
  return [...(typeof node.properties?.dataCode === 'string' ? [node.properties.dataCode.replaceAll('\x7f','\n')] : []),
    ...(node.children ?? []).flatMap(copies)];
}
const expected = [];
let prefixes = 0;
let diffs = 0;
let corruptionControls = 0;
for (const puzzle of report.puzzles) {
  const html = await readFile(new URL(`../dist/lab/${puzzle.slug}/index.html`,import.meta.url),'utf8');
  diffs += checkArtifactDiffDisplay(html, puzzle);
  if (!corruptionControls) {
    const candidate = html.match(/<details\b[^>]*data-artifact-diff="[^"]+"[^>]*>[\s\S]*?<\/details>/g)
      ?.find(value => value.includes('Prefix only:'));
    if (candidate) {
      const changes = [
        ['full summary count', value => value.replace(/−[\d,]+ bytes?/, '−999999 bytes')],
        ['full range total', value => value.replace(/([\d,]+ bytes?) total/, '999999 bytes total')],
        ['full range offset', value => value.replace(/0x[0-9a-f]+/, '0xffffff')],
        ['preview byte', value => value.replace(/(data-byte-preview="[^"]+"[^>]*><code[^>]*>[^<]*?0x[0-9a-f]+\s+)[0-9a-f]{2}/, '$1XX')],
        ['prefix scope', value => value.replace('Prefix only:', 'Complete artifact:')],
        ['exact full download', value => value.replace(/(href=")[^"]+\.clvm\.hex/, '$1/chialisp-oxide/reports/compiled/wrong.clvm.hex')],
      ];
      for (const [label, change] of changes) {
        const changed = change(candidate);
        assert.notEqual(changed, candidate, `${label}: corruption did not alter fixture`);
        assert.throws(() => checkArtifactDiffDisplay(html.replace(candidate, changed), puzzle), undefined, `${label}: display corruption accepted`);
        corruptionControls++;
      }
    }
  }
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
assert.equal(corruptionControls, 6, 'Missing actual rendered byte-diff corruption controls');
console.log(`PASS compiled display: ${expected.length} exact full downloads, ${diffs} complete-input byte diffs, ${corruptionControls} visible display corruptions, bounded copy payloads, ${prefixes} labeled prefixes and path controls; human source comparisons remain complete`);
