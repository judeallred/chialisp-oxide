// Retained counter witness checks. These readers add no VM/SDK execution,
// independent BLS verification, source authenticity or compiler-preservation credit.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { catTreeHash } from '../../scripts/cat-protocol.mjs';

export const counterSlugs = ['counter-increment', 'counter-reset'];
export const counterProtocol = 'chialisp-oxide-action-counter/v1';
export const counterModes = [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]];
export const counterEmptySignature = 'c0' + '00'.repeat(95);
const profile = {sdk:'chia-sdk-test 0.36.0', consensus:'chia-consensus 0.36.1', sdk_vm:'clvmr 0.16.2', network:'TESTNET11', primary_vm:'clvmr 0.21.0', primary_protocol:'clvmr-0.21.0/v1', primary_flags:535, primary_max_cost:250000000, sdk_max_block_cost_clvm:11000000000, sdk_transaction_max_cost:5500000000};
const canonical = [
  ['action_layer', 630, 'afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b'],
  ['default_finalizer', 494, '9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b'],
  ['singleton_launcher', 175, 'eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9'],
  ['singleton_top_layer_v1_1', 967, '7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f'],
];
const standardHash = 'e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52';
const intent = Buffer.from('chialisp-oxide/counter/reset:v1!').toString('hex');
const maximum = 1048576;
const closed = (value, keys) => {assert(value && typeof value === 'object' && !Array.isArray(value)); assert.deepEqual(Object.keys(value).sort(), [...keys].sort());};
const hex = (value, bytes) => {assert(typeof value === 'string' && /^(?:[a-f0-9]{2})*$/.test(value)); assert(value.length <= 2 * maximum); if (bytes !== undefined) assert.equal(value.length, 2 * bytes); return Buffer.from(value, 'hex');};
const hash = value => catTreeHash(value);
const sha = value => createHash('sha256').update(value).digest('hex');
const pair = (first, rest) => {assert(first.length + rest.length + 2 <= maximum * 2, 'Pair transport precharge'); return 'ff' + first + rest;};
const list = values => {assert(Array.isArray(values) && values.length <= 64); return values.reduceRight((tail, value) => pair(value, tail), '80');};
const atom = value => {const raw = hex(value); let prefix; if (raw.length === 1 && raw[0] < 128) return value; if (raw.length < 64) prefix = Buffer.from([128 + raw.length]); else if (raw.length < 8192) prefix = Buffer.from([192 + (raw.length >> 8), raw.length & 255]); else prefix = Buffer.from([224 + (raw.length >> 16), (raw.length >> 8) & 255, raw.length & 255]); assert(raw.length + prefix.length <= maximum); return prefix.toString('hex') + value;};
const integer = value => {assert(Number.isSafeInteger(value) && value >= 0 && value < 128); return value ? atom(value.toString(16).padStart(2, '0')) : '80';};
export function counterCurry(module, args) {hash(module); assert(Array.isArray(args) && args.length <= 8); let environment = '01'; for (const value of [...args].reverse()) {hash(value); environment = list(['04', pair('01', value), environment]);} const result = list(['02', pair('01', module), environment]); hash(result); return result;}

// Decode only after complete bounded canonical inspection. Tree allocation is
// at most the same 65536 nodes/512 depth admitted by the hash preflight above.
function decode(value) {
  hash(value); const raw = hex(value); let cursor = 0; const stack = [];
  while (true) {
    const start = cursor, head = raw[cursor++]; if (head === 255) {stack.push(null); continue;}
    let node;
    if (head < 128) node = raw.subarray(start, cursor).toString('hex');
    else if (head === 128) node = '';
    else {let width = 0; while (head & (128 >> width)) width++; let length = head & ((1 << (7 - width)) - 1); for (let i = 0; i < width - 1; i++) length = length * 256 + raw[cursor++]; node = raw.subarray(cursor, cursor + length).toString('hex'); cursor += length;}
    while (stack.length && stack.at(-1) !== null) node = [stack.pop(), node];
    if (stack.length) stack[stack.length - 1] = node; else {assert.equal(cursor, raw.length); return node;}
  }
}
function encode(tree) {const pending = [tree]; const pieces = []; let bytes = 0, nodes = 0; while (pending.length) {assert(++nodes <= 65536); const node = pending.pop(); if (Array.isArray(node)) {assert.equal(node.length, 2); bytes++; assert(bytes <= maximum); pieces.push('ff'); pending.push(node[1], node[0]);} else {const value = atom(node); bytes += value.length / 2; assert(bytes <= maximum); pieces.push(value);}} const result = pieces.join(''); hash(result); return result;}
function proper(tree, count) {const values = []; while (Array.isArray(tree)) {assert(values.length < 64); values.push(tree[0]); tree = tree[1];} assert.equal(tree, ''); if (count !== undefined) assert.equal(values.length, count); return values;}
function standardWalletModule(reveal, owner) {const [operation, quoted, environment] = proper(decode(reveal), 3); assert.equal(operation, '02'); assert(Array.isArray(quoted) && quoted[0] === '01'); const [cons, argument, original] = proper(environment, 3); assert.equal(cons, '04'); assert.equal(original, '01'); assert(Array.isArray(argument) && argument[0] === '01'); assert.equal(argument[1], owner); const module = encode(quoted[1]); assert.equal(hash(module), standardHash); assert.equal(reveal, counterCurry(module, [atom(owner)])); return module;}

function coin(value, parent, puzzle) {closed(value, ['coin_id','parent_coin_id','puzzle_hash','amount']); for (const key of ['coin_id','parent_coin_id','puzzle_hash']) hex(value[key], 32); assert.equal(value.amount, 1); assert.equal(value.coin_id, sha(Buffer.concat([hex(value.parent_coin_id,32), hex(value.puzzle_hash,32), Buffer.from([1])]))); if (parent !== undefined) assert.equal(value.parent_coin_id, parent); if (puzzle !== undefined) assert.equal(value.puzzle_hash, puzzle); return value.coin_id;}
function primary(value, output, runtime = false) {closed(value, ['outcome','cost','failure']); if (runtime) {assert.deepEqual(value.outcome, {status:'rejected'}); assert.equal(value.cost, null); assert.deepEqual(value.failure, {kind:'program',code:'raise'});} else {assert.deepEqual(value.outcome, {status:'accepted',output_hex:output}); assert.equal(value.failure, null); assert(Number.isSafeInteger(value.cost) && value.cost > 0 && value.cost <= profile.primary_max_cost); hash(output);}}
function outcome(value, code = null) {if (code === null) assert.deepEqual(value, {status:'accepted'}); else assert.deepEqual(value, {status:'rejected',stage:code === 'GeneratorRuntimeError' ? 'runtime_umbrella' : 'consensus',code,complete_state_unchanged:true});}
function expectedPolicy(increment, resetTemplate, owner, launcher, modules) {
  const reset = counterCurry(resetTemplate, [atom(owner)]);
  const leaves = [increment, reset].map(module => sha(Buffer.concat([Buffer.from([1]), hex(hash(module),32)])));
  const root = sha(Buffer.concat([Buffer.from([2]), ...leaves.map(value => hex(value,32))]));
  const first = counterCurry(modules.default_finalizer, [atom(hash(modules.action_layer)), atom(launcher)]);
  const finalizer = counterCurry(first, [atom(hash(first))]);
  const inner = state => counterCurry(modules.action_layer, [finalizer, atom(root), integer(state)]);
  const full = state => counterCurry(modules.singleton_top_layer_v1_1, [pair(atom(hash(modules.singleton_top_layer_v1_1)),pair(atom(launcher),atom(hash(modules.singleton_launcher)))), inner(state)]);
  const solution = (selected, bad = false) => list([list([selected ? reset : increment]),list([pair('02',list([integer(selected),atom(bad ? '45'.repeat(32) : leaves[1-selected])]))]),list(['80'])]);
  const fullSolution = (lineage, selected, bad = false) => list([lineage,'01',solution(selected,bad)]);
  const conditions = (state, parent, signed) => list([list(['49','01']),list(['47',atom(parent)]),list(['33',atom(hash(full(state))),'01',list([atom(launcher)])]),...(signed ? [list(['32',atom(owner),atom(intent)])] : [])]);
  return {root, inner, full, fullSolution, conditions};
}

export function counterCounts(slug) {assert(counterSlugs.includes(slug)); return slug === 'counter-increment' ? {groups:5,attempts:25,accepted:15,rejected:5,runtime_umbrellas:5,vm:30} : {groups:5,attempts:35,accepted:15,rejected:15,runtime_umbrellas:5,vm:40};}
export function checkCounterComponent(puzzle, counterpart) {
  const selected = counterSlugs.indexOf(puzzle.slug); assert(selected >= 0); assert.equal(counterpart.slug,counterSlugs[1-selected]);
  for (const row of [puzzle,counterpart]) {assert.equal(row.kind,'protocol_component'); assert.equal(row.environment ?? 'strict','strict'); assert.equal(row.validated,true); assert.deepEqual(row.simulator,[]); assert.deepEqual(row.simulator_comparisons,[]); assert.deepEqual(row.artifacts.map(a => [a.backend,a.optimize_lir]),counterModes); assert(row.artifacts.every(a => typeof a.optimize_lir === 'boolean'));}
  const value = puzzle.protocol_validation, totals = counterCounts(puzzle.slug);
  closed(value,['schema_version','protocol','status','selected_action',...Object.keys(profile),'owner_public_key','canonical_components','builds','sdk_transaction_attempts','accepted','typed_rejected','runtime_umbrellas','component_primary_vm_runs','scope']);
  assert.equal(value.schema_version,1); assert.equal(value.protocol,counterProtocol); assert.equal(value.status,'validated'); assert.equal(value.selected_action,puzzle.slug); assert(typeof value.scope === 'string' && value.scope.length > 0);
  for (const [key,expected] of Object.entries(profile)) assert.equal(value[key],expected);
  for (const [key,total] of Object.entries({sdk_transaction_attempts:totals.attempts,accepted:totals.accepted,typed_rejected:totals.rejected,runtime_umbrellas:totals.runtime_umbrellas,component_primary_vm_runs:totals.vm})) assert.equal(value[key],total);
  const owner = value.owner_public_key; hex(owner,48); const modules = {};
  assert.deepEqual(value.canonical_components.map(row => row.name),canonical.map(row => row[0]));
  for (const [index,row] of value.canonical_components.entries()) {closed(row,['name','module_hex','bytes','module_tree_hash']); const [,bytes,treeHash] = canonical[index]; assert.equal(hex(row.module_hex).length,bytes); assert.equal(row.bytes,bytes); assert.equal(hash(row.module_hex),treeHash); assert.equal(row.module_tree_hash,treeHash); modules[row.name] = row.module_hex;}
  assert.deepEqual(value.builds.map(build => [build.backend,build.optimize_lir]),counterModes);
  for (const [index,build] of value.builds.entries()) {
    closed(build,['backend','optimize_lir','selected_action','module_tree_hash','counterpart','lifecycle']); assert.equal(typeof build.optimize_lir,'boolean'); assert.equal(build.selected_action,puzzle.slug);
    const artifact = puzzle.artifacts[index]; assert.equal(hex(artifact.clvm_hex).length,artifact.bytes); assert.equal(hash(artifact.clvm_hex),artifact.tree_hash); assert.equal(build.module_tree_hash,artifact.tree_hash);
    const reference = counterpart.artifacts[2]; assert.equal(hash(reference.clvm_hex),reference.tree_hash); assert.equal(hex(reference.clvm_hex).length,reference.bytes);
    assert.deepEqual(build.counterpart,{action:counterpart.slug,backend:'chialisp',optimize_lir:true,module_hex:reference.clvm_hex,module_tree_hash:reference.tree_hash});
    const lane = build.lifecycle; closed(lane,['selected_action','initial_state','selected_child_state','continued_child_state','owner_public_key','merkle_root','increment_module_hex','reset_template_hex','genesis','launcher','eve','selected_child','continued_child','attempts','sdk_transaction_attempts','accepted','typed_rejected','runtime_umbrellas','component_primary_vm_runs']);
    const next = selected ? 0 : 6; assert.equal(lane.selected_action,puzzle.slug); assert.equal(lane.initial_state,5); assert.equal(lane.selected_child_state,next); assert.equal(lane.continued_child_state,next+1); assert.equal(lane.owner_public_key,owner);
    assert.equal(lane.increment_module_hex,selected ? reference.clvm_hex : artifact.clvm_hex); assert.equal(lane.reset_template_hex,selected ? artifact.clvm_hex : reference.clvm_hex);
    const genesis = coin(lane.genesis); const launcher = coin(lane.launcher,genesis,hash(modules.singleton_launcher));
    const policy = expectedPolicy(lane.increment_module_hex,lane.reset_template_hex,owner,launcher,modules); assert.equal(lane.merkle_root,policy.root);
    const eve = coin(lane.eve,launcher,hash(policy.full(5))), child = coin(lane.selected_child,eve,hash(policy.full(next))); coin(lane.continued_child,child,hash(policy.full(next+1)));
    const names = ['fund_and_launch_singleton','wrong_membership_sibling',...(selected ? ['missing_reset_signature','wrong_key_reset_signature'] : []),'selected_action','consumed_bundle_replay','increment_actual_child'];
    assert.deepEqual(lane.attempts.map(attempt => attempt.name),names);
    for (const [key,number] of Object.entries({sdk_transaction_attempts:names.length,accepted:3,typed_rejected:selected ? 3 : 1,runtime_umbrellas:1,component_primary_vm_runs:names.length+1})) assert.equal(lane[key],number);
    const selectedSolution = policy.fullSolution(list([atom(genesis),'01']),selected);
    const continuedSolution = policy.fullSolution(list([atom(launcher),atom(hash(policy.inner(5))),'01']),0);
    const launchSolution = list([atom(hash(policy.full(5))),'01','80']);
    let selectedTransaction;
    for (const attempt of lane.attempts) {
      closed(attempt,['name','transaction','primary_vm','outcome']); const tx = attempt.transaction; closed(tx,['signature_hex','spends']); hex(tx.signature_hex,96);
      const name = attempt.name, funding = name === names[0], continuation = name === 'increment_actual_child', runtime = name === 'wrong_membership_sibling';
      const code = runtime ? 'GeneratorRuntimeError' : name === 'consumed_bundle_replay' ? 'DoubleSpend' : ['missing_reset_signature','wrong_key_reset_signature'].includes(name) ? 'BadAggregateSignature' : null;
      outcome(attempt.outcome,code); assert.equal(tx.spends.length,funding ? 2 : 1); assert.equal(attempt.primary_vm.length,tx.spends.length);
      for (const spend of tx.spends) {closed(spend,['coin','reveal_hex','solution_hex']); coin(spend.coin); assert.equal(hash(spend.reveal_hex),spend.coin.puzzle_hash); hash(spend.solution_hex);}
      if (funding) {
        assert.deepEqual(tx.spends.map(spend => spend.coin),[lane.genesis,lane.launcher]); standardWalletModule(tx.spends[0].reveal_hex,owner); assert.equal(tx.spends[1].reveal_hex,modules.singleton_launcher); assert.equal(tx.spends[1].solution_hex,launchSolution);
        const announcement = sha(Buffer.concat([hex(launcher,32),hex(hash(launchSolution),32)]));
        const conditions = list([list(['33',atom(hash(modules.singleton_launcher)),'01']),list(['3d',atom(announcement)])]); const delegated = pair('01',conditions);
        assert.equal(tx.spends[0].solution_hex,list(['80',delegated,'80']));
        primary(attempt.primary_vm[0],pair(list(['32',atom(owner),atom(hash(delegated))]),conditions)); primary(attempt.primary_vm[1],list([list(['33',atom(hash(policy.full(5))),'01']),list(['3c',atom(hash(launchSolution))])])); assert.notEqual(tx.signature_hex,counterEmptySignature);
      } else {
        const spend = tx.spends[0]; assert.deepEqual(spend.coin,continuation ? lane.selected_child : lane.eve); assert.equal(spend.reveal_hex,policy.full(continuation ? next : 5)); assert.equal(spend.solution_hex,runtime ? policy.fullSolution(list([atom(genesis),'01']),selected,true) : continuation ? continuedSolution : selectedSolution);
        primary(attempt.primary_vm[0],policy.conditions(continuation ? next+1 : next,continuation ? eve : launcher,!continuation && !!selected),runtime);
        if (name === 'selected_action') selectedTransaction = tx;
        if (name === 'consumed_bundle_replay') assert.deepEqual(tx,selectedTransaction);
        if (['wrong_membership_sibling','missing_reset_signature','increment_actual_child'].includes(name) || (!selected && name === 'selected_action')) assert.equal(tx.signature_hex,counterEmptySignature);
        if (name === 'wrong_key_reset_signature' || (selected && name === 'selected_action')) assert.notEqual(tx.signature_hex,counterEmptySignature);
      }
    }
    if (selected) {const accepted = lane.attempts.find(a => a.name === 'selected_action'), wrong = lane.attempts.find(a => a.name === 'wrong_key_reset_signature'); assert.deepEqual(accepted.transaction.spends,wrong.transaction.spends); assert.notEqual(accepted.transaction.signature_hex,wrong.transaction.signature_hex);}
  }
  return totals;
}

export function checkCounterPair(puzzles) {assert(Array.isArray(puzzles)); const rows = counterSlugs.map(slug => {const matches = puzzles.filter(puzzle => puzzle.slug === slug); assert.equal(matches.length,1,`Missing or duplicated ${slug}`); return matches[0];}); const counts = rows.map((row,index) => checkCounterComponent(row,rows[1-index])); return {groups:10,attempts:60,accepted:30,rejected:20,runtime_umbrellas:10,vm:70,rows,counts};}

const visibleValue = (markup, attribute, expected, tag = '[a-z]+') => {assert(new RegExp(`<(${tag})\\b[^>]*${attribute}(?:="[^"]*")?[^>]*>\\s*${expected}\\s*</\\1>`).test(markup),`Missing visible ${attribute}=${expected}`);};
export function checkCounterDisplay(markup,puzzles,selectedSlug) {
  const checked = checkCounterPair(puzzles); assert(selectedSlug === undefined || counterSlugs.includes(selectedSlug));
  const rows = selectedSlug ? checked.rows.filter(row => row.slug === selectedSlug) : checked.rows;
  const section = markup.match(/<section\b[^>]*id="counter-lifecycle"[^>]*>[\s\S]*?<\/section>/)?.[0]; assert(section,'Missing counter lifecycle section'); assert(section.includes(`data-counter-protocol="${counterProtocol}"`));
  const totals = rows.map(row => counterCounts(row.slug)).reduce((sum,value) => Object.fromEntries(Object.keys(sum).map(key => [key,sum[key]+value[key]])),{groups:0,attempts:0,accepted:0,rejected:0,runtime_umbrellas:0,vm:0});
  for (const [key,value] of Object.entries(totals)) visibleValue(section,`data-counter-${key === 'runtime_umbrellas' ? 'umbrellas' : key}`,value);
  const visible = section.replace(/<[^>]+>/g,' ').replace(/\s+/g,' ');
  for (const phrase of ['WIP prototype','not ready for real funds','independently compiled Chialisp','registry and slots remain pending','live Testnet11 acceptance remains pending','clvmr 0.21.0','clvmr 0.16.2','not a Rust port of the outer action layer']) assert(visible.includes(phrase),`Missing counter scope: ${phrase}`);
  const sides = [...section.matchAll(/<div\b[^>]*data-counter-side="([^"]+)"[^>]*>([\s\S]*?)<\/div>/g)]; assert.deepEqual(sides.map(row => row[1]),rows.map(row => row.slug));
  for (const [sideIndex,side] of sides.entries()) {
    const puzzle = rows[sideIndex], builds = [...side[2].matchAll(/<details\b[^>]*data-counter-build="([^"]+)"[^>]*>([\s\S]*?)<\/details>/g)]; assert.deepEqual(builds.map(row => row[1]),counterModes.map(([backend,optimize]) => `${backend}/${optimize}`));
    const sideVisible = side[2].replace(/<[^>]+>/g,' ').replace(/\s+/g,' ');
    for (const key of ['primary_vm','sdk_vm','consensus','network']) assert(sideVisible.includes(puzzle.protocol_validation[key]),`Missing visible counter version ${key}`);
    for (const [index,detail] of builds.entries()) {
      const build = puzzle.protocol_validation.builds[index], lane = build.lifecycle, body = detail[2];
      const summary = body.match(/<summary\b[^>]*>([\s\S]*?)<\/summary>/)?.[1]?.replace(/<[^>]+>/g,' ').replace(/\s+/g,' '); assert(summary,'Missing counter build summary');
      assert(summary.includes(`${({rust_mir:'Rust',chialisp:'Chialisp',rue_source:'Rue'})[build.backend]} / ${build.optimize_lir ? 'on' : 'off'}`),'Missing visible selected backend/mode'); assert(summary.includes(`${lane.sdk_transaction_attempts} submissions`),'Missing visible build submission count');
      for (const [key,value] of Object.entries({module:build.module_tree_hash,counterpart:build.counterpart.module_tree_hash,root:lane.merkle_root,launcher:lane.launcher.coin_id,eve:lane.eve.coin_id,child:lane.selected_child.coin_id,continuation:lane.continued_child.coin_id})) visibleValue(body,`data-counter-${key}`,value,'code');
      for (const [key,value] of Object.entries({initial:5,selected:lane.selected_child_state,continued:lane.continued_child_state})) visibleValue(body,`data-counter-state-${key}`,value);
      const attempts = [...body.matchAll(/<li\b[^>]*data-counter-attempt="([^"]+)"[^>]*>([\s\S]*?)<\/li>/g)]; assert.deepEqual(attempts.map(row => row[1]),lane.attempts.map(row => row.name));
      for (const [i,row] of attempts.entries()) {const attempt = lane.attempts[i]; visibleValue(row[2],'data-counter-name',attempt.name,'code'); visibleValue(row[2],'data-counter-status',attempt.outcome.status); if (attempt.outcome.code) visibleValue(row[2],'data-counter-code',attempt.outcome.code,'code'); else assert(!row[2].includes('data-counter-code'));}
    }
  }
}

// Constructed corruptions of retained witnesses. Applying these never runs a
// compiler or ledger and adds no execution coverage.
export const counterCorruptionControls = [
  ['wrong initial state', rows => {rows[0].protocol_validation.builds[0].lifecycle.initial_state = 4;}],
  ['wrong selected state', rows => {rows[0].protocol_validation.builds[0].lifecycle.selected_child_state = 7;}],
  ['wrong continuation state', rows => {rows[1].protocol_validation.builds[0].lifecycle.continued_child_state = 2;}],
  ['missing language mode', rows => {rows[0].protocol_validation.builds.pop();}],
  ['numeric optimization flag', rows => {rows[0].protocol_validation.builds[0].optimize_lir = 0;}],
  ['changed module hash', rows => {rows[0].protocol_validation.builds[0].module_tree_hash = '00'.repeat(32);}],
  ['changed counterpart language', rows => {rows[0].protocol_validation.builds[0].counterpart.backend = 'rust_mir';}],
  ['changed child parent', rows => {rows[0].protocol_validation.builds[0].lifecycle.selected_child.parent_coin_id = '00'.repeat(32);}],
  ['changed child commitment', rows => {rows[1].protocol_validation.builds[0].lifecycle.selected_child.puzzle_hash = '00'.repeat(32);}],
  ['changed lineage input', rows => {rows[0].protocol_validation.builds[0].lifecycle.attempts.at(-1).transaction.spends[0].solution_hex = '80';}],
  ['changed VM output', rows => {rows[1].protocol_validation.builds[0].lifecycle.attempts.find(a => a.name === 'selected_action').primary_vm[0].outcome.output_hex = '80';}],
  ['resource failure credited', rows => {rows[0].protocol_validation.builds[0].lifecycle.attempts[1].primary_vm[0].failure.kind = 'resource';}],
  ['changed typed code', rows => {rows[1].protocol_validation.builds[0].lifecycle.attempts[2].outcome.code = 'DoubleSpend';}],
  ['missing full state equality', rows => {rows[0].protocol_validation.builds[0].lifecycle.attempts[1].outcome.complete_state_unchanged = false;}],
  ['numeric state equality', rows => {rows[1].protocol_validation.builds[0].lifecycle.attempts[2].outcome.complete_state_unchanged = 1;}],
  ['empty wrong-key signature', rows => {rows[1].protocol_validation.builds[0].lifecycle.attempts[3].transaction.signature_hex = counterEmptySignature;}],
  ['altered replay', rows => {rows[0].protocol_validation.builds[0].lifecycle.attempts.at(-2).transaction.signature_hex = 'ab'.repeat(96);}],
  ['changed SDK VM', rows => {rows[0].protocol_validation.sdk_vm = 'clvmr 0.21.0';}],
  ['changed protocol counts', rows => {rows[1].protocol_validation.sdk_transaction_attempts = 36;}],
];

const replaceVisible = (markup,attribute,replacement) => markup.replace(new RegExp(`(<[a-z]+\\b[^>]*${attribute}(?:="[^"]*")?[^>]*>)[^<]+`),(_,prefix) => prefix + replacement);
export const counterDisplayCorruptionControls = [
  ['changed visible total', markup => replaceVisible(markup,'data-counter-attempts','999')],
  ['changed visible initial state', markup => replaceVisible(markup,'data-counter-state-initial','4')],
  ['changed visible selected state', markup => replaceVisible(markup,'data-counter-state-selected','8')],
  ['changed visible continuation state', markup => replaceVisible(markup,'data-counter-state-continued','9')],
  ['changed visible module', markup => replaceVisible(markup,'data-counter-module','00'.repeat(32))],
  ['changed visible companion', markup => replaceVisible(markup,'data-counter-counterpart','00'.repeat(32))],
  ['changed visible child coin', markup => replaceVisible(markup,'data-counter-child','00'.repeat(32))],
  ['changed visible Merkle root', markup => replaceVisible(markup,'data-counter-root','00'.repeat(32))],
  ['changed visible outcome', markup => replaceVisible(markup,'data-counter-status','changed')],
  ['changed visible witness', markup => replaceVisible(markup,'data-counter-name','removed_witness')],
  ['changed visible code', markup => replaceVisible(markup,'data-counter-code','CostExceeded')],
  ['changed visible backend', markup => markup.replace('Rust / off','Other / off')],
  ['changed visible SDK version', markup => markup.replace('clvmr 0.16.2','clvmr 0.21.0')],
  ['changed pending scope', markup => markup.replace('registry and slots remain pending','registry and slots are complete')],
];
