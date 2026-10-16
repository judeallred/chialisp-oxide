import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import path from 'node:path';

const prefix = 'proofs/p2-conditions/';
// Publishing another proof artifact requires an explicit allowlist change.
const allowed = new Set([
  'accept-atom.ir.json', 'accept-atom.smt2', 'atom-case.smt2', 'atom-domain.smt2',
  'equivalence-atom-case.smt2', 'equivalence-atom-domain.smt2',
  'equivalence-pair-case.smt2', 'equivalence-pair-domain.smt2',
  'optimized-atom-case.smt2', 'optimized-atom-domain.smt2',
  'optimized-pair-case.smt2', 'optimized-pair-domain.smt2', 'optimized.ir.json',
  'pair-case.smt2', 'pair-domain.smt2', 'program.ir.json',
  'unsafe-guard-after.ir.json', 'unsafe-guard-before.ir.json', 'unsafe-guard.smt2',
  'wrong-quote.ir.json', 'wrong-quote.smt2',
]);

/** @param {{path: string, sha256: string}} artifact */
function checked(artifact) {
  if (typeof artifact.path !== 'string' || !artifact.path.startsWith(prefix)) {
    throw new Error('Structural downloads require the declared proof-directory prefix.');
  }
  const basename = artifact.path.slice(prefix.length);
  if (!allowed.has(basename) || !/^[a-z][a-z0-9-]*(?:\.ir\.json|\.smt2)$/.test(basename)) {
    throw new Error('Structural download basename is not allowlisted.');
  }
  if (!/^[0-9a-f]{64}$/.test(artifact.sha256)) {
    throw new Error('Structural download requires a SHA-256 binding.');
  }
  return { ...artifact, basename };
}

/** @param {{path: string, sha256: string}[]} artifacts */
export function structuralArtifacts(artifacts) {
  const entries = artifacts.map(checked);
  if (entries.length !== allowed.size || new Set(entries.map(row => row.basename)).size !== allowed.size) {
    throw new Error('Structural downloads must declare every allowlisted artifact exactly once.');
  }
  return entries;
}

async function repository() {
  // Astro bundles endpoint modules, so import.meta.url is not a stable source
  // location during prerendering. Accept only the checkout root or its www cwd.
  for (const candidate of [process.cwd(), path.dirname(process.cwd())]) {
    const packageFile = path.join(candidate, 'www/package.json');
    const manifest = await readFile(packageFile, 'utf8').then(JSON.parse).catch(() => null);
    if (manifest?.name === 'chialisp-oxide-docs') return realpath(candidate);
  }
  throw new Error('Structural downloads must be built from the checkout root or www.');
}

/** @param {{path: string, sha256: string}} artifact */
export async function readStructuralArtifact(artifact) {
  const entry = checked(artifact);
  const root = await repository();
  const directory = path.join(root, prefix);
  if (await realpath(directory) !== path.resolve(directory)) {
    throw new Error('Structural proof directory must not be a symlink.');
  }
  const filename = path.join(directory, entry.basename);
  if (await realpath(filename) !== filename) {
    throw new Error('Structural downloads cannot follow file symlinks.');
  }
  const bytes = await readFile(filename);
  if (createHash('sha256').update(bytes).digest('hex') !== entry.sha256) {
    throw new Error(`Structural artifact SHA-256 mismatch: ${entry.basename}`);
  }
  return bytes;
}
