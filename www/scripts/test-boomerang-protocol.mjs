import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import {
  boomerangProtocol, checkBoomerangComponent, checkBoomerangDisplay,
  identitySignature, ownerKey, otherKey,
} from './boomerang-protocol.mjs';
import { checkLedgerReport, ledgerProtocol } from './ledger-rejections.mjs';

const args = new Set(process.argv.slice(2));
assert([...args].every(arg => ['--constructed','--built'].includes(arg)) && !(args.has('--constructed') && args.has('--built')));
const constructed = args.has('--constructed'), built = args.has('--built');
// Exercise the independent JS expectations with the Python reader's explicit
// synthetic fixtures. Marker modules and fake signatures are unit data only;
// this path writes no corpus and executes no compiler, native policy, VM or SDK.
const puzzle = constructed ? JSON.parse(execFileSync('python3', ['-c',
  'import sys,json; sys.path.insert(0,"scripts"); from test_boomerang import fixture; print(json.dumps(fixture()))',
], { cwd: fileURLToPath(new URL('../../', import.meta.url)), encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 }))
  : JSON.parse(await readFile(new URL('../../corpus/validation.json', import.meta.url))).puzzles.find(row => row.slug === 'boomerang');
assert(puzzle, 'Actual boomerang capture pending; --constructed is isolated unit data only');
const expected = checkBoomerangComponent(puzzle);
assert.deepEqual(expected, { builds:5, attempts:145, accepted:25, rejected:80, umbrellas:40 });
checkLedgerReport({ simulator:{rejection_protocol:ledgerProtocol}, puzzles:[puzzle] });
let controls = 0;
const change = (index, mutate) => value => mutate(value.coin_instances.ledger[0].attempts[index]);
const mutations = [
  p => { p.kind='pure_function'; }, p => { p.simulator=[{name:'invented spend'}]; },
  p => { p.simulator_comparisons=[{backend:'chialisp',optimize_lir:true,scenarios:[]}]; },
  p => { p.protocol_validation={}; }, p => { p.curried_instances={}; },
  p => { p.coin_instances.protocol='chialisp-oxide-boomerang-instances/v2'; },
  p => { p.coin_instances.role='canonical_clawback'; }, p => { p.coin_instances.ledger.pop(); },
  p => { p.coin_instances.examples.push(p.coin_instances.examples[0]); },
  p => { p.coin_instances.examples[0].argument_hex='80'; },
  p => { p.coin_instances.examples[0].instances[0].sdk_hash_equal=1; },
  p => { p.coin_instances.ledger[0].optimize_lir=0; },
  p => { p.coin_instances.ledger[0].module_tree_hash='00'.repeat(32); },
  p => { p.coin_instances.ledger[0].instance_tree_hash=p.coin_instances.ledger[0].module_tree_hash; },
  p => { p.coin_instances.ledger[0].transaction_attempts=28; },
  p => { p.coin_instances.ledger[0].attempts.reverse(); },
  change(0,r=>{r.timestamp=100;}), change(0,r=>{r.after.children[0].amount=999;}),
  change(0,r=>{r.after.children[0].puzzle_hash='44'.repeat(32);}),
  change(0,r=>{r.after.children[0].parent_coin_id='00'.repeat(32);}),
  change(0,r=>{r.signature_hex=identitySignature;}),change(0,r=>{r.after.next_timestamp=99;}),
  change(3,r=>{r.consensus.code='BadAggregateSignature';}),
  change(3,r=>{r.signature_kind='empty';r.signature_hex=identitySignature;r.signature_context=null;}),
  change(6,r=>{r.consensus.code='MissingKey';}),change(6,r=>{r.full_state_unchanged=1;}),
  change(6,r=>{r.after.height=1;}),change(6,r=>{r.before_debug='unstable diagnostic text';}),
  change(7,r=>{r.signature_hex=identitySignature;}),change(7,r=>{r.signature_context.public_key=ownerKey;}),
  change(8,r=>{r.signature_origin.coin.parent_coin_id='00'.repeat(32);}),
  change(8,r=>{r.signature_context.intent_hex='ab'.repeat(32);}),
  change(9,r=>{r.signature_context.coin_id=r.coin.coin_id;}),
  change(10,r=>{r.coin.amount=1000;}),change(12,r=>{r.argument_hex='80';}),
  change(12,r=>{r.reveal_tree_hash=puzzle.coin_instances.ledger[0].instance_tree_hash;}),
  change(18,r=>{r.before.children=[];}),change(20,r=>{r.before.next_timestamp=101;}),
  change(21,r=>{r.consensus.semantic_rejection_credit=true;}),
  change(21,r=>{r.vm_021.failure.kind='resource';}),
  change(27,r=>{r.vm_021.failure.code='raise';}),
  p => { p.cases.find(row=>row.native.status==='rejected').runs[0].execution.failure.code='path_into_atom'; },
  change(21,r=>{r.vm_021={outcome:{status:'inconclusive'},cost:null,error:'cost ceiling',failure:{kind:'resource',code:'cost_exceeded'}};}),
  change(22,r=>{r.solution_hex='80';}),change(28,r=>{r.full_state_unchanged=false;}),
];
for (const mutate of mutations) {
  checkBoomerangComponent(puzzle);
  const bad=structuredClone(puzzle);mutate(bad);assert.notDeepEqual(bad,puzzle);
  assert.throws(()=>checkBoomerangComponent(bad));controls++;
}
const wrongSlug=structuredClone(puzzle);wrongSlug.slug='unreviewed-coin';
assert.throws(()=>checkLedgerReport({simulator:{rejection_protocol:ledgerProtocol},puzzles:[wrongSlug]}));controls++;

const boundaries = ['template hash','committed instance hash','not ordinary ledger scenario groups','permissionless refund',
  'no automatic refund service','runtime umbrellas never earn semantic rejection credit','clvmr 0.21.0','clvmr 0.16.2',
  'not a BLS proof','not compiler preservation','Optional output is outside the current proof models',
  'not a serialized state proof','public fixture keys','not ready for real funds'];
function render() {
  return `<section data-coin-instances="${boomerangProtocol}" ${Object.entries(expected).map(([k,v])=>`data-coin-${k}="${v}"`).join(' ')}>
    ${['attempts','accepted','rejected','umbrellas'].map(key=>`<div data-coin-count="${key}"><strong>${expected[key]}</strong></div>`).join('')}
    <p data-coin-summary>${expected.attempts} actual submissions; ${expected.accepted} accepted; ${expected.rejected} typed consensus rejections; ${expected.umbrellas} runtime umbrellas; ${boundaries.join('; ')}</p>
    ${puzzle.coin_instances.ledger.map(build=>`<details data-coin-mode="${build.backend}:${build.optimize_lir}">
      <p data-coin-identities>Own template hash: ${build.module_tree_hash}. Own committed instance hash: ${build.instance_tree_hash}.</p><table>${build.attempts.map(row=>`<tr data-coin-attempt="${row.label}">
        <th>${row.label} t=${row.timestamp} amount=${row.coin.amount} ${row.solution}</th><td data-coin-reveal>${row.reveal_tree_hash}</td>
        <td data-coin-signature>Signature ${row.signature_kind} ${row.signature_context?['public_key','coin_id','intent_hex'].map(field=>row.signature_context[field].slice(0,16)).join(' '):''}</td>
        <td data-coin-outcome>VM ${row.vm_021.outcome.status}${row.vm_021.failure?` / ${row.vm_021.failure.code}`:''} Consensus ${row.consensus.code??'accepted'} ${row.consensus.semantic_rejection_credit?'semantic rejection':'no semantic rejection credit'}</td>
        <td data-coin-state>input before=${row.before.spent_height??'unspent'} after=${row.after.spent_height??'unspent'} ${row.full_state_unchanged?'full SDK equality checked in memory':row.after.children.map(child=>`${child.coin_id} ${child.parent_coin_id} ${child.puzzle_hash} child amount=${child.amount}`).join(' ')}</td></tr>`).join('')}</table>
      <a href="/chialisp-oxide/reports/boomerang/boomerang--${build.backend}--${build.optimize_lir?'on':'off'}.clvm.hex">exact reveal</a></details>`).join('')}
    <a href="/chialisp-oxide/reports/boomerang.json">exact evidence</a></section>`;
}
function display(markup) {
  checkBoomerangDisplay(markup,puzzle);
  const section=markup.match(/<section\b[^>]*data-coin-instances[^>]*>[\s\S]*?<\/section>/)?.[0];assert(section);
  const substitutions=[['145 actual submissions','144 actual submissions'],['25 accepted','26 accepted'],
    ['data-coin-count="accepted"><strong>25','data-coin-count="accepted"><strong>26'],
    ['80 typed consensus rejections','120 typed consensus rejections'],['40 runtime umbrellas','40 semantic rejections'],
    ['t=99','t=100'],['amount=1000','amount=1001'],['BadAggregateSignature','MissingKey'],
    ['AssertBeforeSecondsAbsoluteFailed','AssertSecondsAbsoluteFailed'],['full SDK equality checked in memory','unverified'],
    ['no semantic rejection credit','semantic rejection'],['child amount=1000','child amount=999'],
    ['data-coin-attempt="claim_wrong_key"','data-coin-attempt="claim_missing_signature"'],
    ['data-coin-mode="rust_mir:false"','data-coin-mode="rust_mir:true"'],
    [otherKey.slice(0,16),ownerKey.slice(0,16)],['runtime umbrellas never earn semantic rejection credit','runtime umbrellas are secure rejections'],
    ['Optional output is outside the current proof models','Optional output has a proof'],
    [puzzle.coin_instances.ledger[0].instance_tree_hash,'00'.repeat(32)]];
  for(const [from,to]of substitutions) {
    assert(section.includes(from),`No-op display mutation ${from}`);
    const target=from==='full SDK equality checked in memory'
      ? [...section.matchAll(/<td\b[^>]*data-coin-state[^>]*>[\s\S]*?<\/td>/g)].map(match=>match[0]).find(cell=>cell.includes(from))
      : section;
    assert(target,`Missing witnessed state cell for ${from}`);
    const bad=markup.replace(section,section.replace(target,target.replace(from,to)));assert.notEqual(bad,markup);
    assert.throws(()=>checkBoomerangDisplay(bad,puzzle),`Display mutation escaped: ${from}`);controls++;
  }
}
display(render());
if(built) {
  for(const route of ['tutorials/boomerang','lab/boomerang']) {
    const bytes=await readFile(new URL(`../dist/${route}/index.html`,import.meta.url));
    assert(bytes.length<=512*1024,`${route}: HTML exceeds its 512KiB budget`);
    display(bytes.toString('utf8'));
  }
  assert.deepEqual(JSON.parse(await readFile(new URL('../dist/reports/boomerang.json',import.meta.url))),puzzle.coin_instances);
  for(const {artifact}of puzzle.coin_instances.examples[0].instances) {
    const filename=`boomerang--${artifact.backend}--${artifact.optimize_lir?'on':'off'}.clvm.hex`;
    assert.equal(await readFile(new URL(`../dist/reports/boomerang/${filename}`,import.meta.url),'utf8'),artifact.clvm_hex+'\n');
  }
}
console.log(`PASS ${controls} independent boomerang protocol/display corruptions; ${constructed?'explicit synthetic fixtures only':built?'actual retained report, two built views and exact downloads':'actual retained report only'}. No VM/SDK/BLS, state-proof or compiler-preservation credit.`);
