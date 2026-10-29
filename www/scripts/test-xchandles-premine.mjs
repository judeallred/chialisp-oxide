// Presentation consistency after Python admission. Explicit fixture mode is
// historical/constructed evidence only, never a substitute for current packets.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync, realpathSync, symlinkSync, openSync, ftruncateSync, closeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { projectXchandlesPremine, projectXchandlesHistory, projectXchandlesSimulator, readXchandlesPremine, readXchandlesArtifact, readXchandlesBytes, checkXchandlesDisplay, xchandlesRoute, xchandlesScope } from '../src/lib/xchandles-premine.mjs';
let historyPath,nativePath,built;
for(let index=2;index<process.argv.length;index++){
  const flag=process.argv[index];assert(['--history-summary','--native-summary','--built'].includes(flag)&&index+1<process.argv.length,'Expected explicit summary paths or --built DIST');
  const value=path.resolve(process.argv[++index]);
  if(flag==='--history-summary'){assert(!historyPath);historyPath=value;}else if(flag==='--native-summary'){assert(!nativePath);nativePath=value;}else{assert(!built);built=value;}
}
assert(Boolean(historyPath)===Boolean(nativePath),'Both independently admitted fixture summaries are required');assert(!(built&&historyPath),'Built checks require current packets, never alternate summaries');
const json=filename=>JSON.parse(readXchandlesBytes(filename,2*1024*1024).toString('utf8'));
const history=historyPath?json(historyPath):null,native=nativePath?json(nativePath):null;
const evidence=history?projectXchandlesPremine(history,native):await readXchandlesPremine({required:true});
assert(evidence,'Both complete current XCHandles packets are mandatory');
let historyControls=0,nativeControls=0;
if(history){
  const controls=[
    ['old history profile',x=>x.protocol='xchandles-mainnet-premine-history/v1'],
    ['execution credit',x=>x.newReaderExecutions=1],['name coverage',x=>x.primary.exact_csv_handle_names=3835],
    ['child completeness misclaim',x=>x.primary.bootstrap_child_lists_complete=true],['block observation count',x=>x.blockRpcObservations=129],
    ['wrong artifact bytes',x=>x.identities.registerModule.bytes=1621],['wrong artifact hash',x=>x.identities.registerModule.treeHash='00'.repeat(32)],
    ['deployment authentication',x=>x.identities.sourceDeploymentAuthenticated=true],['manifest digest type',x=>x.packet.manifest.sha256=true],
    ['duplicate late descriptor',x=>x.packet.members.at(-1).path=x.packet.members[0].path],['late oversized member',x=>x.packet.members.at(-1).bytes=2*1024*1024+1],
    ['late escaped path',x=>x.packet.members.at(-1).path='../escape'],['manifest cap',x=>x.packet.manifest.bytes=512*1024+1],
    ['missing final spend',x=>x.rows.pop()],['final row order',x=>x.rows[128].index=127],['final batch count',x=>x.rows[128].namesCount=30],
    ['final cumulative count',x=>x.rows[128].cumulativeNames=3835],['late successor edge',x=>x.rows[128].coin='00'.repeat(32)],
    ['late state change',x=>x.rows[128].registryStateTreeHash='00'.repeat(32)],['late action root change',x=>x.rows[128].allowedActionMerkleRoot='00'.repeat(32)],
    ['primary sum',x=>x.primary.primary_cost_sum=0],['late zero-coin outputs',x=>x.rows[128].createZero=0],
    ['fee precision conversion',x=>x.rows[128].fullBlockFeesDecimal=Number(x.rows[128].fullBlockFeesDecimal)],
    ['negative fee',x=>x.rows[128].fullBlockFeesDecimal='-1'],['header authentication',x=>x.rows[128].headerRecomputed=true],
    ['unadmitted step download',x=>x.rows[128].downloads.stdout='not-admitted.json'],['wrong CREATE unit',x=>x.conditionalCreateCost.unit=1800],
    ['wrong CREATE floor',x=>x.conditionalCreateCost.zeroOnly=5302597203],['wrong ceiling',x=>x.conditionalCreateCost.pinnedBlockCeiling=1],
    ['whole block fee number',x=>x.wholeBlockFeesSumDecimal=0],['source license confusion',x=>x.identities.sourceCorrespondence.license='MIT'],
  ];
  for(const [name,mutate]of controls){const changed=structuredClone(history);mutate(changed);assert.throws(()=>projectXchandlesHistory(changed),undefined,`Accepted history projection corruption: ${name}`);historyControls++;}
  const controlsNative=[
    ['native protocol',x=>x.protocol='other'],['SDK count',x=>x.summary.sdk_transaction_attempts=81],['acceptance count',x=>x.summary.accepted=12],
    ['typed/runtime conflation',x=>x.summary.exact_typed_consensus_rejections=9],['runtime count',x=>x.summary.runtime_umbrellas=0],
    ['component count',x=>x.summary.component_vm_runs=20],['cost count',x=>x.summary.plain_generator_cost_passes=48],
    ['native VM version',x=>x.summary.component_vm='clvmr 0.21.0'],['cost BLS scope',x=>x.summary.cost_profile='full-signature-validation'],
    ['missing last batch',x=>x.summary.batches.pop()],['last batch size',x=>x.summary.batches[2].batch_size=2],
    ['last charged bytes',x=>x.summary.batches[2].charged_generator_bytes++],['last total cost',x=>x.summary.batches[2].total_cost++],
    ['native fee number',x=>x.summary.batches[2].fee_mojos=1],['handle coverage',x=>x.summary.handles.pop()],
    ['SDK license confusion',x=>x.summary.licenses.sdk='Prosperity-3.0'],['puzzle license confusion',x=>x.summary.licenses.retained_register_source='Apache-2.0'],
    ['SDK archive hash',x=>x.summary.licenses.sdk_driver_archive_sha256='00'.repeat(32)],['late duplicate descriptor',x=>x.files.at(-1).path=x.files[0].path],
    ['late native payload size',x=>x.files.at(-1).bytes=2*1024*1024+1],['checkpoint coverage inflation',x=>x.summary.gate_process_matrix_totals.sdk_attempts=20],
  ];
  for(const [name,mutate]of controlsNative){const changed=structuredClone(native);mutate(changed);assert.throws(()=>projectXchandlesSimulator(changed),undefined,`Accepted native projection corruption: ${name}`);nativeControls++;}
}
const escape=value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const attr=value=>value.replace(/[A-Z]/g,c=>'-'+c.toLowerCase());
const field=(name,value)=>`<code ${name}>${escape(value)}</code>`;
const h=evidence.history,n=evidence.simulator;
const heading={spends:129,names:3836,primary:h.primaryCost,zeroCreates:11508,zeroFloor:h.conditionalCost.zeroOnly,blockCeiling:h.conditionalCost.pinnedBlockCeiling,historyIndex:h.manifestSHA,nativeIndex:n.indexSHA,wholeBlockCost:h.wholeBlockCost,wholeBlockFees:h.wholeBlockFees,registry:h.identities.registryReveal.module_tree_hash,state:h.identities.registryStateTreeHash,actionRoot:h.identities.allowedActionMerkleRoot,register:h.identities.registerModule.treeHash};
const inspector=JSON.stringify({rows:h.rows,downloadBase:'/chialisp-oxide/reports/xchandles-premine/history/'}).replaceAll('<','\\u003c');
const markup=`<main><p>${xchandlesScope.join('; ')}</p><p>clvmr 0.21.0; ${n.versions.component}; ${n.versions.consensus}; DONT_VALIDATE_SIGNATURE</p>${Object.entries(heading).map(([key,value])=>field(`data-xchandles-${attr(key)}`,value)).join('')}${Object.entries(n.counts).map(([key,value])=>field(`data-xchandles-native-${key}`,value)).join('')}<table>${n.batches.map(row=>`<tr data-xchandles-batch="${row.batch_size}">${['plain_generator_bytes','charged_generator_bytes','execution_cost','condition_cost','byte_cost','total_cost','fee_mojos'].map(key=>`<td>${field(`data-xchandles-batch-${key.replaceAll('_','-')}`,row[key])}</td>`).join('')}</tr>`).join('')}</table><script type="application/json" data-xchandles-step-data>${inspector}</script></main>`;
checkXchandlesDisplay(markup,evidence);
const replace=(html,name,value)=>html.replace(new RegExp(`(<code ${name}>)[^<]*`),(_,prefix)=>prefix+value);
const displayControls=[...Object.keys(heading).map(key=>[`visible ${key}`,html=>replace(html,`data-xchandles-${attr(key)}`,'wrong')]),...Object.keys(n.counts).map(key=>[`visible native ${key}`,html=>replace(html,`data-xchandles-native-${key}`,'wrong')]),...xchandlesScope.map(scope=>[`scope ${scope}`,html=>html.replace(scope,'scope removed')]),['VM version',html=>html.replace('clvmr 0.21.0','VM removed')],['cost profile',html=>html.replace('DONT_VALIDATE_SIGNATURE','profile removed')],['late batch total',html=>html.replace(/(<tr data-xchandles-batch="3">[\s\S]*?<code data-xchandles-batch-total-cost>)[^<]*/,(_,prefix)=>prefix+'0')],['missing last batch',html=>html.replace(/<tr data-xchandles-batch="3">[\s\S]*?<\/tr>/,'')],['late inspector row',html=>{const changed=structuredClone(h.rows);changed[128].cumulative=0;return html.replace(inspector,JSON.stringify({rows:changed,downloadBase:'/chialisp-oxide/reports/xchandles-premine/history/'}));}],['inspector missing',html=>html.replace(/<script[\s\S]*?<\/script>/,'')],['suffixed attribute',html=>html.replace('data-xchandles-history-index>','data-xchandles-history-index-hidden>')]];
for(const [name,mutate]of displayControls){const changed=mutate(markup);assert.notEqual(changed,markup,`Inert display mutation: ${name}`);assert.throws(()=>checkXchandlesDisplay(changed,evidence),undefined,`Accepted visible corruption: ${name}`);}
const temp=realpathSync(mkdtempSync(path.join(tmpdir(),'xchandles-display-')));
try{const file=path.join(temp,'one.json');writeFileSync(file,'{}');assert.equal(readXchandlesBytes(file,2).toString(),'{}');assert.throws(()=>readXchandlesBytes(file,Infinity));assert.throws(()=>readXchandlesBytes(file,2*1024*1024+1));assert.throws(()=>readXchandlesBytes(file,1));assert.throws(()=>readXchandlesBytes(file,2,{bytes:2,sha256:'00'.repeat(32)}));symlinkSync(file,path.join(temp,'alias'));assert.throws(()=>readXchandlesBytes(path.join(temp,'alias'),2));const fd=openSync(path.join(temp,'large'),'w');ftruncateSync(fd,2*1024*1024+1);closeSync(fd);assert.throws(()=>readXchandlesBytes(path.join(temp,'large'),2*1024*1024));}finally{rmSync(temp,{recursive:true,force:true});}
if(built){
  const html=readXchandlesBytes(path.join(built,xchandlesRoute,'index.html'),512*1024).toString('utf8');checkXchandlesDisplay(html,evidence);
  for(const lane of ['history','simulator'])for(const descriptor of evidence[lane].downloads){const actual=readXchandlesBytes(path.join(built,'reports/xchandles-premine',lane,descriptor.path),2*1024*1024,descriptor);const expected=await readXchandlesArtifact(lane,descriptor.path);assert(actual.equals(expected));}
}
console.log(`PASS ${historyControls} history projection, ${nativeControls} native projection and ${displayControls.length} visible corruption controls; bounded read/hash/alias/oversize controls. ${built?'Actual built case study and all byte-exact downloads checked.':'Constructed markup only.'} ${history?'Explicit admitted historical fixtures; no current publication credit.':'Both complete current Python admissions required.'} No new VM/SDK execution, authentication or preservation proof.`);
