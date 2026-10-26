// Compact presentation after complete Python semantic admission plus independent
// JS transport checks. This is not a second whole-protocol validator or VM run.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { constants, openSync, fstatSync, readSync, closeSync, lstatSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repository } from './p2-costs.mjs';
import { counterCurry } from './counter-protocol.mjs';
import { catTreeHash } from '../../scripts/cat-protocol.mjs';
const checkerRoot=await repository();
// Preserve the frozen Node checker and its repository identity through Astro SSR bundling.
const {checkRegistryReport,checkRegistryTransport,parseRegistryIndex,registryModes,registrySlugs,registryLimits}=await import(/* @vite-ignore */ pathToFileURL(path.join(checkerRoot,'www/scripts/registry-protocol.mjs')).href);

export { registrySlugs };
export const registryPageLimit = 512 * 1024;
export const registryRoutes = ['tutorials/action-registry',...registrySlugs.map(slug=>`lab/${slug}`)];
// Labs also retain every complete declared supporting source. The focused
// tutorial and the protocol section itself keep their smaller display budget.
export function registryHTMLLimit(route) {
  assert(registryRoutes.includes(route),'Unknown registry display route');
  return route.startsWith('lab/') ? 2 * 1024 * 1024 : registryPageLimit;
}
export const registryDisplayCounts = Object.freeze({ordinaryGroups:5,ordinarySdk:55,ordinaryPrimary:95,ordinaryAccepted:30,ordinaryTyped:25,adversarialGroups:5,adversarialSdk:50,adversarialPrimary:90,adversarialAccepted:10,adversarialTyped:15,adversarialRuntime:25,setupSdk:10,setupPrimary:15,causalPrimary:15,causalAccepted:10,causalRejected:5,files:11});
export const registryScope = [
  'WIP prototype; not ready for real funds',
  'Original fixed two-name teaching actions',
  'all three selected actions in each build',
  'same five-build group on every view; do not add the views together',
  'separate from ordinary ledger scenario groups and raw vectors',
  'Python owns complete wire and causal admission',
  'not an independently complete JS validator',
  'this base view adds no Rust source-mutant credit',
  'not execution authentication or compiler preservation',
  'no BLS or general uniqueness proof',
  'no live Testnet11 acceptance or complete issue #14 claim',
];
const root=checkerRoot;
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const bytes=(hex,width)=>{assert(typeof hex==='string'&&/^(?:[0-9a-f]{2})*$/.test(hex)&&hex.length<=2*1024*1024);if(width!==undefined)assert.equal(hex.length,2*width);return Buffer.from(hex,'hex');};
const hash=catTreeHash;
const atom=hex=>{const raw=bytes(hex);assert(raw.length<64);return raw.length===1&&raw[0]<128?hex:(128+raw.length).toString(16)+hex;};
const pair=(a,b)=>{assert(a.length+b.length+2<=2*1024*1024);return 'ff'+a+b;};
const list=values=>values.reduceRight((tail,value)=>pair(value,tail),'80');
const entry=(side,value)=>sha(Buffer.concat([Buffer.from([2]),Buffer.from(sha(Buffer.concat([Buffer.from([1]),Buffer.from(sha(Buffer.from(`chialisp-oxide/registry/v1/${side}`)),'hex')])),'hex'),Buffer.from(sha(Buffer.concat([Buffer.from([1]),bytes(value,32)])),'hex')]));
const zero='00'.repeat(32),alpha='11'.repeat(32),beta='22'.repeat(32),emptySignature='c0'+'00'.repeat(95);
export const registryAttr=key=>key.replace(/[A-Z]/g,letter=>'-'+letter.toLowerCase());
const labels={'rust-off':'Rust / off','rust-on':'Rust / on',chialisp:'Chialisp','rue-off':'Rue / off','rue-on':'Rue / on'};
const coin=value=>{assert.equal(value.coin_id,sha(Buffer.concat([bytes(value.parent_coin_id,32),bytes(value.puzzle_hash,32),value.amount===0?Buffer.alloc(0):Buffer.from([1])])));assert(value.amount===0||value.amount===1);return value;};
const state=(initialized,a,b)=>({initialized,alpha_hash:entry('alpha',a),beta_hash:entry('beta',b)});
const stateWire=value=>pair(value.initialized?'01':'80',pair(atom(value.alpha_hash),atom(value.beta_hash)));
const stages=outcome=>({status:outcome.status,stage:outcome.stage??'accepted',code:outcome.code??'none'});
function execution(value){if(value.outcome.status==='accepted'){assert(Number.isSafeInteger(value.cost)&&value.cost>0&&value.cost<=250000000);assert.equal(value.failure,null);return String(value.cost);}assert.equal(value.cost,null);assert.deepEqual(value.failure,{code:'raise',kind:'program'});return 'unavailable: typed raise';}
const attemptContracts={ordinary:[['fund_and_launch',null],['initialize',null],['missing_owner_signature','BadAggregateSignature'],['nonempty_wrong_owner_signature','BadAggregateSignature'],['controller_without_slot','MessageNotSentOrReceived'],['slot_without_controller','MessageNotSentOrReceived'],['set_alpha',null],['set_beta_with_older_creator',null],['alpha_noop',null],['consumed_slot_with_fresh_controller','DoubleSpend'],['live_slot_same_fresh_controller',null]],adversarial:[['wrong_membership_sibling','GeneratorRuntimeError'],['missing_membership_sibling','GeneratorRuntimeError'],['wrong_creator_proof','AssertMyParentIdFailed'],['different_current_spender_inner_hash','MessageNotSentOrReceived'],['wrong_old_value','GeneratorRuntimeError'],['reinitialize','GeneratorRuntimeError'],['two_registry_actions_one_spend','GeneratorRuntimeError'],['fund_independent_lookalike_slot',null],['independently_funded_lookalike_parent','AssertMyParentIdFailed'],['valid_alpha_after_all_rejections',null]],setup:[['fund_and_launch',null],['initialize',null]]};
function attempts(lane,rows){assert.equal(rows.length,attemptContracts[lane].length);return rows.map((row,i)=>{const [name,code]=attemptContracts[lane][i];assert.equal(row.name,name);assert.deepEqual(row.outcome,code===null?{status:'accepted'}:{status:'rejected',stage:code==='GeneratorRuntimeError'?'consensus_runtime_umbrella':'consensus',code,complete_state_unchanged:true});return{lane,name:row.name,...stages(row.outcome),fullEquality:code===null?'not applicable':'true',costs:row.primary_vm.map(execution)};});}
function totalCost(rows){return rows.flatMap(row=>row.primary_vm).reduce((sum,row)=>sum+(row.cost??0),0);}

export function readRegistryBytes(filename,maximum){
  const info=lstatSync(filename);assert(info.isFile()&&!info.isSymbolicLink()&&info.nlink===1&&info.size>0&&info.size<=maximum,'Bounded regular registry display input');
  const fd=openSync(filename,constants.O_RDONLY|constants.O_NOFOLLOW);
  try{const opened=fstatSync(fd);assert(opened.isFile()&&opened.nlink===1&&opened.size===info.size,'Registry display input changed before read');const buffer=Buffer.alloc(info.size+1);let used=0;while(used<buffer.length){const got=readSync(fd,buffer,used,buffer.length-used,null);if(!got)break;used+=got;}assert.equal(used,info.size);return buffer.subarray(0,used);}finally{closeSync(fd);}
}

export function projectRegistryProtocol(index,journals,report){
  assert.equal(index.transport,'registry-slot-sidecars/v1');assert.equal(index.header.status,'validated');
  const h=index.header;assert.equal(h.owner_public_key,'b1b01d4389e60a12b1c9312312f706825184320ed91f2a8f16510cbea3ea900f4e6b465a045781841997b209017fb6a4');
  assert.deepEqual(h.totals,{ordinary:{scenario_runs:5,sdk_transaction_attempts:55,accepted:30,typed_consensus_rejections:25,component_primary_observations:95},adversarial:{scenario_runs:5,sdk_transaction_attempts:50,accepted:10,typed_consensus_rejections:15,runtime_umbrellas:25,component_primary_observations:90},adversarial_setup:{sdk_transaction_attempts:10,accepted:10,component_primary_observations:15},causal_primary:{primary_observations:15,accepted:10,typed_program_rejections:5}});
  const canonical=Object.fromEntries(h.canonical_components.map(c=>{assert.equal(hash(c.module_hex),c.module_tree_hash);assert.equal(c.module_hex.length,2*c.bytes);assert.equal(sha(bytes(c.module_hex)),c.blob_sha256);return[c.name,c];}));
  for(const [name,expected]of Object.entries({action_layer:'afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b',default_finalizer:'9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b',slot:'8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3'}))assert.equal(canonical[name].module_tree_hash,expected);
  const builds=index.builds.map((build,i)=>{
    assert.deepEqual([build.backend,build.optimize_lir,build.mode],registryModes[i]);
    const ordinary=journals[`registry-slot-protocol/${build.mode}-ordinary.json`],adversarial=journals[`registry-slot-protocol/${build.mode}-adversarial.json`];assert.equal(ordinary.mode,build.mode);assert.equal(adversarial.mode,build.mode);
    assert.deepEqual(ordinary.nonce,{u64_hex:'0x434f5852454731',canonical_atom_hex:'434f5852454731',decimal_string:'18946064195602225'});
    const ss=pair(atom(canonical.singleton_top_layer_v1_1.module_tree_hash),pair(atom(ordinary.launcher.coin_id),atom(canonical.singleton_launcher.module_tree_hash)));
    const first=counterCurry(canonical.slot.module_hex,[pair(atom(canonical.singleton_top_layer_v1_1.module_tree_hash),atom(hash(ss))),atom(ordinary.nonce.canonical_atom_hex)]);assert.equal(ordinary.slot_first_curry_hex,first);assert.equal(ordinary.slot_first_curry_hash,hash(first));
    const curried=registrySlugs.map((slug,j)=>{const row=report.puzzles.find(p=>p.slug===slug),artifact=row.artifacts.find(a=>a.backend===build.backend&&a.optimize_lir===build.optimize_lir);assert.equal(build.artifacts[j].module_tree_hash,artifact.tree_hash);const args=j===0?[atom(hash(first))]:[atom(h.owner_public_key),atom(hash(first))];return counterCurry(artifact.clvm_hex,args);});assert.deepEqual(ordinary.curried_actions_hex,curried);
    const leaves=curried.map(p=>sha(Buffer.concat([Buffer.from([1]),bytes(hash(p),32)]))),left=sha(Buffer.concat([Buffer.from([2]),bytes(leaves[0],32),bytes(leaves[1],32)]));const merkle=sha(Buffer.concat([Buffer.from([2]),bytes(left,32),bytes(leaves[2],32)]));assert.equal(ordinary.merkle_root,merkle);
    const f1=counterCurry(canonical.default_finalizer.module_hex,[atom(canonical.action_layer.module_tree_hash),atom(ordinary.launcher.coin_id)]),finalizer=counterCurry(f1,[atom(hash(f1))]);
    const inner=s=>counterCurry(canonical.action_layer.module_hex,[finalizer,atom(merkle),stateWire(s)]),full=s=>counterCurry(canonical.singleton_top_layer_v1_1.module_hex,[ss,inner(s)]);
    const expected=[state(false,zero,zero),state(true,zero,zero),state(true,alpha,zero),state(true,alpha,beta),state(true,alpha,beta),state(true,alpha,beta)];
    assert.deepEqual(ordinary.controller_progress.map(p=>p.name),['eve','c1','c2','c3','c4','c5']);let parent=coin(ordinary.launcher).coin_id;
    const progress=ordinary.controller_progress.map((row,j)=>{assert.deepEqual(row.state,expected[j]);coin(row.coin);assert.equal(row.coin.parent_coin_id,parent);assert.equal(row.coin.puzzle_hash,hash(full(row.state)));parent=row.coin.coin_id;return{name:row.name,initialized:row.state.initialized,alphaHash:row.state.alpha_hash,betaHash:row.state.beta_hash,coinId:row.coin.coin_id,parentId:row.coin.parent_coin_id,puzzleHash:row.coin.puzzle_hash};});
    const creators={alpha0:0,beta0:0,alpha1:1,beta1:2,alpha2:3,alpha3:4};
    const slots=Object.entries(ordinary.slots).map(([name,row])=>{const creator=ordinary.controller_progress[creators[name]];assert.equal(row.entry_hash,entry(name.startsWith('alpha')?'alpha':'beta',name.endsWith('0')?zero:name.startsWith('alpha')?alpha:beta));assert.deepEqual(row.creator_controller,creator.coin);assert.deepEqual(row.creator_state,creator.state);coin(row.coin);assert.equal(row.coin.amount,0);assert.equal(row.coin.parent_coin_id,creator.coin.coin_id);assert.equal(row.coin.puzzle_hash,hash(counterCurry(first,[atom(row.entry_hash)])));return{name,coinId:row.coin.coin_id,parentId:row.coin.parent_coin_id,puzzleHash:row.coin.puzzle_hash,creatorId:row.creator_controller.coin_id,entryHash:row.entry_hash,amount:row.coin.amount};});
    const get=name=>ordinary.attempts.find(a=>a.name===name),old=ordinary.slots.alpha1,live=ordinary.slots.alpha2,fresh=ordinary.controller_progress[4],consumed=get('consumed_slot_with_fresh_controller'),accepted=get('live_slot_same_fresh_controller');
    assert.deepEqual(consumed.transaction.spends[0],accepted.transaction.spends[0]);assert.equal(consumed.transaction.signature_hex,accepted.transaction.signature_hex);assert.deepEqual(consumed.transaction.spends[0].coin,fresh.coin);assert.notEqual(fresh.coin.coin_id,ordinary.controller_progress[3].coin.coin_id);assert.deepEqual(consumed.transaction.spends[1].coin,old.coin);assert.deepEqual(accepted.transaction.spends[1].coin,live.coin);assert.equal(old.coin.puzzle_hash,live.coin.puzzle_hash);assert.notEqual(old.coin.coin_id,live.coin.coin_id);assert.deepEqual(consumed.outcome,{status:'rejected',stage:'consensus',code:'DoubleSpend',complete_state_unchanged:true});assert.deepEqual(accepted.outcome,{status:'accepted'});
    const slotSolution=(slot,currentState)=>pair(pair(atom(slot.creator_controller.parent_coin_id),pair(atom(hash(inner(slot.creator_state))),'01')),atom(hash(inner(currentState))));
    const slotOutput=(slot,currentCoin)=>list([list(['47',atom(slot.creator_controller.coin_id)]),list(['43','12','80',atom(currentCoin.puzzle_hash)])]);
    for(const [attempt,slot]of [[consumed,old],[accepted,live]]){assert.equal(attempt.transaction.spends[1].solution_hex,slotSolution(slot,fresh.state));assert.equal(attempt.primary_vm[1].outcome.output_hex,slotOutput(slot,fresh.coin));}
    const b0=ordinary.slots.beta0,current=ordinary.controller_progress[2],betaAttempt=get('set_beta_with_older_creator');assert.notEqual(b0.creator_controller.coin_id,current.coin.coin_id);const creatorInner=hash(inner(b0.creator_state)),currentInner=hash(inner(current.state));const solution=pair(pair(atom(b0.creator_controller.parent_coin_id),pair(atom(creatorInner),'01')),atom(currentInner));assert.equal(betaAttempt.transaction.spends[1].solution_hex,solution);assert.equal(betaAttempt.primary_vm[1].outcome.output_hex,list([list(['47',atom(b0.creator_controller.coin_id)]),list(['43','12','80',atom(current.coin.puzzle_hash)])]));
    const signatures=['missing_owner_signature','nonempty_wrong_owner_signature','set_alpha'].map(name=>{const row=get(name);return{name,signature:row.transaction.signature_hex,...stages(row.outcome)};});assert.equal(signatures[0].signature,emptySignature);assert.notEqual(signatures[1].signature,emptySignature);assert.notEqual(signatures[1].signature,signatures[2].signature);assert(signatures.slice(0,2).every(s=>s.code==='BadAggregateSignature'));
    const causal=adversarial.causal_primary.map((row,j)=>{const ep=j===1?'01':'',oldValue=j===0?zero:alpha,newValue=j===0?alpha:beta,truth=pair(atom(ep),stateWire(expected[j===0?1:2]));assert.equal(row.environment_hex,list([truth,atom(oldValue),atom(newValue)]));assert.equal(row.execution.outcome.status,j===1?'rejected':'accepted');if(j===1)assert.deepEqual(row.execution.failure,{code:'raise',kind:'program'});return{name:row.name,ephemeral:ep||'nil',oldValue,newValue,environment:row.environment_hex,status:row.execution.outcome.status,code:row.execution.failure?.code??'none',cost:execution(row.execution)};});
    const mutationRows=adversarial.attempts.filter(a=>a.mutation).map(row=>{const m=row.mutation,values=[];for(const key of ['kind','path','before_siblings_hex','after_siblings_hex','actual_full_hash','mutated_full_hash','original_hex','mutated_hex','new_hex','proof_reuse_selector'])if(Object.hasOwn(m,key))values.push({key,value:Array.isArray(m[key])?m[key].join(' / '):String(m[key])});for(const key of ['original_creator','mutated_creator','unchanged_current_controller','funding_source','original_slot','lookalike_slot'])if(m[key])values.push({key,value:m[key].coin_id});return{name:row.name,values};});
    const all=[...attempts('ordinary',ordinary.attempts),...attempts('adversarial',adversarial.attempts),...attempts('setup',adversarial.setup_attempts)];assert.equal(all.length,23);assert.equal(all.filter(a=>a.status==='accepted').length,10);assert.equal(all.filter(a=>a.stage==='consensus_runtime_umbrella').length,5);assert.equal(all.filter(a=>a.code==='BadAggregateSignature').length,2);
    const cost={ordinary:totalCost(ordinary.attempts),adversarial:totalCost(adversarial.attempts),setup:totalCost(adversarial.setup_attempts),causal:adversarial.causal_primary.reduce((sum,r)=>sum+(r.execution.cost??0),0)};assert(Object.values(cost).every(Number.isSafeInteger));
    return{mode:build.mode,label:labels[build.mode],artifacts:build.artifacts.map((a,j)=>({slug:a.slug,bytes:a.bytes,moduleHash:a.module_tree_hash,curriedHash:hash(curried[j])})),root:merkle,progress,slots,binding:{creator:b0.creator_controller.coin_id,creatorParent:b0.creator_controller.parent_coin_id,creatorInner,current:current.coin.coin_id,currentInner,currentPuzzle:current.coin.puzzle_hash,slot:b0.coin.coin_id,solution,mode:18},isolation:{fresh:fresh.coin.coin_id,old:old.coin.coin_id,live:live.coin.coin_id,slotHash:old.coin.puzzle_hash,oldParent:old.coin.parent_coin_id,liveParent:live.coin.parent_coin_id,oldProof:slotSolution(old,fresh.state),liveProof:slotSolution(live,fresh.state),negative:consumed.outcome.code,positive:accepted.outcome.status,fullEquality:consumed.outcome.complete_state_unchanged,sameController:true,sameSignature:true},signatures,attempts:all,causal,mutations:mutationRows,cost};
  });
  assert.equal(builds.length,5);
  return{counts:registryDisplayCounts,builds,canonical:h.canonical_components.map(c=>({name:c.name,bytes:c.bytes,moduleHash:c.module_tree_hash,blobSHA:c.blob_sha256})),sourceControlScope:h.source_control_scope,owner:h.owner_public_key,nonce:'18946064195602225',versions:{primary:h.primary_vm.version,sdk:h.sdk.vm,consensus:h.sdk.consensus,network:h.sdk.network,primaryMax:h.primary_vm.max_cost,sdkMax:h.sdk.transaction_max_cost},scope:registryScope};
}

let cached;
export async function readRegistryProtocol(options={}){
  const load=()=>{
    const sourceRoot=options.sourceRoot??root,sidecarRoot=options.sidecarRoot??path.join(sourceRoot,'corpus');
    const report=options.report??JSON.parse(readRegistryBytes(path.join(sourceRoot,'corpus/validation.json'),registryLimits.total));
    const selected=report.puzzles.filter(p=>registrySlugs.includes(p.slug));if(!selected.length)return null;assert.equal(selected.length,3,'Partial registry report cannot render');
    try{lstatSync(path.join(sidecarRoot,'registry-slot-protocol/index.json'));}catch(error){if(error.code==='ENOENT')return null;throw error;}
    const admitted=checkRegistryReport(report,{sidecarRoot}),transport=checkRegistryTransport(report,{sidecarRoot}),payloads=new Map(),indexItem=selected[0].protocol_validation.index;
    const inventory=[indexItem,...transport.index.builds.flatMap(b=>['ordinary','adversarial'].map(l=>b.journals[l]))];let remaining=registryLimits.total;
    for(const item of inventory){assert(item.bytes<=remaining);remaining-=item.bytes;const raw=readRegistryBytes(path.join(sidecarRoot,item.path),item.bytes);assert.equal(raw.length,item.bytes);assert.equal(sha(raw),item.sha256);payloads.set(item.path,raw);}
    const journals=Object.fromEntries([...payloads].filter(([name])=>name!=='registry-slot-protocol/index.json').map(([name,raw])=>[name,parseRegistryIndex(raw)]));
    const view=projectRegistryProtocol(transport.index,journals,report);
    return{...view,index:transport.index,indexSHA:transport.indexSHA,journals,report,payloads,physicalNames:transport.files,sourceRoot,admitted,downloads:inventory.map(i=>({path:i.path,filename:i.path.split('/')[1],bytes:i.bytes,sha256:i.sha256}))};
  };
  if(Object.keys(options).length)return load();cached??=Promise.resolve().then(load);return cached;
}

const plain=value=>value.replace(/<[^>]+>/g,' ').replace(/&amp;/g,'&').replace(/\s+/g,' ').trim();
function visible(markup,attribute,value){attribute=attribute.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');const matches=[...markup.matchAll(new RegExp(`<[^>]+\\b${attribute}(?:="[^"]*")?[^>]*>([\\s\\S]*?)</[^>]+>`,'g'))];assert(matches.length,`Missing visible ${attribute}`);assert(matches.some(m=>plain(m[1])===String(value)),`Wrong visible ${attribute}`);}
export function checkRegistryDisplay(markup,evidence,{route}={}){
  assert(Buffer.byteLength(markup)<=(route===undefined?registryPageLimit:registryHTMLLimit(route)),'Registry page exceeds its route HTML display bound');assert(evidence,'Admitted complete registry packet required');
  const section=markup.match(/<section\b[^>]*data-registry-protocol[^>]*>[\s\S]*?<\/section>/)?.[0];assert(section,'Missing actual registry protocol section');
  assert(Buffer.byteLength(section)<=registryPageLimit,'Registry protocol section exceeds512KiB HTML display bound');
  for(const scope of registryScope)assert(plain(section).includes(scope),`Missing registry scope: ${scope}`);
  for(const [key,value]of Object.entries(evidence.counts))visible(section,`data-registry-count="${key}"`,value);
  visible(section,'data-registry-index-sha',evidence.indexSHA);visible(section,'data-registry-source-controls-scope',evidence.sourceControlScope);visible(section,'data-registry-owner',evidence.owner);visible(section,'data-registry-nonce',evidence.nonce);
  for(const value of Object.values(evidence.versions))assert(plain(section).includes(String(value)),'Missing visible registry VM/SDK profile');
  const builds=[...section.matchAll(/<details\b[^>]*data-registry-build="([^"]+)"[^>]*>([\s\S]*?)<\/details>/g)];assert.deepEqual(builds.map(b=>b[1]),registryModes.map(m=>m[2]));
  for(const [i,row]of builds.entries()){
    const b=evidence.builds[i],body=row[2];assert(plain(body.match(/<summary\b[^>]*>([\s\S]*?)<\/summary>/)?.[1]??'').includes(b.label));visible(body,'data-registry-root',b.root);
    for(const artifact of b.artifacts){const tr=body.match(new RegExp(`<tr\\b[^>]*data-registry-module="${artifact.slug}"[^>]*>([\\s\\S]*?)</tr>`))?.[1];assert(tr);visible(tr,'data-registry-module-hash',artifact.moduleHash);visible(tr,'data-registry-module-bytes',artifact.bytes);visible(tr,'data-registry-leaf-hash',artifact.curriedHash);}
    const progress=[...body.matchAll(/<tr\b[^>]*data-registry-progress="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];assert.deepEqual(progress.map(p=>p[1]),b.progress.map(p=>p.name));for(const [j,p]of progress.entries())for(const [key,value]of Object.entries(b.progress[j]))visible(p[2],`data-registry-state-${registryAttr(key)}`,value);
    for(const slot of b.slots){const tr=body.match(new RegExp(`<tr\\b[^>]*data-registry-slot="${slot.name}"[^>]*>([\\s\\S]*?)</tr>`))?.[1];assert(tr);for(const [key,value]of Object.entries(slot))visible(tr,`data-registry-slot-${registryAttr(key)}`,value);}
    for(const [group,value]of [['binding',b.binding],['isolation',b.isolation]])for(const [key,item]of Object.entries(value))visible(body,`data-registry-${group}-${registryAttr(key)}`,item);
    const attempts=[...body.matchAll(/<tr\b[^>]*data-registry-attempt="([^"]+)"[^>]*>([\s\S]*?)<\/tr>/g)];assert.deepEqual(attempts.map(a=>a[1]),b.attempts.map(a=>`${a.lane}/${a.name}`));for(const [j,a]of attempts.entries()){const expected=b.attempts[j];for(const key of ['lane','name','status','stage','code','fullEquality'])visible(a[2],`data-registry-attempt-${registryAttr(key)}`,expected[key]);visible(a[2],'data-registry-attempt-costs',expected.costs.join(' / '));}
    for(const row of b.causal){const tr=body.match(new RegExp(`<tr\\b[^>]*data-registry-causal="${row.name}"[^>]*>([\\s\\S]*?)</tr>`))?.[1];assert(tr);for(const [key,value]of Object.entries(row))visible(tr,`data-registry-causal-${registryAttr(key)}`,value);}
    for(const row of b.signatures){const article=body.match(new RegExp(`<article\\b[^>]*data-registry-signature="${row.name}"[^>]*>([\\s\\S]*?)</article>`))?.[1];assert(article);for(const [key,value]of Object.entries(row))visible(article,`data-registry-signature-${registryAttr(key)}`,value);}
    for(const row of b.mutations){const article=body.match(new RegExp(`<article\\b[^>]*data-registry-mutation="${row.name}"[^>]*>([\\s\\S]*?)</article>`))?.[1];assert(article);assert(plain(article).includes(row.name),'Missing visible registry mutation name');for(const {key,value}of row.values)visible(article,`data-registry-mutation-${registryAttr(key)}`,value);}
    for(const [key,value]of Object.entries(b.cost))visible(body,`data-registry-cost-${key}`,value);
  }
  for(const component of evidence.canonical){const tr=section.match(new RegExp(`<tr\\b[^>]*data-registry-canonical="${component.name}"[^>]*>([\\s\\S]*?)</tr>`))?.[1];assert(tr);for(const [key,value]of Object.entries(component))visible(tr,`data-registry-canonical-${registryAttr(key)}`,value);}
  for(const file of evidence.downloads){assert(section.includes(`data-registry-download="${file.path}"`));assert(section.includes(`/reports/${file.path}`));visible(section,`data-registry-download-bytes="${file.path}"`,file.bytes);visible(section,`data-registry-download-sha="${file.path}"`,file.sha256);}
  for(const phrase of ['Prosperity Public License 3.0.0','Cargo metadata says MIT','Apache-2.0 does not establish an upstream relicense','Primary VM cost','generator costs are not recorded','Mode 18 binds puzzle hashes, not coin IDs'])assert(plain(section).includes(phrase),`Missing registry boundary ${phrase}`);
  return evidence.counts;
}

export const registryMetadataCorruptions=[
  ['progress state',({journals})=>journals['registry-slot-protocol/rust-off-ordinary.json'].controller_progress[2].state.alpha_hash=zero],
  ['creator versus spender',({journals})=>{const j=journals['registry-slot-protocol/rust-off-ordinary.json'];j.slots.beta0.creator_controller=j.controller_progress[2].coin;}],
  ['slot-only fresh controller',({journals})=>{const j=journals['registry-slot-protocol/rust-off-ordinary.json'];j.attempts[9].transaction.spends[0].coin=j.controller_progress[3].coin;}],
  ['live replacement',({journals})=>{const j=journals['registry-slot-protocol/rust-off-ordinary.json'];j.attempts[10].transaction.spends[1].coin=j.slots.alpha1.coin;}],
  ['consensus code',({journals})=>journals['registry-slot-protocol/rust-off-ordinary.json'].attempts[9].outcome.code='GeneratorRuntimeError'],
  ['signature kind',({journals})=>journals['registry-slot-protocol/rust-off-ordinary.json'].attempts[3].transaction.signature_hex=emptySignature],
  ['used ephemeral',({journals})=>{const j=journals['registry-slot-protocol/rust-off-adversarial.json'];j.causal_primary[1].environment_hex=j.causal_primary[2].environment_hex;}],
  ['group counts',({index})=>index.header.totals.ordinary.sdk_transaction_attempts=56],
  ['mode identity',({index})=>index.builds[0].mode='rust-on'],
  ['canonical slot identity',({index})=>index.header.canonical_components.find(c=>c.name==='slot').module_tree_hash=zero],
];
