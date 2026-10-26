// Consensus observations use a separate stack/version from primary VM failures.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { checkProtocolComponent } from './p2-protocol.mjs';
import { checkBoomerangComponent } from './boomerang-protocol.mjs';
import { checkRegistryReport, registrySlugs } from './registry-protocol.mjs';
import { checkCounterPair, counterSlugs } from '../src/lib/counter-protocol.mjs';

const table = JSON.parse(readFileSync(new URL('../../crates/chialisp-oxide-simulator/rejection-protocol.json', import.meta.url), 'utf8'));
assert.deepEqual(Object.keys(table).sort(), ['codes', 'protocol', 'puzzles', 'scenarios', 'schema_version', 'stage']);
assert.equal(table.schema_version, 1);
assert.equal(table.protocol, 'chia-consensus-0.36.1/v1');
assert.equal(table.stage, 'consensus');
export const ledgerProtocol = table.protocol;
const codes = new Set(table.codes);
assert.equal(codes.size, table.codes.length);
assert(table.codes.every(code => typeof code === 'string' && /^[A-Z][A-Za-z0-9]*$/.test(code)));
assert(Object.keys(table.scenarios).length > 0);
for (const expected of Object.values(table.scenarios)) assert(Array.isArray(expected) && expected.every(code => codes.has(code)));
assert(Object.keys(table.puzzles).length > 0);
for (const [slug, profile] of Object.entries(table.puzzles)) {
  assert(/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug) && Array.isArray(profile) && profile.length > 0, 'Malformed per-puzzle ledger contract');
  for (const row of profile) {
    assert.deepEqual(Object.keys(row).sort(), ['codes', 'name']);
    assert(Object.hasOwn(table.scenarios, row.name) && Array.isArray(row.codes) && row.codes.every(code => codes.has(code)), 'Malformed per-puzzle scenario/code contract');
  }
  assert.equal(new Set(profile.map(row => row.name)).size, profile.length);
}

export function checkScenario(row, label = 'ledger scenario', expectedCodes) {
  assert(row !== null && typeof row === 'object' && !Array.isArray(row), `${label}: malformed scenario`);
  assert.deepEqual(Object.keys(row).sort(), ['name', 'passed', 'rejections', 'scope'], `${label}: missing or unexpected observations`);
  assert(row.passed === true && row.scope === 'ledger', `${label}: unsuccessful/non-ledger scenario`);
  assert(typeof row.name === 'string' && Object.hasOwn(table.scenarios, row.name), `${label}: unknown scenario name`);
  assert(Array.isArray(row.rejections), `${label}: malformed consensus observations`);
  for (const rejection of row.rejections) {
    assert(rejection !== null && typeof rejection === 'object' && !Array.isArray(rejection), `${label}: malformed rejection`);
    assert.deepEqual(Object.keys(rejection).sort(), ['code', 'stage'], `${label}: malformed consensus rejection fields`);
    assert(rejection.stage === 'consensus' && codes.has(rejection.code), `${label}: unknown/non-consensus rejection`);
  }
  assert.deepEqual(row.rejections, (expectedCodes ?? table.scenarios[row.name]).map(code => ({ stage: 'consensus', code })), `${label}: wrong/missing expected consensus sequence`);
}

export function checkLedgerReport(report, options = {}) {
  assert.equal(report.simulator?.rejection_protocol, ledgerProtocol, 'Missing/unsupported ledger rejection protocol');
  // The selected counter side needs the opposite entry's actual Chialisp
  // artifact. Validate both once before iterating the ordinary ledger groups.
  const hasCounter = report.puzzles.some(puzzle => counterSlugs.includes(puzzle.slug));
  if (hasCounter) checkCounterPair(report.puzzles);
  if (report.puzzles.some(puzzle => registrySlugs.includes(puzzle.slug))) checkRegistryReport(report, options);
  for (const puzzle of report.puzzles) {
    assert(Array.isArray(puzzle.simulator), `${puzzle.slug}: malformed scenario list`);
    assert.equal(new Set(puzzle.simulator.map(row => row.name)).size, puzzle.simulator.length, `${puzzle.slug}: duplicate scenario grouping`);
    if (puzzle.slug === 'boomerang') { checkBoomerangComponent(puzzle); continue; }
    assert(!Object.hasOwn(puzzle, 'coin_instances'), `${puzzle.slug}: unexpected coin-instance protocol`);
    if (puzzle.kind === 'protocol_component') {
      if (![...counterSlugs,...registrySlugs].includes(puzzle.slug)) checkProtocolComponent(puzzle);
      continue;
    }
    assert(!Object.hasOwn(puzzle, 'protocol_validation'), `${puzzle.slug}: unexpected dedicated protocol evidence`);
    assert(['coin_puzzle', 'pure_function', 'program_factory'].includes(puzzle.kind), `${puzzle.slug}: unknown ledger puzzle kind`);
    if (puzzle.kind === 'coin_puzzle') {
      assert(Object.hasOwn(table.puzzles, puzzle.slug), `${puzzle.slug}: no reviewed ledger scenario contract`);
      assert.deepEqual(puzzle.simulator.map(row => row.name), table.puzzles[puzzle.slug].map(row => row.name), `${puzzle.slug}: missing, extra or reordered required ledger scenarios`);
      assert.deepEqual((puzzle.simulator_comparisons ?? []).map(row => [row.backend, row.optimize_lir]), [['chialisp', true], ['rue_source', true]], `${puzzle.slug}: missing language ledger scenario runs`);
    } else assert(puzzle.simulator.length === 0 && (puzzle.simulator_comparisons ?? []).length === 0, `${puzzle.slug}: non-spend function claims ledger observations`);
    const expectedCodes = new Map((table.puzzles[puzzle.slug] ?? []).map(row => [row.name, row.codes]));
    for (const scenario of puzzle.simulator) checkScenario(scenario, `${puzzle.slug}/Rust`, expectedCodes.get(scenario.name));
    for (const comparison of puzzle.simulator_comparisons ?? []) {
      for (const scenario of comparison.scenarios) checkScenario(scenario, `${puzzle.slug}/${comparison.backend}`, expectedCodes.get(scenario.name));
      assert.deepEqual(comparison.scenarios, puzzle.simulator, `${puzzle.slug}: consensus observations differ across languages`);
    }
  }
}

export function testLedgerProtocol() {
  let controls = 0;
  for (const [name, expected] of Object.entries(table.scenarios)) checkScenario({ name, passed: true, scope: 'ledger', rejections: expected.map(code => ({ stage: 'consensus', code })) });
  const base = { name: 'missing signature rejected with rollback', passed: true, scope: 'ledger', rejections: [{ stage: 'consensus', code: 'BadAggregateSignature' }] };
  for (const code of ['MissingKey', 'GeneratorRuntimeError', 'CostExceeded', 'UnknownCode', 'DoubleSpend']) {
    const bad = structuredClone(base); bad.rejections[0].code = code;
    assert.throws(() => checkScenario(bad)); controls++;
  }
  for (const mutate of [
    row => { delete row.rejections; }, row => { row.rejections = []; }, row => { row.rejections[0].stage = 'sdk_preflight'; },
    row => { row.rejections.push(row.rejections[0]); }, row => { row.name = 'valid spend produces exact recipients and total'; },
    row => { row.name = 'invented scenario'; }, row => { row.name = 'recovery requires both delay and recovery key'; row.rejections.push({ stage: 'consensus', code: 'AssertSecondsAbsoluteFailed' }); },
  ]) {
    const bad = structuredClone(base); mutate(bad); assert.throws(() => checkScenario(bad)); controls++;
  }
  function fixture(slug) {
    const scenarios = table.puzzles[slug].map(row => ({ name: row.name, passed: true, scope: 'ledger', rejections: row.codes.map(code => ({ stage: 'consensus', code })) }));
    return { simulator: { rejection_protocol: ledgerProtocol }, puzzles: [{ slug, kind: 'coin_puzzle', simulator: scenarios, simulator_comparisons: ['chialisp', 'rue_source'].map(backend => ({ backend, optimize_lir: true, scenarios: structuredClone(scenarios) })) }] };
  }
  // Validate the positive control first: a different contract error must not
  // accidentally satisfy these all-language code-corruption assertions.
  for (const language of ['rust_mir', 'chialisp', 'rue_source']) {
    const report = fixture('signature-payment'); checkLedgerReport(report);
    const row = language === 'rust_mir' ? report.puzzles[0].simulator[0] : report.puzzles[0].simulator_comparisons.find(item => item.backend === language).scenarios[0];
    row.rejections[0].code = 'MissingKey';
    assert.throws(() => checkLedgerReport(report)); controls++;
  }
  for (const [slug, profile] of Object.entries(table.puzzles)) {
    const original = fixture(slug); checkLedgerReport(original);
    for (let index = 0; index < profile.length; index++) {
      const report = structuredClone(original); const puzzle = report.puzzles[0];
      puzzle.simulator.splice(index, 1);
      for (const comparison of puzzle.simulator_comparisons) comparison.scenarios.splice(index, 1);
      assert.throws(() => checkLedgerReport(report)); controls++;
    }
    const reversed = structuredClone(original); reversed.puzzles[0].simulator.reverse();
    for (const comparison of reversed.puzzles[0].simulator_comparisons) comparison.scenarios.reverse();
    assert.throws(() => checkLedgerReport(reversed)); controls++;
  }
  const unknown = { simulator: { rejection_protocol: ledgerProtocol }, puzzles: [{ slug: 'coin-without-contract', kind: 'coin_puzzle', simulator: [], simulator_comparisons: [] }] };
  assert.throws(() => checkLedgerReport(unknown)); controls++;
  for (const kind of ['pure_function', 'program_factory']) {
    unknown.puzzles[0].kind = kind; checkLedgerReport(unknown);
    const bad = structuredClone(unknown); bad.puzzles[0].simulator = [base];
    assert.throws(() => checkLedgerReport(bad)); controls++;
  }
  // Independent fixture fact: this policy emits CREATE_COIN 1,000 without
  // ASSERT_MY_AMOUNT; the wrong 999 coin must fail conservation (MintingCoin).
  const boolean = fixture('boolean-gate'); checkLedgerReport(boolean);
  const wrongName = 'underfunded coin rejected with rollback';
  assert.deepEqual(table.puzzles['boolean-gate'].find(row => row.name === wrongName).codes, ['MintingCoin']);
  for (const scenarios of [boolean.puzzles[0].simulator, ...boolean.puzzles[0].simulator_comparisons.map(row => row.scenarios)]) scenarios.find(row => row.name === wrongName).rejections[0].code = 'AssertMyAmountFailed';
  assert.throws(() => checkLedgerReport(boolean)); controls++;
  return controls;
}
