// Closed retained CAT metadata/wire checks. The independent Python reader also
// checks complete curry/conditions/lineage/source relations; neither executes.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
export const catSlug='cat-genesis-by-coin-id';
export const catProtocol='chialisp-oxide-cat-genesis-by-coin-id/v1';
export const catModes=[['rust_mir',false],['rust_mir',true],['chialisp',true],['rue_source',false],['rue_source',true]];
export const catTail='493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150';
export const catModule='37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a';
export const catEmptySignature='c0'+'00'.repeat(95);
export const catScenarios=[
 ['fund_genesis_to_cat_eve','accepted',null],['eve_empty_owner_signature','consensus_rejected','BadAggregateSignature'],
 ['eve_nonempty_wrong_key_signature','consensus_rejected','BadAggregateSignature'],['eve_nonempty_wrong_message_signature','consensus_rejected','BadAggregateSignature'],
 ['eve_wrong_ring_successor','consensus_rejected','AssertCoinAnnouncementFailed'],['eve_nonzero_extra_delta','runtime_umbrella','raise'],
 ['eve_wrong_tail_reveal_hash','runtime_umbrella','raise'],['eve_malformed_solution','runtime_umbrella','path_into_atom'],
 ['zero_delta_tail_issuance','accepted',null],['transfer_wrong_parent_lineage','runtime_umbrella','raise'],
 ['transfer_cat_ring_imbalance','consensus_rejected','AssertCoinAnnouncementFailed'],['owner_transfers_issued_cat','accepted',null],
 ['recipient_rejects_previous_owner_signature','consensus_rejected','BadAggregateSignature'],['recipient_spends_with_valid_lineage','accepted',null],
 ['consumed_genesis_input_replay','consensus_rejected','DoubleSpend']];
const profile={sdk:'chia-sdk-test 0.36.0',consensus:'chia-consensus 0.36.1',sdk_vm:'clvmr 0.16.2',network:'TESTNET11',primary_vm:'clvmr 0.21.0',primary_protocol:'clvmr-0.21.0/v1',primary_flags:535,primary_max_cost:250000000,sdk_max_block_cost_clvm:11000000000,sdk_transaction_max_cost:5500000000};
const totals={sdk_transaction_attempts:75,accepted:20,typed_rejected:35,runtime_umbrellas:20,component_primary_vm_runs:75};
const closed=(value,keys)=>{assert(value&&typeof value==='object'&&!Array.isArray(value));assert.deepEqual(Object.keys(value).sort(),[...keys].sort());};
const hex=(value,bytes)=>{assert(typeof value==='string'&&/^(?:[0-9a-f]{2})+$/.test(value));assert(value.length<=(bytes??1048576)*2);if(bytes)assert.equal(value.length,bytes*2);return Buffer.from(value,'hex');};
const sha=bytes=>createHash('sha256').update(bytes).digest();
// Iterative canonical-plain inspection before hash-frame growth, matching this
// consumer's 1MiB/65536-node/512-depth transport profile, not compiler guards.
export function catTreeHash(value){
 const raw=hex(value);assert(raw.length>0);let cursor=0,nodes=0;const frames=[];
 while(true){assert(cursor<raw.length&&++nodes<=65536&&frames.length+1<=512);const start=cursor,head=raw[cursor++];
  if(head===255){assert(frames.length+1<512&&nodes+2<=65536);frames.push(null);continue;}
  let payload;if(head<128)payload=raw.subarray(start,cursor);else if(head===128)payload=raw.subarray(cursor,cursor);else{
   let width=0;while(head&(128>>width))width++;assert(width>=1&&width<=6&&cursor+width-1<=raw.length);
   let length=head&((1<<(7-width))-1);for(let i=0;i<width-1;i++)length=length*256+raw[cursor++];
   assert(length>0&&length<=1048576&&(width===1||length>=2**(7*width-8))&&cursor+length<=raw.length);
   assert(!(length===1&&raw[cursor]<128));payload=raw.subarray(cursor,cursor+length);cursor+=length;
  }
  let hash=sha(Buffer.concat([Buffer.from([1]),payload]));while(frames.length&&frames.at(-1)!==null)hash=sha(Buffer.concat([Buffer.from([2]),frames.pop(),hash]));
  if(frames.length)frames[frames.length-1]=hash;else{assert.equal(cursor,raw.length);return hash.toString('hex');}
 }
}
const pair=(a,b)=>'ff'+a+b;
const list=items=>items.reduceRight((tail,item)=>pair(item,tail),'80');
export const catCurry=(module,genesis)=>{hex(genesis,32);return list(['02',pair('01',module),list(['04',pair('01','a0'+genesis),'01'])]);};
function coin(value){closed(value,['coin_id','parent_coin_id','puzzle_hash','amount']);for(const k of ['coin_id','parent_coin_id','puzzle_hash'])hex(value[k],32);assert(Number.isSafeInteger(value.amount)&&value.amount===1000);}
export function checkCatComponent(puzzle){
 assert.equal(puzzle.slug,catSlug);assert.equal(puzzle.kind,'protocol_component');assert.equal(puzzle.environment,'raw');assert.equal(puzzle.validated,true);
 assert.deepEqual(puzzle.simulator,[]);assert.deepEqual(puzzle.simulator_comparisons,[]);
 const value=puzzle.protocol_validation;
 closed(value,['schema_version','protocol','status',...Object.keys(profile),'cat_module_hex','cat_module_tree_hash','canonical_tail_hex','canonical_tail_tree_hash','owner_public_key','recipient_public_key','builds',...Object.keys(totals),'scope']);
 assert.equal(value.schema_version,1);assert.equal(value.protocol,catProtocol);assert.equal(value.status,'validated');for(const [key,expected]of Object.entries(profile))assert.equal(value[key],expected);
 assert(typeof value.scope==='string'&&value.scope.length>0);assert.equal(hex(value.canonical_tail_hex).length,45);assert.equal(catTreeHash(value.canonical_tail_hex),catTail);assert.equal(value.canonical_tail_tree_hash,catTail);
 assert.equal(hex(value.cat_module_hex).length,1672);assert.equal(catTreeHash(value.cat_module_hex),catModule);assert.equal(value.cat_module_tree_hash,catModule);
 hex(value.owner_public_key,48);hex(value.recipient_public_key,48);assert.notEqual(value.owner_public_key,value.recipient_public_key);
 assert(Array.isArray(value.builds)&&Array.isArray(puzzle.artifacts));assert.deepEqual(value.builds.map(b=>[b.backend,b.optimize_lir]),catModes);assert.deepEqual(puzzle.artifacts.map(b=>[b.backend,b.optimize_lir]),catModes);
 for(const [index,build]of value.builds.entries()){
  closed(build,['backend','optimize_lir','module_tree_hash','lifecycle']);const artifact=puzzle.artifacts[index];assert.equal(typeof build.optimize_lir,'boolean');assert.equal(typeof artifact.optimize_lir,'boolean');assert.equal(catTreeHash(artifact.clvm_hex),artifact.tree_hash);assert.equal(artifact.bytes,hex(artifact.clvm_hex).length);assert.equal(build.module_tree_hash,artifact.tree_hash);
  if(build.backend==='chialisp')assert.equal(artifact.clvm_hex,value.canonical_tail_hex);else assert.notEqual(build.module_tree_hash,catTail);
  const lane=build.lifecycle;closed(lane,['asset_id','tail_reveal_hex','owner_inner_reveal_hex','recipient_inner_reveal_hex','owner_cat_reveal_hex','recipient_cat_reveal_hex','genesis','eve','issued','received','continuation','attempts']);
  for(const key of ['genesis','eve','issued','received','continuation'])coin(lane[key]);
  assert.equal(lane.tail_reveal_hex,catCurry(artifact.clvm_hex,lane.genesis.coin_id));assert.equal(lane.asset_id,catTreeHash(lane.tail_reveal_hex));
  for(const [key,reveal]of [['genesis','owner_inner_reveal_hex'],['eve','owner_cat_reveal_hex'],['issued','owner_cat_reveal_hex'],['received','recipient_cat_reveal_hex'],['continuation','recipient_cat_reveal_hex']])assert.equal(lane[key].puzzle_hash,catTreeHash(lane[reveal]));
  for(const [key,parent]of [['eve','genesis'],['issued','eve'],['received','issued'],['continuation','received']])assert.equal(lane[key].parent_coin_id,lane[parent].coin_id);
  assert(Array.isArray(lane.attempts)&&lane.attempts.length===15);
  for(const [i,attempt]of lane.attempts.entries()){
   closed(attempt,['name','transaction','primary_vm','outcome']);const [name,status,code]=catScenarios[i];assert.equal(attempt.name,name);
   const expected=status==='accepted'?{status}:status==='consensus_rejected'?{status,code,full_simulator_equality:true}:{status,code:'GeneratorRuntimeError',semantic_rejection_credit:false,primary_program_code:code,full_simulator_equality:true};assert.deepEqual(attempt.outcome,expected);
   closed(attempt.transaction,['signature_hex','spends']);hex(attempt.transaction.signature_hex,96);assert.equal(attempt.transaction.spends.length,1);assert.equal(attempt.primary_vm.length,1);
   const spend=attempt.transaction.spends[0];closed(spend,['coin','reveal_hex','solution_hex']);coin(spend.coin);assert.equal(catTreeHash(spend.reveal_hex),spend.coin.puzzle_hash);catTreeHash(spend.solution_hex);
   const run=attempt.primary_vm[0];if(status==='runtime_umbrella'){assert.deepEqual(run.outcome,{status:'rejected'});assert.equal(run.cost,null);assert.deepEqual(run.failure,{kind:'program',code});}else{assert.equal(run.outcome.status,'accepted');assert.equal(run.failure,null);assert(Number.isSafeInteger(run.cost)&&run.cost>0&&run.cost<=profile.primary_max_cost);catTreeHash(run.outcome.output_hex);}
  }
  const attempts=lane.attempts;assert.equal(attempts[0].transaction.signature_hex===catEmptySignature,false);assert.equal(attempts[1].transaction.signature_hex,catEmptySignature);for(const i of [2,3,4,8,10,11,12,13,14])assert.notEqual(attempts[i].transaction.signature_hex,catEmptySignature);
  for(const i of [1,2,3])assert.deepEqual(attempts[i].transaction.spends,attempts[8].transaction.spends);
  assert.deepEqual(attempts[12].transaction.spends,attempts[13].transaction.spends);assert.notEqual(attempts[12].transaction.signature_hex,attempts[13].transaction.signature_hex);assert.deepEqual(attempts[14].transaction,attempts[0].transaction);
 }
 for(const [key,expected]of Object.entries(totals))assert.equal(value[key],expected);
 return{groups:5,attempts:75,accepted:20,rejected:35,runtime_umbrellas:20,vm:75};
}
export function checkCatDisplay(markup,puzzle){
 checkCatComponent(puzzle);
 const section=markup.match(/<section\b[^>]*id="cat-genesis-lifecycle"[^>]*>[\s\S]*?<\/section>/)?.[0];assert(section,'Missing CAT lifecycle section');
 assert(section.includes(`data-cat-protocol="${catProtocol}"`));
 const visible=section.replace(/<[^>]+>/g,' ').replace(/\s+/g,' ');
 for(const [key,value]of Object.entries({groups:5,attempts:75,accepted:20,rejected:35,umbrellas:20,'component-runs':75})){
  const expression=new RegExp(`<([a-z]+)\\b[^>]*data-cat-${key}(?:="[^"]*")?[^>]*>\\s*${value}\\s*</\\1>`);
  assert(expression.test(section),`Missing visible CAT count ${key}=${value}`);
 }
 for(const text of ['75 actual SDK submissions','35 exact typed consensus rejections','20 runtime umbrellas','live','Sage','pending','clvmr 0.21.0','clvmr 0.16.2'])assert(visible.includes(text),`Missing visible CAT scope: ${text}`);
 const details=[...section.matchAll(/<details\b[^>]*data-cat-build="([^"]+)"[^>]*>([\s\S]*?)<\/details>/g)];
 assert.deepEqual(details.map(row=>row[1]),catModes.map(([backend,mode])=>`${backend}/${mode}`));
 for(const [index,detail]of details.entries()){
  const build=puzzle.protocol_validation.builds[index],body=detail[2];
  for(const [key,value]of Object.entries({module:build.module_tree_hash,asset:build.lifecycle.asset_id}))assert(new RegExp(`<code\\b[^>]*data-cat-${key}(?:="[^"]*")?[^>]*>${value}</code>`).test(body),`Missing visible CAT ${key}`);
  const rows=[...body.matchAll(/<li\b[^>]*data-cat-attempt="([^"]+)"[^>]*>([\s\S]*?)<\/li>/g)];
  assert.deepEqual(rows.map(row=>row[1]),catScenarios.map(row=>row[0]));
  for(const [i,row]of rows.entries()){
   const outcome=build.lifecycle.attempts[i].outcome;
   assert(new RegExp(`<code\\b[^>]*>${catScenarios[i][0]}</code>`).test(row[2]),'Missing visible scenario name');
   assert(new RegExp(`<span\\b[^>]*data-cat-status(?:="[^"]*")?[^>]*>${outcome.status}</span>`).test(row[2]),'Missing visible actual outcome');
   if(outcome.code)assert(new RegExp(`<code\\b[^>]*data-cat-code(?:="[^"]*")?[^>]*>${outcome.code}</code>`).test(row[2]),'Missing visible typed consensus/runtime code');
   else assert(!row[2].includes('data-cat-code'),'Accepted attempt claims a rejection code');
  }
 }
 assert(section.includes('reports/cat-genesis-by-coin-id.json'),'Missing exact CAT evidence download');
}
