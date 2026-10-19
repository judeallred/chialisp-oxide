import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile, mkdir, copyFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repository } from '../src/lib/p2-costs.mjs';
import { delegatedSourceControlEvidence, readDelegatedSourceControls, checkDelegatedSourceControlDisplay,
  sourceControlOutcomeText, sourceControlsProtocol, sourceControlNames } from '../src/lib/p2dc-source-controls.mjs';

const args = new Set(process.argv.slice(2));
assert([...args].every(arg => ['--constructed','--built'].includes(arg)) && !(args.has('--constructed') && args.has('--built')));
const constructed = args.has('--constructed'), built = args.has('--built');
const sha = value => createHash('sha256').update(value).digest('hex');
const parse = value => JSON.parse(Buffer.from(value).toString('utf8'));
const root = new URL('../../', import.meta.url);
const baseBytes = await readFile(new URL('puzzles/p2-delegated-conditions/puzzle.rs', root));
const recipe = parse(await readFile(new URL('puzzles/p2-delegated-conditions/mutation-recipes.json', root)));
const vectors = parse(await readFile(new URL('puzzles/p2-delegated-conditions/independent-vectors.json', root)));
const strictFormat = value => value.replace('    assert!(matches!(environment, Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))));\n', '    assert!(matches!(\n        environment,\n        Tree::Pair(_, Tree::Pair(_, Tree::Atom([])))\n    ));\n');

function fixture() {
  // Constructed reader/display unit data only. These are not captured compiler,
  // native, CLVM, SDK or current23 admission records and are never saved to corpus.
  const baseline = baseBytes.toString('utf8');
  const sources = { baseline, ...Object.fromEntries(recipe.mutations.map((r,i) => [r.name, i === 2 ? strictFormat(baseline.replace(r.before,r.after)) : baseline.replace(r.before,r.after)])) };
  const witnesses = recipe.mutations.map((r,i) => {
    const vector = vectors.accepted.find(row => row.label === r.witness);
    return { name:r.name, witness:r.witness, solution:vector.solution, environment_hex:vector.environment_hex,
      baseline:{ status:'accepted',output_hex:vector.expected_output_hex }, mutant:i === 2 ? {status:'rejected'} : {status:'accepted',output_hex:i === 0 ? '80' : '01'} };
  });
  const nativeLog = Buffer.from(witnesses.map(w => `P2DC_MUTATION_NATIVE ${JSON.stringify({name:w.name,witness:w.witness,baseline:w.baseline,actual:w.mutant})}\n`).join('') + 'test result: ok. 1 passed; 0 failed;\n');
  const run = outcome => outcome.status === 'accepted' ? {outcome,cost:1,error:null} : {outcome,cost:null,error:'Constructed policy panic fixture',failure:{kind:'program',code:'raise'}};
  const report = {schema_version:1,protocol:sourceControlsProtocol,status:'actual_source_controls_captured',
    scope:'One actual native Rust test group; three independent source-policy observations; twelve retained primary VM witnesses; not execution authentication. Constructed fixture only.',
    context:{constructed:true},prepared:{witnesses,source_files:[...Object.entries(sources).map(([n,v])=>({path:`src/${n}.rs`,sha256:sha(v)})),...['Cargo.toml','src/lib.rs','tests/native.rs','witnesses.json','Cargo.lock'].map(p=>({path:p,sha256:'0'.repeat(64)}))]},
    native:{native_test_groups:1,native_policy_observations:3,exit_code:0,log:'native.log',log_sha256:sha(nativeLog)},
    bundles:Object.fromEntries(['off','on'].map(mode=>[mode,{manifest:`bundles/${mode}/manifest.json`,manifest_sha256:'0'.repeat(64),recompiled:['baseline',...sourceControlNames].map(name=>({entry:`chialisp_oxide_p2dc_controls::${name}::puzzle`}))}])),
    runs:['off','on'].flatMap(mode=>witnesses.flatMap(w=>['baseline',w.name].map(name=>({mode,name,witness:w.witness,solution:w.solution,artifact_sha256:'0'.repeat(64),execution:run(name === 'baseline' ? w.baseline : w.mutant)})))),
    commands:Array.from({length:22},()=>({constructed:true})),files:[]};
  const inputs = {baselineSource:baseBytes,recipe:structuredClone(recipe),vectors:structuredClone(vectors),nativeLog,sources,reportBytes:Buffer.from(JSON.stringify(report))};
  const puzzle = {slug:'p2-delegated-conditions',validated:true,protocol_validation:{status:'validated',raw_validation:{cases:witnesses.map(w=>({label:w.witness,solution:w.solution,native:w.baseline}))}}};
  return {report,inputs,puzzle};
}
const escape = value => value.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#39;');
function render(evidence) {
  const counts = Object.entries(evidence.counts).map(([key,value])=>`data-p2dc-controls-${key.replace(/[A-Z]/g,letter=>`-${letter.toLowerCase()}`)}="${value}"`).join(' ');
  const outcome = (role,value,vm=false) => `<td data-p2dc-outcome="${role}">${escape(sourceControlOutcomeText(value,{vm}))}</td>`;
  const rows = evidence.rows.map(row=>`<article data-p2dc-mutation="${row.name}"><h3>${row.title}</h3><p>${row.witness}</p><code data-p2dc-solution>${escape(row.solution)}</code><code data-p2dc-environment>${row.environment_hex}</code><div data-p2dc-recipe="before"><pre><code><span>${escape(row.before)}</span></code></pre></div><div data-p2dc-recipe="after"><pre><code><span>${escape(row.after)}</span></code></pre></div><table><tbody><tr>${outcome('native-baseline',row.native.baseline)}${outcome('native-mutant',row.native.actual)}</tr>${row.modes.map(mode=>`<tr>${outcome(`${mode.mode}-baseline`,mode.baseline,true)}${outcome(`${mode.mode}-mutant`,mode.mutant,true)}</tr>`).join('')}</tbody></table></article>`).join('');
  return `<section data-p2dc-source-controls ${counts}><p>1 native test group; 3 paired policy observations; 4 annotated Cargo exports; both optimization modes; 12 primary VM observations; valid Rust; raw policy counterexamples; not SDK admissions; not compiler preservation; not exhaustive security; 200 additional native; 1,000 additional VM; 60 SDK submissions</p>${rows}<p>${evidence.protocol} ${evidence.reportSHA}</p></section>`;
}

let controls = 0, relocationChecks = 0;
const metadataMutations = [
  x=>{x.report.schema_version=true;}, x=>{x.report.status='pending';}, x=>{x.report.protocol='wrong';},
  x=>{x.inputs.recipe.source_sha256='0'.repeat(64);}, x=>{x.inputs.recipe.mutations[0].after+=' ';},
  x=>{x.report.native.native_test_groups=3;}, x=>{x.report.native.native_policy_observations=4;}, x=>{x.report.native.exit_code=1;},
  x=>{x.inputs.nativeLog=Buffer.from('test result: ok. 3 passed; 0 failed;');x.report.native.log_sha256=sha(x.inputs.nativeLog);},
  x=>{x.report.prepared.witnesses[0].solution+=' ';}, x=>{x.report.prepared.witnesses[0].environment_hex='80';},
  x=>{x.report.prepared.witnesses[0].baseline.output_hex='01';}, x=>{x.report.prepared.witnesses[0].mutant={status:'rejected'};},
  x=>{x.inputs.sources.wrong_conditions_tail+='\n';}, x=>{x.report.prepared.source_files.pop();},
  x=>{x.report.bundles.off.recompiled.pop();}, x=>{x.report.bundles.on.recompiled[0].entry='wrong';},
  x=>{x.report.runs.pop();}, x=>{x.report.runs[0].mode='on';}, x=>{x.report.runs[1].name='wrong_conditions_tail';},
  x=>{x.report.runs[1].execution.outcome.output_hex='02';},
  x=>{x.report.runs[5].execution={outcome:{status:'inconclusive'},cost:null,error:'Constructed cost exhaustion',failure:{kind:'resource',code:'cost_exceeded'}};},
  x=>{x.report.runs[5].execution.failure={kind:'program',code:'path_into_atom'};},
  x=>{x.puzzle.protocol_validation.raw_validation.cases[0].native={status:'rejected'};}, x=>{x.puzzle.validated=false;},
];
for (const mutate of metadataMutations) {
  const good=fixture();delegatedSourceControlEvidence(good.report,good.inputs,good.puzzle);
  const changed=fixture();mutate(changed);changed.inputs.reportBytes=Buffer.from(JSON.stringify(changed.report));
  assert.throws(()=>delegatedSourceControlEvidence(changed.report,changed.inputs,changed.puzzle));controls++;
}
function displayControls(evidence, page) {
  checkDelegatedSourceControlDisplay(page,evidence);
  // Mutate the section under test even when the page repeats a count elsewhere.
  page = page.match(/<section\b[^>]*data-p2dc-source-controls[^>]*>[\s\S]*?<\/section>/)[0];
  const replacements = [
    ['data-p2dc-controls-vm="12"','data-p2dc-controls-vm="13"'], ['1 native test group','3 native test groups'],
    ['3 paired policy observations','4 paired policy observations'],['both optimization modes','optimized mode only'],
    ['raw policy counterexamples','proved policy safety'],['not SDK admissions','SDK admissions'],
    ['200 additional native','201 additional native'],['60 SDK submissions','61 SDK submissions'],
    [evidence.reportSHA,'f'.repeat(64)], ['Use the wrong atom hash prefix','Use the correct atom hash prefix'],
    ['data-p2dc-mutation="wrong_conditions_tail"','data-p2dc-mutation="other"'],
    ['rejected · program/raise','rejected · resource/cost_exceeded'],['source policy panic','VM fault'],
  ];
  for (const [from,to] of replacements) {
    assert(page.includes(from),`Constructed/built display has no mutation target: ${from}`);
    const changed=page.replaceAll(from,to);assert.notEqual(changed,page);assert.throws(()=>checkDelegatedSourceControlDisplay(changed,evidence), `Display mutation was not rejected: ${from}`);controls++;
  }
  for (const attribute of ['solution','environment']) {
    const changed=page.replace(new RegExp(`(<code\\b[^>]*data-p2dc-${attribute}[^>]*>)`),'$1WRONG_VISIBLE_VALUE ');
    assert.notEqual(changed,page);assert.throws(()=>checkDelegatedSourceControlDisplay(changed,evidence));controls++;
  }
  // Mutate visible per-row output while leaving all correct output metadata and
  // other rows intact. A retained hash elsewhere must not mask a false display.
  for (const row of evidence.rows) for (const role of ['native-baseline','native-mutant','off-baseline','off-mutant','on-baseline','on-mutant']) {
    const block=page.match(new RegExp(`<article\\b[^>]*data-p2dc-mutation="${row.name}"[^>]*>[\\s\\S]*?<\\/article>`))?.[0];assert(block);
    const bad=block.replace(new RegExp(`(<td\\b[^>]*data-p2dc-outcome="${role}"[^>]*>)[\\s\\S]*?(<\\/td>)`),'$1WRONG VISIBLE OUTCOME$2');
    assert.notEqual(bad,block);assert.throws(()=>checkDelegatedSourceControlDisplay(page.replace(block,bad),evidence));controls++;
  }
  for (const kind of ['before','after']) {
    const bad=page.replace(new RegExp(`(<div\\b[^>]*data-p2dc-recipe="${kind}"[^>]*>[\\s\\S]*?<code[^>]*>)`),'$1WRONG_VISIBLE_SOURCE');
    assert.notEqual(bad,page);assert.throws(()=>checkDelegatedSourceControlDisplay(bad,evidence));controls++;
  }
}
const constructedData=fixture();
const constructedEvidence=delegatedSourceControlEvidence(constructedData.report,constructedData.inputs,constructedData.puzzle);
displayControls(constructedEvidence,render(constructedEvidence));
// Explicitly constructed nested Code wrapper, not actual Astro render output.
// A header closes a div before the code element, exposing first-</div> slicing.
const nestedMarkup = render(constructedEvidence)
  .replace(/(<div data-p2dc-recipe="(?:before|after)">)<pre>/g, '$1<div class="expressive-code"><div class="frame"><div class="header">Constructed nested wrapper</div><pre>')
  .replaceAll('</code></pre></div>', '</code></pre></div></div></div>');
checkDelegatedSourceControlDisplay(nestedMarkup,constructedEvidence);controls++;
displayControls(constructedEvidence,nestedMarkup);
// Astro emits numeric entities inside highlighted Rust source.
for (const [lessThan, ampersand] of [['&#x3C;', '&#x26;'], ['&#60;', '&#38;']]) {
  const numericMarkup = nestedMarkup.replaceAll('&lt;', lessThan).replaceAll('&amp;', ampersand);
  assert.notEqual(numericMarkup, nestedMarkup);
  checkDelegatedSourceControlDisplay(numericMarkup, constructedEvidence);controls++;
}
for (const [from,to] of [
  ['data-p2dc-recipe="after"','data-p2dc-recipe="before"'],
  ['</code>',''],
  ['</code></pre>','</code><code>UNEXPECTED_SECOND_SNIPPET</code></pre>'],
  ['<table>','<wrong-table>'],
]) {
  const changed=nestedMarkup.replace(from,to);assert.notEqual(changed,nestedMarkup);
  assert.throws(()=>checkDelegatedSourceControlDisplay(changed,constructedEvidence));controls++;
}
const temporary=await mkdtemp(path.join(os.tmpdir(),'oxide-p2dc-docs-controls-'));
try {
  assert.equal(await readDelegatedSourceControls(undefined,{sourceRoot:temporary}),null);
  assert.equal(await readDelegatedSourceControls(constructedData.puzzle,{sourceRoot:temporary}),null);controls++;
  await mkdir(path.join(temporary,'corpus/p2dc-source-controls'),{recursive:true});
  await writeFile(path.join(temporary,'corpus/p2dc-source-controls/report.json'),'{}');
  await assert.rejects(readDelegatedSourceControls(constructedData.puzzle,{sourceRoot:temporary}));controls++;
} finally { await rm(temporary,{recursive:true,force:true}); }

if (!constructed) {
  const report=parse(await readFile(new URL('corpus/validation.json',root)));
  assert.equal(report.puzzles.length,23,'Actual current23 capture required; --constructed is unit data only');
  const puzzle=report.puzzles.find(row=>row.slug==='p2-delegated-conditions');assert(puzzle);
  const evidence=await readDelegatedSourceControls(puzzle);assert(evidence,'Actual source-control packet is pending');
  // Source-only SSR relocation regression: move only the presentation module
  // and its ordinary helper dependencies away from checkout-relative tables.
  // The real Node reader must stay at its resolved source path. This is not an
  // Astro build or new compiler/VM admission.
  const relocatedDirectory=await mkdtemp(path.join(os.tmpdir(),'oxide-p2dc-ssr-relocation-'));
  try {
    const nested=path.join(relocatedDirectory,'www/dist/.prerender');await mkdir(nested,{recursive:true});
    for (const name of ['p2dc-source-controls.mjs','p2-costs.mjs','runtime-checkpoints.mjs']) {
      await copyFile(new URL(`../src/lib/${name}`,import.meta.url),path.join(nested,name));
    }
    const relocated=await import(pathToFileURL(path.join(nested,'p2dc-source-controls.mjs')).href);
    assert.deepEqual(await relocated.readDelegatedSourceControls(puzzle),evidence);relocationChecks++;
    assert.deepEqual(await relocated.readDelegatedSourceControls(puzzle,{sourceRoot:await repository()}),evidence);relocationChecks++;
  } finally { await rm(relocatedDirectory,{recursive:true,force:true}); }
  if (built) for (const route of ['tutorials/delegated-conditions','lab/p2-delegated-conditions']) {
    const bytes=await readFile(new URL(`../dist/${route}/index.html`,import.meta.url));assert(bytes.length<2*1024*1024);
    displayControls(evidence,bytes.toString('utf8'));
  }
}
console.log(`PASS ${controls} source-control metadata/display controls; ${constructed?'explicit constructed unit data only, no actual packet admission':built?'actual retained packet and two built views':'actual retained packet only'}. ${relocationChecks} source-module relocation checks. No new native/compiler/VM/SDK/proof execution credit.`);
