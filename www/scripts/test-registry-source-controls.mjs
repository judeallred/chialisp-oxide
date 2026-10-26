// Recorded projection/display consistency and constructed corruption controls.
// These checks execute no VM or SDK policy; complete Python packet admission is separate.
import assert from 'node:assert/strict';
import { constants, closeSync, fstatSync, openSync, readSync } from 'node:fs';
import path from 'node:path';
import { projectRegistryControls, readRegistrySourceControls, checkRegistryControlsDisplay } from '../src/lib/registry-source-controls.mjs';
import { registryHTMLLimit } from '../src/lib/registry-protocol.mjs';

const routes=['tutorials/action-registry','lab/registry-initialize','lab/registry-set-alpha','lab/registry-set-beta'];
let summaryPath,built;
for(let index=2;index<process.argv.length;index++){
  const flag=process.argv[index];
  assert((flag==='--summary'&&summaryPath===undefined)||(flag==='--built'&&built===undefined),'Expected unique --summary PATH or --built DIST');
  assert(index+1<process.argv.length&&!process.argv[index+1].startsWith('--'),'Option requires a path');
  if(flag==='--summary')summaryPath=path.resolve(process.argv[++index]);else built=path.resolve(process.argv[++index]);
}
assert(!(summaryPath&&built),'Built publication checks must use the current packet, never an alternate summary');
function readBounded(filename,limit,allowEmpty=false){
  const descriptor=openSync(filename,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
  try{
    const info=fstatSync(descriptor);assert(info.isFile()&&(allowEmpty||info.size>0)&&info.size<=limit,'Bounded regular presentation input');
    const bytes=Buffer.alloc(info.size+1);let total=0;
    while(total<bytes.length){const size=readSync(descriptor,bytes,total,bytes.length-total,null);if(!size)break;total+=size;}
    assert.equal(total,info.size,'Presentation input changed during bounded read');return bytes.subarray(0,total);
  }finally{closeSync(descriptor);}
}
let summary,evidence;
if(summaryPath){
  summary=JSON.parse(readBounded(summaryPath,2*1024*1024).toString('utf8'));
  evidence=projectRegistryControls(summary);
}else{
  evidence=await readRegistrySourceControls();
  assert(evidence,'Current registry source-control packet is missing: pending local UI cannot pass this gate');
}
const expectedIDs=['alpha-drop-send','alpha-drop-create','beta-drop-send','beta-drop-create'].flatMap(name=>[`${name}-off`,`${name}-on`]);
assert.deepEqual(evidence.builds.map(build=>build.id),expectedIDs);
let projectionControls=0;
if(summary){
  const projectionMutations=[
    ['component VM profile',s=>s.component_vm='clvmr 0.16.2'],
    ['component VM failure protocol',s=>s.component_failure_protocol='other'],
    ['group count',s=>s.counts.groups=7],
    ['setup SDK count',s=>s.counts.setup_sdk_attempts=17],
    ['setup primary count',s=>s.counts.setup_primary_observations=0],
    ['mutation SDK count',s=>s.counts.mutation_sdk_attempts=false],
    ['mutation primary count',s=>s.counts.mutation_primary_observations=24],
    ['total SDK count',s=>s.counts.accepted_sdk_attempts=36],
    ['native baseline count',s=>s.native_counts.baseline_policy_calls=0],
    ['native mutant count',s=>s.native_counts.variant_policy_calls=8],
    ['native SDK miscredit',s=>s.native_counts.SDK_submissions=1],
    ['repeat mismatch',s=>s.repeat_byte_identical=false],
    ['missing template',s=>s.templates.pop()],
    ['duplicate late template',s=>s.templates[7]=structuredClone(s.templates[0])],
    ['late template mode type',s=>s.templates[7].optimize_lir=1],
    ['late template rejection',s=>s.templates[7].execution.outcome.status='rejected'],
    ['late template artifact',s=>s.templates[7].artifact.tree_hash='00'.repeat(32)],
    ['missing SDK group',s=>s.sdk_capture.record.groups.pop()],
    ['late SDK mode',s=>s.sdk_capture.record.groups[7].optimize_lir=false],
    ['SDK group order',s=>[s.sdk_capture.record.groups[0],s.sdk_capture.record.groups[1]]=[s.sdk_capture.record.groups[1],s.sdk_capture.record.groups[0]]],
    ['late SDK side',s=>s.sdk_capture.record.groups[7].side='alpha'],
    ['late SDK entry',s=>s.sdk_capture.record.groups[7].entry='chialisp_oxide_registry_controls::beta_drop_send::puzzle'],
    ['late old-slot state',s=>s.sdk_capture.record.groups[7].postconditions.old_slot_spent=false],
    ['late peer-slot state',s=>s.sdk_capture.record.groups[7].postconditions.peer_slot_unspent=false],
    ['late replacement count',s=>s.sdk_capture.record.groups[7].postconditions.replacement_child_count=1],
    ['late root shape',s=>s.sdk_capture.record.groups[7].policy.merkle_root='bad'],
    ['late slot identity shape',s=>s.sdk_capture.record.groups[7].policy.old_slot.coin.coin_id='bad'],
    ['late replacement identity shape',s=>s.sdk_capture.record.groups[7].postconditions.replacement_puzzle_hash='bad'],
    ['late child identity shape',s=>s.sdk_capture.record.groups[7].postconditions.controller_children[0].coin_id='bad'],
    ['index digest shape',s=>s.index_sha256='bad'],
  ];
  for(const [name,mutate]of projectionMutations){
    const changed=structuredClone(summary);mutate(changed);
    assert.throws(()=>projectRegistryControls(changed),undefined,`Accepted registry source-control projection corruption: ${name}`);projectionControls++;
  }
}
const scopes=['not ready for real funds','four baseline and four mutant native calls','eight retained template VM executions','funded under its own mutant root','repeat adds no coverage','not execution authentication','not compiler preservation'];
const escape=value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const attribute=name=>name.replaceAll('_','-');
const field=(name,value)=>`<code ${name}>${escape(value)}</code>`;
function article(build){return `<article data-registry-control="${build.id}">${field('data-registry-control-entry',build.entry)}${field('data-registry-control-mode',build.mode)}${field('data-registry-control-artifact',build.artifact.module_tree_hash)}${Object.entries(build.fields).map(([key,value])=>field(`data-registry-control-${attribute(key)}`,value)).join('')}</article>`;}
const markup=`<section data-registry-source-controls><p>${scopes.join('; ')}; ${escape(evidence.componentVm)}; ${escape(evidence.componentFailureProtocol)}; ${escape(evidence.primary)}; ${escape(evidence.consensus)}; ${escape(evidence.network)}</p>${field('data-registry-controls-index',evidence.packetSHA)}${Object.entries(evidence.counts).map(([key,value])=>field(`data-registry-controls-${attribute(key)}`,value)).join('')}${evidence.builds.map(article).join('')}</section>`;
checkRegistryControlsDisplay(markup,evidence);
const replaceIn=(html,attribute,value)=>html.replace(new RegExp(`(<code ${attribute}>)[^<]*`),(_,prefix)=>prefix+escape(value));
const changeArticle=(html,index,change)=>html.replace(/<article\b[^>]*>[\s\S]*?<\/article>/g,((seen=0)=>value=>seen++===index?change(value):value)());
const displayMutations=[
  ['packet digest',html=>replaceIn(html,'data-registry-controls-index','00'.repeat(32))],
  ...Object.keys(evidence.counts).map(key=>[`visible ${key}`,html=>replaceIn(html,`data-registry-controls-${attribute(key)}`,999)]),
  ...evidence.builds.map((build,index)=>[`visible mode for ${build.id}`,html=>changeArticle(html,index,row=>replaceIn(row,'data-registry-control-mode','changed'))]),
  ...['entry','artifact',...Object.keys(evidence.builds[0].fields)].map(key=>[`late visible ${key}`,html=>changeArticle(html,7,row=>replaceIn(row,`data-registry-control-${attribute(key)}`,'changed'))]),
  ['missing late row',html=>html.replace(article(evidence.builds[7]),'')],
  ['duplicate late row',html=>html.replace(article(evidence.builds[7]),article(evidence.builds[7])+article(evidence.builds[7]))],
  ['swapped row order',html=>html.replace(article(evidence.builds[0])+article(evidence.builds[1]),article(evidence.builds[1])+article(evidence.builds[0]))],
  ['wrong late row ID',html=>html.replace(`data-registry-control="${evidence.builds[7].id}"`,'data-registry-control="other"')],
  ['suffixed packet attribute',html=>html.replace('data-registry-controls-index>', 'data-registry-controls-index-hidden>')],
  ['missing packet digest',html=>html.replace(field('data-registry-controls-index',evidence.packetSHA),'')],
  ['duplicate packet digest',html=>html.replace(field('data-registry-controls-index',evidence.packetSHA),field('data-registry-controls-index',evidence.packetSHA).repeat(2))],
  ...['componentVm','componentFailureProtocol','primary','consensus','network'].map(key=>[`missing visible ${key} profile`,html=>html.replace(escape(evidence[key]),'profile removed')]),
  ...scopes.map(phrase=>[`missing scope ${phrase}`,html=>html.replace(phrase,'scope removed')]),
];
for(const [name,mutate]of displayMutations){
  const changed=mutate(markup);assert.notEqual(changed,markup,`Inert visible corruption: ${name}`);
  assert.throws(()=>checkRegistryControlsDisplay(changed,evidence),undefined,`Accepted registry source-control visible corruption: ${name}`);
}
if(built){
  for(const route of routes)checkRegistryControlsDisplay(readBounded(path.join(built,route,'index.html'),registryHTMLLimit(route)).toString('utf8'),evidence);
  // Presentation transport only; complete source/IR/protocol admission ran above.
  for(const row of evidence.downloads){
    const actual=readBounded(path.join(built,'reports/registry-source-controls',row.path),2*1024*1024,row.bytes===0);
    assert(actual.equals(evidence.payloads.get(row.path)),`Built registry control payload changed: ${row.path}`);
  }
}
console.log(`PASS ${projectionControls} registry source-control projection and ${displayMutations.length} visible corruption controls; ${built?'four actual built views within explicit tutorial512KiB/labs2MiB limits and byte-exact packet downloads':'constructed markup only'}. ${summaryPath?'Explicit alternate summary: historical/prototype recorded projection, no current publication credit.':'Current complete Python packet admission required and checked.'} No VM/SDK execution, authentication or preservation credit.`);
