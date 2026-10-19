// Recorded/presentation controls. --pending-controls never admits a public loader.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { costArtifacts, checkpointArtifact, p2CostEvidence, readP2CostInputs, compareCostCaptures, summarizeCosts, checkCostDisplay, checkCostDownloads } from '../src/lib/p2-costs.mjs';
const args = new Set(process.argv.slice(2));
assert([...args].every(arg => ['--pending-controls','--built'].includes(arg)) && !(args.has('--pending-controls') && args.has('--built')));
const pending = args.has('--pending-controls'), built = args.has('--built');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const parse = bytes => JSON.parse(Buffer.from(bytes).toString('utf8'));
const fmt = value => new Intl.NumberFormat('en-US').format(value);
const original = await readP2CostInputs();
const report = parse(original.validation), previous = parse(original['before-p2.json']), puzzle = report.puzzles.find(row => row.slug === 'p2-singleton');
const before = parse(original['before-costs.json']), now = parse(original['costs.json']);
const positive = () => compareCostCaptures(previous, puzzle, before, now, { allowRustOff: true });
let evidence;
if (pending) {
  const pendingProfile = structuredClone(parse(original['composition-profile']));
  pendingProfile.status = 'pending_actual_composition_capture_review'; pendingProfile.reviewed_after = null;
  const constructedPending = { ...original, 'composition-profile': Buffer.from(JSON.stringify(pendingProfile)) };
  assert.throws(() => p2CostEvidence(constructedPending), /pending profiles cannot publish/);
  const comparison = positive();
  evidence = { ...comparison, summary: summarizeCosts(comparison.rows), context: { label: 'Unsigned reviewed source-only checkpoint', kind: 'unsigned_reviewed_content_addressed_checkpoint', reportSHA: sha(original.validation), anchor: parse(original['composition-profile']).signed_anchor },
    downloads: [...costArtifacts.map(row => ({ ...row, content: original[row.basename], sha256: sha(original[row.basename]), bytes: original[row.basename].length })),
      { ...checkpointArtifact, content: original.validation, sha256: sha(original.validation), bytes: original.validation.length }] };
} else evidence = p2CostEvidence(original);
assert.deepEqual([evidence.summary.beforeBytes,evidence.summary.bytes,evidence.summary.beforeRawVM,evidence.summary.rawVM,evidence.summary.delta], [1972,1685,63896,65150,-3442746]);
assert.equal(sha(original.validation), '1a19cca4b2f1d840bf49c3119305f8872ff03e4ea91c5f74e93ad57ef4ad8b03');
let controls = 0;
for (const name of ['total','height','quote','byte-rate','spend','mode','phase','float','bool','condition']) {
  positive(); const changed = structuredClone(now), row = changed.observations[1].joint;
  if (name === 'total') row.total_conditions_cost++;
  if (name === 'height') row.previous_transaction_height++;
  if (name === 'quote') row.excluded_quote_wrapper_bytes = 0;
  if (name === 'byte-rate') row.cost_per_byte++;
  if (name === 'spend') row.per_spend[0].coin_id = '0'.repeat(64);
  if (name === 'mode') changed.observations[1].optimize_lir = 1;
  if (name === 'phase') delete changed.observations[1].lineage;
  if (name === 'float') row.execution_cost += .5;
  if (name === 'bool') row.execution_cost = true;
  if (name === 'condition') { row.condition_cost++; row.total_conditions_cost++; row.per_spend[0].condition_cost++; }
  assert.throws(() => compareCostCaptures(previous,puzzle,before,changed,{allowRustOff:true}), undefined, name); controls++;
}
if (!pending) for (const name of ['pending','signed-context','checkpoint','latest-report','controls']) {
  p2CostEvidence(original); const changed = Object.fromEntries(Object.entries(original).map(([k,v]) => [k,Buffer.from(v)]));
  if (name === 'pending' || name === 'signed-context') { const profile = parse(changed['composition-profile']); if (name === 'pending') profile.status = 'pending_actual_composition_capture_review'; else profile.signed_anchor.commit = null; changed['composition-profile'] = Buffer.from(JSON.stringify(profile)); }
  if (name === 'checkpoint') changed.validation = Buffer.concat([changed.validation,Buffer.from('\n')]);
  if (name === 'latest-report') changed.validation = await readFile(new URL('../../corpus/validation.json',import.meta.url));
  if (name === 'controls') changed['controls.json'] = Buffer.concat([changed['controls.json'],Buffer.from('\n')]);
  assert.throws(() => p2CostEvidence(changed)); controls++;
}
const pair = (before,now) => `data-before="${before}" data-current="${now}">${fmt(before)} → ${fmt(now)}`;
const context = `<p data-cost-checkpoint>Unsigned reviewed source-only checkpoint · validation SHA-256 ${evidence.context.reportSHA} · signed historical E20 anchor <a href="https://github.com/judeallred/chialisp-oxide/commit/${evidence.context.anchor.commit}">${evidence.context.anchor.commit}</a></p>`;
const summary = [['module-bytes',evidence.summary.beforeBytes,evidence.summary.bytes],['raw-vm',evidence.summary.beforeRawVM,evidence.summary.rawVM],['joint-total',evidence.summary.beforeTotal,evidence.summary.total]].map(([key,a,b]) => `<div class="metric"><strong data-cost-summary="${key}" ${pair(a,b)}</strong><span>${{"module-bytes":"optimized Rust module bytes","raw-vm":"ordinary input · primary VM cost increases","joint-total":"joint bundle · total conditions cost decreases"}[key]}</span></div>`).join('');
const delta = `<p data-cost-delta="${evidence.summary.delta}">${fmt(-evidence.summary.delta)} fewer cost units (${evidence.summary.percent}%) execution component increases by ${fmt(evidence.summary.executionDelta)}; charged generator shrinks by ${fmt(-evidence.summary.byteDelta)} bytes</p>`;
const boundary = '<p>primary VM cost increases; total conditions cost decreases; not a universal performance bound; clvmr 0.21.0; clvmr 0.16.2; chia-consensus 0.36.1; TESTNET11; do not add the raw VM table</p>';
const rowLabel = row => `<th scope="row">${row.label}</th>`;
const matrix = `<table data-cost-matrix><caption>Before → source-only checkpoint total conditions cost: all five builds and three accepted phases</caption><thead><tr>${['Build','Genesis','Joint collection','Lineage'].map(s=>`<th scope="col">${s}</th>`).join('')}</tr></thead><tbody>${evidence.rows.map(row=>`<tr data-cost-build="${row.backend}:${row.optimize}">${rowLabel(row)}${row.phases.map(phase=>`<td data-cost-phase="${phase.phase}" ${pair(phase.before.total,phase.current.total)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
const keys = ['execution','conditions','chargedBytes','byteCost','total'];
const joint = `<table data-cost-joint><caption>Source-only checkpoint joint collection: the three cost components and their sum</caption><thead><tr>${['Build','Execution','Conditions','Charged bytes','Byte charge','Total'].map(s=>`<th scope="col">${s}</th>`).join('')}</tr></thead><tbody>${evidence.rows.map(row=>`<tr data-cost-breakdown="${row.backend}:${row.optimize}">${rowLabel(row)}${keys.map(key=>`<td data-cost-metric="${key}" data-value="${row.phases[1].current[key]}">${fmt(row.phases[1].current[key])}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
const raw = `<table data-cost-raw><caption>Separate module and primary-VM comparison: before → source-only checkpoint for “${evidence.witness}”</caption><thead><tr>${['Build','Serialized module bytes','Raw VM execution cost'].map(s=>`<th scope="col">${s}</th>`).join('')}</tr></thead><tbody>${evidence.rows.map(row=>`<tr data-cost-raw-build="${row.backend}:${row.optimize}">${rowLabel(row)}<td data-cost-module ${pair(row.beforeBytes,row.bytes)}</td><td data-cost-execution ${pair(row.beforeRawVM,row.rawVM)}</td></tr>`).join('')}</tbody></table>`;
const extra = `<p>12,000 cost units per charged generator byte; two-byte quote wrapper; no compressed-generator cost is claimed; conditions API skips signature verification; not additional admissions; do not authenticate execution or prove compiler preservation; host counterparts, not new Rust ports; ${evidence.receipts.attempts} submissions; ${evidence.receipts.accepted} accepted; ${evidence.receipts.rejected} typed rejections</p>`;
const links = `<ul>${evidence.downloads.map(a=>`<li><a data-cost-download="${a.basename}" href="/chialisp-oxide/reports/p2-source-cost/${a.basename}">${a.label}</a> ${fmt(a.bytes)} bytes ${a.sha256}</li>`).join('')}</ul>`;
const fixture = `<section data-transaction-costs data-cost-view="full">${context}${summary}${delta}${boundary}${extra}${matrix}${joint}${raw}${links}</section>`;
const compact = `<section data-transaction-costs data-cost-view="compact">${context}${summary}${delta}${boundary}<a href="/chialisp-oxide/tutorials/transaction-costs/">lesson</a></section>`;
function mutations(markup,compact) {
 const page=markup;markup=page.match(/<section\b[^>]*data-transaction-costs[^>]*>[\s\S]*?<\/section>/)?.[0];assert(markup);
  const changes = [['Unsigned reviewed source-only checkpoint','Current'],[evidence.context.anchor.commit,'WRONG-CHECKPOINT'],[evidence.context.reportSHA,'0'.repeat(64)],['1,972 → 1,685','1,972 → 1,684'],['7.06%','7.05%'],['ordinary input · primary VM cost increases','joint bundle · primary VM cost increases'],['clvmr 0.16.2','clvmr 0.21.0']];
  if (!compact) changes.push([/(<th\b[^>]*scope="row"[^>]*>)Rust \/ off(<\/th>)/,'$1Rue / off$2'],[/(<th\b[^>]*scope="col"[^>]*>)Genesis(<\/th>)/,'$1Joint$2'],['Before → source-only checkpoint total conditions cost:','Current cost:'],['Charged bytes','Module bytes'],['data-cost-build="rust_mir:false"','data-cost-build="rust_mir:true"'],['153,974','153,973']);
  const paragraph = markup.match(/<p\b[^>]*data-cost-checkpoint[^>]*>[\s\S]*?<\/p>/)?.[0]; assert(paragraph);
  const anchor = paragraph.match(/<a\b[^>]*>[\s\S]*?<\/a>/)?.[0]; assert(anchor);
  const wrongVisibleCommit = paragraph.replace(anchor, anchor.replace(/(>)[\s\S]*(<\/a>)/, '$1WRONG-VISIBLE-COMMIT$2'));
  const wrongVisibleHash = paragraph.replace('<p ', `<p data-retained-correct-hash="${evidence.context.reportSHA}" `).replace(/>([^<]*)</g, (_match, text) => `>${text.replace(evidence.context.reportSHA, 'WRONG-VISIBLE-HASH')}<`);
  const wrongLabel = paragraph.replace('Unsigned reviewed source-only checkpoint', 'Latest result');
  const contextMutations = [wrongVisibleCommit, wrongVisibleHash, wrongLabel].map(value => { assert.notEqual(value,paragraph); return markup.replace(paragraph,value); });
  return [...changes.map(([a,b])=>{if(typeof a==='string')assert(markup.includes(a),a);else assert(a.test(markup),String(a));const changed=markup.replace(a,b);assert.notEqual(changed,markup);return changed;}), ...contextMutations].map(section=>{const altered=page.replace(markup,section);assert.notEqual(altered,page);return altered;});
}
for (const [markup,isCompact] of [[fixture,false],[compact,true]]) {
  checkCostDisplay(markup,evidence,{compact:isCompact});
  for (const changed of mutations(markup,isCompact)) { checkCostDisplay(markup,evidence,{compact:isCompact});assert.throws(()=>checkCostDisplay(changed,evidence,{compact:isCompact}));controls++; }
}
const temp=await mkdtemp(path.join(os.tmpdir(),'oxide-history-costs-'));
try {
  for (const a of evidence.downloads) await writeFile(path.join(temp,a.basename),a.content);
  for (const a of evidence.downloads) {await checkCostDownloads(temp,evidence);await writeFile(path.join(temp,a.basename),Buffer.concat([a.content,Buffer.from('\n')]));await assert.rejects(checkCostDownloads(temp,evidence));await writeFile(path.join(temp,a.basename),a.content);controls++;}
  await writeFile(path.join(temp,'unknown.json'),'{}');await assert.rejects(checkCostDownloads(temp,evidence));controls++;await rm(path.join(temp,'unknown.json'));
  const first=evidence.downloads[0];await rm(path.join(temp,first.basename));await symlink(path.resolve('package.json'),path.join(temp,first.basename));await assert.rejects(checkCostDownloads(temp,evidence));controls++;
} finally {await rm(temp,{recursive:true,force:true});}
if (built) {
  for (const [route,isCompact] of [['tutorials/transaction-costs',false],['tutorials/p2-singleton',true]]) {
    const bytes=await readFile(new URL(`../dist/${route}/index.html`,import.meta.url));assert(bytes.length<= (route.endsWith('p2-singleton')?1024:512)*1024);const markup=bytes.toString('utf8');checkCostDisplay(markup,evidence,{compact:isCompact});for(const changed of mutations(markup,isCompact)){assert.throws(()=>checkCostDisplay(changed,evidence,{compact:isCompact}));controls++;}
  }
  await checkCostDownloads(new URL('../dist/reports/p2-source-cost',import.meta.url),evidence);
}
assert.deepEqual(await readP2CostInputs(),original);
console.log(`PASS ${controls} historical cost controls; ${pending?'explicit pending-profile rejection plus constructed-context arithmetic/display fixtures only':built?'two actual rendered historical views and seven exact downloads':'recorded metadata and synthetic display/download controls only'}. No compiler/VM/ledger execution or profile promotion.`);
