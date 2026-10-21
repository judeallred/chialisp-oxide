// Recorded packet admission is independent Python code. This module projects
// finite observations for display; it runs no compiler, native puzzle, VM or SDK.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repository } from './p2-costs.mjs';
const sourceRoot=await repository();
// Keep Node-only checker module identity after Astro SSR bundling.
const {checkExecution,checkNativeOutcome}=await import(/* @vite-ignore */ pathToFileURL(path.join(sourceRoot,'www/scripts/vm-failures.mjs')).href);
const {conditionHex,ownerKey,recipient,refund,identitySignature,coinRecord}=await import(/* @vite-ignore */ pathToFileURL(path.join(sourceRoot,'www/scripts/boomerang-protocol.mjs')).href);

export const controlsProtocol = 'chialisp-oxide-boomerang-source-controls/v1';
export const controlNames = ['original','equivalent','drop_claim_signature','late_claim','early_refund'];
export const controlSpecs = [
  ...['original','equivalent'].flatMap(name => [
    {name,witness:'timely_unsigned_claim',claim:true,timestamp:99,signing:'none',code:'BadAggregateSignature'},
    {name,witness:'signed_late_claim',claim:true,timestamp:101,signing:'recipient',code:'AssertBeforeSecondsAbsoluteFailed'},
    {name,witness:'early_unsigned_refund',claim:false,timestamp:99,signing:'none',code:'AssertSecondsAbsoluteFailed'},
  ]),
  {name:'drop_claim_signature',witness:'timely_unsigned_claim',claim:true,timestamp:99,signing:'none',code:null},
  {name:'late_claim',witness:'signed_late_claim',claim:true,timestamp:101,signing:'recipient',code:null},
  {name:'early_refund',witness:'early_unsigned_refund',claim:false,timestamp:99,signing:'none',code:null},
];
const specs = ['off','on'].flatMap(mode => controlSpecs.map(spec => ({mode,...spec})));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
export function controlOutput(name,claim) {
  const values=[ownerKey,recipient,refund,1000,100];
  if(name==='drop_claim_signature'&&claim)return conditionHex(false,[ownerKey,recipient,recipient,1000,100]).replace(/^ffff51/,'ffff55');
  if(name==='late_claim'&&claim)return conditionHex(true,[...values.slice(0,4),0]).replace(/^ffff55/,'ffff51');
  if(name==='early_refund'&&!claim)return conditionHex(false,[...values.slice(0,4),0]);
  return conditionHex(claim);
}

export function projectBoomerangControls(report,nativeRows) {
  assert.equal(report.protocol,controlsProtocol);assert.equal(report.schema_version,1);assert.equal(report.status,'actual_source_controls_captured');
  assert.equal(report.native.exit_code,0);assert.equal(report.native.native_test_groups,1);assert.equal(report.native.native_policy_observations,9);
  assert.deepEqual(report.prepared.recipe.controls.map(row=>row.name),controlNames);
  assert.deepEqual(Object.keys(report.instances),['off','on']);
  for(const mode of ['off','on'])assert.deepEqual(Object.keys(report.instances[mode]).sort(),[...controlNames].sort());
  assert.equal(nativeRows.length,9);assert.equal(report.runs.length,18);
  assert.equal(report.ledger.result.protocol,'chialisp-oxide-boomerang-source-controls-ledger/v1');
  assert.equal(report.ledger.result.transaction_attempts,18);assert.equal(report.ledger.result.attempts.length,18);
  const rows=[];
  for(const [index,spec]of specs.entries()) {
    const native=nativeRows[index%9],run=report.runs[index],ledger=report.ledger.result.attempts[index];
    const identity=({mode,name,witness})=>({mode,name,witness});
    assert.deepEqual({name:native.name,witness:native.witness},{name:spec.name,witness:spec.witness});
    assert.deepEqual(identity(run),identity(spec));assert.deepEqual(identity(ledger),identity(spec));
    const expected={status:'accepted',output_hex:controlOutput(spec.name,spec.claim)};
    assert.deepEqual(checkNativeOutcome(native.actual),expected);assert.deepEqual(checkExecution(run.execution),expected);
    assert.equal(run.solution,spec.claim?'(1)':'(())');
    const instance=report.instances[spec.mode][spec.name];
    assert.equal(run.instance_tree_hash,instance.tree_hash);assert.equal(ledger.instance_tree_hash,instance.tree_hash);
    assert.equal(ledger.module_tree_hash,instance.module_tree_hash);assert.equal(ledger.instance_hex,instance.clvm_hex);
    assert.equal(ledger.claim,spec.claim);assert.equal(ledger.timestamp,spec.timestamp);assert.equal(ledger.signing,spec.signing);
    assert.equal(ledger.solution_hex,spec.claim?'ff0180':'ff8080');
    assert.deepEqual(ledger.input,coinRecord(instance.tree_hash));
    assert.match(ledger.signature_hex,/^[0-9a-f]{192}$/);
    assert.equal(ledger.signature_hex===identitySignature,spec.signing==='none');
    if(spec.code)assert.deepEqual(ledger.outcome,{status:'consensus_rejected',code:spec.code,semantic_rejection_credit:true,full_state_unchanged:true});
    else {
      const destination=spec.claim?recipient:refund;
      assert.deepEqual(ledger.outcome,{status:'accepted',input_spent:true,child_count:1,child:coinRecord(destination,1000,ledger.input.coin_id)});
    }
    rows.push({...spec,native:native.actual,execution:run.execution,ledger,instance});
  }
  return{protocol:report.protocol,scope:report.scope,controls:report.prepared.recipe.controls,rows,
    counts:{nativeGroups:report.native.native_test_groups,native: nativeRows.length,exports:controlNames.length,
      exportModes:Object.values(report.instances).reduce((sum,mode)=>sum+Object.keys(mode).length,0),
      vm:report.runs.length,attempts:report.ledger.result.attempts.length,
      accepted:rows.filter(row=>row.ledger.outcome.status==='accepted').length,
      rejected:rows.filter(row=>row.ledger.outcome.status==='consensus_rejected').length}};
}

const packetRoot=path.join(sourceRoot,'corpus/boomerang-source-controls');
const packet=path.join(packetRoot,'report.json');
let cached;
export function readBoomerangControls() {
  cached ??= (async()=>{
    let bytes;
    try {bytes=await readFile(packet);}catch(error){if(error.code==='ENOENT')return undefined;throw error;}
    // This is a recorded reader, never the source-control producer or replay.
    const admitted=JSON.parse(execFileSync('python3',['scripts/check-boomerang.py','--source-controls',packet,'--source-controls-only'],
      {cwd:sourceRoot,encoding:'utf8',maxBuffer:1024*1024}));
    assert.equal(admitted.status,'recorded_actual_source_controls_checked');
    const report=JSON.parse(bytes),log=await readFile(path.join(packetRoot,'native.log'),'utf8');
    const nativeRows=[...log.matchAll(/BOOMERANG_MUTATION_NATIVE (\{[^\n]+\})/g)].map(match=>JSON.parse(match[1]));
    const projected=projectBoomerangControls(report,nativeRows);
    assert.deepEqual(await readFile(packet),bytes,'Source-control report changed during presentation admission');
    return {...projected,report,reportSHA:sha(bytes)};
  })();
  return cached;
}
export const controlPacketRoot=packetRoot;

const plain=value=>value.replace(/<[^>]*>/g,' ').replace(/\s+/g,' ').trim();
export function checkBoomerangControlsDisplay(markup,evidence) {
  const sections=[...markup.matchAll(/<section\b[^>]*data-boomerang-source-controls(?:\s|=|>)[^>]*>[\s\S]*?<\/section>/g)];assert.equal(sections.length,1);
  const section=sections[0][0],text=plain(section);
  for(const [key,value]of Object.entries(evidence.counts))assert.equal(section.match(new RegExp(`data-boomerang-controls-${key.replace(/[A-Z]/g,letter=>'-'+letter.toLowerCase())}="([0-9]+)"`))?.[1],String(value));
  for(const [key,label]of [['nativeGroups','native test group'],['native','native output observations'],['exports','source exports'],['exportModes','export/mode artifacts'],['vm','primary VM observations'],['attempts','separate SDK submissions'],['accepted','accepted forbidden spends'],['rejected','typed consensus rejections']])assert(text.includes(`${evidence.counts[key]} ${label}`),`Visible mutant count ${key} changed`);
  const rows=[...section.matchAll(/<tr\b[^>]*data-boomerang-control="([^"]+)"[^>]*>[\s\S]*?<\/tr>/g)];
  assert.deepEqual(rows.map(row=>row[1]),evidence.rows.map(row=>`${row.mode}:${row.name}:${row.witness}`));
  for(const [index,row]of rows.entries()) {
    const observed=evidence.rows[index],visible=plain(row[0]);
    for(const value of [observed.name,observed.witness,`t=${observed.timestamp}`,`signing=${observed.signing}`,
      `Native accepted / ${observed.native.output_hex}`,`VM accepted / ${observed.execution.outcome.output_hex}`,
      observed.instance.tree_hash,`Consensus ${observed.code??'accepted'}`])assert(visible.includes(value),`Missing exact mutant witness ${value}`);
    if(observed.code)assert(visible.includes('full SDK equality checked in memory'));
    else for(const value of [observed.ledger.outcome.child.coin_id,observed.ledger.outcome.child.parent_coin_id,
      observed.ledger.outcome.child.puzzle_hash,`child amount=${observed.ledger.outcome.child.amount}`])assert(visible.includes(value),'Missing exact mutant accepted child');
  }
  for(const phrase of ['each mutant’s own committed instance','constructing a condition does not enforce it','ordinary Rust, Clippy and the subset checks',
    'not errors Rust makes impossible','no Rue or Chialisp mutant runs','not compiler preservation','not a BLS proof','public fixture key',
    'separate from the original policy’s','not ready for real funds'])assert(text.includes(phrase),`Missing mutant scope: ${phrase}`);
  assert(section.includes('href="/chialisp-oxide/reports/boomerang-source-controls/report.json"'));
  assert(text.includes(evidence.reportSHA));
  return evidence.counts;
}
