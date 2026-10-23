import type { APIRoute, GetStaticPaths } from 'astro';
import { execFileSync } from 'node:child_process';
import { lstat, open, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from '../../../lib/p2-costs.mjs';

const maxFile = 2 * 1024 * 1024;
const maxTotal = 8 * 1024 * 1024;
async function boundedRead(filename: string, limit: number) {
  const before = await lstat(filename);
  if (!before.isFile() || before.isSymbolicLink() || await realpath(filename) !== filename) throw new Error('Invalid CAT download file');
  const file = await open(filename, 'r');
  try {
    const stat = await file.stat();
    if (!stat.isFile() || stat.size > limit || stat.size !== before.size) throw new Error('Invalid bounded CAT download size');
    const buffer = Buffer.alloc(stat.size + 1);
    let length = 0;
    while (length < buffer.length) {
      const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
      if (!bytesRead) break;
      length += bytesRead;
    }
    if (length !== stat.size || length > limit) throw new Error('CAT download changed during bounded read');
    return Buffer.from(buffer.subarray(0, length));
  } finally { await file.close(); }
}

export const getStaticPaths: GetStaticPaths = async () => {
  const root = await repository();
  const base = await realpath(path.join(root, 'corpus/cat-genesis-source-controls'));
  const indexPath = path.join(base, 'report.json');
  const indexBytes = await boundedRead(indexPath, maxFile);
  const admitted = JSON.parse(execFileSync('python3', ['scripts/check-cat-source-controls.py', '--list-json'], {
    cwd: root, encoding: 'utf8', timeout: 120_000, maxBuffer: 64 * 1024,
  }));
  if (!(await boundedRead(indexPath, indexBytes.length)).equals(indexBytes)) throw new Error('CAT index changed across admission');
  const index = JSON.parse(indexBytes.toString('utf8'));
  if (!Array.isArray(index.files) || index.files.length > 128 || !Array.isArray(admitted.files)) throw new Error('Invalid admitted CAT inventory');
  const names = ['report.json', ...index.files.map((row: { path: string }) => row.path)].sort();
  if (new Set(names).size !== names.length || JSON.stringify([...admitted.files].sort()) !== JSON.stringify(names)) throw new Error('CAT download inventory differs from admission');
  let reserved = indexBytes.length;
  for (const row of index.files) {
    if (typeof row.path !== 'string' || !/^[A-Za-z0-9_./-]+$/.test(row.path) || row.path.split('/').some((part: string) => !part || part === '.' || part === '..')) throw new Error('Invalid CAT download path');
    if (!Number.isSafeInteger(row.bytes) || row.bytes < 0 || row.bytes > maxFile || row.bytes > maxTotal - reserved || !/^[a-f0-9]{64}$/.test(row.sha256)) throw new Error('Invalid CAT download reservation');
    reserved += row.bytes;
  }
  const routes = [{ params: { artifact: 'report.json' }, props: { bytes: indexBytes, json: true } }];
  // The complete inventory is reserved before payload allocation. Sequential
  // bounded reads also admit the capture's intentionally empty command logs.
  for (const row of index.files) {
    const bytes = await boundedRead(path.join(base, row.path), row.bytes);
    if (bytes.length !== row.bytes || createHash('sha256').update(bytes).digest('hex') !== row.sha256) throw new Error('CAT download differs from admitted inventory');
    routes.push({ params: { artifact: row.path }, props: { bytes, json: row.path.endsWith('.json') } });
  }
  if (!(await boundedRead(indexPath, indexBytes.length)).equals(indexBytes)) throw new Error('CAT index changed during downloads');
  return routes;
};
export const GET: APIRoute = ({ props }) => new Response(Uint8Array.from(props.bytes).buffer, {
  headers: { 'Content-Type': props.json ? 'application/json; charset=utf-8' : 'text/plain; charset=utf-8' },
});
