import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import path from 'node:path';

const prefix = 'proofs/atom-not-equal/';
const models = [
  'original', 'operator_eq', 'self_inequality', 'constant_true', 'constant_false',
  'swap_operands', 'loose_environment', 'fixed_length_narrowing',
];
const queries = [
  'abi_nonvacuity', 'acceptance_nonvacuity', 'true_nonvacuity',
  'false_nonvacuity', 'counterexample',
];
// Deliberately closed: eight model IDs × eight outputs, plus one independent
// specification. New report paths require an explicit publication change.
const allowed = new Set([
  ...models.flatMap(model => [
    `${model}.ir.json`, ...queries.map(query => `${model}-${query}.smt2`),
    `${model}-off.clvm.hex`, `${model}-on.clvm.hex`,
  ]),
  'specification.json',
]);

/** @param {{path: string, sha256: string}} artifact */
function checked(artifact) {
  if (!artifact || typeof artifact.path !== 'string' || !artifact.path.startsWith(prefix)
      || artifact.path.includes('\\') || path.posix.normalize(artifact.path) !== artifact.path) {
    throw new Error('Byte-proof downloads require a normalized proof-directory path.');
  }
  const basename = artifact.path.slice(prefix.length);
  if (!allowed.has(basename)) throw new Error('Byte-proof download basename is not allowlisted.');
  if (typeof artifact.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(artifact.sha256)) {
    throw new Error('Byte-proof downloads require a SHA-256 binding.');
  }
  return { path: artifact.path, sha256: artifact.sha256, basename };
}

/** @param {{artifacts: {path: string, sha256: string}[], specification: {path: string, sha256: string}}} report */
export function byteProofArtifacts(report) {
  if (!report || !Array.isArray(report.artifacts) || report.artifacts.length !== 64
      || report.specification?.path !== `${prefix}specification.json`) {
    throw new Error('Byte-proof downloads require 64 model outputs and the independent specification.');
  }
  const entries = [...report.artifacts, report.specification].map(checked);
  if (entries.length !== allowed.size || new Set(entries.map(entry => entry.path)).size !== allowed.size) {
    throw new Error('Byte-proof downloads must declare every allowlisted artifact exactly once.');
  }
  return entries;
}

async function repository() {
  // Astro bundles endpoint modules. Use the same checkout/www cwd contract as
  // the existing structural downloads rather than bundled import.meta.url.
  for (const candidate of [process.cwd(), path.dirname(process.cwd())]) {
    const manifest = await readFile(path.join(candidate, 'www/package.json'), 'utf8').then(JSON.parse).catch(() => null);
    if (manifest?.name === 'chialisp-oxide-docs') return realpath(candidate);
  }
  throw new Error('Byte-proof downloads must be built from the checkout root or www.');
}

/** @param {{path: string, sha256: string}} artifact */
export async function readByteProofArtifact(artifact) {
  const entry = checked(artifact);
  const root = await repository();
  const directory = path.join(root, prefix);
  if (await realpath(directory) !== path.resolve(directory)) {
    throw new Error('Byte-proof download directory must not be a symlink.');
  }
  const filename = path.join(directory, entry.basename);
  if (await realpath(filename) !== filename) {
    throw new Error('Byte-proof downloads cannot follow file symlinks.');
  }
  const bytes = await readFile(filename);
  if (createHash('sha256').update(bytes).digest('hex') !== entry.sha256) {
    throw new Error(`Byte-proof artifact SHA-256 mismatch: ${entry.basename}`);
  }
  return bytes;
}
