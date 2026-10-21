import assert from 'node:assert/strict';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { artifactFile, previewLimit } from '../src/lib/compiled-artifacts.mjs';

const parser = unified().use(rehypeParse);
const text = node => node.type === 'text' ? node.value : (node.children ?? []).map(text).join('');
const normalized = node => text(node).replace(/\s+/g, ' ').trim();
function all(node, predicate) {
  return [...(predicate(node) ? [node] : []), ...(node.children ?? []).flatMap(child => all(child, predicate))];
}
const count = value => `${value.toLocaleString('en-US')} ${value === 1 ? 'byte' : 'bytes'}`;
const offset = value => `0x${value.toString(16).padStart(6, '0')}`;

// Read the complete recorded byte arrays independently of the bounded display.
function replacement(before, after) {
  const left = Buffer.from(before, 'hex');
  const right = Buffer.from(after, 'hex');
  let prefix = 0;
  while (prefix < left.length && prefix < right.length && left[prefix] === right[prefix]) prefix++;
  let suffix = 0;
  while (suffix < left.length - prefix && suffix < right.length - prefix
    && left[left.length - suffix - 1] === right[right.length - suffix - 1]) suffix++;
  return { identical: left.equals(right), prefix, suffix,
    sides: [left, right].map(bytes => ({ offset: prefix, end: bytes.length - suffix,
      bytes: bytes.subarray(prefix, bytes.length - suffix), total: bytes.length })) };
}

export function checkArtifactDiffDisplay(html, puzzle) {
  const get = (backend, optimize) => {
    const artifact = puzzle.artifacts.find(item => item.backend === backend && item.optimize_lir === optimize);
    assert(artifact, `${puzzle.slug}: missing byte-diff input`);
    return artifact;
  };
  const rustOff = get('rust_mir', false), rustOn = get('rust_mir', true);
  const chialisp = puzzle.artifacts.find(item => item.backend === 'chialisp');
  assert(chialisp);
  const rueOff = get('rue_source', false), rueOn = get('rue_source', true);
  const pairs = [[rustOff, rustOn], [chialisp, rustOn], [chialisp, rueOn], [rustOn, rueOn], [rueOff, rueOn]];
  const details = all(parser.parse(html), node => node.tagName === 'details' && node.properties?.dataArtifactDiff !== undefined);
  assert.equal(details.length, pairs.length, `${puzzle.slug}: byte-diff comparison count`);
  pairs.forEach(([before, after], index) => {
    const detail = details[index];
    assert.equal(detail.properties.dataArtifactDiff, `${before.backend}:${before.optimize_lir}/${after.backend}:${after.optimize_lir}`);
    const diff = replacement(before.clvm_hex, after.clvm_hex);
    const summary = all(detail, node => node.tagName === 'summary');
    assert.equal(summary.length, 1);
    assert(normalized(summary[0]).endsWith(diff.identical ? 'identical bytes' : `−${count(diff.sides[0].bytes.length)} / +${count(diff.sides[1].bytes.length)}`), 'Displayed full replacement counts differ');
    if (diff.identical) {
      assert(normalized(detail).includes(`All ${count(diff.sides[0].total)} match; there is no replaced range.`));
      assert.equal(all(detail, node => node.properties?.dataByteRange !== undefined).length, 0);
      return;
    }
    assert(normalized(detail).includes(`Unchanged: ${count(diff.prefix)} at the start · ${count(diff.suffix)} at the end.`));
    assert(normalized(detail).includes('Matching previews do not establish identical artifacts or semantic equivalence.'));
    diff.sides.forEach((range, side) => {
      const kind = side ? 'added' : 'removed';
      const marker = side ? '+' : '-';
      const paragraphs = all(detail, node => node.tagName === 'p' && node.properties?.dataByteRange === kind);
      assert.equal(paragraphs.length, 1);
      assert.equal(normalized(paragraphs[0]), `[${offset(range.offset)}, ${offset(range.end)}) · ${count(range.bytes.length)} / ${count(range.total)} total`, 'Displayed full byte range differs');
      const column = all(detail, node => node.tagName === 'section'
        && (node.properties?.className ?? []).includes(kind));
      assert.equal(column.length, 1);
      const previews = all(column[0], node => node.tagName === 'pre' && node.properties?.dataBytePreview === kind);
      if (!range.bytes.length) {
        assert.equal(previews.length, 0);
        assert(normalized(column[0]).includes('∅ · No bytes on this side of the replacement.'));
        return;
      }
      assert.equal(previews.length, 1);
      const prefix = range.bytes.subarray(0, previewLimit / 2);
      const lines = [];
      for (let start = 0; start < prefix.length; start += 16) {
        lines.push(`${marker} ${offset(range.offset + start)}  ${Array.from(prefix.subarray(start, start + 16), byte => byte.toString(16).padStart(2, '0')).join(' ')}`);
      }
      assert.equal(text(previews[0]), lines.join('\n'), 'Displayed byte prefix or offsets differ');
      const note = range.bytes.length > previewLimit / 2
        ? `Prefix only: ${previewLimit / 2} of ${range.bytes.length} replacement bytes shown. This prefix is incomplete.`
        : 'Complete replacement range shown.';
      assert(normalized(column[0]).includes(note), 'Missing accurate prefix scope');
      const download = all(column[0], node => node.tagName === 'a' && text(node) === 'Download exact complete artifact');
      assert.equal(download.length, 1);
      assert.equal(download[0].properties.href,
        `/chialisp-oxide/reports/compiled/${artifactFile(puzzle.slug, side ? after : before, 'clvm.hex')}`, 'Full artifact download differs');
    });
  });
  return details.length;
}
