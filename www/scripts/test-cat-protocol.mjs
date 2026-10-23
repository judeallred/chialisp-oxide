import assert from 'node:assert/strict';
import {lstat,open,readFile,realpath} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {catSlug,catProtocol,catModes,catScenarios,catTail,catModule,catTreeHash,catCurry,catEmptySignature,checkCatComponent,checkCatDisplay} from './cat-protocol.mjs';
const args=new Set(process.argv.slice(2));assert([...args].every(arg=>['--constructed','--built'].includes(arg))&&!(args.has('--constructed')&&args.has('--built')));
const constructed=args.has('--constructed'),built=args.has('--built');
const tail=(await readFile(new URL('../../experiments/cat-genesis-tail/upstream/genesis_by_coin_id.clsp.hex',import.meta.url),'utf8')).trim();
const cat=(await readFile(new URL('../../experiments/cat-genesis-tail/upstream/cat_v2.clsp.hex',import.meta.url),'utf8')).trim();
function fixture(){
 // Constructed metadata/wire-hash unit data only. No executed conditions,
 // compiler/VM/SDK/native outputs or corpus publication is claimed here.
 const modules=['01','02',tail,'03','04'];
 const artifacts=catModes.map(([backend,optimize_lir],i)=>({backend,optimize_lir,bytes:modules[i].length/2,clvm_hex:modules[i],tree_hash:catTreeHash(modules[i]),disassembly:'Constructed fixture'}));
 const builds=artifacts.map(a=>{
  const owner='05',recipient='06';const coin=(id,parent,reveal)=>({coin_id:id.repeat(32),parent_coin_id:parent.repeat(32),puzzle_hash:catTreeHash(reveal),amount:1000});
  const genesis=coin('11','10',owner),eve=coin('12','11',owner),issued=coin('13','12',owner),received=coin('14','13',recipient),continuation=coin('15','14',recipient);
  const tail_reveal_hex=catCurry(a.clvm_hex,genesis.coin_id);
  const attempts=catScenarios.map(([name,status,code],i)=>{
   const selected=[0,14].includes(i)?genesis:i<=8?eve:i<=11?issued:received;
   const reveal=selected===received?recipient:owner;
   const signature_hex=i===1?catEmptySignature:(i===12?'cd':'ab').repeat(96);
   const outcome=status==='accepted'?{status}:status==='consensus_rejected'?{status,code,full_simulator_equality:true}:{status,code:'GeneratorRuntimeError',semantic_rejection_credit:false,primary_program_code:code,full_simulator_equality:true};
   return{name,transaction:{signature_hex,spends:[{coin:selected,reveal_hex:reveal,solution_hex:'80'}]},primary_vm:[status==='runtime_umbrella'?{outcome:{status:'rejected'},cost:null,failure:{kind:'program',code}}:{outcome:{status:'accepted',output_hex:'80'},cost:1,failure:null}],outcome};
  });
  return{backend:a.backend,optimize_lir:a.optimize_lir,module_tree_hash:a.tree_hash,lifecycle:{asset_id:catTreeHash(tail_reveal_hex),tail_reveal_hex,owner_inner_reveal_hex:owner,recipient_inner_reveal_hex:recipient,owner_cat_reveal_hex:owner,recipient_cat_reveal_hex:recipient,genesis,eve,issued,received,continuation,attempts}};
 });
 return{slug:catSlug,kind:'protocol_component',environment:'raw',validated:true,simulator:[],simulator_comparisons:[],artifacts,protocol_validation:{schema_version:1,protocol:catProtocol,status:'validated',sdk:'chia-sdk-test 0.36.0',consensus:'chia-consensus 0.36.1',sdk_vm:'clvmr 0.16.2',network:'TESTNET11',primary_vm:'clvmr 0.21.0',primary_protocol:'clvmr-0.21.0/v1',primary_flags:535,primary_max_cost:250000000,sdk_max_block_cost_clvm:11000000000,sdk_transaction_max_cost:5500000000,cat_module_hex:cat,cat_module_tree_hash:catModule,canonical_tail_hex:tail,canonical_tail_tree_hash:catTail,owner_public_key:'22'.repeat(48),recipient_public_key:'33'.repeat(48),builds,sdk_transaction_attempts:75,accepted:20,typed_rejected:35,runtime_umbrellas:20,component_primary_vm_runs:75,scope:'Constructed unit data, no execution credit.'}};
}
const report=constructed?null:JSON.parse(await readFile(new URL('../../corpus/validation.json',import.meta.url)));
const puzzle=constructed?fixture():report.puzzles.find(p=>p.slug===catSlug);assert(puzzle,'Actual paired CAT capture required; use --constructed for isolated unit controls only');
assert.deepEqual(checkCatComponent(puzzle),{groups:5,attempts:75,accepted:20,rejected:35,runtime_umbrellas:20,vm:75});
const mutations=[
 p=>{p.kind='pure_function';},p=>{p.protocol_validation.schema_version=true;},p=>{p.protocol_validation.primary_flags=0;},
 p=>{p.protocol_validation.sdk_vm='clvmr 0.21.0';},p=>{p.protocol_validation.builds.pop();},p=>{p.protocol_validation.sdk_transaction_attempts=76;},
 p=>{p.protocol_validation.builds[0].module_tree_hash=catTail;},p=>{p.protocol_validation.builds[0].optimize_lir=0;},
 p=>{p.protocol_validation.builds[0].lifecycle.asset_id=catTail;},p=>{p.protocol_validation.builds[0].lifecycle.attempts.pop();},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[1].outcome.full_simulator_equality=false;},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[1].outcome.full_simulator_equality=1;},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[5].outcome.full_simulator_equality=1;},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[5].outcome.semantic_rejection_credit=0;},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[1].outcome.code='DoubleSpend';},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[5].outcome.semantic_rejection_credit=true;},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[5].primary_vm[0].failure.kind='resource';},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[12].transaction.signature_hex=catEmptySignature;},
 p=>{p.protocol_validation.builds[0].lifecycle.received.parent_coin_id='00'.repeat(32);},
 p=>{p.protocol_validation.builds[0].lifecycle.attempts[14].transaction.signature_hex='ef'.repeat(96);},
];
for(const mutate of mutations){const bad=structuredClone(puzzle);mutate(bad);assert.throws(()=>checkCatComponent(bad));}
for(const bad of ['8080','8101','ff'.repeat(512)+'80'.repeat(513),'ff80'.repeat(32768)+'80'])assert.throws(()=>catTreeHash(bad));
function markup(){return`<section id="cat-genesis-lifecycle" data-cat-protocol="${catProtocol}"><p><strong data-cat-attempts>75</strong> actual SDK submissions across <strong data-cat-groups>5</strong> builds: <span data-cat-accepted>20</span> accepted, <span data-cat-rejected>35</span> exact typed consensus rejections and <span data-cat-umbrellas>20</span> runtime umbrellas. <span data-cat-component-runs>75</span> component runs;live and Sage pending;clvmr 0.21.0;clvmr 0.16.2</p>${puzzle.protocol_validation.builds.map(b=>`<details data-cat-build="${b.backend}/${b.optimize_lir}"><p><code data-cat-module>${b.module_tree_hash}</code><code data-cat-asset>${b.lifecycle.asset_id}</code></p><ol>${b.lifecycle.attempts.map(a=>`<li data-cat-attempt="${a.name}"><code>${a.name}</code>: <span data-cat-status>${a.outcome.status}</span>${a.outcome.code?`<code data-cat-code>${a.outcome.code}</code>`:''}</li>`).join('')}</ol></details>`).join('')}<a href="/chialisp-oxide/reports/cat-genesis-by-coin-id.json">exact evidence</a></section>`;}
checkCatDisplay(markup(),puzzle);
const displayChanges=[['>75</strong> actual SDK submissions','>76</strong> actual SDK submissions'],['>35</span> exact typed consensus rejections','>0</span> exact typed consensus rejections'],['20</span> runtime umbrellas','20</span> typed rejections'],['Sage pending','Sage complete'],['data-cat-groups>5','data-cat-groups>6'],[puzzle.protocol_validation.builds[0].module_tree_hash,'00'.repeat(32)],['data-cat-status>consensus_rejected','data-cat-status>accepted'],['data-cat-code>BadAggregateSignature','data-cat-code>CostExceeded'],['<code>eve_empty_owner_signature</code>','<code>removed witness</code>']];
for(const [before,after]of displayChanges)assert.throws(()=>checkCatDisplay(markup().replace(before,after),puzzle));

async function boundedPacketBytes(filename,limit){
 const before=await lstat(filename);
 assert(before.isFile()&&!before.isSymbolicLink()&&before.nlink===1&&await realpath(filename)===filename,'Regular unaliased CAT download required');
 const file=await open(filename,'r');
 try{
  const stat=await file.stat();
  assert(stat.isFile()&&stat.nlink===1&&stat.size===before.size&&stat.size<=limit,'CAT download exceeds declared byte bound');
  const bytes=Buffer.alloc(stat.size+1);let length=0;
  while(length<bytes.length){const {bytesRead}=await file.read(bytes,length,bytes.length-length,null);if(!bytesRead)break;length+=bytesRead;}
  assert.equal(length,stat.size,'CAT download changed during bounded read');
  return bytes.subarray(0,length);
 }finally{await file.close();}
}
async function checkBuiltPacket(){
 const root=await realpath(fileURLToPath(new URL('../../',import.meta.url)));
 const source=path.join(root,'corpus/cat-genesis-source-controls');
 const output=path.join(root,'www/dist/reports/cat-source-controls');
 const maximum=2*1024*1024,totalMaximum=8*1024*1024;
 const indexPath=path.join(source,'report.json');
 const indexBytes=await boundedPacketBytes(indexPath,maximum);
 const admitted=JSON.parse(execFileSync('python3',['-B','scripts/check-cat-source-controls.py','--list-json'],{cwd:root,encoding:'utf8',timeout:120000,maxBuffer:64*1024}));
 assert.deepEqual(await boundedPacketBytes(indexPath,indexBytes.length),indexBytes,'CAT index changed across independent admission');
 const index=JSON.parse(indexBytes);
 assert(Array.isArray(index.files)&&index.files.length<=128&&Array.isArray(admitted.files),'Bounded admitted CAT packet inventory required');
 const names=['report.json',...index.files.map(row=>row.path)].sort();
 assert.equal(new Set(names).size,names.length,'Duplicate CAT packet download');
 assert.deepEqual([...admitted.files].sort(),names,'Built CAT inventory differs from independently admitted packet');
 let reserved=indexBytes.length;
 for(const row of index.files){
  assert(typeof row.path==='string'&&row.path.length<=512&&/^[A-Za-z0-9_./-]+$/.test(row.path)&&row.path.split('/').every(part=>part&&part!=='.'&&part!=='..'),'Confined CAT packet download path required');
  assert(Number.isSafeInteger(row.bytes)&&row.bytes>=0&&row.bytes<=maximum&&row.bytes<=totalMaximum-reserved,'CAT packet aggregate byte reservation exceeded');
  assert(typeof row.sha256==='string'&&/^[a-f0-9]{64}$/.test(row.sha256),'Exact CAT download digest required');
  reserved+=row.bytes;
 }
 assert.deepEqual(await boundedPacketBytes(path.join(output,'report.json'),indexBytes.length),indexBytes,'Built CAT index differs from exact admitted corpus bytes');
 for(const row of index.files){
  const original=await boundedPacketBytes(path.join(source,row.path),row.bytes);
  assert.equal(original.length,row.bytes);
  assert.equal(createHash('sha256').update(original).digest('hex'),row.sha256,'CAT source download differs from admitted identity');
  const published=await boundedPacketBytes(path.join(output,row.path),row.bytes);
  assert.deepEqual(published,original,`Built CAT packet download differs: ${row.path}`);
 }
 assert.deepEqual(await boundedPacketBytes(indexPath,indexBytes.length),indexBytes,'CAT index changed during built download checks');
 return names.length;
}
let packetDownloads=0;
if(built){for(const route of ['tutorials/cat-genesis-tail',`lab/${catSlug}`])checkCatDisplay(await readFile(new URL(`../dist/${route}/index.html`,import.meta.url),'utf8'),puzzle);assert.deepEqual(JSON.parse(await readFile(new URL(`../dist/reports/${catSlug}.json`,import.meta.url))),puzzle.protocol_validation);packetDownloads=await checkBuiltPacket();}
console.log(`PASS CAT ${mutations.length} metadata/wire controls,4 codec controls,9 display controls${built?`,${packetDownloads} exact admitted packet downloads`:''}${constructed?'; constructed unit data only, no execution credit':''}`);
