// Presentation follows the complete current-source Python packet gate.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { constants } from 'node:fs';
import { lstat, open, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { repository } from './p2-costs.mjs';

export const registryControlPacket = 'corpus/registry-source-controls';
export const registryControlCounts = Object.freeze({groups:8,setup_sdk_attempts:16,setup_primary_observations:24,mutation_sdk_attempts:8,mutation_primary_observations:12,accepted_sdk_attempts:24});
const profile = {files:1024,member:2*1024*1024,total:16*1024*1024};
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const names = ['alpha-drop-send','alpha-drop-create','beta-drop-send','beta-drop-create'];
function relative(value) { assert(typeof value==='string'&&value.length>0&&value.length<=512&&!value.includes('\\')&&!path.isAbsolute(value)&&value.split('/').every(part=>part&&part!=='.'&&part!=='..'));return value; }
async function read(filename,limit) {
  assert.equal(await realpath(filename),filename,'Registry control payload must not follow aliases');
  const file=await open(filename,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
  try {const info=await file.stat();assert(info.isFile()&&info.nlink===1&&info.size<=limit);const bytes=Buffer.alloc(info.size+1);let size=0;while(size<bytes.length){const result=await file.read(bytes,size,bytes.length-size,null);if(!result.bytesRead)break;size+=result.bytesRead;}assert.equal(size,info.size);return bytes.subarray(0,size);}finally{await file.close();}
}

export function projectRegistryControls(summary) {
  assert.deepEqual(summary.counts,registryControlCounts);
  assert.equal(summary.component_vm,'clvmr 0.21.0 MEMPOOL_MODE');assert.equal(summary.component_failure_protocol,'clvmr-0.21.0/v1');
  assert.deepEqual(summary.native_counts,{baseline_policy_calls:4,variant_policy_calls:4,abi_observations:0,VM_executions:0,SDK_submissions:0});
  assert.equal(summary.repeat_byte_identical,true);
  assert.equal(summary.templates.length,8);
  assert.deepEqual(summary.templates.map(row=>{assert.equal(typeof row.optimize_lir,'boolean');return `${row.name}/${row.optimize_lir}`;}).sort(),names.flatMap(name=>[`${name}/false`,`${name}/true`]).sort());
  const groups=summary.sdk_capture.record.groups;
  assert.equal(groups.length,8);
  const builds=groups.map((group,index)=>{
    const name=names[Math.floor(index/2)],mode=index%2?'on':'off';
    assert.equal(`${group.side}-${group.kind.replaceAll('_','-')}`,name);
    assert.equal(group.optimize_lir,index%2===1);
    assert.equal(group.entry,`chialisp_oxide_registry_controls::${name.replaceAll('-','_')}::puzzle`);
    const template=summary.templates.find(row=>row.name===name&&row.optimize_lir===group.optimize_lir);
    assert(template&&template.execution.outcome.status==='accepted');
    assert.equal(template.artifact.tree_hash,group.artifact.module_tree_hash);
    const p=group.postconditions;
    for(const value of [group.artifact.module_tree_hash,group.policy.merkle_root,group.policy.old_slot.coin.coin_id,group.policy.peer_slot.coin.coin_id,p.replacement_puzzle_hash,p.controller_children[0].coin_id])assert(typeof value==='string'&&/^[0-9a-f]{64}$/.test(value));
    assert.equal(p.old_slot_spent,group.kind==='drop_create');assert.equal(p.peer_slot_unspent,true);
    assert.equal(p.replacement_child_count,group.kind==='drop_send'?1:0);
    return {id:`${name}-${mode}`,name,mode,entry:group.entry,artifact:group.artifact,fields:{
      root:group.policy.merkle_root,old_slot:group.policy.old_slot.coin.coin_id,
      peer_slot:group.policy.peer_slot.coin.coin_id,old_slot_spent:String(p.old_slot_spent),
      peer_slot_unspent:String(p.peer_slot_unspent),replacement_count:p.replacement_child_count,
      replacement_hash:p.replacement_puzzle_hash,controller_child:p.controller_children[0].coin_id,
    }};
  });
  assert(/^[0-9a-f]{64}$/.test(summary.index_sha256));
  return {packetSHA:summary.index_sha256,counts:summary.counts,builds,downloads:summary.downloads,
    componentVm:summary.component_vm,componentFailureProtocol:summary.component_failure_protocol,primary:summary.sdk_capture.vm,consensus:summary.sdk_capture.consensus,network:summary.sdk_capture.network};
}

const cached=new Map();
export async function readRegistrySourceControls(sourceRoot) {
  const root=await repository(sourceRoot);
  if(cached.has(root))return cached.get(root);
  const base=path.join(root,registryControlPacket);
  try {await lstat(base);}catch(error){if(error.code==='ENOENT')return null;throw error;}
  const summary=JSON.parse(execFileSync('python3',['-B',path.join(root,'scripts/check-registry-source-controls.py'),'--packet',base,'--report',path.join(root,'corpus/validation.json'),'--json'],{cwd:root,maxBuffer:2*1024*1024,timeout:180000}).toString());
  const evidence=projectRegistryControls(summary),payloads=new Map();
  assert(Array.isArray(evidence.downloads)&&evidence.downloads.length<=profile.files);
  let remaining=profile.total;const declared=new Set();
  // Precharge the complete declared transport before the first payload read.
  for(const row of evidence.downloads){relative(row.path);assert(!declared.has(row.path));declared.add(row.path);assert(Number.isSafeInteger(row.bytes)&&row.bytes>=0&&row.bytes<=profile.member&&row.bytes<=remaining);remaining-=row.bytes;assert(/^[0-9a-f]{64}$/.test(row.sha256));}
  assert(declared.has('report.json'));
  for(const row of evidence.downloads){const bytes=await read(path.join(base,row.path),row.bytes);assert.equal(bytes.length,row.bytes);assert.equal(sha(bytes),row.sha256);payloads.set(row.path,bytes);}
  assert.equal(sha(payloads.get('report.json')),evidence.packetSHA);
  const value={...evidence,payloads};cached.set(root,value);return value;
}

const plain=html=>html.replace(/<[^>]+>/g,' ').replace(/\s+/g,' ').trim();
function visible(html,attribute,value){const matches=[...html.matchAll(new RegExp(`<([a-z][a-z0-9]*)\\b[^>]*${attribute}(?=[\\s=>])(?:="[^"]*")?[^>]*>([\\s\\S]*?)<\\/\\1>`,'g'))];assert.equal(matches.length,1,`Missing/duplicate visible ${attribute}`);assert.equal(plain(matches[0][2]),String(value),`Wrong visible ${attribute}`);}
export function checkRegistryControlsDisplay(html,evidence) {
  const section=html.match(/<section\b[^>]*data-registry-source-controls(?:\s|=|>)[^>]*>[\s\S]*?<\/section>/)?.[0];assert(section,'Missing registry Rust mutation evidence');
  visible(section,'data-registry-controls-index',evidence.packetSHA);
  for(const [name,count]of Object.entries(evidence.counts))visible(section,`data-registry-controls-${name.replaceAll('_','-')}`,count);
  const rows=[...section.matchAll(/<article\b[^>]*data-registry-control="([^"]+)"[^>]*>([\s\S]*?)<\/article>/g)];assert.deepEqual(rows.map(row=>row[1]),evidence.builds.map(row=>row.id));
  for(const [index,row]of rows.entries()){const build=evidence.builds[index];visible(row[2],'data-registry-control-entry',build.entry);visible(row[2],'data-registry-control-mode',build.mode);visible(row[2],'data-registry-control-artifact',build.artifact.module_tree_hash);for(const [name,value]of Object.entries(build.fields))visible(row[2],`data-registry-control-${name.replaceAll('_','-')}`,value);}
  const text=plain(section);for(const version of [evidence.componentVm,evidence.componentFailureProtocol,evidence.primary,evidence.consensus,evidence.network])assert(text.includes(version),'Missing visible registry control version');for(const phrase of ['not ready for real funds','four baseline and four mutant native calls','eight retained template VM executions','funded under its own mutant root','repeat adds no coverage','not execution authentication','not compiler preservation'])assert(text.includes(phrase),`Missing registry control scope: ${phrase}`);
}
