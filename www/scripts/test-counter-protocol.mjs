// Pure retained witness/display checks; no compiler, VM or SDK execution.
import assert from 'node:assert/strict';
import { constants } from 'node:fs';
import { open } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import {
  counterSlugs, counterModes, counterProtocol, counterCounts,
  checkCounterPair, checkCounterDisplay,
  counterCorruptionControls, counterDisplayCorruptionControls,
} from '../src/lib/counter-protocol.mjs';
import { checkLedgerReport, ledgerProtocol, testLedgerProtocol } from './ledger-rejections.mjs';
import { checkProtocolComponent, checkProtocolReport, checkProtocolDisplay } from './p2-protocol.mjs';

let requestedReport, built = false;
for (let index = 2; index < process.argv.length; index++) {
  const argument = process.argv[index];
  if (argument === '--built') {assert(!built); built = true;}
  else if (argument === '--report') {assert(requestedReport === undefined && index + 1 < process.argv.length); requestedReport = process.argv[++index];}
  else assert.fail(`Unknown argument ${argument}`);
}
assert(!(built && requestedReport), 'Built checks require the canonical current corpus report');

async function boundedRead(filename, maximum = 16 * 1024 * 1024) {
  const file = await open(filename, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat(); assert(before.isFile() && before.size <= maximum, 'Bounded regular counter input');
    const bytes = Buffer.alloc(before.size + 1); let length = 0;
    while (length < bytes.length) {const { bytesRead } = await file.read(bytes,length,bytes.length-length,null); if (!bytesRead) break; length += bytesRead;}
    assert.equal(length,before.size,'Counter input changed during bounded read');
    return bytes.subarray(0,length);
  } finally {await file.close();}
}

const filename = requestedReport ? path.resolve(requestedReport) : fileURLToPath(new URL('../../corpus/validation.json',import.meta.url));
const report = JSON.parse(await boundedRead(filename));
const rows = counterSlugs.map(slug => {const selected = report.puzzles.filter(puzzle => puzzle.slug === slug); assert.equal(selected.length,1,`Missing actual counter capture ${slug}; use --report for an explicit partial capture`); return selected[0];});
const checked = checkCounterPair(rows);
assert.deepEqual(Object.fromEntries(['groups','attempts','accepted','rejected','runtime_umbrellas','vm'].map(key => [key,checked[key]])),{groups:10,attempts:60,accepted:30,rejected:20,runtime_umbrellas:10,vm:70});
for (const [name,mutate] of counterCorruptionControls) {const changed = structuredClone(rows); mutate(changed); assert.throws(() => checkCounterPair(changed),undefined,`Accepted constructed counter corruption: ${name}`);}

// Generic report readers must require the complete actual counterpart pair,
// without reopening this counter protocol for old reports that lack its slugs.
checkLedgerReport(report);
checkProtocolReport(report);
const counterReport = {simulator:{rejection_protocol:ledgerProtocol},puzzles:rows};
const emptyP2Counts = {groups:0,attempts:0,accepted:0,rejected:0,codes:{}};
checkLedgerReport(counterReport);
assert.deepEqual(checkProtocolReport(counterReport),emptyP2Counts,'Counter admissions must not inflate P2 singleton totals');
let dispatchControls = 0;
for (const [index,row] of rows.entries()) {
  assert.throws(() => checkProtocolComponent(row),/requires its actual independently compiled Chialisp counterpart/);
  assert.deepEqual(checkProtocolComponent(row,rows[1-index]),counterCounts(row.slug));
  assert.throws(() => checkProtocolDisplay('',row),/requires its actual counterpart/);
  dispatchControls += 2;
}
for (const mutate of [
  value => {value.puzzles.pop();},
  value => {value.puzzles.push(structuredClone(value.puzzles[0]));},
  value => {value.puzzles[0].kind='pure_function';delete value.puzzles[0].protocol_validation;},
  value => {value.puzzles[1].protocol_validation.builds[0].counterpart.backend='rust_mir';},
  value => {value.puzzles[1].protocol_validation.builds[0].lifecycle.selected_child_state=1;},
  value => {value.puzzles[0].protocol_validation.builds[0].lifecycle.continued_child.coin_id='00'.repeat(32);},
  value => {value.puzzles.push({slug:'unreviewed-component',kind:'protocol_component',simulator:[],simulator_comparisons:[],protocol_validation:{}});},
]) {
  const bad=structuredClone(counterReport);mutate(bad);
  assert.throws(() => checkLedgerReport(bad));
  assert.throws(() => checkProtocolReport(bad));
  dispatchControls += 2;
}
for (const name of ['checkpoints/p2-source-cost-validation.json','checkpoints/p2dc-append-validation.json','cost-optimization-baseline.json']) {
  const historical=JSON.parse(await boundedRead(new URL(`../../corpus/${name}`,import.meta.url)));
  assert(!historical.puzzles.some(row=>counterSlugs.includes(row.slug)));
  checkProtocolReport(historical);
  // The oldest signed optimization report intentionally predates typed ledger
  // receipts; the generic ledger API has no historical untyped bypass.
  if (name !== 'cost-optimization-baseline.json') checkLedgerReport(historical);
}
testLedgerProtocol();

function constructedMarkup(selectedSlug) {
  // The metadata is actual retained data; this HTML fixture is constructed
  // only to test visible-witness gates. It is not an Astro render or execution.
  const selected = selectedSlug ? rows.filter(row => row.slug === selectedSlug) : rows;
  const totals = selected.map(row => counterCounts(row.slug)).reduce((sum,value) => Object.fromEntries(Object.keys(sum).map(key => [key,sum[key]+value[key]])),{groups:0,attempts:0,accepted:0,rejected:0,runtime_umbrellas:0,vm:0});
  const numbers = Object.entries(totals).map(([key,value]) => `<span data-counter-${key === 'runtime_umbrellas' ? 'umbrellas' : key}>${value}</span>`).join(' ');
  const scope = 'WIP prototype; not ready for real funds; independently compiled Chialisp; registry and slots remain pending; live Testnet11 acceptance remains pending; not a Rust port of the outer action layer.';
  const sides = selected.map(puzzle => `<div data-counter-side="${puzzle.slug}">${puzzle.protocol_validation.builds.map(build => {
    const lane = build.lifecycle;
    const values = {module:build.module_tree_hash,counterpart:build.counterpart.module_tree_hash,root:lane.merkle_root,launcher:lane.launcher.coin_id,eve:lane.eve.coin_id,child:lane.selected_child.coin_id,continuation:lane.continued_child.coin_id};
    const hashes = Object.entries(values).map(([key,value]) => `<code data-counter-${key}>${value}</code>`).join(' ');
    const states = Object.entries({initial:lane.initial_state,selected:lane.selected_child_state,continued:lane.continued_child_state}).map(([key,value]) => `<strong data-counter-state-${key}>${value}</strong>`).join(' ');
    const attempts = lane.attempts.map(attempt => `<li data-counter-attempt="${attempt.name}"><code data-counter-name>${attempt.name}</code>: <span data-counter-status>${attempt.outcome.status}</span>${attempt.outcome.code ? `<code data-counter-code>${attempt.outcome.code}</code>` : ''}</li>`).join('');
    return `<details data-counter-build="${build.backend}/${build.optimize_lir}"><summary>${({rust_mir:'Rust',chialisp:'Chialisp',rue_source:'Rue'})[build.backend]} / ${build.optimize_lir ? 'on' : 'off'} · ${lane.sdk_transaction_attempts} submissions</summary><p>${states}</p><p>${hashes}</p><ol>${attempts}</ol></details>`;
  }).join('')}<p>${puzzle.protocol_validation.primary_vm}; ${puzzle.protocol_validation.sdk_vm}; ${puzzle.protocol_validation.consensus}; ${puzzle.protocol_validation.network}</p></div>`).join('');
  return `<section id="counter-lifecycle" data-counter-protocol="${counterProtocol}"><p>${numbers}</p><p>${scope}</p>${sides}</section>`;
}
const markup = constructedMarkup();
checkCounterDisplay(markup,rows);
for (const slug of counterSlugs) checkCounterDisplay(constructedMarkup(slug),rows,slug);
for (const [index,row] of rows.entries()) assert.deepEqual(checkProtocolDisplay(constructedMarkup(row.slug),row,rows[1-index]),counterCounts(row.slug));
for (const [name,mutate] of counterDisplayCorruptionControls) {const changed = mutate(markup); assert.notEqual(changed,markup,`Inert constructed display control: ${name}`); assert.throws(() => checkCounterDisplay(changed,rows),undefined,`Accepted visible counter corruption: ${name}`);}

if (built) {
  for (const route of ['tutorials/action-counter',...counterSlugs.map(slug => `lab/${slug}`)]) {
    const markup = (await boundedRead(new URL(`../dist/${route}/index.html`,import.meta.url))).toString('utf8');
    checkCounterDisplay(markup,rows,route.startsWith('lab/') ? route.split('/')[1] : undefined);
  }
  const published = JSON.parse(await boundedRead(new URL('../dist/reports/validation.json',import.meta.url)));
  for (const row of rows) assert.deepEqual(published.puzzles.find(puzzle => puzzle.slug === row.slug)?.protocol_validation,row.protocol_validation,'Published counter journal differs from the current corpus');
}
assert.equal(counterModes.length,5);
console.log(`PASS counter ${counterCorruptionControls.length} retained-witness controls, ${counterDisplayCorruptionControls.length} visible-witness controls, ${dispatchControls} generic-dispatch controls, three historical protocol reports, two selected-side displays and combined tutorial${built ? ', three actual built routes and published journals' : '; constructed HTML only, no Astro/VM/SDK execution'}`);
