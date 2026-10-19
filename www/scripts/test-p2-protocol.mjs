import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { checkProtocolComponent, checkProtocolReport } from './p2-protocol.mjs';
import { checkLedgerReport } from './ledger-rejections.mjs';
const report = JSON.parse(readFileSync(new URL('../../corpus/validation.json', import.meta.url), 'utf8'));
const puzzle = report.puzzles.find(row => row.slug === 'p2-singleton');
assert(puzzle, 'Retained singleton component required; no silent skip');
const expected = checkProtocolComponent(puzzle);
assert.deepEqual([expected.groups, expected.attempts, expected.accepted, expected.rejected], [5,110,15,95]);
assert.deepEqual(checkProtocolReport(report), expected);
checkLedgerReport(report);
let controls = 0;
const mutations = [
  p => { p.artifacts.push(structuredClone(p.artifacts[0])); },
  p => { p.kind = 'pure_function'; }, p => { p.slug = 'another-component'; },
  p => { p.simulator.push({}); }, p => { delete p.protocol_validation; },
  p => { p.protocol_validation.schema_version = true; }, p => { p.protocol_validation.ledger_VM = 'clvmr 0.21.0'; },
  p => { p.protocol_validation.observations.pop(); }, p => { p.protocol_validation.observations[0].optimize_lir = 0; },
  p => { p.protocol_validation.observations[0].module_tree_hash = '0'.repeat(64); },
  p => { p.protocol_validation.observations[0].negatives.pop(); },
  p => { p.protocol_validation.observations[0].negatives[0].stage = 'signer_preflight'; },
  p => { p.protocol_validation.observations[0].negatives[0].observed = 'CostExceeded'; },
  p => { p.protocol_validation.observations[0].negatives[0].full_simulator_equality = 1; },
  p => { p.protocol_validation.observations[0].negatives[0].consensus.semantic_rejection_credit = 1; },
  p => { p.protocol_validation.observations[0].negatives[0].transaction.spends = []; },
  p => { p.protocol_validation.observations[0].lifecycle.receipts.pop(); },
  p => { p.protocol_validation.observations[0].lifecycle.receipts[0].bundle_field = 'joint'; },
  p => { p.protocol_validation.observations[0].lifecycle.genesis.signature_hex = ''; },
];
for (const field of ['before_debug', 'after_debug']) {
  mutations.push(p => { p.protocol_validation.observations[0].negatives[0][field] = 'obsolete SDK Debug text'; });
  mutations.push(p => { p.protocol_validation.observations[0].lifecycle.rejections[0][field] = 'obsolete SDK Debug text'; });
}
for (const field of ['genesis_before_debug', 'genesis_after_debug', 'joint_after_debug', 'lineage_after_debug']) {
  mutations.push(p => { p.protocol_validation.observations[0].lifecycle[field] = 'obsolete SDK Debug text'; });
}
for (const mutate of mutations) {
  assert.deepEqual(checkProtocolComponent(puzzle),expected); // positive first
  const altered = structuredClone(puzzle); mutate(altered);
  assert.throws(() => checkProtocolComponent(altered)); controls++;
}
for (const mutate of [r => { const p = r.puzzles.find(p => p.slug === 'p2-singleton'); p.kind = 'pure_function'; delete p.protocol_validation; }, r => r.puzzles.push(structuredClone(puzzle)), r => { r.puzzles[0].protocol_validation = puzzle.protocol_validation; }]) {
  checkProtocolReport(report);
  const altered = structuredClone(report); mutate(altered);
  assert.throws(() => checkProtocolReport(altered)); controls++;
}
console.log(`PASS: recorded P2 five-build110-attempt closed schema without SDK Debug payloads, separate generic ledger contract, and ${controls} positive-normalized corruption controls; no replay/authentication credit`);
