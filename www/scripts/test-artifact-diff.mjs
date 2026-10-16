import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

/**
 * One exact replacement bounded by the longest equal byte prefix and a
 * non-overlapping equal suffix. This is not an edit-distance or instruction diff.
 *
 * @param {string} beforeHex
 * @param {string} afterHex
 */
export function compareSerializedBytes(beforeHex, afterHex) {
  if (!/^(?:[0-9a-f]{2})*$/i.test(beforeHex) || !/^(?:[0-9a-f]{2})*$/i.test(afterHex)) {
    throw new Error('Serialized-byte comparison requires complete hexadecimal bytes.');
  }
  const before = beforeHex.toLowerCase();
  const after = afterHex.toLowerCase();
  const beforeBytes = before.length / 2;
  const afterBytes = after.length / 2;
  let prefixBytes = 0;
  while (prefixBytes < Math.min(beforeBytes, afterBytes)
    && before.slice(prefixBytes * 2, prefixBytes * 2 + 2) === after.slice(prefixBytes * 2, prefixBytes * 2 + 2)) {
    prefixBytes++;
  }
  let suffixBytes = 0;
  while (suffixBytes < beforeBytes - prefixBytes && suffixBytes < afterBytes - prefixBytes
    && before.slice((beforeBytes - suffixBytes - 1) * 2, (beforeBytes - suffixBytes) * 2)
      === after.slice((afterBytes - suffixBytes - 1) * 2, (afterBytes - suffixBytes) * 2)) {
    suffixBytes++;
  }
  return {
    identical: before === after,
    prefixBytes,
    suffixBytes,
    before: {
      offset: prefixBytes,
      end: beforeBytes - suffixBytes,
      bytes: beforeBytes - prefixBytes - suffixBytes,
      hex: before.slice(prefixBytes * 2, (beforeBytes - suffixBytes) * 2),
      total: beforeBytes,
    },
    after: {
      offset: prefixBytes,
      end: afterBytes - suffixBytes,
      bytes: afterBytes - prefixBytes - suffixBytes,
      hex: after.slice(prefixBytes * 2, (afterBytes - suffixBytes) * 2),
      total: afterBytes,
    },
  };
}

function test() {
  const cases = [
    ['identical', '00ff12', '00ff12', 3, 0, '', ''],
    ['empty', '', '', 0, 0, '', ''],
    ['insertion', '0012', '00ff12', 1, 1, '', 'ff'],
    ['deletion', '00ff12', '0012', 1, 1, 'ff', ''],
    ['middle', '00112233', '00aabb33', 1, 1, '1122', 'aabb'],
    ['append', '00', '0012', 1, 0, '', '12'],
    ['prepend', '12', '0012', 0, 1, '', '00'],
    ['remove all', '0012', '', 0, 0, '0012', ''],
    ['no overlap', 'aaaa', 'aaaaaa', 2, 0, '', 'aa'],
    ['byte alignment', '01ab', '0aab', 0, 1, '01', '0a'],
    ['case normalized', 'ABCD', 'abcd', 2, 0, '', ''],
  ];
  for (const [label, before, after, prefix, suffix, removed, added] of cases) {
    const result = compareSerializedBytes(before, after);
    assert.equal(result.prefixBytes, prefix, `${label}: prefix`);
    assert.equal(result.suffixBytes, suffix, `${label}: suffix`);
    assert.equal(result.before.hex, removed, `${label}: removed bytes`);
    assert.equal(result.after.hex, added, `${label}: added bytes`);
    assert.equal(result.before.bytes, removed.length / 2, `${label}: removed size`);
    assert.equal(result.after.bytes, added.length / 2, `${label}: added size`);
    assert.equal(result.identical, before.toLowerCase() === after.toLowerCase(), `${label}: identical`);
    assert(result.prefixBytes + result.suffixBytes <= Math.min(before.length, after.length) / 2, `${label}: prefix/suffix overlap`);
    const sharedPrefix = before.slice(0, prefix * 2).toLowerCase();
    const sharedSuffix = suffix ? before.slice(-suffix * 2).toLowerCase() : '';
    assert.equal(sharedPrefix + result.before.hex + sharedSuffix, before.toLowerCase(), `${label}: before reconstruction`);
    assert.equal(sharedPrefix + result.after.hex + sharedSuffix, after.toLowerCase(), `${label}: after reconstruction`);
    assert.equal(result.before.end, before.length / 2 - suffix, `${label}: before ending offset`);
    assert.equal(result.after.end, after.length / 2 - suffix, `${label}: after ending offset`);
  }
  for (const invalid of ['0', '0g', '0x12', '12 34', '\n12']) {
    assert.throws(() => compareSerializedBytes(invalid, ''), /complete hexadecimal bytes/);
    assert.throws(() => compareSerializedBytes('', invalid), /complete hexadecimal bytes/);
  }
  // Exhaust short repeated-byte combinations to catch accidental prefix/suffix
  // overlap and verify that replacing the reported middle reconstructs exactly.
  const values = [''];
  for (let length = 1; length <= 4; length++) {
    for (let bits = 0; bits < 2 ** length; bits++) {
      values.push(Array.from({ length }, (_, index) => bits & (1 << index) ? 'ff' : '00').join(''));
    }
  }
  for (const before of values) for (const after of values) {
    const result = compareSerializedBytes(before, after);
    const prefix = before.slice(0, result.prefixBytes * 2);
    const suffix = result.suffixBytes ? before.slice(-result.suffixBytes * 2) : '';
    assert.equal(prefix + result.before.hex + suffix, before);
    assert.equal(prefix + result.after.hex + suffix, after);
    assert(result.prefixBytes + result.suffixBytes <= Math.min(before.length, after.length) / 2);
  }
  console.log(`PASS: ${cases.length} byte-diff cases, invalid-input rejection, and ${values.length ** 2} exhaustive reconstruction pairs`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) test();
