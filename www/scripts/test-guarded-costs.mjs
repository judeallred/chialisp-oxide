import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp,readFile,rm,writeFile,symlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { guardedArtifacts,readGuardedCostInputs,guardedCostEvidence,checkGuardedDisplay,checkGuardedDownloads } from '../src/lib/guarded-costs.mjs';
import { compareCostCaptures,summarizeCosts } from '../src/lib/p2-costs.mjs';
const args=new Set(process.argv.slice(2));assert([...args].every(a=>['--pending-controls','--built'].includes(a))&&!(args.has('--pending-controls')&&args.has('--built')));
const pending=args.has('--pending-controls'),built=args.has('--built');
const sha=b=>createHash('sha256').update(b).digest('hex'),parse=b=>JSON.parse(Buffer.from(b).toString('utf8')),fmt=n=>new Intl.NumberFormat('en-US').format(n);
const inputs=await readGuardedCostInputs(),old=parse(inputs.checkpoint).puzzles.at(-1),current=parse(inputs.validation).puzzles.at(-1),before=parse(inputs['before-costs']),now=parse(inputs['costs.json']);
const positive=()=>compareCostCaptures(old,current,before,now);
let evidence;
if(pending){
 assert.equal(parse(inputs['profile.json']).status,'pending_signed_checkpoint_and_capture_review');assert.throws(()=>guardedCostEvidence(inputs),/pending profiles cannot publish/);
 const comparison=positive();evidence={...comparison,summary:summarizeCosts(comparison.rows),context:{commit:'CONSTRUCTED-CONTEXT-NOT-A-COMMIT',reportSHA:sha(inputs.checkpoint)},currentReportSHA:sha(inputs.validation),downloads:guardedArtifacts.map(a=>({...a,content:inputs[a.basename],bytes:inputs[a.basename].length,sha256:sha(inputs[a.basename])}))};
}else evidence=guardedCostEvidence(inputs);
assert.deepEqual([evidence.summary.beforeBytes,evidence.summary.bytes,evidence.summary.beforeRawVM,evidence.summary.rawVM,evidence.summary.beforeTotal,evidence.summary.total,evidence.summary.delta,evidence.summary.percent],[1685,1462,65150,60409,45309974,42629233,-2680741,'5.92']);
let controls=0;
for(const name of ['total','mode','per-spend','height','quote','byte-rate','signature','float','bool','off-regression']){
 positive();const changed=structuredClone(now),joint=changed.observations[1].joint;
 if(name==='total')joint.total_conditions_cost++;
 if(name==='mode')changed.observations[1].optimize_lir=1;
 if(name==='per-spend')joint.per_spend[0].execution_cost++;
 if(name==='height')joint.previous_transaction_height++;
 if(name==='quote')joint.excluded_quote_wrapper_bytes=0;
 if(name==='byte-rate')joint.cost_per_byte++;
 if(name==='signature')joint.signature_scope='Signatures checked by metric helper';
 if(name==='float')joint.execution_cost+=.5;
 if(name==='bool')joint.execution_cost=true;
 if(name==='off-regression'){const off=changed.observations[0].joint;off.execution_cost++;off.total_conditions_cost++;off.per_spend[0].execution_cost++;}
 assert.throws(()=>compareCostCaptures(old,current,before,changed),undefined,name);controls++;
}
if(!pending)for(const name of ['pending','signed-context','checkpoint','current','metrics','controls','mode','stages']){
 guardedCostEvidence(inputs);const changed=Object.fromEntries(Object.entries(inputs).map(([k,v])=>[k,Buffer.from(v)]));
 if(['pending','signed-context','mode','stages'].includes(name)){const p=parse(changed['profile.json']);if(name==='pending')p.status='pending_signed_checkpoint_and_capture_review';if(name==='signed-context')p.baseline.commit=null;if(name==='mode')p.reviewed_after.bundles.on.optimize_lir=1;if(name==='stages')p.optimization_pipeline.after.rust_mir.optimized.reverse();changed['profile.json']=Buffer.from(JSON.stringify(p));}
 else{const key={checkpoint:'checkpoint',current:'validation',metrics:'costs.json',controls:'controls.json'}[name];changed[key]=Buffer.concat([changed[key],Buffer.from('\n')]);}
 assert.throws(()=>guardedCostEvidence(changed));controls++;
}
const pair=(a,b)=>`data-before="${a}" data-current="${b}">${fmt(a)} → ${fmt(b)}`;
const summary=[['module',evidence.summary.beforeBytes,evidence.summary.bytes],['vm',evidence.summary.beforeRawVM,evidence.summary.rawVM],['joint',evidence.summary.beforeTotal,evidence.summary.total]].map(([key,a,b])=>`<div class="metric"><strong data-guarded-summary="${key}" ${pair(a,b)}</strong><span>${{module:'optimized Rust module bytes',vm:'ordinary input · primary VM cost',joint:'joint bundle · total conditions cost'}[key]}</span></div>`).join('');
const context=`<p data-guarded-context>Baseline: signed source-only commit <a href="https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.commit}">${evidence.context.commit}</a> source-only validation SHA-256 ${evidence.context.reportSHA} guarded compiler validation SHA-256 ${evidence.currentReportSHA}</p>`;
const scopes=`<p>${fmt(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%); ${fmt(-evidence.summary.executionDelta)} fewer execution units; ${fmt(-evidence.summary.byteDelta)} fewer charged bytes; clvmr 0.21.0; clvmr 0.16.2; chia-consensus 0.36.1; 12,000; two-byte quote wrapper; not a universal cost bound; not additional admissions</p><a href="/chialisp-oxide/tutorials/transaction-costs/">historical lesson</a>`;
const matrix=`<table data-guarded-matrix><caption>Source-only checkpoint → guarded compiler checkpoint: total conditions costs</caption><thead><tr>${['Build','Genesis','Joint collection','Lineage'].map(t=>`<th scope="col">${t}</th>`).join('')}</tr></thead><tbody>${evidence.rows.map(row=>`<tr data-guarded-build="${row.backend}:${row.optimize}"><th scope="row">${row.label}</th>${row.phases.map(p=>`<td data-guarded-phase="${p.phase}" ${pair(p.before.total,p.current.total)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
const downloads=`<ul>${evidence.downloads.map(a=>`<li><a data-guarded-download="${a.basename}" href="/chialisp-oxide/reports/guarded-projection/${a.basename}">${a.label}</a> ${a.sha256}</li>`).join('')}</ul>`;
const render=compact=>`<section data-guarded-costs data-guarded-view="${compact?'compact':'full'}"><h2>Source-only checkpoint → guarded compiler checkpoint</h2>${context}${summary}${scopes}${compact?'':matrix+downloads}</section>`;
function mutations(markup,compact){
 const changes=[['Source-only checkpoint → guarded compiler checkpoint','Current costs'],[evidence.context.commit,'WRONG-SIGNED-CHECKPOINT'],[evidence.context.reportSHA,'0'.repeat(64)],[evidence.currentReportSHA,'f'.repeat(64)],['1,685 → 1,462','1,685 → 1,461'],['2,680,741 fewer cost units','2,680,740 fewer cost units'],['5.92%','5.91%'],['223 fewer charged bytes','222 fewer charged bytes'],['ordinary input · primary VM cost','joint bundle · primary VM cost'],['clvmr 0.16.2','clvmr 0.21.0'],['two-byte quote wrapper','no wrapper']];
 if(!compact)changes.push(['<th scope="row">Rust / off</th>','<th scope="row">Rue / off</th>'],['<th scope="col">Genesis</th>','<th scope="col">Joint</th>'],['checkpoint: total conditions costs','checkpoint: raw VM costs'],['data-guarded-build="rust_mir:false"','data-guarded-build="rust_mir:true"'],['data-guarded-phase="lineage"','data-guarded-phase="other"'],['45,309,974 → 42,629,233','45,309,974 → 42,629,232']);
 const paragraph=markup.match(/<p\b[^>]*data-guarded-context[^>]*>[\s\S]*?<\/p>/)?.[0];assert(paragraph);
 const anchor=paragraph.match(/<a\b[^>]*>[\s\S]*?<\/a>/)?.[0];assert(anchor);
 const wrongCommit=paragraph.replace(anchor,anchor.replace(/(>)[\s\S]*(<\/a>)/,'$1WRONG-VISIBLE-COMMIT$2'));
 const wrongHashes=[evidence.context.reportSHA,evidence.currentReportSHA].map(hash=>paragraph.replace('<p ',`<p data-retained-correct-hash="${hash}" `).replace(/>([^<]*)</g,(_match,text)=>`>${text.replace(hash,'WRONG-VISIBLE-HASH')}<`));
 const wrongLabel=paragraph.replace('source-only validation SHA-256','other validation SHA-256');
 const contextMutations=[wrongCommit,...wrongHashes,wrongLabel].map(value=>{assert.notEqual(value,paragraph);return markup.replace(paragraph,value);});
 return [...changes.map(([a,b])=>{assert(markup.includes(a),a);const altered=markup.replace(a,b);assert.notEqual(altered,markup);return altered;}),...contextMutations];
}
for(const compact of [false,true]){const markup=render(compact);checkGuardedDisplay(markup,evidence,{compact});for(const altered of mutations(markup,compact)){checkGuardedDisplay(markup,evidence,{compact});assert.throws(()=>checkGuardedDisplay(altered,evidence,{compact}));controls++;}}
const temp=await mkdtemp(path.join(os.tmpdir(),'oxide-guarded-costs-'));
try{
 for(const a of evidence.downloads)await writeFile(path.join(temp,a.basename),a.content);
 for(const a of evidence.downloads){await checkGuardedDownloads(temp,evidence);await writeFile(path.join(temp,a.basename),Buffer.concat([a.content,Buffer.from('\n')]));await assert.rejects(checkGuardedDownloads(temp,evidence));await writeFile(path.join(temp,a.basename),a.content);controls++;}
 await writeFile(path.join(temp,'extra.json'),'{}');await assert.rejects(checkGuardedDownloads(temp,evidence));await rm(path.join(temp,'extra.json'));controls++;
 const a=evidence.downloads[0];await rm(path.join(temp,a.basename));await symlink(path.resolve('package.json'),path.join(temp,a.basename));await assert.rejects(checkGuardedDownloads(temp,evidence));controls++;
}finally{await rm(temp,{recursive:true,force:true});}
if(built){
 for(const [route,compact] of [['tutorials/guarded-projections',false],['tutorials/p2-singleton',true],['lab/p2-singleton',true]]){const bytes=await readFile(new URL(`../dist/${route}/index.html`,import.meta.url));assert(bytes.length<=(route.endsWith('p2-singleton')?1024:512)*1024);const markup=bytes.toString('utf8');checkGuardedDisplay(markup,evidence,{compact});for(const altered of mutations(markup,compact)){assert.throws(()=>checkGuardedDisplay(altered,evidence,{compact}));controls++;}}
 await checkGuardedDownloads(new URL('../dist/reports/guarded-projection',import.meta.url),evidence);
}
assert.deepEqual(await readGuardedCostInputs(),inputs);
console.log(`PASS ${controls} guarded comparison controls; ${pending?'explicit pending-profile rejection and constructed-context data/display/download fixtures only':built?'three actual Astro views plus three exact downloads':'recorded metadata/synthetic display/downloads only'}. No compiler/VM/solver/ledger execution or promotion.`);
