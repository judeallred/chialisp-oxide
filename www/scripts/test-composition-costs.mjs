import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp,readFile,rm,writeFile,symlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { compositionArtifacts,readCompositionCostInputs,compositionCostEvidence,checkCompositionDisplay,checkCompositionDownloads } from '../src/lib/composition-costs.mjs';
import { compareCostCaptures,summarizeCosts } from '../src/lib/p2-costs.mjs';
const args=new Set(process.argv.slice(2));assert([...args].every(a=>['--pending-controls','--built'].includes(a))&&!(args.has('--pending-controls')&&args.has('--built')));
const pending=args.has('--pending-controls'),built=args.has('--built');
const sha=b=>createHash('sha256').update(b).digest('hex'),parse=b=>JSON.parse(Buffer.from(b).toString('utf8')),fmt=n=>new Intl.NumberFormat('en-US').format(n);
const inputs=await readCompositionCostInputs(),old=parse(inputs.checkpoint).puzzles.find(row=>row.slug==='p2-singleton'),current=parse(inputs.validation).puzzles.find(row=>row.slug==='p2-singleton'),before=parse(inputs['before-costs']),now=parse(inputs['costs.json']);
const positive=()=>compareCostCaptures(old,current,before,now);
let evidence;
if(pending){
 const pendingProfile=structuredClone(parse(inputs['profile.json']));pendingProfile.status='pending_actual_composition_capture_review';pendingProfile.reviewed_after=null;
 assert.throws(()=>compositionCostEvidence({...inputs,'profile.json':Buffer.from(JSON.stringify(pendingProfile))}),/pending profiles cannot publish/);
 const comparison=positive();evidence={...comparison,summary:summarizeCosts(comparison.rows),context:{label:'Unsigned reviewed source-only checkpoint',kind:'unsigned_reviewed_content_addressed_checkpoint',reportSHA:sha(inputs.checkpoint),anchor:parse(inputs['profile.json']).signed_anchor},currentReportSHA:sha(inputs.validation),activeReportSHA:sha(inputs['active-validation']),downloads:compositionArtifacts.map(a=>({...a,content:inputs[a.basename],bytes:inputs[a.basename].length,sha256:sha(inputs[a.basename])}))};
}else evidence=compositionCostEvidence(inputs);
assert.deepEqual([evidence.summary.beforeBytes,evidence.summary.bytes,evidence.summary.beforeRawVM,evidence.summary.rawVM,evidence.summary.beforeTotal,evidence.summary.total,evidence.summary.delta,evidence.summary.percent],[1685,1334,65150,57925,45309974,41090749,-4219225,'9.31']);
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
if(!pending)for(const name of ['pending','signed-context','intermediate-git','intermediate-kind','checkpoint','current','metrics','controls','mode','stages']){
 compositionCostEvidence(inputs);const changed=Object.fromEntries(Object.entries(inputs).map(([k,v])=>[k,Buffer.from(v)]));
 if(['pending','signed-context','intermediate-git','intermediate-kind','mode','stages'].includes(name)){const p=parse(changed['profile.json']);if(name==='pending')p.status='pending_actual_composition_capture_review';if(name==='signed-context')p.signed_anchor.commit=null;if(name==='intermediate-git')p.reviewed_intermediate.commit=p.signed_anchor.commit;if(name==='intermediate-kind')p.reviewed_intermediate.kind='signed_git_checkpoint';if(name==='mode')p.reviewed_after.bundles.on.optimize_lir=1;if(name==='stages')p.optimization_pipeline.after.rust_mir.optimized.reverse();changed['profile.json']=Buffer.from(JSON.stringify(p));}
 else{const key={checkpoint:'checkpoint',current:'validation',metrics:'costs.json',controls:'controls.json'}[name];changed[key]=Buffer.concat([changed[key],Buffer.from('\n')]);}
 assert.throws(()=>compositionCostEvidence(changed));controls++;
}
const pair=(a,b)=>`data-before="${a}" data-current="${b}">${fmt(a)} → ${fmt(b)}`;
const summary=[['module',evidence.summary.beforeBytes,evidence.summary.bytes],['vm',evidence.summary.beforeRawVM,evidence.summary.rawVM],['joint',evidence.summary.beforeTotal,evidence.summary.total]].map(([key,a,b])=>`<div class="metric"><strong data-composition-summary="${key}" ${pair(a,b)}</strong><span>${{module:'optimized Rust module bytes',vm:'ordinary input · primary VM cost',joint:'joint bundle · total conditions cost'}[key]}</span></div>`).join('');
const context=`<p data-composition-context>Baseline: unsigned reviewed source-only checkpoint SHA-256 ${evidence.context.reportSHA} · signed historical E20 anchor <a href="https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.anchor.commit}">${evidence.context.anchor.commit}</a> · composed compiler validation SHA-256 ${evidence.currentReportSHA} · active corpus validation SHA-256 ${evidence.activeReportSHA}</p>`;
const scopes=`<p>${fmt(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%); ${fmt(-evidence.summary.executionDelta)} fewer execution units; ${fmt(-evidence.summary.byteDelta)} fewer charged bytes; clvmr 0.21.0; clvmr 0.16.2; chia-consensus 0.36.1; 12,000; two-byte quote wrapper; not a universal cost bound; not additional admissions</p><a href="/chialisp-oxide/tutorials/transaction-costs/">historical lesson</a>`;
const matrix=`<table data-composition-matrix><caption>Source-only checkpoint → composed compiler checkpoint: total conditions costs</caption><thead><tr>${['Build','Genesis','Joint collection','Lineage'].map(t=>`<th scope="col">${t}</th>`).join('')}</tr></thead><tbody>${evidence.rows.map(row=>`<tr data-composition-build="${row.backend}:${row.optimize}"><th scope="row">${row.label}</th>${row.phases.map(p=>`<td data-composition-phase="${p.phase}" ${pair(p.before.total,p.current.total)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
const downloads=`<ul>${evidence.downloads.map(a=>`<li><a data-composition-download="${a.basename}" href="/chialisp-oxide/reports/runtime-composition/${a.basename}">${a.label}</a> ${a.sha256}</li>`).join('')}</ul>`;
const render=compact=>`<section data-composition-costs data-composition-view="${compact?'compact':'full'}"><h2>Source-only checkpoint → composed compiler checkpoint</h2>${context}${summary}${scopes}${compact?'':matrix+downloads}</section>`;
function mutations(markup,compact){
 const page=markup;markup=page.match(/<section\b[^>]*data-composition-costs[^>]*>[\s\S]*?<\/section>/)?.[0];assert(markup);
 const changes=[['Source-only checkpoint → composed compiler checkpoint','Current costs'],[evidence.context.anchor.commit,'WRONG-SIGNED-CHECKPOINT'],[evidence.context.reportSHA,'0'.repeat(64)],[evidence.currentReportSHA,'f'.repeat(64)],['1,685 → 1,334','1,685 → 1,333'],['4,219,225 fewer cost units','4,219,224 fewer cost units'],['9.31%','9.30%'],['351 fewer charged bytes','350 fewer charged bytes'],['ordinary input · primary VM cost','joint bundle · primary VM cost'],['clvmr 0.16.2','clvmr 0.21.0'],['two-byte quote wrapper','no wrapper']];
 if(!compact)changes.push([/(<th\b[^>]*scope="row"[^>]*>)Rust \/ off(<\/th>)/,'$1Rue / off$2'],[/(<th\b[^>]*scope="col"[^>]*>)Genesis(<\/th>)/,'$1Joint$2'],['checkpoint: total conditions costs','checkpoint: raw VM costs'],['data-composition-build="rust_mir:false"','data-composition-build="rust_mir:true"'],['data-composition-phase="lineage"','data-composition-phase="other"'],['45,309,974 → 41,090,749','45,309,974 → 41,090,748']);
 const paragraph=markup.match(/<p\b[^>]*data-composition-context[^>]*>[\s\S]*?<\/p>/)?.[0];assert(paragraph);
 const anchor=paragraph.match(/<a\b[^>]*>[\s\S]*?<\/a>/)?.[0];assert(anchor);
 const wrongCommit=paragraph.replace(anchor,anchor.replace(/(>)[\s\S]*(<\/a>)/,'$1WRONG-VISIBLE-COMMIT$2'));
 const wrongHashes=[evidence.context.reportSHA,evidence.currentReportSHA,evidence.activeReportSHA].map(hash=>paragraph.replace('<p ',`<p data-retained-correct-hash="${hash}" `).replace(/>([^<]*)</g,(_match,text)=>`>${text.replace(hash,'WRONG-VISIBLE-HASH')}<`));
 const wrongLabel=paragraph.replace('unsigned reviewed source-only checkpoint SHA-256','signed source-only checkpoint SHA-256');
 const contextMutations=[wrongCommit,...wrongHashes,wrongLabel].map(value=>{assert.notEqual(value,paragraph);return markup.replace(paragraph,value);});
 return [...changes.map(([a,b])=>{if(typeof a==='string')assert(markup.includes(a),a);else assert(a.test(markup),String(a));const altered=markup.replace(a,b);assert.notEqual(altered,markup);return altered;}),...contextMutations].map(section=>{const altered=page.replace(markup,section);assert.notEqual(altered,page);return altered;});
}
for(const compact of [false,true]){const markup=render(compact);checkCompositionDisplay(markup,evidence,{compact});for(const altered of mutations(markup,compact)){checkCompositionDisplay(markup,evidence,{compact});assert.throws(()=>checkCompositionDisplay(altered,evidence,{compact}));controls++;}}
const temp=await mkdtemp(path.join(os.tmpdir(),'oxide-composition-costs-'));
try{
 for(const a of evidence.downloads)await writeFile(path.join(temp,a.basename),a.content);
 for(const a of evidence.downloads){await checkCompositionDownloads(temp,evidence);await writeFile(path.join(temp,a.basename),Buffer.concat([a.content,Buffer.from('\n')]));await assert.rejects(checkCompositionDownloads(temp,evidence));await writeFile(path.join(temp,a.basename),a.content);controls++;}
 await writeFile(path.join(temp,'extra.json'),'{}');await assert.rejects(checkCompositionDownloads(temp,evidence));await rm(path.join(temp,'extra.json'));controls++;
 const a=evidence.downloads[0];await rm(path.join(temp,a.basename));await symlink(path.resolve('package.json'),path.join(temp,a.basename));await assert.rejects(checkCompositionDownloads(temp,evidence));controls++;
}finally{await rm(temp,{recursive:true,force:true});}
if(built){
 for(const [route,compact] of [['tutorials/guarded-projections',false],['tutorials/p2-singleton',true],['lab/p2-singleton',true]]){const bytes=await readFile(new URL(`../dist/${route}/index.html`,import.meta.url));assert(bytes.length<=(route.endsWith('p2-singleton')?1024:512)*1024);const markup=bytes.toString('utf8');checkCompositionDisplay(markup,evidence,{compact});for(const altered of mutations(markup,compact)){assert.throws(()=>checkCompositionDisplay(altered,evidence,{compact}));controls++;}}
 await checkCompositionDownloads(new URL('../dist/reports/runtime-composition',import.meta.url),evidence);
}
assert.deepEqual(await readCompositionCostInputs(),inputs);
console.log(`PASS ${controls} composition comparison controls; ${pending?'explicit pending-profile rejection and constructed-context data/display/download fixtures only':built?'three actual Astro views plus three exact downloads':'recorded metadata/synthetic display/downloads only'}. No compiler/VM/solver/ledger execution or promotion.`);
