// Registry presentation/transport admission. Python owns the complete independent
// wire/coin/causal reader; these JS checks are not a second whole-protocol proof.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { lstatSync, openSync, fstatSync, readSync, closeSync, opendirSync, constants } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { catTreeHash } from './cat-protocol.mjs';

export const registrySlugs = ['registry-initialize','registry-set-alpha','registry-set-beta'];
export const registryTransport = 'registry-slot-sidecars/v1';
export const registryModes = [['rust_mir',false,'rust-off'],['rust_mir',true,'rust-on'],['chialisp',true,'chialisp'],['rue_source',false,'rue-off'],['rue_source',true,'rue-on']];
export const registryLimits = Object.freeze({ member:2*1024*1024,total:16*1024*1024,files:11,module:1024*1024,moduleInventory:9*1024*1024 });
const root = fileURLToPath(new URL('../../',import.meta.url));
const indexName = 'registry-slot-protocol/index.json';
const sha = value => createHash('sha256').update(value).digest('hex');
const fields = (value,keys) => { assert(value && typeof value === 'object' && !Array.isArray(value)); assert.deepEqual(Object.keys(value).sort(),[...keys].sort()); };
function descriptor(item,name){fields(item,['path','bytes','sha256']);assert.equal(item.path,name);assert(Number.isSafeInteger(item.bytes)&&item.bytes>0&&item.bytes<=registryLimits.member,'Registry member budget before read');assert(typeof item.sha256==='string'&&/^[0-9a-f]{64}$/.test(item.sha256));}
function physical(base,item){let current=base;assert(lstatSync(current).isDirectory()&&!lstatSync(current).isSymbolicLink());for(const part of item.path.split('/')){current=path.join(current,part);assert(!lstatSync(current).isSymbolicLink(),'Registry symlink forbidden');}const info=lstatSync(current);assert(info.isFile()&&info.nlink===1&&info.size===item.bytes,'Registry declared physical size');return current;}
function read(base,item){const fd=openSync(physical(base,item),constants.O_RDONLY|constants.O_NOFOLLOW);try{const info=fstatSync(fd);assert(info.isFile()&&info.nlink===1&&info.size===item.bytes);const buffer=Buffer.alloc(item.bytes+1);let used=0;while(used<buffer.length){const got=readSync(fd,buffer,used,buffer.length-used,null);if(got===0)break;used+=got;}assert.equal(used,item.bytes);const bytes=buffer.subarray(0,used);assert.equal(sha(bytes),item.sha256);return bytes;}finally{closeSync(fd);}}


export function parseRegistryIndex(bytes){
  assert(Buffer.isBuffer(bytes)&&bytes.length>0&&bytes.length<=registryLimits.member,'Bounded registry index bytes');
  let quoted=false,escaped=false,depth=0,tokens=0;
  for(const byte of bytes){
    if(quoted){if(escaped)escaped=false;else if(byte===92)escaped=true;else if(byte===34)quoted=false;}
    else if(byte===34)quoted=true;
    else if(byte===91||byte===123){depth++;tokens++;assert(depth<=128,'Registry index depth bound before parse');assert(tokens<=250000,'Registry index structure bound before parse');}
    else if(byte===93||byte===125)depth--;
    else if(byte===44||byte===58){tokens++;assert(tokens<=250000,'Registry index structure bound before parse');}
  }
  return JSON.parse(bytes.toString('utf8'));
}

export function checkRegistryTransport(report,{sidecarRoot=path.join(root,'corpus')}={}){
  const selected=report.puzzles.filter(p=>registrySlugs.includes(p.slug));
  assert.equal(selected.length,3,'Complete registry triplet required');
  assert.deepEqual(selected.map(p=>p.slug).sort(),[...registrySlugs].sort());
  let indexDescriptor;
  for(const slug of registrySlugs){const p=selected.find(p=>p.slug===slug);assert.equal(p.kind,'protocol_component');assert.equal(p.validated,true);assert.deepEqual(p.simulator,[]);assert.deepEqual(p.simulator_comparisons,[]);assert(!Object.hasOwn(p,'coin_instances'));fields(p.protocol_validation,['protocol','role','index']);assert.equal(p.protocol_validation.protocol,registryTransport);assert.equal(p.protocol_validation.role,slug);descriptor(p.protocol_validation.index,indexName);indexDescriptor??=p.protocol_validation.index;assert.deepEqual(p.protocol_validation.index,indexDescriptor);}
  const index=parseRegistryIndex(read(sidecarRoot,indexDescriptor));fields(index,['schema_version','transport','header','builds']);assert.equal(index.schema_version,1);assert.equal(index.transport,registryTransport);assert(Array.isArray(index.builds)&&index.builds.length===5);
  const inventory=[indexDescriptor],artifacts=[];let total=indexDescriptor.bytes,moduleRemaining=registryLimits.moduleInventory;
  for(const [i,build]of index.builds.entries()){fields(build,['backend','optimize_lir','mode','artifacts','counts','journals']);assert.deepEqual([build.backend,build.optimize_lir,build.mode],registryModes[i]);assert(Array.isArray(build.artifacts)&&build.artifacts.length===3);for(const [j,slug]of registrySlugs.entries()){const row=selected.find(p=>p.slug===slug);assert(Array.isArray(row.artifacts)&&row.artifacts.length===5);const matches=row.artifacts.filter(a=>a.backend===build.backend&&a.optimize_lir===build.optimize_lir);assert.equal(matches.length,1);const a=matches[0];assert(Number.isSafeInteger(a.bytes)&&a.bytes>0&&a.bytes<=registryLimits.module&&a.bytes<=moduleRemaining);moduleRemaining-=a.bytes;assert(typeof a.clvm_hex==='string'&&a.clvm_hex.length===2*a.bytes&&/^(?:[0-9a-f]{2})+$/.test(a.clvm_hex));artifacts.push([slug,a,build.artifacts[j]]);}fields(build.journals,['ordinary','adversarial']);for(const lane of ['ordinary','adversarial']){const item=build.journals[lane];descriptor(item,`registry-slot-protocol/${build.mode}-${lane}.json`);assert(item.bytes<=registryLimits.total-total,'Registry aggregate precharge before journal reads');total+=item.bytes;physical(sidecarRoot,item);inventory.push(item);}}
  const names=inventory.map(v=>v.path).sort();assert.equal(new Set(names).size,registryLimits.files);const directory=opendirSync(path.join(sidecarRoot,'registry-slot-protocol')),physicalNames=[];try{for(let entry;(entry=directory.readSync())!==null;){assert(physicalNames.length<registryLimits.files,'Bounded physical registry inventory');assert(entry.isFile()&&!entry.isSymbolicLink());physicalNames.push(`registry-slot-protocol/${entry.name}`);}}finally{directory.closeSync();}assert.deepEqual(physicalNames.sort(),names);
  // All inventory/lengths are admitted before reading journals or hashing code.
  for(const [slug,a,claimed]of artifacts){const bytes=Buffer.from(a.clvm_hex,'hex'),treeHash=catTreeHash(a.clvm_hex);assert.equal(a.tree_hash,treeHash);assert.deepEqual(claimed,{slug,bytes:a.bytes,blob_sha256:sha(bytes),module_tree_hash:treeHash});}
  for(const item of inventory.slice(1))read(sidecarRoot,item);
  return {index,files:names,bytes:total,indexSHA:indexDescriptor.sha256,sidecarRoot};
}

export function checkRegistryReport(report,options={}){
  const transport=checkRegistryTransport(report,options),input=JSON.stringify(report);
  assert(Buffer.byteLength(input)<=registryLimits.total,'Bounded registry main report');
  const admitted=JSON.parse(execFileSync('python3',['-B','scripts/registry_protocol.py','--stdin','--sidecar-root',transport.sidecarRoot],{cwd:root,input,encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:120000,maxBuffer:256*1024,env:{...process.env,PYTHONDONTWRITEBYTECODE:'1'}}));
  assert.equal(admitted.protocol,registryTransport);assert.equal(admitted.status,'recorded_consistency_checked');assert.equal(admitted.files,registryLimits.files);assert.equal(admitted.bytes,transport.bytes);assert.equal(admitted.index_sha256,transport.indexSHA);
  return admitted;
}

export function checkRegistryComponent(puzzle,report,options={}){
  assert(report&&Array.isArray(report.puzzles),'Registry component requires its complete report/triplet context');
  assert(registrySlugs.includes(puzzle.slug));const matches=report.puzzles.filter(p=>p.slug===puzzle.slug);assert.equal(matches.length,1);assert.deepEqual(matches[0],puzzle);return checkRegistryReport(report,options);
}
