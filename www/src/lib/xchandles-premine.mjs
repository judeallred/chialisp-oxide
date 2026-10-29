// Compact presentation after both complete Python admissions. No network, Cargo,
// compiler or VM work is performed by this loader or its download reader.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { constants, openSync, fstatSync, readSync, closeSync, lstatSync, realpathSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

export const xchandlesRoute='case-studies/xchandles-premine';
export const xchandlesPackets=Object.freeze({history:'corpus/xchandles-premine-history-v2',simulator:'corpus/xchandles-simulator'});
export const xchandlesScope=Object.freeze([
  'WIP prototype; not ready for real funds',
  'Canonical host SDK demonstration, not an on-chain Rust frontend port',
  'Off-chain allocation list; no authenticated remaining-work queue',
  'Exact CSV name coverage does not establish recipient, NFT or expiration agreement',
  'Bare primary VM replay cost is not whole-bundle cost',
  'Whole-block fees include unrelated transactions; no per-bundle fee attribution',
  'Source-reviewed unchanged-output arithmetic, not an executed consensus-parser proof',
  'No chain or execution authentication, fairness, liveness, BLS proof or compiler preservation',
]);
const roots=new Map();
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const hash=value=>assert(typeof value==='string'&&/^[0-9a-f]{64}$/.test(value),'Exact displayed hash');
const uint=value=>assert(Number.isSafeInteger(value)&&value>=0,'Exact nonnegative safe integer');
const money=value=>assert(typeof value==='string'&&/^(?:0|[1-9][0-9]*)$/.test(value),'Fees remain decimal strings');
function relative(value){assert(typeof value==='string'&&value.length>0&&Buffer.byteLength(value)<=512&&!value.includes('\\')&&!path.isAbsolute(value)&&value.split('/').every(p=>p&&p!=='.'&&p!=='..')&&!/[\x00-\x1f\x7f]/.test(value),'Confined artifact path');return value;}
function descriptors(rows,{files,total,member},manifest){
  assert(Array.isArray(rows)&&rows.length>0&&rows.length<=files,'Complete bounded descriptor inventory');
  const names=new Set();let remaining=total;
  return rows.map(row=>{relative(row.path);assert(!names.has(row.path),'Unique artifact descriptor');names.add(row.path);uint(row.bytes);assert(row.bytes<=member&&row.bytes<=remaining,'Precharged descriptor bytes');remaining-=row.bytes;hash(row.sha256);if(row.path===manifest||row.path.endsWith('/history-packet.json'))assert(row.bytes<=512*1024,'Manifest role bound');return {path:row.path,bytes:row.bytes,sha256:row.sha256};});
}
export function projectXchandlesHistory(summary){
  assert.equal(summary.protocol,'xchandles-mainnet-premine-history/v2');assert.equal(summary.status,'retained_consistency_passed');
  assert.equal(summary.primaryComponentObservations,129);assert.equal(summary.newReaderExecutions,0);
  assert.equal(summary.primary.exact_csv_handle_names,3836);assert.equal(summary.primary.internal_successor_edges,128);
  assert.equal(summary.primary.bootstrap_child_lists_complete,false);assert.equal(summary.blockRpcObservations,258);assert.equal(summary.blockHeights,129);
  assert.equal(summary.identities.registerModule.bytes,1478);assert.equal(summary.identities.registerModule.treeHash,'150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8');
  assert.equal(summary.identities.artifactCorrespondenceOnly,true);assert.equal(summary.identities.sourceDeploymentAuthenticated,false);
  const ids=summary.identities;for(const value of [ids.launcherCoinId,ids.registryReveal.module_tree_hash,ids.registryStateTreeHash,ids.allowedActionMerkleRoot,ids.actionLayerModuleTreeHash,ids.registerModule.blobSha256])hash(value);
  assert.equal(summary.packet.maximumPhysicalFiles,2048);assert.equal(summary.packet.maximumAggregateBytes,128*1024*1024);assert.equal(summary.packet.maximumMemberBytes,2*1024*1024);assert.equal(summary.packet.maximumManifestBytes,512*1024);
  const downloads=descriptors([summary.packet.manifest,...summary.packet.members],{files:2048,total:128*1024*1024,member:2*1024*1024},'history-packet.json');
  assert.equal(summary.packet.manifest.path,'history-packet.json');assert.equal(downloads.length,summary.physicalFiles);
  const declared=new Set(downloads.map(row=>row.path));assert(Array.isArray(summary.rows)&&summary.rows.length===129);
  let cumulative=0,primary=0;
  const seen=new Set();
  const rows=summary.rows.map((row,index)=>{
    assert.equal(row.index,index);assert.equal(row.namesCount,index===0?1:index===128?25:30);cumulative+=row.namesCount;assert.equal(row.cumulativeNames,cumulative);
    for(const value of [row.coin,row.successor])hash(value);assert(!seen.has(row.coin));seen.add(row.coin);if(index)assert.equal(summary.rows[index-1].successor,row.coin);
    assert.equal(row.registryRevealTreeHash,ids.registryReveal.module_tree_hash);assert.equal(row.registryStateTreeHash,ids.registryStateTreeHash);assert.equal(row.allowedActionMerkleRoot,ids.allowedActionMerkleRoot);
    for(const value of [row.height,row.primaryCost,row.createZero,row.createOne,row.fullBlockCost,row.timestamp])uint(value);
    assert(row.primaryCost>0);primary+=row.primaryCost;assert.equal(row.createZero,3*row.namesCount);assert.equal(row.createOne,1);
    assert.equal(row.headerRecomputed,false);money(row.fullBlockFeesDecimal);
    for(const name of Object.values(row.downloads))assert(declared.has(name),'Every step download was admitted');
    return {index,ordinal:index+1,coin:row.coin,successor:row.successor,height:row.height,names:row.namesCount,cumulative,
      primaryCost:row.primaryCost,zeroCreates:row.createZero,fullBlockCost:row.fullBlockCost,wholeBlockFees:row.fullBlockFeesDecimal,
      timestamp:row.timestamp,headerReported:row.headerReported,downloads:row.downloads};
  });
  assert.equal(cumulative,3836);assert.equal(primary,5302597203);assert.equal(summary.primary.primary_cost_sum,primary);
  const floor=summary.conditionalCreateCost;assert.equal(floor.unit,1800000);assert.equal(floor.zeroOnly,20714400000);assert.equal(floor.pinnedBlockCeiling,11000000000);
  assert.equal(rows.reduce((sum,row)=>sum+row.zeroCreates,0),11508);money(summary.wholeBlockFeesSumDecimal);uint(summary.wholeBlockCostSum);
  assert.equal(ids.sourceCorrespondence.source_commit,'6edc94856306d0f77e065c6c99aacf1055e51e2f');assert.equal(ids.sourceCorrespondence.license,'Prosperity Public License 3.0.0');
  return {manifestSHA:summary.packet.manifest.sha256,downloads,rows,identities:ids,primaryCost:primary,
    wholeBlockCost:summary.wholeBlockCostSum,wholeBlockFees:summary.wholeBlockFeesSumDecimal,
    conditionalCost:floor,scope:summary.scope,blockScope:summary.blockScope,physicalBytes:summary.physicalBytes,
    primaryScope:summary.primary.scope,counts:{spends:129,names:3836,internalEdges:128,blockObservations:258,zeroCreates:11508},
    evidenceCounts:{collectorRequests:summary.primary.collector_http_requests,retainedEarlierDependencies:summary.primary.earlier_dependency_files,olderRequestDescriptors:summary.primary.earlier_index_request_descriptors,unretainedUncreditedDescriptors:summary.primary.earlier_unretained_uncredited_descriptors,laterTerminalPointRequests:summary.primary.later_terminal_point_requests}};
}
export function projectXchandlesSimulator(input){
  const s=input.summary;assert.equal(input.protocol,'chialisp-oxide-xchandles-recorded-native-pilot/v1');hash(input.index_sha256);assert(s&&typeof s.scope==='string'&&s.scope.length>0);
  assert.equal(s.sdk_transaction_attempts,20);assert.equal(s.accepted,11);assert.equal(s.exact_typed_consensus_rejections,6);assert.equal(s.runtime_umbrellas,3);assert.equal(s.component_vm_runs,48);assert.equal(s.plain_generator_cost_passes,11);assert.equal(s.checkpoint_refusal_processes,11);
  assert.deepEqual(s.batch_sizes,[1,2,3]);assert(Array.isArray(s.handles)&&s.handles.length===6&&new Set(s.handles).size===6);assert(Array.isArray(s.batches)&&s.batches.length===3);
  assert.equal(s.status,'admitted_recorded_observations');assert.equal(s.sdk,'chia-sdk-driver/test 0.36.0');assert.equal(s.consensus,'chia-consensus 0.36.1');assert.equal(s.component_vm,'clvmr 0.16.2');assert.equal(s.cost_profile,'chia-consensus0.36.1/plain-generator/no-BLS/flags720902/max5500000000');
  const batches=s.batches.map((row,index)=>{assert.equal(row.batch_size,index+1);assert(Array.isArray(row.handles)&&row.handles.length===index+1);money(row.fee_mojos);for(const key of ['plain_generator_bytes','charged_generator_bytes','execution_cost','condition_cost','byte_cost','total_cost'])uint(row[key]);assert.equal(row.plain_generator_bytes-row.charged_generator_bytes,2);assert.equal(row.execution_cost+row.condition_cost+row.byte_cost,row.total_cost);return {...row};});
  assert.deepEqual(s.batches.flatMap(row=>row.handles),s.handles);
  assert.equal(s.licenses.sdk,'Apache-2.0');assert.equal(s.licenses.retained_register_source,'Prosperity-3.0');assert.equal(s.licenses.sdk_driver_archive_sha256,'33d22cc417877a290eff196fe851cfc1175a5432166f8d9bedeb9f55e010bc76');
  assert.deepEqual(s.gate_process_matrix_totals,{accepted:45,exact_typed_consensus_rejections:24,retained_component_vm_runs:204,runtime_umbrellas:12,sdk_attempts:81,separate_cost_passes:45});
  const register=s.canonical_modules.find(row=>row.name==='register');assert(register&&register.bytes===1478&&register.tree_hash==='150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8');
  const downloads=descriptors(input.files,{files:128,total:16*1024*1024,member:2*1024*1024},'report.json');assert(downloads.some(row=>row.path==='report.json'&&row.sha256===input.index_sha256));
  return {indexSHA:input.index_sha256,downloads,batches,scope:s.scope,status:s.status,recipeFingerprint:s.recipe_fingerprint,
    versions:{sdk:s.sdk,consensus:s.consensus,component:s.component_vm,cost:s.cost_profile},canonical:s.canonical_modules,licenses:s.licenses,limitations:s.limitations,sourceFiles:s.source_files,
    counts:{sdk:20,accepted:11,typed:6,runtime:3,component:48,cost:11,hostRefusals:11,names:6},gateProcessMatrix:s.gate_process_matrix_totals};
}
export function projectXchandlesPremine(historySummary,nativeSummary){return {history:projectXchandlesHistory(historySummary),simulator:projectXchandlesSimulator(nativeSummary)};}

// Publication assumes immutable local build directories. realpath plus leaf
// O_NOFOLLOW does not confine concurrent ancestor-directory replacement. Exact
// size/digest checks still bind emitted payload bytes to the admitted inventory.
export function readXchandlesBytes(filename,limit,expected){
  assert(Number.isSafeInteger(limit)&&limit>=0&&limit<=2*1024*1024,'Fixed bounded read role');
  assert.equal(realpathSync(filename),filename,'Artifact ancestors must not be aliases');
  const fd=openSync(filename,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
  try{const before=fstatSync(fd);assert(before.isFile()&&before.nlink===1&&before.size<=limit&&before.size>=0,'Bounded regular artifact');if(expected)assert.equal(before.size,expected.bytes);const buffer=Buffer.alloc(before.size+1);let total=0;while(total<buffer.length){const count=readSync(fd,buffer,total,buffer.length-total,null);if(!count)break;total+=count;}assert.equal(total,before.size);const after=fstatSync(fd);assert.equal(after.size,before.size);const value=buffer.subarray(0,total);if(expected)assert.equal(sha(value),expected.sha256,'Exact admitted artifact bytes');return value;}finally{closeSync(fd);}
}
export async function readXchandlesPremine({sourceRoot,required=false}={}){
  const root=await repository(sourceRoot);if(roots.has(root))return roots.get(root);
  const packets=Object.fromEntries(Object.entries(xchandlesPackets).map(([lane,name])=>[lane,path.join(root,name)]));
  const present=Object.fromEntries(Object.entries(packets).map(([lane,name])=>{try{const info=lstatSync(name);assert(info.isDirectory()&&!info.isSymbolicLink(),'Present packet must be a real directory');return [lane,true];}catch(error){if(error.code==='ENOENT')return [lane,false];throw error;}}));
  // A missing companion may produce the local pending view. Every present
  // packet is still fully admitted, so corruption never becomes "pending".
  const history=present.history?JSON.parse(execFileSync('python3',['-B',path.join(root,'scripts/check-xchandles-history-v2.py'),'check',packets.history],{cwd:root,maxBuffer:2*1024*1024,timeout:180000}).toString()):null;
  const native=present.simulator?JSON.parse(execFileSync('python3',['-B',path.join(root,'scripts/check-xchandles-simulator.py'),'--packet',packets.simulator,'--list-json'],{cwd:root,maxBuffer:2*1024*1024,timeout:180000}).toString()):null;
  if(!history||!native){assert(!required,'Both complete XCHandles packets are required for publication');return null;}
  const value={...projectXchandlesPremine(history,native),sourceRoot:root,packetRoots:packets};roots.set(root,value);return value;
}
export async function readXchandlesArtifact(lane,name,{sourceRoot}={}){
  assert(lane==='history'||lane==='simulator');relative(name);const evidence=await readXchandlesPremine({sourceRoot,required:true});
  const descriptor=evidence[lane].downloads.find(row=>row.path===name);assert(descriptor,'Selected artifact belongs to admitted complete packet');
  const limit=name==='history-packet.json'||name.endsWith('/history-packet.json')||name==='report.json'?512*1024:2*1024*1024;
  return readXchandlesBytes(path.join(evidence.packetRoots[lane],name),limit,descriptor);
}
const plain=html=>html.replace(/<[^>]+>/g,' ').replace(/\s+/g,' ').trim();
function visible(html,name,value){const matches=[...html.matchAll(new RegExp(`<([a-z][a-z0-9]*)\\b[^>]*${name}(?=[\\s=>])(?:="[^"]*")?[^>]*>([\\s\\S]*?)<\\/\\1>`,'g'))];assert.equal(matches.length,1,`Missing or duplicate visible ${name}`);assert.equal(plain(matches[0][2]),String(value),`Wrong visible ${name}`);}
export function checkXchandlesDisplay(html,evidence){
  const text=plain(html);for(const scope of xchandlesScope)assert(text.includes(scope),`Missing XCHandles scope: ${scope}`);
  const h=evidence.history,n=evidence.simulator;
  for(const [name,value]of Object.entries({spends:129,names:3836,primary:h.primaryCost,zeroCreates:11508,zeroFloor:h.conditionalCost.zeroOnly,blockCeiling:h.conditionalCost.pinnedBlockCeiling,historyIndex:h.manifestSHA,nativeIndex:n.indexSHA}))visible(html,`data-xchandles-${name.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())}`,value);
  for(const value of ['clvmr 0.21.0',n.versions.component,n.versions.consensus,'DONT_VALIDATE_SIGNATURE'])assert(text.includes(value),'Visible independent VM/cost profile');
  for(const [name,value]of Object.entries({wholeBlockCost:h.wholeBlockCost,wholeBlockFees:h.wholeBlockFees}))visible(html,`data-xchandles-${name.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())}`,value);
  const scripts=[...html.matchAll(/<script\b[^>]*data-xchandles-step-data(?=[\s=>])[^>]*>([\s\S]*?)<\/script>/g)];assert.equal(scripts.length,1,'One complete local step-inspector payload');const inspector=JSON.parse(scripts[0][1]);assert.deepEqual(inspector.rows,h.rows,'All129 admitted inspector rows, including unselected final step');assert(typeof inspector.downloadBase==='string'&&inspector.downloadBase.endsWith('/reports/xchandles-premine/history/'));
  for(const [name,value]of Object.entries(n.counts))visible(html,`data-xchandles-native-${name}`,value);
  for(const [name,value]of Object.entries({registry:h.identities.registryReveal.module_tree_hash,state:h.identities.registryStateTreeHash,actionRoot:h.identities.allowedActionMerkleRoot,register:h.identities.registerModule.treeHash}))visible(html,`data-xchandles-${name.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())}`,value);
  const batches=[...html.matchAll(/<tr\b[^>]*data-xchandles-batch="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];assert.deepEqual(batches.map(row=>row[1]),['1','2','3']);
  for(const [index,row]of batches.entries()){for(const key of ['plain_generator_bytes','charged_generator_bytes','execution_cost','condition_cost','byte_cost','total_cost','fee_mojos'])visible(row[2],`data-xchandles-batch-${key.replaceAll('_','-')}`,n.batches[index][key]);}
}
