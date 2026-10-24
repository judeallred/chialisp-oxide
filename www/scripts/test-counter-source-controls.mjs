// Metadata/display and exact physical-download checks; no VM/SDK execution.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import path from 'node:path';
import {
  boundedCounterRead, counterPacketProtocol, counterPacketProfile, counterPacketScope,
  counterControlCounts, checkCounterPacketIndex, readCounterSourceControls,
  readCounterCanonicalModules, projectCounterSdkWitnesses, counterControlsCorruptions,
  checkCounterControlsDisplay, counterPhysicalNames,
} from '../src/lib/counter-source-controls.mjs';

let built=false,historicalReport,historicalArtifacts;
for(let index=2;index<process.argv.length;index++){
  const argument=process.argv[index];
  if(argument==='--built'){assert(!built);built=true;}
  else if(argument==='--historical-report'){assert(historicalReport===undefined&&index+1<process.argv.length);historicalReport=process.argv[++index];}
  else if(argument==='--historical-artifacts'){assert(historicalArtifacts===undefined&&index+1<process.argv.length);historicalArtifacts=process.argv[++index];}
  else assert.fail(`Unknown argument ${argument}`);
}
assert(!!historicalReport===!!historicalArtifacts,'Both explicit historical input paths are required');
assert(!(built&&historicalReport),'Historical inputs cannot establish built/current packet evidence');
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');

function indexFixture(){return{protocol:counterPacketProtocol,profile:{...counterPacketProfile},origin_raw:'/constructed/raw',origin_protocol:'/constructed/protocol',origin_root:'/constructed/root',files:[{path:'raw/item.json',bytes:2,sha256:'00'.repeat(32)}],aliases:{'protocol/repeat.json':'raw/item.json'},scope:counterPacketScope};}
const positive=indexFixture();checkCounterPacketIndex(positive,Buffer.from(JSON.stringify(positive)));
let profileControls=0;
for(const mutate of [
  value=>{value.profile.max_files=true;},value=>{value.profile.max_total_bytes=32*1024*1024;},
  value=>{value.files[0].path='../escape';},value=>{value.files.push({...value.files[0]});},
  value=>{value.files[0].bytes=counterPacketProfile.max_file_bytes+1;},value=>{value.files[0].bytes=true;},
  value=>{value.aliases['raw/item.json']='raw/item.json';},value=>{value.aliases['protocol/repeat.json']='missing.json';},
  value=>{value.aliases['raw/bundle-off/repeat']='raw/item.json';},
]){const index=indexFixture();mutate(index);assert.throws(()=>checkCounterPacketIndex(index,Buffer.from(JSON.stringify(index))));profileControls++;}

function metadataControls(report,artifacts,context){const base=projectCounterSdkWitnesses(report,artifacts,context);for(const [name,mutate]of counterControlsCorruptions){const bad=structuredClone(report);mutate(bad);assert.throws(()=>projectCounterSdkWitnesses(bad,artifacts,context),undefined,`Accepted source-control metadata corruption: ${name}`);}return base;}
if(historicalReport){
  const report=JSON.parse(await boundedCounterRead(path.resolve(historicalReport))),artifactBytes=await boundedCounterRead(path.resolve(historicalArtifacts));
  metadataControls(report,JSON.parse(artifactBytes),{artifactBytes,modules:await readCounterCanonicalModules()});
  console.log(`PASS ${counterControlsCorruptions.length} SDK witness controls and ${profileControls} constructed index/profile controls on explicit historical inputs only. Current portable admission, native/raw projection, displayed views and built downloads remain untested; no new execution credit.`);
  process.exit(0);
}

const evidence=await readCounterSourceControls();
assert(evidence,'Current portable counter packet missing; historical inputs require explicit flags and add no public current evidence');
assert.deepEqual(evidence.counts,counterControlCounts);
metadataControls(evidence.report,evidence.artifacts,{artifactBytes:evidence.artifactBytes,modules:evidence.modules});

function constructedMarkup(){
  const spans=Object.entries(evidence.counts).map(([key,value])=>`<span data-counter-controls-${key.replace(/[A-Z]/g,letter=>'-'+letter.toLowerCase())}>${value}</span>`).join(' ');
  const scope='not ready for real funds; separate from the paired main corpus; Python packet gate; not execution authentication; registry and slot work remains pending; not a BLS proof; not compiler preservation. 37 SDK submissions: 16 accepted, 15 exact typed consensus rejections and six runtime umbrellas.';
  const builds=evidence.builds.map(build=>`<details data-counter-packet-build="${build.mode}"><summary>${({off:'Rust / off',on:'Rust / on',chialisp:'Chialisp','rue-off':'Rue / off','rue-on':'Rue / on'})[build.mode]} · 37 SDK submissions</summary>${Object.entries({increment:build.incrementHash,reset:build.resetHash,root:build.merkle_root,initial:build.increment_child.coin_id}).map(([key,value])=>`<code data-counter-packet-${key}>${value}</code>`).join('')}${build.orders.map(order=>`<article data-counter-packet-order="${order.name}">${Object.entries({initial:1,next:order.next_state,continued:order.continued_state,child:order.child.coin_id,continuation:order.continued_child.coin_id}).map(([key,value])=>`<code data-counter-order-${key}>${value}</code>`).join('')}</article>`).join('')}<ol>${build.all.map(a=>`<li data-counter-packet-attempt="${a.id}"><code data-counter-packet-name>${a.id}</code><span data-counter-packet-status>${a.outcome.status}</span>${a.outcome.code?`<code data-counter-packet-code>${a.outcome.code}</code><span data-counter-packet-stage>${a.outcome.stage}</span>`:''}</li>`).join('')}</ol></details>`).join('');
  const mutants=evidence.mutants.map(m=>`<article data-counter-packet-mutant="${m.control}/${m.mode}">accepted unsigned transition${Object.entries({module:m.moduleHash,companion:m.companionHash,root:m.merkle_root,eve:m.eve.coin_id,child:m.child.coin_id,initial:5,observed:m.observed_state,expected:m.original_policy_expected_state,signature:m.unsignedSignature,'original-signature':m.original_policy_requires_signature?'yes':'no'}).map(([key,value])=>`<code data-counter-mutant-${key}>${value}</code>`).join('')}</article>`).join('');
  return `<section data-counter-source-controls>${spans}<p>${scope}</p><p>${evidence.primary.version};${evidence.sdk.vm};${evidence.sdk.consensus};${evidence.sdk.network}</p>${builds}${mutants}<code data-counter-packet-sha>${evidence.packetSHA}</code><a href="/chialisp-oxide/reports/counter-source-controls/report.json">exact index</a></section>`;
}
const markup=constructedMarkup();checkCounterControlsDisplay(markup,evidence);
const visible=(attribute,replacement)=>text=>text.replace(new RegExp(`(<[a-z]+\\b[^>]*${attribute}(?:="[^"]*")?[^>]*>)[^<]+`),(_,prefix)=>prefix+replacement);
const displayChanges=[
  visible('data-counter-controls-attempts','194'),visible('data-counter-controls-native-calls','297'),visible('data-counter-controls-native-adapters','0'),
  visible('data-counter-controls-raw','693'),visible('data-counter-order-next','9'),visible('data-counter-order-child','00'.repeat(32)),
  visible('data-counter-mutant-observed','9'),visible('data-counter-mutant-expected','5'),visible('data-counter-mutant-root','00'.repeat(32)),
  visible('data-counter-mutant-signature','ab'.repeat(96)),visible('data-counter-packet-status','changed'),visible('data-counter-packet-code','CostExceeded'),
  visible('data-counter-packet-stage','accepted'),text=>text.replace('Rust / off','Other / off'),text=>text.replace('clvmr 0.16.2','clvmr 0.21.0'),
];
for(const mutate of displayChanges){const bad=mutate(markup);assert.notEqual(bad,markup,'Inert display corruption');assert.throws(()=>checkCounterControlsDisplay(bad,evidence));}


if(built){
  for(const route of ['tutorials/action-counter','lab/counter-increment','lab/counter-reset'])checkCounterControlsDisplay((await boundedCounterRead(path.join(evidence.sourceRoot,'www/dist',route,'index.html'),16*1024*1024)).toString('utf8'),evidence);
  const base=path.join(evidence.sourceRoot,'www/dist/reports/counter-source-controls');
  assert.deepEqual(await counterPhysicalNames(base),evidence.physicalNames,'Built counter packet has missing payloads or extra physical alias files');
  for(const name of evidence.physicalNames){const expected=evidence.payloads.get(name),actual=await boundedCounterRead(path.join(base,name),expected.length);assert(actual.equals(expected),`Built counter packet bytes differ: ${name}`);assert.equal(sha(actual),sha(expected));}
}
console.log(`PASS counter packet ${counterControlsCorruptions.length} SDK witness controls, ${profileControls} index/profile controls, ${displayChanges.length} visible-witness controls${built?`, three built views and ${evidence.physicalNames.length} exact physical downloads; aliases remain index-only`:'. Constructed HTML only; actual built views/downloads not checked'}. Recorded readers add no execution credit.`);
