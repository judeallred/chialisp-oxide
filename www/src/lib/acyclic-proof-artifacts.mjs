import { createHash } from 'node:crypto';
import { open, readFile, realpath } from 'node:fs/promises';
import path from 'node:path';

const directory = 'proofs/acyclic-scalar-source';
const experiment = 'experiments/acyclic-scalar';
const modules = ['base', 'guard_mutant', 'output_mutant', 'strict_mutant'];
const policies = ['guarded', 'unused_argument', 'unused_initializer', 'copied_result'];
const privateFiles = ['Cargo.lock', 'Cargo.toml', 'bridge.json', 'specifications.json',
  'src/lib.rs', 'src/main.rs', ...modules.map(module => `src/${module}.rs`)];
const obligations = [
  ...policies.flatMap(policy => ['acceptance', 'output'].map(kind => ['base', policy, kind, 'proven'])),
  ['guard_mutant', 'guarded', 'acceptance', 'refuted'],
  ['output_mutant', 'guarded', 'output', 'refuted'],
  ['strict_mutant', 'unused_argument', 'acceptance', 'refuted'],
  ['strict_mutant', 'unused_initializer', 'acceptance', 'refuted'],
];
const allowed = new Set([
  ...modules.flatMap(module => policies.flatMap(policy => [
    `ir/${module}-${policy}.json`,
    ...['False', 'True'].map(mode => `artifacts/${module}-${policy}-${mode}.clvm.hex`),
  ])),
  ...obligations.flatMap(([module, policy, kind, status]) => [
    `models/${module}-${policy}-${kind}.spec.json`, `models/${module}-${policy}-${kind}.smt2`,
    ...(status === 'refuted' ? [`models/${module}-${policy}-${kind}.witness.json`] : []),
  ]),
]);
export const acyclicProofLimits = Object.freeze({ report: 8 * 1024 * 1024, artifact: 1024 * 1024,
  totalArtifacts: 4 * 1024 * 1024, source: 64 * 1024, totalSources: 256 * 1024 });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function require(condition, message) {
  if (!condition) throw new Error(`Acyclic proof: ${message}`);
}

/** @param {any} witness */
export function acyclicWitnessBytes(witness) {
  require(witness && Object.keys(witness).join() === 'arguments' && Array.isArray(witness.arguments)
    && witness.arguments.length === 1, 'witness needs one typed argument');
  const value = witness.arguments[0];
  require(value && Object.keys(value).sort().join() === 'kind,value' && value.kind === 'int'
    && typeof value.value === 'string' && /^(0|[1-9][0-9]{0,2})$/.test(value.value)
    && Number(value.value) <= 255, 'witness must be canonical typed u8');
  // Reconstruct the producer's sorted, two-space JSON. Never derive a witness
  // identity by trusting whatever bytes happen to be on disk.
  return Buffer.from(`${JSON.stringify({ arguments: [{ kind: 'int', value: value.value }] }, null, 2)}\n`);
}

/** @param {any} value */
function checkedEntry(value) {
  require(value && typeof value.relative === 'string' && allowed.has(value.relative)
    && path.posix.normalize(value.relative) === value.relative && !value.relative.includes('\\')
    && value.path === `${directory}/${value.relative}` && value.basename === path.posix.basename(value.relative),
  'download path/basename is not allowlisted');
  require(typeof value.sha256 === 'string' && /^[0-9a-f]{64}$/.test(value.sha256), 'download needs a SHA-256 identity');
  if (value.expectedText !== undefined) {
    require(typeof value.expectedText === 'string' && value.relative.endsWith('.witness.json')
      && sha(Buffer.from(value.expectedText)) === value.sha256, 'invalid derived witness bytes');
  }
  return value;
}

/** Derive only direct artifacts, never a captured bundle or arbitrary report path.
 * The Python recorded checker remains responsible for the complete model record.
 * @param {any} report
 */
export function acyclicProofArtifacts(report) {
  require(report?.schema_version === 1 && report.protocol === 'chialisp-oxide-acyclic-source/v1'
    && report.status === 'validated', 'unknown report protocol/status');
  require(Array.isArray(report.exports) && report.exports.length === 16
    && Array.isArray(report.obligations) && report.obligations.length === obligations.length,
  'incomplete direct export/obligation table');
  /** @type {any[]} */
  const entries = [];
  const add = (reference, expected, expectedBytes = undefined) => {
    require(reference?.path === expected, 'artifact differs from its declared role');
    const entry = { relative: reference.path, path: `${directory}/${reference.path}`,
      basename: path.posix.basename(reference.path), sha256: reference.sha256,
      ...(expectedBytes ? { expectedText: expectedBytes.toString('utf8') } : {}) };
    entries.push(checkedEntry(entry));
  };
  let index = 0;
  for (const module of modules) for (const policy of policies) {
    const exported = report.exports[index++];
    require(exported?.key === `${module}::${policy}` && Array.isArray(exported.builds)
      && exported.builds.length === 2, 'missing/reordered source export or mode');
    add(exported.IR, `ir/${module}-${policy}.json`);
    for (const [mode, build] of exported.builds.entries()) {
      require(build?.optimize === Boolean(mode), 'missing/reordered Boolean optimization mode');
      add(build.retained_CLVM, `artifacts/${module}-${policy}-${mode ? 'True' : 'False'}.clvm.hex`);
    }
  }
  for (const [index, [module, policy, kind, status]] of obligations.entries()) {
    const row = report.obligations[index];
    require(row?.module === module && row.policy === policy && row.kind === kind && row.expected === status
      && row.verdict?.model === 'acyclic_scalar_calls/v1' && row.verdict.verification?.status === status,
    'changed model obligation identity/result');
    const stem = `models/${module}-${policy}-${kind}`;
    add(row.specification, `${stem}.spec.json`);
    add(row.query, `${stem}.smt2`);
    if (status === 'refuted') {
      const bytes = acyclicWitnessBytes(row.verdict.verification.counterexample);
      require(row.witness?.input === Number(row.verdict.verification.counterexample.arguments[0].value),
        'witness input does not match typed counterexample');
      add({ path: row.witness.path, sha256: sha(bytes) }, `${stem}.witness.json`, bytes);
    } else require(row.witness === undefined, 'unexpected counterexample in a proven model');
  }
  require(entries.length === 76 && allowed.size === 76
    && new Set(entries.map(entry => entry.path)).size === 76
    && new Set(entries.map(entry => entry.basename)).size === 76, 'missing/duplicate artifact identity');
  return entries;
}

async function repository() {
  for (const candidate of [process.cwd(), path.dirname(process.cwd())]) {
    const manifest = await readFile(path.join(candidate, 'www/package.json'), 'utf8').then(JSON.parse).catch(() => null);
    if (manifest?.name === 'chialisp-oxide-docs') return realpath(candidate);
  }
  throw new Error('Acyclic proof: build from the checkout root or www');
}

async function boundedFile(root, relative, limit, budget) {
  const filename = path.join(root, relative);
  require(await realpath(filename) === filename, 'files and parent directories cannot be symlinks');
  const handle = await open(filename, 'r');
  try {
    const stat = await handle.stat();
    require(stat.isFile() && stat.size > 0 && stat.size <= limit && stat.size <= budget.remaining,
      'file/aggregate byte budget exceeded');
    budget.remaining -= stat.size; // Charge before allocation or copying.
    const buffer = Buffer.alloc(stat.size + 1);
    let total = 0;
    while (total < buffer.length) {
      const { bytesRead } = await handle.read(buffer, total, buffer.length - total, null);
      if (!bytesRead) break;
      total += bytesRead;
    }
    require(total === stat.size, 'file size changed while reading');
    return buffer.subarray(0, total);
  } finally { await handle.close(); }
}

/** @param {any} entry */
export async function readAcyclicProofArtifact(entry) {
  checkedEntry(entry);
  const root = await repository();
  const bytes = await boundedFile(root, entry.path, acyclicProofLimits.artifact,
    { remaining: acyclicProofLimits.artifact });
  require(sha(bytes) === entry.sha256, `SHA-256 mismatch: ${entry.basename}`);
  if (entry.expectedText !== undefined) require(bytes.equals(Buffer.from(entry.expectedText)), 'witness bytes differ from verdict');
  return bytes;
}

/** Validate all selected bytes before a component or static route is generated.
 * Recorded validation is freshness/consistency; it adds no execution credit.
 */
export async function loadAcyclicProofEvidence() {
  const root = await repository();
  const raw = await boundedFile(root, `${directory}/result.json`, acyclicProofLimits.report,
    { remaining: acyclicProofLimits.report });
  const report = JSON.parse(raw.toString('utf8'));
  const catalog = acyclicProofArtifacts(report);
  acyclicProofSummary(report);
  const budget = { remaining: acyclicProofLimits.totalArtifacts };
  const artifacts = [];
  for (const entry of catalog) {
    const bytes = await boundedFile(root, entry.path, acyclicProofLimits.artifact, budget);
    require(sha(bytes) === entry.sha256, `SHA-256 mismatch: ${entry.basename}`);
    if (entry.expectedText !== undefined) require(bytes.equals(Buffer.from(entry.expectedText)), 'witness bytes differ from verdict');
    artifacts.push({ ...entry, bytes });
  }
  const recorded = report.sources?.experiment?.sources;
  require(Array.isArray(recorded) && recorded.length === privateFiles.length
    && new Set(recorded.map(row => row.path)).size === privateFiles.length, 'incomplete current source table');
  const sources = new Map();
  const sourceBudget = { remaining: acyclicProofLimits.totalSources };
  for (const relative of privateFiles) {
    const item = recorded.find(row => row.path === relative);
    require(item && /^[0-9a-f]{64}$/.test(item.sha256), 'source missing its recorded identity');
    const bytes = await boundedFile(root, `${experiment}/${relative}`, acyclicProofLimits.source, sourceBudget);
    require(sha(bytes) === item.sha256, `current source SHA-256 mismatch: ${relative}`);
    sources.set(relative, bytes.toString('utf8'));
  }
  return { report, raw, sha256: sha(raw), artifacts, sources };
}

/** @param {any} report */
export function acyclicProofSummary(report) {
  acyclicProofArtifacts(report);
  for (const exported of report.exports) {
    require(Array.isArray(exported.native) && exported.native.length === 256, 'incomplete native finite domain');
    for (const build of exported.builds) require(Array.isArray(build.canonical) && build.canonical.length === 256
      && Array.isArray(build.malformed) && build.malformed.length === 8, 'incomplete retained VM domains');
  }
  for (const row of report.obligations) {
    acyclicWitnessBytes(row.verdict.verification.nonvacuity);
    require(Array.isArray(row.retained_query_statuses) && row.retained_query_statuses.length === 2,
      'missing retained query statuses');
    if (row.expected === 'refuted') require(Array.isArray(row.witness.replays) && row.witness.replays.length === 2
      && row.witness.replays.every(replay => Array.isArray(replay.runs) && replay.runs.length === 2),
    'incomplete original/mutant witness modes');
  }
  const counts = {
    actual_exports: report.exports.length,
    baseline_actual_tables: report.exports.filter(row => row.key.startsWith('base::') && row.kind === 'actual_frontend_table').length,
    nonvacuous_proven_obligations: report.obligations.filter(row => row.expected === 'proven').length,
    actual_source_refutations: report.obligations.filter(row => row.expected === 'refuted').length,
    retained_query_scripts: report.obligations.length,
    retained_query_status_checks: report.obligations.reduce((n, row) => n + row.retained_query_statuses.length, 0),
    native_finite_cases: report.exports.reduce((n, row) => n + row.native.length, 0),
    retained_canonical_VM_cases: report.exports.reduce((n, row) => n + row.builds.reduce((m, build) => m + build.canonical.length, 0), 0),
    retained_malformed_VM_cases: report.exports.reduce((n, row) => n + row.builds.reduce((m, build) => m + build.malformed.length, 0), 0),
    recompiled_witness_VM_cases: report.obligations.reduce((n, row) => n + (row.witness?.replays.reduce((m, replay) => m + replay.runs.length, 0) ?? 0), 0),
  };
  require(counts.baseline_actual_tables === 4 && report.counts && Object.keys(report.counts).length === Object.keys(counts).length
    && Object.entries(counts).every(([key, value]) => Number.isSafeInteger(report.counts[key]) && report.counts[key] === value),
  'display counts do not match recorded observations');
  return counts;
}

/** @param {any} native */
export function acyclicOutcomeLabel(native) {
  if (native?.status === 'rejected') return 'rejected';
  require(native?.status === 'accepted' && Number.isInteger(native.value) && native.value >= 0 && native.value <= 255,
    'invalid displayed native outcome');
  return `accepted: u8 ${native.value}`;
}

const decode = value => value.replace(/&(?:#x([0-9a-f]+)|#([0-9]+)|(amp|lt|gt|quot|apos));/gi,
  (_, hex, decimal, named) => hex || decimal ? String.fromCodePoint(Number.parseInt(hex ?? decimal, hex ? 16 : 10))
    : ({ amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" })[named.toLowerCase()]);
const attribute = (tag, name) => tag.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
const text = value => decode(value.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim();
const code = value => value.replaceAll('\t', '  ').split(/\r?\n/).map(line => line.trimEnd()).join('\n').trim();

/** Pure actual-render checker shared with the site gate. No tool execution.
 * @param {string} markup @param {any} report @param {any[]} artifacts @param {Map<string,string>} sources
 */
export function checkAcyclicDisplay(markup, report, artifacts, sources) {
  require(typeof markup === 'string' && Buffer.byteLength(markup) <= 512 * 1024, 'page byte budget exceeded');
  const counts = acyclicProofSummary(report);
  const plain = text(markup);
  for (const label of ['WIP prototype', 'not ready for use with funds', 'Typed u8 inputs after ABI decoding',
    'No Rust/MIR/IR/CLVM preservation theorem', 'Recorded consistency checks add no solver, native or VM execution credit',
    'No Chialisp/Rue port or ledger-validity credit']) require(plain.includes(label), `missing scope label: ${label}`);
  for (const [key, value] of Object.entries(counts)) {
    const rows = [...markup.matchAll(/<strong\b([^>]*)>([\s\S]*?)<\/strong>/g)]
      .filter(match => attribute(match[1], 'data-acyclic-count') === key);
    require(rows.length === 1 && text(rows[0][2]) === new Intl.NumberFormat('en-US').format(value), `wrong displayed count: ${key}`);
  }
  const rows = [...markup.matchAll(/<tr\b([^>]*)>([\s\S]*?)<\/tr>/g)];
  const modeled = rows.filter(row => attribute(row[1], 'data-acyclic-obligation') !== undefined);
  require(modeled.length === report.obligations.length, 'missing/extra displayed obligations');
  for (const row of report.obligations) {
    const key = `${row.module}::${row.policy}/${row.kind}`;
    const matches = modeled.filter(match => attribute(match[1], 'data-acyclic-obligation') === key);
    require(matches.length === 1 && attribute(matches[0][1], 'data-acyclic-status') === row.expected
      && text(matches[0][2]).includes(row.expected), `wrong displayed obligation: ${key}`);
    const displayed = [...matches[0][2].matchAll(/<(?:th|td)\b[^>]*>([\s\S]*?)<\/(?:th|td)>/g)]
      .map(cell => text(cell[1]));
    require(displayed.length === 3 && displayed[0] === `${row.module}::${row.policy} ${row.kind}`
      && displayed[1] === row.expected
      && displayed[2] === row.verdict.verification.nonvacuity.arguments[0].value,
    `wrong visible obligation cells: ${key}`);
    if (row.expected === 'refuted') {
      const mutations = rows.filter(match => attribute(match[1], 'data-acyclic-mutation') === key);
      require(mutations.length === 1 && attribute(mutations[0][1], 'data-acyclic-input') === String(row.witness.input),
        `wrong mutation input: ${key}`);
      const cells = [...mutations[0][2].matchAll(/<td\b([^>]*)>([\s\S]*?)<\/td>/g)];
      const headings = [...mutations[0][2].matchAll(/<th\b[^>]*>([\s\S]*?)<\/th>/g)];
      require(headings.length === 1 && text(headings[0][1]) === `${row.module}::${row.policy} ${row.kind}`
        && cells.length === 3 && text(cells[0][2]) === String(row.witness.input),
      `wrong visible mutation cells: ${key}`);
      for (const [index, role] of ['original', 'mutant'].entries()) {
        const cell = cells.find(cell => attribute(cell[1], 'data-acyclic-outcome') === role);
        require(cell && text(cell[2]) === acyclicOutcomeLabel(row.witness.replays[index].native), `wrong mutation outcome: ${key}`);
      }
    }
  }
  require(rows.filter(row => attribute(row[1], 'data-acyclic-mutation') !== undefined).length === 4,
    'missing/extra source mutation rows');
  const links = [...markup.matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/g)]
    .filter(match => attribute(match[1], 'data-acyclic-download') !== undefined);
  require(links.length === artifacts.length && artifacts.length === 76, 'missing/extra direct artifact links');
  for (const artifact of artifacts) {
    const selected = links.filter(link => attribute(link[1], 'data-acyclic-download') === artifact.basename);
    require(selected.length === 1 && attribute(selected[0][1], 'href') === `/chialisp-oxide/reports/acyclic-scalar/${artifact.basename}`
      && text(selected[0][2]).includes(artifact.basename) && text(selected[0][2]).includes(artifact.sha256),
    `changed artifact link/hash: ${artifact.basename}`);
  }
  require(markup.includes('href="/chialisp-oxide/reports/acyclic-scalar.json"'), 'missing raw report link');
  const copies = [...markup.matchAll(/\bdata-code="([^"]*)"/g)].map(match => code(decode(match[1]).replaceAll('\x7f', '\n')));
  for (const name of ['src/base.rs', 'src/guard_mutant.rs', 'src/output_mutant.rs', 'src/strict_mutant.rs', 'specifications.json']) {
    require(sources.has(name) && copies.includes(code(sources.get(name))), `changed actual source/spec copy: ${name}`);
  }
  return { counts, downloads: links.length, obligations: modeled.length, mutations: 4, sourceCopies: 5 };
}
