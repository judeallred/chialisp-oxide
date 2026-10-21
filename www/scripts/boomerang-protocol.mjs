// Independent closed presentation contract. Python checks current source,
// complete CLVM trees/curry relations and retained replay. This reader executes
// no VM, SDK or BLS and does not authenticate the producer's rollback receipt.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { checkExecution, checkNativeOutcome } from './vm-failures.mjs';

export const boomerangProtocol = 'chialisp-oxide-boomerang-instances/v1';
export const boomerangModes = [['rust_mir', false], ['rust_mir', true], ['chialisp', true], ['rue_source', false], ['rue_source', true]];
export const ownerKey = '8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5';
export const otherKey = 'b47fa1e15162a5b1f9444d1aae57d131a1d5c98bade8f8e93b8ebfe1649b95c55787b8731d2f1a0638b3e1d23ada48e8';
export const claimIntent = Buffer.from('chialisp-oxide/boomerang/claim:1').toString('hex');
export const additionalData = '37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615';
export const identitySignature = `c0${'00'.repeat(95)}`;
export const recipient = '42'.repeat(32), refund = '43'.repeat(32);
const parent = 'ae2915f8495b82ab18f7f06a86c7498e34d193781a0906f753a08d4b9afd3fc8';
const otherParent = '1c9bdb76cf327fe9af5a533ffc71baa6c93ca0ebc7fe37eb86f4ae90502771a7';
const closed = (value, names, label) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), label);
  assert.deepEqual(Object.keys(value).sort(), [...names].sort(), `${label}: missing/unknown fields`);
};
const hex = (value, bytes, label) => assert(typeof value === 'string' && /^(?:[0-9a-f]{2})+$/.test(value)
  && value.length <= 4_000_000 && (bytes === undefined || value.length === bytes * 2), label);
const modes = rows => rows.map(row => { assert.equal(typeof row.optimize_lir, 'boolean'); return [row.backend, row.optimize_lir]; });
function checkProgramFailure(execution, backend, optimize, label) {
  const residualPair = backend === 'rust_mir' && !optimize && [
    'extra outer argument','extra terms field','extra path argument','malformed_extra_argument',
  ].includes(label);
  assert.deepEqual(execution.failure,{kind:'program',code:residualPair?'invalid_op_arg':'raise'},
    'Retain the exact known ABI program failure; resources are inconclusive');
}
const integerBytes = value => {
  assert(Number.isSafeInteger(value) && value >= 0);
  if (value === 0) return '';
  let bytes = value.toString(16); if (bytes.length % 2) bytes = `0${bytes}`;
  return Number.parseInt(bytes.slice(0, 2), 16) >= 128 ? `00${bytes}` : bytes;
};
const atom = bytes => bytes === '' ? '80' : bytes.length === 2 && Number.parseInt(bytes, 16) < 128
  ? bytes : `${(128 + bytes.length / 2).toString(16)}${bytes}`;
const number = value => atom(integerBytes(value));
const list = values => values.reduceRight((rest, first) => `ff${first}${rest}`, '80');
export const termsHex = (values = [ownerKey, recipient, refund, 1000, 100]) => list(values.map((value, index) => index < 3 ? atom(value) : number(value)));
export const conditionHex = (claim, values = [ownerKey, recipient, refund, 1000, 100], intent = claimIntent) => list([
  list([number(claim ? 85 : 81), number(values[4])]), list([number(73), number(values[3])]),
  list([number(51), atom(values[claim ? 1 : 2]), number(values[3])]),
  ...(claim ? [list([number(50), atom(values[0]), atom(intent)])] : []),
]);
export const coinRecord = (puzzleHash, amount = 1000, parentID = parent) => ({
  coin_id: createHash('sha256').update(Buffer.from(`${parentID}${puzzleHash}${integerBytes(amount)}`, 'hex')).digest('hex'),
  parent_coin_id: parentID, puzzle_hash: puzzleHash, amount,
});

// These are explicit finite cases, not a universal consensus specification.
export const boomerangScenarios = [
  ['claim_before_deadline',99,true,'recipient',null],
  ['refund_at_deadline',100,false,'empty',null],
  ['refund_after_deadline',101,false,'empty',null],
  ['claim_at_deadline',100,true,'recipient','AssertBeforeSecondsAbsoluteFailed'],
  ['claim_after_deadline',101,true,'recipient','AssertBeforeSecondsAbsoluteFailed'],
  ['refund_before_deadline',99,false,'empty','AssertSecondsAbsoluteFailed'],
  ['claim_missing_signature',99,true,'empty','BadAggregateSignature'],
  ['claim_wrong_key',99,true,'wrong_key','BadAggregateSignature'],
  ['claim_wrong_intent',99,true,'wrong_intent','BadAggregateSignature'],
  ['claim_other_coin_signature',99,true,'other_coin','BadAggregateSignature'],
  ['refund_amount_too_small',100,false,'empty','AssertMyAmountFailed'],
  ['refund_amount_too_large',100,false,'empty','AssertMyAmountFailed'],
  ...['recipient_key','recipient','refund','amount','deadline'].map(field => [`substituted_${field}`,101,false,'empty','WrongPuzzleHash']),
  ['refund_replay_setup',100,false,'empty',null], ['refund_replay',101,false,'empty','DoubleSpend'],
  ['claim_then_refund_setup',99,true,'recipient',null], ['claim_then_refund',100,false,'empty','DoubleSpend'],
  ...['empty_solution','atom_solution','bool_two','zero_byte','padded_true','pair_bool','extra_argument','improper_tail']
    .map(label => [`malformed_${label}`,100,null,'empty','GeneratorRuntimeError']),
].map(([label,timestamp,claim,signing,code]) => ({label,timestamp,claim,signing,code}));
export const malformedSolutions = [
  ['()','80'], ['1','01'], ['(2)','ff0280'], ['(0x00)','ff0080'],
  ['(0x0001)','ff82000180'], ['((1))','ffff018080'], ['(1 0)','ff01ff8080'], ['(1 . 2)','ff0102'],
];

function checkCoin(value) {
  closed(value, ['coin_id','parent_coin_id','puzzle_hash','amount'], 'coin');
  for (const field of ['coin_id','parent_coin_id','puzzle_hash']) hex(value[field], 32, field);
  assert.deepEqual(value, coinRecord(value.puzzle_hash, value.amount, value.parent_coin_id), 'Coin identity changed');
}
function expectedState(coin, timestamp, destination, spent = false) {
  return {height:spent ? 1 : 0,next_timestamp:timestamp,created_height:0,spent_height:spent ? 0 : null,
    children:spent ? [coinRecord(destination,1000,coin.coin_id)] : []};
}
function checkSignature(row, original) {
  hex(row.signature_hex,96,'signature');
  assert.notEqual(row.signature_hex,'00'.repeat(96),'All-zero bytes are not compressed BLS identity');
  if(row.signature_kind === 'empty') {
    assert.equal(row.signature_hex,identitySignature); assert.equal(row.signature_context,null); assert.equal(row.signature_origin,null); return;
  }
  assert.notEqual(row.signature_hex,identitySignature,'A wrong-key control must actually sign');
  const context=row.signature_context;
  closed(context,['public_key','intent_hex','coin_id','additional_data','message_hex'],'signature context');
  const key=row.signature_kind==='wrong_key'?otherKey:ownerKey;
  const intent=row.signature_kind==='wrong_intent'?'cd'.repeat(32):claimIntent;
  let coin=row.coin;
  if(row.signature_kind==='other_coin')coin=coinRecord(original.artifact.tree_hash,1000,otherParent);
  assert.deepEqual(context,{public_key:key,intent_hex:intent,coin_id:coin.coin_id,additional_data:additionalData,message_hex:intent+coin.coin_id+additionalData});
  if(['wrong_intent','other_coin'].includes(row.signature_kind)) {
    const origin=row.signature_origin;
    closed(origin,['coin','reveal_hex','reveal_tree_hash','solution','solution_hex'],'signer-only origin'); checkCoin(origin.coin);
    assert.deepEqual(origin.coin,coin); assert.equal(origin.solution,'(1)'); assert.equal(origin.solution_hex,'ff0180');
    if(row.signature_kind==='other_coin') {
      assert.equal(origin.reveal_hex,original.artifact.clvm_hex); assert.equal(origin.reveal_tree_hash,original.artifact.tree_hash);
    } else {
      assert.equal(origin.reveal_hex,`ff01${conditionHex(true,undefined,'cd'.repeat(32))}`);
      hex(origin.reveal_tree_hash,32,'signer-only reveal hash'); assert.notEqual(origin.reveal_tree_hash,row.reveal_tree_hash);
    }
  } else assert.equal(row.signature_origin,null);
}

function checkAttempt(row, spec, index, original) {
  closed(row,['label','timestamp','argument_hex','coin','reveal_hex','reveal_tree_hash','solution','solution_hex',
    'signature_kind','signature_hex','signature_context','signature_origin','vm_021','consensus','before','after','full_state_unchanged'],'attempt');
  assert.equal(row.label,spec.label); assert.equal(row.timestamp,spec.timestamp); assert.equal(row.signature_kind,spec.signing);
  checkCoin(row.coin); assert.equal(row.coin.parent_coin_id,parent); assert.equal(row.coin.puzzle_hash,original.artifact.tree_hash);
  assert.equal(row.coin.amount,index===10?999:index===11?1001:1000);
  const values=[ownerKey,recipient,refund,1000,100];
  if(index>=12&&index<=16)values[index-12]=[otherKey,'44'.repeat(32),'45'.repeat(32),1001,101][index-12];
  assert.equal(row.argument_hex,termsHex(values)); hex(row.reveal_hex,undefined,'reveal'); hex(row.reveal_tree_hash,32,'reveal hash');
  if(index>=12&&index<=16)assert.notEqual(row.reveal_tree_hash,original.artifact.tree_hash,'Substituted terms must change commitment');
  else {assert.equal(row.reveal_hex,original.artifact.clvm_hex);assert.equal(row.reveal_tree_hash,original.artifact.tree_hash);}
  assert.deepEqual([row.solution,row.solution_hex],spec.claim===null?malformedSolutions[index-21]:spec.claim?['(1)','ff0180']:['(())','ff8080']);
  const outcome=checkExecution(row.vm_021);
  if(spec.claim===null) {assert.deepEqual(outcome,{status:'rejected'});checkProgramFailure(row.vm_021,original.backend,original.optimize_lir,spec.label);}
  else assert.deepEqual(outcome,{status:'accepted',output_hex:conditionHex(spec.claim,values)});
  checkSignature(row,original);
  const umbrella=spec.code==='GeneratorRuntimeError';
  assert.deepEqual(row.consensus,{status:spec.code===null?'accepted':umbrella?'runtime_umbrella':'consensus_rejected',
    code:spec.code,error:spec.code===null?null:`Validation(${spec.code})`,semantic_rejection_credit:spec.code!==null&&!umbrella});
  for(const state of [row.before,row.after]) {
    closed(state,['height','next_timestamp','created_height','spent_height','children'],'state');
    assert(Array.isArray(state.children)); for(const child of state.children)checkCoin(child);
  }
  const replay=index===18||index===20;
  const previousDestination=index===18?refund:recipient;
  assert.deepEqual(row.before,expectedState(row.coin,row.timestamp,previousDestination,replay));
  assert.deepEqual(row.after,spec.code===null?expectedState(row.coin,row.timestamp+1,spec.claim?recipient:refund,true):row.before);
  assert.equal(row.full_state_unchanged,spec.code!==null,'Only rejected actual submissions receive rollback credit');
}

export function checkBoomerangComponent(puzzle) {
  assert.equal(puzzle.slug,'boomerang');assert.equal(puzzle.kind,'coin_puzzle');assert.equal(puzzle.validated,true);
  assert.deepEqual(puzzle.simulator,[]);assert.deepEqual(puzzle.simulator_comparisons,[]);
  assert.equal(puzzle.protocol_validation,undefined);assert.equal(puzzle.curried_instances,undefined);assert.equal(puzzle.curried_examples,undefined);
  assert.deepEqual(modes(puzzle.artifacts),boomerangModes);
  assert.equal(puzzle.cases.length,101);
  for(const test of puzzle.cases) {
    checkNativeOutcome(test.native);assert.deepEqual(modes(test.runs),boomerangModes);
    for(const run of test.runs) {
      assert.deepEqual(checkExecution(run.execution),test.native);
      if(test.native.status==='rejected')checkProgramFailure(run.execution,run.backend,run.optimize_lir,test.label);
    }
  }
  const evidence=puzzle.coin_instances;
  closed(evidence,['protocol','role','scheme','scope','examples','ledger'],'coin instances');
  assert.equal(evidence.protocol,boomerangProtocol);assert.equal(evidence.role,'original_teaching_coin_instances');
  assert.equal(evidence.scheme,'(a (q . M) (c (q . A) 1))');assert.equal(typeof evidence.scope,'string');
  assert(evidence.scope.includes('no automatic refund service')&&evidence.scope.includes('compiler preservation'));
  assert.equal(evidence.examples.length,1);
  const example=evidence.examples[0];
  closed(example,['label','argument','argument_hex','argument_tree_hash','scheme','constructor','instances','cases'],'demonstration example');
  assert.equal(example.label,'public_demo_payment');assert.equal(example.argument_hex,termsHex());hex(example.argument_tree_hash,32,'argument hash');
  assert.equal(example.scheme,evidence.scheme);assert.equal(example.constructor,'independent clvmr standard-form construction cross-checked with chialisp_oxide::curry');
  assert.deepEqual(modes(example.instances),boomerangModes);assert.deepEqual(modes(evidence.ledger),boomerangModes);
  assert.equal(example.cases.length,14);
  for(const test of example.cases) {
    checkNativeOutcome(test.native);assert.deepEqual(modes(test.runs),boomerangModes);
    for(const run of test.runs) {
      assert.deepEqual(checkExecution(run.execution),test.native);
      if(test.native.status==='rejected')checkProgramFailure(run.execution,run.backend,run.optimize_lir,test.label);
    }
  }
  let accepted=0,rejected=0,umbrellas=0;
  for(const [index,build] of evidence.ledger.entries()) {
    const original=example.instances[index],module=puzzle.artifacts[index];
    closed(original,['backend','optimize_lir','module_tree_hash','artifact','sdk_owned_curry_bytes_equal','sdk_hash_equal'],'instance artifact');
    assert.equal(original.module_tree_hash,module.tree_hash);assert.equal(original.sdk_owned_curry_bytes_equal,true);assert.equal(original.sdk_hash_equal,true);
    assert.deepEqual([original.artifact.backend,original.artifact.optimize_lir],[module.backend,module.optimize_lir]);
    hex(original.artifact.tree_hash,32,'instance hash');hex(original.artifact.clvm_hex,undefined,'instance bytes');
    assert.equal(original.artifact.bytes,original.artifact.clvm_hex.length/2);assert.notEqual(original.artifact.tree_hash,module.tree_hash);
    closed(build,['backend','optimize_lir','instance_label','module_tree_hash','instance_tree_hash','simulator','consensus','vm','network','transaction_attempts','attempts'],'ledger build');
    assert.equal(build.instance_label,example.label);assert.equal(build.module_tree_hash,module.tree_hash);assert.equal(build.instance_tree_hash,original.artifact.tree_hash);
    for(const [key,value]of Object.entries({simulator:'chia-sdk-test 0.36.0',consensus:'chia-consensus 0.36.1',vm:'clvmr 0.16.2',network:'TESTNET11'}))assert.equal(build[key],value);
    assert.equal(build.transaction_attempts,29);assert.equal(build.attempts.length,29);
    build.attempts.forEach((row,i)=>{checkAttempt(row,boomerangScenarios[i],i,original);if(row.consensus.status==='accepted')accepted++;else if(row.consensus.semantic_rejection_credit)rejected++;else umbrellas++;});
    const matching=build.attempts[0].signature_hex;
    for(const i of [3,4,19])assert.equal(build.attempts[i].signature_hex,matching);
    for(const i of [7,8,9])assert.notEqual(build.attempts[i].signature_hex,matching);
    for(const [setup,replay]of [[17,18],[19,20]]) {
      assert.deepEqual(build.attempts[setup].coin,build.attempts[replay].coin);
      assert.deepEqual(build.attempts[setup].after,build.attempts[replay].before);
    }
  }
  assert.deepEqual([accepted,rejected,umbrellas],[25,80,40]);
  return{builds:evidence.ledger.length,attempts:accepted+rejected+umbrellas,accepted,rejected,umbrellas};
}
export function checkBoomerangReport(report) {
  const selected=report.puzzles.filter(row=>row.slug==='boomerang');assert.equal(selected.length,1,'Exactly one current boomerang puzzle required');
  return checkBoomerangComponent(selected[0]);
}

const plain=value=>value.replace(/<[^>]*>/g,' ').replace(/&(?:gt|lt|amp|quot);/g,entity=>({'&gt;':'>','&lt;':'<','&amp;':'&','&quot;':'"'})[entity]).replace(/\s+/g,' ').trim();
export function checkBoomerangDisplay(markup,puzzle) {
  const counts=checkBoomerangComponent(puzzle);
  const sections=[...markup.matchAll(/<section\b[^>]*data-coin-instances[^>]*>[\s\S]*?<\/section>/g)];assert.equal(sections.length,1);
  const section=sections[0][0],text=plain(section);
  for(const [key,value]of Object.entries(counts))assert.equal(section.match(new RegExp(`data-coin-${key}="([0-9]+)"`))?.[1],String(value));
  const summaries=[...section.matchAll(/<p\b[^>]*data-coin-summary(?=\s|=|>)[^>]*>[\s\S]*?<\/p>/g)];assert.equal(summaries.length,1);
  const summary=plain(summaries[0][0]);
  assert(summary.includes(`${counts.attempts} actual submissions`)&&summary.includes(`${counts.accepted} accepted`)
    &&summary.includes(`${counts.rejected} typed consensus rejections`)&&summary.includes(`${counts.umbrellas} runtime umbrellas`),'Visible summary counts differ from actual finite receipts');
  for(const key of ['attempts','accepted','rejected','umbrellas']) {
    const metrics=[...section.matchAll(new RegExp(`<div\\b[^>]*data-coin-count="${key}"[^>]*>[\\s\\S]*?</div>`,'g'))];assert.equal(metrics.length,1);
    const strong=metrics[0][0].match(/<strong\b[^>]*>([\s\S]*?)<\/strong>/);assert(strong);
    assert.equal(plain(strong[1]),counts[key].toLocaleString('en-US'),`Visible ${key} metric differs`);
  }
  assert.deepEqual([...section.matchAll(/data-coin-mode="([^"]+)"/g)].map(m=>m[1]),boomerangModes.map(([b,o])=>`${b}:${o}`));
  for(const build of puzzle.coin_instances.ledger) {
    const block=section.match(new RegExp(`<details\\b[^>]*data-coin-mode="${build.backend}:${build.optimize_lir}"[^>]*>[\\s\\S]*?</details>`))?.[0];assert(block);
    const identities=block.match(/<p\b[^>]*data-coin-identities[^>]*>[\s\S]*?<\/p>/)?.[0];assert(identities);
    assert(plain(identities).includes(`Own template hash: ${build.module_tree_hash}`)
      &&plain(identities).includes(`Own committed instance hash: ${build.instance_tree_hash}`),'Visible template/instance identities changed');
    assert.deepEqual([...block.matchAll(/data-coin-attempt="([^"]+)"/g)].map(m=>m[1]),boomerangScenarios.map(row=>row.label));
    for(const attempt of build.attempts) {
      const row=block.match(new RegExp(`<tr\\b[^>]*data-coin-attempt="${attempt.label}"[^>]*>[\\s\\S]*?</tr>`))?.[0];assert(row);
      const input=plain(row.match(/<th\b[^>]*>[\s\S]*?<\/th>/)?.[0]??'');
      for(const value of [attempt.label,`t=${attempt.timestamp}`,`amount=${attempt.coin.amount}`,attempt.solution])assert(input.includes(value),`${attempt.label}: missing visible input ${value}`);
      const cell=attribute=>{const content=row.match(new RegExp(`<td\\b[^>]*${attribute}[^>]*>[\\s\\S]*?</td>`))?.[0];assert(content,`Missing ${attribute} cell`);return plain(content);};
      assert(cell('data-coin-reveal').includes(attempt.reveal_tree_hash));
      const signature=cell('data-coin-signature'),outcome=cell('data-coin-outcome'),state=cell('data-coin-state');
      assert(signature.includes(`Signature ${attempt.signature_kind}`));
      if(attempt.signature_context)for(const field of ['public_key','coin_id','intent_hex'])assert(signature.includes(attempt.signature_context[field].slice(0,16)),`Missing visible signer ${field}`);
      assert(outcome.includes(`VM ${attempt.vm_021.outcome.status}${attempt.vm_021.failure?` / ${attempt.vm_021.failure.code}`:''}`));
      assert(outcome.includes(`Consensus ${attempt.consensus.code??'accepted'}`));
      assert(outcome.includes(attempt.consensus.semantic_rejection_credit?'semantic rejection':'no semantic rejection credit'));
      assert(state.includes(`input before=${attempt.before.spent_height??'unspent'} after=${attempt.after.spent_height??'unspent'}`));
      if(attempt.full_state_unchanged)assert(state.includes('full SDK equality checked in memory'));
      else {const child=attempt.after.children[0];for(const value of [child.coin_id,child.parent_coin_id,child.puzzle_hash,`child amount=${child.amount}`])assert(state.includes(value),'Missing exact accepted child');}
    }
    const filename=`boomerang--${build.backend}--${build.optimize_lir?'on':'off'}.clvm.hex`;
    assert(block.includes(`href="/chialisp-oxide/reports/boomerang/${filename}"`));
  }
  for(const phrase of ['template hash','committed instance hash','not ordinary ledger scenario groups','permissionless refund','no automatic refund service',
    'runtime umbrellas never earn semantic rejection credit','clvmr 0.21.0','clvmr 0.16.2','not a BLS proof','not compiler preservation',
    'Optional output is outside the current proof models','not a serialized state proof','public fixture keys','not ready for real funds'])assert(text.includes(phrase),`Missing scope boundary: ${phrase}`);
  assert(section.includes('href="/chialisp-oxide/reports/boomerang.json"'));
  return counts;
}
