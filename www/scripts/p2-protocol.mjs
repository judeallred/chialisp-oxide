// Typed recorded composition receipts. Full source/curry/coin checks live in
// the independent Python reader, called by current-source evidence validation.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkDelegatedComponent, delegatedSlug } from './p2dc-protocol.mjs';

import { checkCatComponent, catSlug } from './cat-protocol.mjs';
import { checkCounterComponent, checkCounterPair, checkCounterDisplay, counterCounts, counterSlugs } from '../src/lib/counter-protocol.mjs';

const tableBytes = readFileSync(new URL('../../crates/chialisp-oxide-simulator/p2-singleton-rejection-protocol.json', import.meta.url));
assert.equal(createHash('sha256').update(tableBytes).digest('hex'), '174c6321e5e8fc3c246fb65a12e596decb545ec9d5aba74c13a0347f71297192', 'Scoped P2 protocol table changed');
const table = JSON.parse(tableBytes.toString('utf8'));
export const protocol = 'chialisp-oxide-p2-singleton/v1';
export const rejectionProtocol = 'chia-consensus-0.36.1/p2-singleton-v1';
const modes = [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]];
const fields = (value, names, label) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), `${label}: expected object`);
  assert.deepEqual(Object.keys(value).sort(), [...names].sort(), `${label}: missing/unknown fields`);
};

export function checkProtocolComponent(puzzle, counterpart) {
  if (puzzle.slug === catSlug) return checkCatComponent(puzzle);
  if (puzzle.slug === delegatedSlug) return checkDelegatedComponent(puzzle);
  if (counterSlugs.includes(puzzle.slug)) {
    assert(counterpart, 'Counter component requires its actual independently compiled Chialisp counterpart entry');
    return checkCounterComponent(puzzle, counterpart);
  }
  assert.equal(puzzle.slug, 'p2-singleton', 'Unknown protocol component');
  assert.equal(puzzle.kind, 'protocol_component');
  assert.deepEqual(puzzle.simulator, [], 'Dedicated composition does not add generic scenario groups');
  assert.deepEqual(puzzle.simulator_comparisons, []);
  const value = puzzle.protocol_validation;
  fields(value, ['schema_version', 'protocol', 'status', 'scope', 'simulator', 'consensus', 'ledger_VM', 'network', 'modules', 'rejection_protocol', 'observations'], 'P2 evidence');
  assert.equal(value.schema_version, 1);
  assert.equal(value.protocol, protocol);
  assert.equal(value.status, 'validated');
  assert.equal(value.rejection_protocol, rejectionProtocol);
  for (const [key, expected] of Object.entries({ simulator: 'chia-sdk-test 0.36.0', consensus: 'chia-consensus 0.36.1', ledger_VM: 'clvmr 0.16.2', network: 'TESTNET11' })) assert.equal(value[key], expected);
  assert(typeof value.scope === 'string' && value.scope.length > 0);
  assert(Array.isArray(value.observations));
  assert(value.observations.every(row => typeof row.optimize_lir === 'boolean'));
  assert.deepEqual(value.observations.map(row => [row.backend, row.optimize_lir]), modes, 'P2 needs exactly five ordered builds');
  assert(Array.isArray(puzzle.artifacts) && puzzle.artifacts.every(row => typeof row.optimize_lir === 'boolean'));
  assert.deepEqual(puzzle.artifacts.map(row => [row.backend, row.optimize_lir]), modes, 'Missing/duplicate own-language compiler artifacts');
  const counts = { groups: 0, attempts: 0, accepted: 0, rejected: 0, codes: {} };
  for (const group of value.observations) {
    fields(group, ['backend', 'optimize_lir', 'module_tree_hash', 'curried_p2_reveal_hex', 'curried_p2_hash', 'canonical_singleton_reveal_hex', 'inner_hash', 'negatives', 'lifecycle'], 'P2 mode');
    const artifact = puzzle.artifacts.find(row => row.backend === group.backend && row.optimize_lir === group.optimize_lir);
    assert(artifact && artifact.tree_hash === group.module_tree_hash, 'P2 group differs from its own module');
    const lifecycle = group.lifecycle;
    fields(lifecycle, ['name', 'funding', 'launcher', 'eve', 'p2', 'continuation', 'next_singleton', 'genesis', 'joint', 'lineage', 'rejections', 'scope', 'receipts'], 'P2 lifecycle');
    assert(Array.isArray(group.negatives) && Array.isArray(lifecycle.rejections));
    assert.deepEqual(group.negatives.map(row => row.name), table.negatives.map(row => row.name));
    assert.deepEqual(lifecycle.rejections.map(row => row.name), table.lifecycle_rejections.map(row => row.name));
    const expectedRejections = [...table.negatives, ...table.lifecycle_rejections];
    for (const [index, row] of [...group.negatives, ...lifecycle.rejections].entries()) {
      fields(row, ['name', 'expected', 'observed', 'stage', 'full_simulator_equality', 'transaction', 'consensus'], 'P2 rejection');
      const { name, code } = expectedRejections[index];
      assert.equal(row.name, name);
      assert.equal(row.expected, code);
      assert.equal(row.observed, code);
      assert.equal(row.stage, 'actual_consensus_submission');
      assert.equal(row.full_simulator_equality, true, 'Recorded full rollback must be Boolean true');
      assert.deepEqual(row.consensus, { stage: 'consensus', protocol: rejectionProtocol, code, status: 'rejected', semantic_rejection_credit: true });
      assert(typeof row.transaction?.signature_hex === 'string' && row.transaction.signature_hex.length === 192 && Array.isArray(row.transaction.spends) && row.transaction.spends.length > 0, 'Missing actual retained bundle');
      counts.rejected++;
      counts.codes[code] = (counts.codes[code] ?? 0) + 1;
    }
    const receipts = table.accepted_receipts.map(name => ({ name, stage: 'actual_consensus_submission', status: 'accepted', bundle_field: name }));
    assert.deepEqual(lifecycle.receipts, receipts, 'Missing/unknown accepted submission receipts');
    for (const receipt of lifecycle.receipts) {
      const bundle = lifecycle[receipt.bundle_field];
      assert(typeof bundle?.signature_hex === 'string' && bundle.signature_hex.length === 192 && Array.isArray(bundle.spends) && bundle.spends.length > 0, 'Missing accepted retained bundle');
      counts.accepted++;
    }
    counts.groups++;
  }
  counts.attempts = counts.accepted + counts.rejected;
  assert.deepEqual([counts.groups, counts.attempts, counts.accepted, counts.rejected], [5, 110, 15, 95]);
  return counts;
}

export function checkProtocolReport(report) {
  // Counter lifecycle evidence binds each selected action to the other entry's
  // actual Chialisp artifact. Admit the complete pair once, never one row alone.
  if (report.puzzles.some(puzzle => counterSlugs.includes(puzzle.slug))) checkCounterPair(report.puzzles);
  const selected = report.puzzles.filter(puzzle => puzzle.slug === 'p2-singleton');
  const otherComponents = report.puzzles.filter(puzzle => !['p2-singleton',delegatedSlug,catSlug,...counterSlugs].includes(puzzle.slug) && (puzzle.kind === 'protocol_component' || Object.hasOwn(puzzle, 'protocol_validation')));
  assert.deepEqual(otherComponents, [], 'Unknown dedicated protocol evidence requires its own reviewed reader.');
  const cat = report.puzzles.filter(puzzle => puzzle.slug === catSlug);
  assert(cat.length <= 1, 'Duplicate CAT component');
  if (cat.length) checkCatComponent(cat[0]);
  const delegated = report.puzzles.filter(puzzle => puzzle.slug === delegatedSlug);
  assert(delegated.length <= 1, 'Duplicate delegated-conditions component');
  if (delegated.length) checkDelegatedComponent(delegated[0]);
  assert(selected.length <= 1, 'Duplicate/unknown dedicated component evidence');
  return selected.length ? checkProtocolComponent(selected[0]) : { groups: 0, attempts: 0, accepted: 0, rejected: 0, codes: {} };
}

export function historicalOptimizationReport(report) {
  if (!report.puzzles.some(puzzle => puzzle.slug === 'p2-singleton')) return report;
  // Actual current22 source/native/typed/sidecars MUST precede composition
  // and historical projection; no restored object is validated as current. This subprocess
  // only reads/checks recorded evidence and sources: no extraction or replay.
  const source = `import json,sys\nsys.path.insert(0,'scripts')\nimport runtime_cost_migration as cost\nreport=json.load(sys.stdin)\ncost.validate_current(report)\ncheckpoint=cost.composition_checkpoint(report)\nsys.stdout.buffer.write(cost.canonical(cost.project_append(checkpoint)))\n`;
  const output = execFileSync('python3', ['-c', source], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)), input: JSON.stringify(report), encoding: 'utf8',
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' }, timeout: 120_000, maxBuffer: 16 * 1024 * 1024,
  });
  const baseline = JSON.parse(output);
  assert.equal(baseline.puzzles.length, 21, 'Projection must preserve exact signed cost21');
  return baseline;
}

export function checkProtocolDisplay(markup, puzzle, counterpart) {
  if (counterSlugs.includes(puzzle.slug)) {
    assert(counterpart, 'Counter display requires its actual counterpart entry');
    checkCounterDisplay(markup, [puzzle, counterpart], puzzle.slug);
    return counterCounts(puzzle.slug);
  }
  // This HTML schema is specifically P2. CAT and delegated-conditions callers
  // use their own display readers rather than falling through P2 field names.
  assert.equal(puzzle.slug, 'p2-singleton', 'Use the dedicated component display reader');
  const counts = checkProtocolComponent(puzzle);
  const section = markup.match(/<section\b[^>]*data-protocol-validation[^>]*>[\s\S]*?<\/section>/)?.[0];
  assert(section, 'Missing dedicated protocol section');
  for (const [name, count] of Object.entries({ groups: counts.groups, attempts: counts.attempts, accepted: counts.accepted, rejected: counts.rejected })) {
    assert.equal(section.match(new RegExp(`data-protocol-${name}="([0-9]+)"`))?.[1], String(count), `Displayed P2 ${name} differs from recorded receipts`);
  }
  const observedModes = [...section.matchAll(/data-protocol-mode="([^"]+)"/g)].map(match => match[1]);
  assert.deepEqual(observedModes, puzzle.protocol_validation.observations.map(group => `${group.backend}:${group.optimize_lir}`));
  for (const group of puzzle.protocol_validation.observations) {
    const mode = `${group.backend}:${group.optimize_lir}`;
    const detail = section.match(new RegExp(`<details\\b[^>]*data-protocol-mode="${mode}"[^>]*>[\\s\\S]*?</details>`))?.[0];
    assert(detail && detail.includes(group.module_tree_hash) && detail.includes(group.curried_p2_hash), 'Displayed P2 commitment differs from its own module');
    assert.deepEqual([...detail.matchAll(/data-protocol-rejection="([^"]+)"/g)].map(match => match[1]), [...group.negatives, ...group.lifecycle.rejections].map(row => row.consensus.code));
    assert.deepEqual([...detail.matchAll(/data-protocol-attempt="([^"]+)"/g)].map(match => match[1]), [...group.negatives, ...group.lifecycle.rejections].map(row => row.name));
    assert.deepEqual([...detail.matchAll(/data-protocol-receipt="([^"]+)"/g)].map(match => match[1]), group.lifecycle.receipts.map(row => row.name));
  }
  const text = section.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');
  for (const boundary of ['host counterparts, not Rust ports', 'complete singleton lifecycle', 'ordinary ledger scenario-group totals', 'signer preflight does not count', 'complete in-memory Simulator', 'does not serialize SDK state or include Debug payloads', 'checking the record does not prove state equality', 'Source freshness and finite tests do not authenticate execution', 'clvmr 0.21.0', 'clvmr 0.16.2', rejectionProtocol]) assert(text.includes(boundary), `P2 display loses scope: ${boundary}`);
  assert(!section.includes('data-ledger-language'), 'Dedicated component inflates ordinary scenario groups');
  return counts;
}
