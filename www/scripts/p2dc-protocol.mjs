// Presentation consistency only. The Python outer/component readers own current
// source, tree/curry/coin recipes and capture admission. No crypto or VM runs here.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { checkExecution, checkNativeOutcome } from './vm-failures.mjs';

export const delegatedSlug = 'p2-delegated-conditions';
export const delegatedProtocol = 'chia-consensus-0.36.1/p2-delegated-conditions-v1';
export const delegatedModes = [['rust_mir',false],['rust_mir',true],['chialisp',true],['rue_source',false],['rue_source',true]];
export const delegatedScenarios = [
  ['missing_signature','BadAggregateSignature'],['wrong_nonempty_key_signature','BadAggregateSignature'],
  ['changed_conditions_reuse_old_signature','BadAggregateSignature'],['other_coin_reuse_signature','BadAggregateSignature'],
  ['correct_signature_cannot_mint_value','MintingCoin'],['owner_key_curry_cannot_be_replaced','WrongPuzzleHash'],
  ['ignored_improper_tail_preserves_signature',null],['owner_signature_exact_payout',null],
  ['spent_coin_cannot_replay','DoubleSpend'],['unsafe_uncurried_caller_selects_attacker_key',null],
  ['unsafe_uncurried_alternative_owner_key',null],['different_language_reveal_cannot_spend_input','WrongPuzzleHash'],
];
export const identitySignature = `c0${'00'.repeat(95)}`;
const sourceBytes = readFileSync(new URL('../../puzzles/p2-delegated-conditions/independent-vectors.json',import.meta.url));
assert.equal(createHash('sha256').update(sourceBytes).digest('hex'),'3b55d630d95dfd3b4527a55b49dc6d0c3b7370fc79a4d17f12dcfd58f000887b');
const vectors = JSON.parse(sourceBytes);
export const delegatedRawSpecs = [...vectors.accepted.map(row=>({label:row.label,solution:row.solution,environment_hex:row.environment_hex,expected:{status:'accepted',output_hex:row.expected_output_hex}})),...vectors.rejected.map(row=>({label:row.label,solution:row.solution,environment_hex:row.environment_hex,expected:{status:'rejected'}}))];
assert.equal(delegatedRawSpecs.length,200);
export const delegatedNamedInputs = JSON.parse(readFileSync(new URL('../../puzzles/p2-delegated-conditions/puzzle.json',import.meta.url))).cases;
assert.equal(delegatedNamedInputs.length,20);
const closed=(value,names,label)=>{assert(value&&typeof value==='object'&&!Array.isArray(value),label);assert.deepEqual(Object.keys(value).sort(),[...names].sort(),`${label}: missing/unknown fields`);};
const hex=(value,bytes,label)=>assert(typeof value==='string'&&/^(?:[0-9a-f]{2})+$/.test(value)&&(bytes===undefined||value.length===bytes*2),label);
const plain=value=>value.replace(/<[^>]*>/g,' ').replace(/\s+/g,' ').trim();
const modes=rows=>rows.map(row=>{assert.equal(typeof row.optimize_lir,'boolean');return[row.backend,row.optimize_lir];});
function coin(value){closed(value,['id','parent_id','puzzle_hash','amount'],'coin');for(const key of ['id','parent_id','puzzle_hash'])hex(value[key],32,key);assert(Number.isSafeInteger(value.amount)&&value.amount>=0);}
function transaction(value){
  closed(value,['signature_hex','spends'],'transaction');hex(value.signature_hex,96,'signature bytes');
  assert.notEqual(value.signature_hex,'00'.repeat(96),'All-zero bytes are not the compressed BLS identity');
  assert.equal(value.spends.length,1);
  for(const spend of value.spends){closed(spend,['coin','reveal_hex','solution_hex'],'spend');coin(spend.coin);hex(spend.reveal_hex,undefined,'reveal');hex(spend.solution_hex,undefined,'solution');}
  return value.signature_hex;
}
function rawCase(row,spec){
  closed(row,['label','solution','environment_hex','expected','canonical_failure_code','native','runs'],'retained raw case');
  assert.deepEqual(Object.fromEntries(['label','solution','environment_hex','expected'].map(key=>[key,row[key]])),spec);
  assert.deepEqual(checkNativeOutcome(row.native),spec.expected);
  assert.equal(row.canonical_failure_code,spec.expected.status==='rejected'?'path_into_atom':null);
  assert.deepEqual(modes(row.runs),delegatedModes);
  for(const run of row.runs){closed(run,['backend','optimize_lir','execution'],'raw run');assert.deepEqual(checkExecution(run.execution),spec.expected);
    if(spec.expected.status==='rejected'){assert(['raise','path_into_atom'].includes(run.execution.failure.code));if(run.backend==='chialisp')assert.equal(run.execution.failure.code,'path_into_atom');}
    else assert(run.execution.cost<=1_000_000);
  }
}

export function checkDelegatedComponent(puzzle){
  assert.equal(puzzle.slug,delegatedSlug);assert.equal(puzzle.kind,'protocol_component');assert.equal(puzzle.environment,'raw');
  assert.deepEqual(puzzle.simulator,[]);assert.deepEqual(puzzle.simulator_comparisons,[]);assert.equal(puzzle.validated,true);
  assert.equal(puzzle.cases.length,20);assert.equal(puzzle.generated_cases,180+2059);
  assert.deepEqual(puzzle.cases.map(({label,solution})=>({label,solution})),delegatedNamedInputs);
  for (const row of puzzle.cases) {
    closed(row,['label','solution','native','runs'],'named raw case');
    const spec=delegatedRawSpecs.find(spec=>spec.label===row.label);assert(spec);
    rawCase({...row,environment_hex:spec.environment_hex,expected:spec.expected,
      canonical_failure_code:spec.expected.status==='rejected'?'path_into_atom':null},spec);
  }
  assert.deepEqual(modes(puzzle.artifacts),delegatedModes);
  const value=puzzle.protocol_validation;
  closed(value,['schema_version','protocol','status','simulator','consensus','vm','network','transaction_attempts','accepted','typed_rejected_with_full_rollback','builds','raw_validation','scope'],'delegated protocol');
  assert.equal(value.schema_version,1);assert.equal(value.protocol,delegatedProtocol);assert.equal(value.status,'validated');
  for(const [key,wanted]of Object.entries({simulator:'chia-sdk-test 0.36.0',consensus:'chia-consensus 0.36.1',vm:'clvmr 0.16.2',network:'TESTNET11'}))assert.equal(value[key],wanted);
  assert.deepEqual([value.transaction_attempts,value.accepted,value.typed_rejected_with_full_rollback],[60,20,40]);
  assert.equal(typeof value.scope,'string');assert(value.scope.includes('Unsafe uncurried key selection is intentionally admitted'));
  assert.deepEqual(modes(value.builds),delegatedModes);
  let accepted=0,rejected=0;
  for(const [index,build]of value.builds.entries()){
    closed(build,['backend','optimize_lir','status','module_tree_hash','module_hex','owner_public_key','attacker_public_key','owner_curried_reveal_hex','owner_instance_hash','transaction_attempts','accepted','typed_rejected_with_full_rollback','observations','scope'],'owner build');
    assert.equal(build.status,'validated');assert.equal(build.module_tree_hash,puzzle.artifacts[index].tree_hash);assert.equal(build.module_hex,puzzle.artifacts[index].clvm_hex);
    hex(build.owner_public_key,48,'owner key');hex(build.attacker_public_key,48,'attacker key');assert.notEqual(build.owner_public_key,build.attacker_public_key);
    hex(build.owner_curried_reveal_hex,undefined,'owner reveal');hex(build.owner_instance_hash,32,'owner hash');assert.notEqual(build.owner_instance_hash,build.module_tree_hash);
    assert.deepEqual([build.transaction_attempts,build.accepted,build.typed_rejected_with_full_rollback],[12,4,8]);
    assert.equal(build.observations.length,12);
    for(const [i,row]of build.observations.entries()){
      const [label,code]=delegatedScenarios[i];
      const fields=['label','status','transaction',...(code===null?['scope','spent_input_observed','child']:['code','full_simulator_equality'])];
      if(i===1)fields.push('signature_source','signature_source_scope');
      if(i===11)fields.push('alternate_build','signature_matches_selected_reveal','selected_reveal_signature_source');
      closed(row,fields,'submission');assert.equal(row.label,label);
      const signature=transaction(row.transaction);assert.equal(signature===identitySignature,i===0,'Omitted signature must use the real compressed identity');
      if(code===null){assert.equal(row.status,'accepted');assert.equal(row.spent_input_observed,true);coin(row.child);assert.equal(typeof row.scope,'string');accepted++;}
      else{assert.equal(row.status,'consensus_rejected');assert.equal(row.code,code);assert.equal(row.full_simulator_equality,true);rejected++;}
      if(i===1){assert.equal(transaction(row.signature_source),signature);assert(row.signature_source_scope.includes('not a separately admitted transaction'));}
      if(i===11){closed(row.alternate_build,['backend','optimize_lir','module_tree_hash','owner_curried_reveal_hex','owner_instance_hash'],'alternate reveal');assert.equal(row.signature_matches_selected_reveal,true);assert.deepEqual(row.selected_reveal_signature_source,build.observations[7].transaction);assert.notEqual(row.alternate_build.owner_instance_hash,build.owner_instance_hash);
        const alternate=build.backend==='chialisp'?['rue_source',true]:['chialisp',true];assert.deepEqual([row.alternate_build.backend,row.alternate_build.optimize_lir],alternate);
        const artifact=puzzle.artifacts.find(row=>row.backend===alternate[0]&&row.optimize_lir===alternate[1]);assert(artifact);assert.equal(row.alternate_build.module_tree_hash,artifact.tree_hash);hex(row.alternate_build.owner_curried_reveal_hex,undefined,'alternate reveal');hex(row.alternate_build.owner_instance_hash,32,'alternate instance');
      }
    }
    const rows=build.observations,complete=rows[7].transaction.signature_hex;
    for(const i of [2,3,6,8,11])assert.equal(rows[i].transaction.signature_hex,complete);
    assert.notEqual(rows[1].transaction.signature_hex,complete);assert.equal(rows[5].transaction.signature_hex,rows[1].transaction.signature_hex);
    assert.equal(rows[9].transaction.spends[0].coin.id,rows[10].transaction.spends[0].coin.id,'Unsafe alternatives must use the same raw coin in cloned states');
    assert.notEqual(rows[9].transaction.spends[0].coin.id,rows[7].transaction.spends[0].coin.id);
  }
  assert.deepEqual([accepted,rejected],[value.accepted,value.typed_rejected_with_full_rollback]);
  const raw=value.raw_validation;
  closed(raw,['max_cost','execution_vm','accepted','rejected','native_evaluations','VM_evaluations','cases','scope'],'retained raw suite');
  assert.equal(raw.execution_vm,'clvmr 0.21.0 MEMPOOL_MODE');assert.equal(typeof raw.scope,'string');
  assert.deepEqual([raw.max_cost,raw.accepted,raw.rejected,raw.native_evaluations,raw.VM_evaluations],[1_000_000,195,5,200,1000]);
  assert.equal(raw.cases.length,200);for(const [index,row]of raw.cases.entries())rawCase(row,delegatedRawSpecs[index]);
  return{groups:value.builds.length,attempts:accepted+rejected,accepted,rejected,native:raw.native_evaluations,vm:raw.VM_evaluations,named:puzzle.cases.length,generated:puzzle.generated_cases};
}
export function checkDelegatedReport(report){const selected=report.puzzles.filter(row=>row.slug===delegatedSlug);assert.equal(selected.length,1,'Exactly one current delegated-conditions component required');return checkDelegatedComponent(selected[0]);}

export function checkDelegatedDisplay(markup,puzzle){
 const counts=checkDelegatedComponent(puzzle),sections=[...markup.matchAll(/<section\b[^>]*data-delegated-validation[^>]*>[\s\S]*?<\/section>/g)];assert.equal(sections.length,1);const section=sections[0][0],text=plain(section);
 for(const [key,value]of Object.entries(counts)){const found=section.match(new RegExp(`data-delegated-${key}="([0-9]+)"`));assert.equal(found?.[1],String(value));}
 assert.deepEqual([...section.matchAll(/data-delegated-mode="([^"]+)"/g)].map(m=>m[1]),delegatedModes.map(([backend,optimize])=>`${backend}:${optimize}`));
 for(const build of puzzle.protocol_validation.builds){
  const block=section.match(new RegExp(`<details\\b[^>]*data-delegated-mode="${build.backend}:${build.optimize_lir}"[^>]*>[\\s\\S]*?</details>`))?.[0];assert(block);
  assert(plain(block).includes(build.module_tree_hash)&&plain(block).includes(build.owner_instance_hash));
  assert.deepEqual([...block.matchAll(/data-delegated-attempt="([^"]+)"/g)].map(m=>m[1]),delegatedScenarios.map(([name])=>name));
  for(const [label,code]of delegatedScenarios){const row=block.match(new RegExp(`<tr\\b[^>]*data-delegated-attempt="${label}"[^>]*>[\\s\\S]*?</tr>`))?.[0];assert(row);assert(plain(row).includes(code??'accepted'));if(code)assert(plain(row).includes('checked in memory'));}
 }
 for(const phrase of ['60 actual submissions','20 accepted','40 typed consensus rejections','200 additional native evaluations','1,000 additional VM evaluations','20 named','180 disjoint generated','2,059 generic raw','not ordinary ledger scenario groups','unsafe uncurried key selection','same raw coin in cloned states','compressed BLS identity','all-zero bytes','not a BLS proof','not execution authentication','clvmr 0.21.0','clvmr 0.16.2'])assert(text.includes(phrase),`Missing delegated boundary: ${phrase}`);
 assert(section.includes('href="/chialisp-oxide/reports/p2-delegated-conditions.json"'));
 return counts;
}
