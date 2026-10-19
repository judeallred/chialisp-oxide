import { createHash } from 'node:crypto';
import { open, readFile, realpath } from 'node:fs/promises';
import path from 'node:path';

export const MAX_SOURCE_DATA_BYTES = 2 * 1024 * 1024;
// These are data dependencies, not an arbitrary repository-file server. Adding
// a published dataset requires an explicit owner/path/basename review here.
const definitions = [
  { slug: 'prefix-list', path: 'puzzles/prefix-list/generated-cases.json', basename: 'prefix-list-generated-cases.json' },
  { slug: 'atom-not-equal', path: 'puzzles/atom-not-equal/generated-cases.json', basename: 'atom-not-equal-generated-cases.json' },
  { slug: 'insertion-sort-by-keys', path: 'puzzles/insertion-sort-by-keys/generated-cases.json', basename: 'insertion-sort-by-keys-generated-cases.json' },
  // The raw ABI also executes 2,048 shared trees, ten widths and one spine.
  // This download is the additional 275-row file, not that algorithmic suite.
  { slug: 'sha256tree', path: 'puzzles/sha256tree/generated-cases.json', basename: 'sha256tree-generated-cases.json', inheritedRows: 2059 },
  // Password-specific byte/hash pairs are downloaded separately from the same
  // 2,059 shared raw-environment inputs executed by the corpus.
  { slug: 'p2-singleton', path: 'puzzles/p2-singleton/vectors.json', basename: 'p2-singleton-vectors.json', kind: 'manifest' },
  { slug: 'p2-singleton', path: 'puzzles/p2-singleton/generated-cases.json', basename: 'p2-singleton-generated-cases.json', inheritedRows: 2059 },
  { slug: 'password-coin', path: 'puzzles/password-coin/generated-cases.json', basename: 'password-coin-generated-cases.json', inheritedRows: 2059 },
  { slug: 'p2-delegated-conditions', path: 'puzzles/p2-delegated-conditions/independent-vectors.json', basename: 'p2-delegated-conditions-independent-vectors.json', kind: 'independent', inheritedRows: 2059, namedRows: 20 },
];
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

/** @param {string} relativePath */
export function sourceDataArtifact(relativePath) {
  const definition = definitions.find(entry => entry.path === relativePath);
  return definition ? { path: definition.path, basename: definition.basename } : undefined;
}

function checked(artifact) {
  const definition = sourceDataArtifact(artifact?.path);
  if (!definition || artifact.basename !== definition.basename) {
    throw new Error('Source-data download path/basename is not allowlisted.');
  }
  if (typeof artifact.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(artifact.sha256)
      || !Number.isSafeInteger(artifact.bytes) || artifact.bytes < 1 || artifact.bytes > MAX_SOURCE_DATA_BYTES
      || !Number.isSafeInteger(artifact.rows) || artifact.rows < 1) {
    throw new Error('Source-data download requires bounded byte/row counts and SHA-256.');
  }
  const kind = definitions.find(row => row.path === definition.path)?.kind;
  return { ...definition, ...(kind ? { kind } : {}), sha256: artifact.sha256, bytes: artifact.bytes, rows: artifact.rows };
}

function parsedRows(source, definition) {
  const value = JSON.parse(source);
  if (definition.kind === 'independent') {
    if (!value || typeof value !== 'object' || Array.isArray(value)
        || Object.keys(value).sort().join(',') !== 'accepted,rejected,schema_version,scope,status'
        || value.schema_version !== 1 || value.status !== 'independent_canonical_reference_preflight_not_Rust_port'
        || !Array.isArray(value.accepted) || value.accepted.length !== 195
        || !Array.isArray(value.rejected) || value.rejected.length !== 5) {
      throw new Error('Independent raw recipes require the closed 195/5 source-preflight schema; no Rust execution claim.');
    }
    const rows = [...value.accepted, ...value.rejected];
    if (rows.some(row => typeof row.label !== 'string' || typeof row.solution !== 'string')
        || new Set(rows.map(row => row.label)).size !== rows.length) throw new Error('Independent raw labels must be unique.');
    return { rows };
  }
  if (definition.kind === 'manifest') {
    if (!value || typeof value !== 'object' || Array.isArray(value)
        || Object.keys(value).sort().join(',') !== 'checks,generated,named,scope'
        || !Array.isArray(value.named) || !Array.isArray(value.generated)
        || value.checks?.named !== value.named.length || value.checks?.generated !== value.generated.length
        || value.checks?.status !== 'expected_relation_codec_and_hash_arithmetic_no_VM_invocation') {
      throw new Error('P2 relation manifest requires explicit finite no-VM scope and exact named/generated recipes.');
    }
    return { rows: [...value.named, ...value.generated], named: value.named, generated: value.generated };
  }
  if (!Array.isArray(value)) throw new Error('Source-data row count requires a row array.');
  return { rows: value };
}

/** Validate the fixed datasets against the recorded dependency strings. */
export function sourceDataArtifacts(report) {
  if (!Array.isArray(report?.puzzles)) throw new Error('Missing source-data fixture records.');
  return definitions.map(definition => {
    const owners = report.puzzles.filter(puzzle => puzzle.slug === definition.slug);
    const occurrences = report.puzzles.flatMap(puzzle => [
      ...(puzzle.source_dependencies ?? []), ...(puzzle.rue?.dependencies ?? []),
    ]).filter(dependency => dependency.path === definition.path);
    const dependencies = owners[0]?.source_dependencies;
    const matches = Array.isArray(dependencies) ? dependencies.filter(dependency => dependency.path === definition.path) : [];
    if (owners.length !== 1 || occurrences.length !== 1 || matches.length !== 1) {
      throw new Error('Source-data requires exactly one dependency under its fixed fixture owner.');
    }
    const dependency = matches[0];
    if (typeof dependency.source !== 'string') throw new Error('Missing exact source-data string.');
    const bytes = Buffer.byteLength(dependency.source, 'utf8');
    if (bytes < 1 || bytes > MAX_SOURCE_DATA_BYTES) throw new Error('Source-data exceeds its byte bound.');
    if (Buffer.from(dependency.source, 'utf8').toString('utf8') !== dependency.source) throw new Error('Source-data must round-trip exact UTF-8.');
    if (digest(dependency.source) !== dependency.sha256) throw new Error('Recorded source-data SHA-256 mismatch.');
    const parsed = parsedRows(dependency.source, definition);
    const rows = parsed.rows;
    if (rows.length === 0) throw new Error('Source-data requires a nonempty row array.');
    if (definition.kind === 'manifest') {
      const inputs = list => list.map(({label,solution}) => ({label,solution}));
      const generated = dependencies.find(row => row.path === 'puzzles/p2-singleton/generated-cases.json');
      if (!Array.isArray(owners[0].cases) || !generated || parsed.named.length !== 50 || parsed.generated.length !== 128
          || JSON.stringify(inputs(parsed.named)) !== JSON.stringify(inputs(owners[0].cases))
          || JSON.stringify(inputs(parsed.generated)) !== JSON.stringify(JSON.parse(generated.source))) {
        throw new Error('Relation manifest must match exact named and generated inputs; it adds no execution or coverage.');
      }
    } else if (definition.kind === 'independent') {
      const named = owners[0].cases;
      if (!Array.isArray(named) || named.length !== definition.namedRows
          || named.some(row => !rows.some(spec => spec.label === row.label && spec.solution === row.solution))
          || new Set(named.map(row => row.label)).size !== named.length
          || owners[0].generated_cases !== rows.length - named.length + definition.inheritedRows) {
        throw new Error('Independent raw recipes must preserve 20 named, 180 disjoint generated and 2059 generic inputs.');
      }
    } else if (owners[0].generated_cases !== rows.length + (definition.inheritedRows ?? 0)) {
      throw new Error('Source-data row count must match the generated-input record.');
    }
    return checked({ ...definition, sha256: dependency.sha256, bytes, rows: rows.length });
  });
}

async function repository() {
  // Astro bundles endpoint modules; accept only the checkout or its www cwd.
  for (const candidate of [process.cwd(), path.dirname(process.cwd())]) {
    const manifest = await readFile(path.join(candidate, 'www/package.json'), 'utf8').then(JSON.parse).catch(() => null);
    if (manifest?.name === 'chialisp-oxide-docs') return realpath(candidate);
  }
  throw new Error('Source-data downloads must be built from the checkout root or www.');
}

/** Read only an allowlisted, non-symlink file with a bounded allocation. */
export async function readSourceDataArtifact(artifact) {
  const entry = checked(artifact);
  const root = await repository();
  const filename = path.join(root, entry.path);
  const directory = path.dirname(filename);
  if (await realpath(directory) !== directory || await realpath(filename) !== filename) {
    throw new Error('Source-data downloads cannot follow directory/file symlinks.');
  }
  const file = await open(filename, 'r');
  try {
    const info = await file.stat();
    if (!info.isFile() || info.size !== entry.bytes || info.size > MAX_SOURCE_DATA_BYTES) {
      throw new Error('Source-data file size/type differs from the bounded record.');
    }
    // Bound reads as well as stat checks. Concurrent source edits are outside
    // build provenance, but cannot turn this read into an unbounded allocation.
    const buffer = Buffer.alloc(MAX_SOURCE_DATA_BYTES + 1);
    let length = 0;
    while (length < buffer.length) {
      const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
      if (!bytesRead) break;
      length += bytesRead;
    }
    if (length !== entry.bytes || length > MAX_SOURCE_DATA_BYTES) throw new Error('Source-data byte bound changed during read.');
    const bytes = buffer.subarray(0, length);
    if (digest(bytes) !== entry.sha256) throw new Error('Source-data file SHA-256 mismatch.');
    const source = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const rows = parsedRows(source, entry).rows;
    if (rows.length !== entry.rows) throw new Error('Source-data file row count differs from its record.');
    return bytes;
  } finally {
    await file.close();
  }
}
