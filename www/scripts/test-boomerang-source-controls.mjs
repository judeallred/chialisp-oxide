import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { controlOutput,projectBoomerangControls,readBoomerangControls,checkBoomerangControlsDisplay } from '../src/lib/boomerang-source-controls.mjs';
import { identitySignature } from './boomerang-protocol.mjs';
const args=new Set(process.argv.slice(2));assert([...args].every(arg=>['--constructed','--built'].includes(arg))&&!(args.has('--constructed')&&args.has('--built')));
const constructed=args.has('--constructed'),built=args.has('--built');
// Full independent-reader unit schema, with synthetic programs/signatures.
// The temporary packet is deleted immediately; no compiler/native/VM/SDK runs
// or corpus writes occur and the unit packet is never publication evidence.
const unit=JSON.parse(execFileSync('python3',['-c',
  'import sys,json,tempfile; from pathlib import Path; sys.path.insert(0,"scripts"); from test_boomerang import control_fixture; from boomerang_source_controls import native_rows,expected_output,NAMES;\nwith tempfile.TemporaryDirectory() as temporary:\n base=Path(temporary); report=control_fixture(base); print(json.dumps({"report":report,"nativeRows":native_rows((base/"native.log").read_text()),"relations":[{"name":name,"claim":claim,"expected":expected_output(name,claim)} for name in NAMES for claim in [False,True]]}))',
],{cwd:fileURLToPath(new URL('../../',import.meta.url)),encoding:'utf8',maxBuffer:8*1024*1024}));
assert.equal(unit.relations.length,10);
for(const relation of unit.relations)assert.deepEqual({status:'accepted',output_hex:controlOutput(relation.name,relation.claim)},relation.expected,
  `${relation.name}/${relation.claim}: mutation must preserve the other branch`);
const evidence=constructed?{...projectBoomerangControls(unit.report,unit.nativeRows),reportSHA:'aa'.repeat(32)}:await readBoomerangControls();
assert(evidence,'Actual source-control packet pending; --constructed tests explicit synthetic unit metadata only');
assert.deepEqual(evidence.counts,{nativeGroups:1,native:9,exports:5,exportModes:10,vm:18,attempts:18,accepted:6,rejected:12});
let controls=0;
for(const mutate of [
  p=>{p.native.native_test_groups=2;},(_,n)=>{n.pop();},p=>{p.prepared.recipe.controls.pop();},p=>{p.runs.pop();},
  p=>{p.runs[0].execution.outcome.output_hex='80';},(_,n)=>{n[0].actual.output_hex='80';},
  p=>{p.runs[0].execution={outcome:{status:'inconclusive'},cost:null,error:'budget',failure:{kind:'resource',code:'cost_exceeded'}};},
  p=>{p.ledger.result.attempts[1].signature_hex=identitySignature;},p=>{p.ledger.result.attempts[1].outcome.code='BadAggregateSignature';},
  p=>{p.ledger.result.attempts[0].outcome.full_state_unchanged=false;},
  p=>{p.ledger.result.attempts[6].input.puzzle_hash=p.instances.off.original.tree_hash;},
  p=>{p.ledger.result.attempts[6].outcome.child.amount=999;},
]) {
  projectBoomerangControls(unit.report,unit.nativeRows);const p=structuredClone(unit.report),n=structuredClone(unit.nativeRows);mutate(p,n);
  assert.throws(()=>projectBoomerangControls(p,n));controls++;
}
const boundaries=['each mutant’s own committed instance','constructing a condition does not enforce it','ordinary Rust, Clippy and the subset checks',
  'not errors Rust makes impossible','no Rue or Chialisp mutant runs','not compiler preservation','not a BLS proof','public fixture key',
  'separate from the original policy’s','not ready for real funds'];
function render() {
  return `<section data-boomerang-source-controls ${Object.entries(evidence.counts).map(([key,value])=>`data-boomerang-controls-${key.replace(/[A-Z]/g,letter=>'-'+letter.toLowerCase())}="${value}"`).join(' ')}>
  <p>1 native test group;9 native output observations;5 source exports;10 export/mode artifacts;18 primary VM observations;18 separate SDK submissions;6 accepted forbidden spends;12 typed consensus rejections;${boundaries.join('; ')};${evidence.reportSHA}</p>
  <table>${evidence.rows.map(row=>`<tr data-boomerang-control="${row.mode}:${row.name}:${row.witness}"><th>${row.name} ${row.witness} t=${row.timestamp} signing=${row.signing}</th><td>${row.instance.tree_hash}</td>
  <td>Native accepted / ${row.native.output_hex} VM accepted / ${row.execution.outcome.output_hex}</td><td>Consensus ${row.code??'accepted'} ${row.code?'full SDK equality checked in memory':`${row.ledger.outcome.child.coin_id} ${row.ledger.outcome.child.parent_coin_id} ${row.ledger.outcome.child.puzzle_hash} child amount=${row.ledger.outcome.child.amount}`}</td></tr>`).join('')}</table>
  <a href="/chialisp-oxide/reports/boomerang-source-controls/report.json">exact packet</a></section>`;
}
function display(page) {
  checkBoomerangControlsDisplay(page,evidence);
  const section=page.match(/<section\b[^>]*data-boomerang-source-controls(?:\s|=|>)[^>]*>[\s\S]*?<\/section>/)?.[0];assert(section);
  for(const [from,to]of [['18 separate SDK submissions','18 VM-only examples'],['6 accepted forbidden spends','12 accepted forbidden spends'],
    ['t=101','t=99'],['signing=recipient','signing=none'],['Consensus AssertBeforeSecondsAbsoluteFailed','Consensus BadAggregateSignature'],
    ['full SDK equality checked in memory','unverified'],['child amount=1000','child amount=999'],
    ['not errors Rust makes impossible','errors Rust makes impossible'],['data-boomerang-control="off:original:timely_unsigned_claim"','data-boomerang-control="off:mutant:timely_unsigned_claim"']]) {
    assert(section.includes(from),`No-op mutant display corruption ${from}`);const bad=page.replace(section,section.replace(from,to));assert.notEqual(bad,page);
    assert.throws(()=>checkBoomerangControlsDisplay(bad,evidence),`Escaped mutant display corruption ${from}`);controls++;
  }
}
display(render());
if(built) {
  for(const route of ['tutorials/boomerang','lab/boomerang'])display(await readFile(new URL(`../dist/${route}/index.html`,import.meta.url),'utf8'));
  const base=new URL('../dist/reports/boomerang-source-controls/',import.meta.url);
  assert.deepEqual(JSON.parse(await readFile(new URL('report.json',base))),evidence.report);
  for(const file of evidence.report.files)assert.equal(createHash('sha256').update(await readFile(new URL(file.path,base))).digest('hex'),file.sha256);
}
console.log(`PASS 10 independent two-branch output relations and ${controls} boomerang source-control presentation corruptions; ${constructed?'synthetic unit metadata only':built?'actual admitted packet, built views and inventory downloads':'actual admitted recorded packet'}. No new execution or proof credit.`);
