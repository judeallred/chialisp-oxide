// Constructed transport/dispatch controls; --report adds current full admission.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { checkRegistryTransport, checkRegistryReport, parseRegistryIndex, registryLimits, registrySlugs } from './registry-protocol.mjs';
import { checkProtocolComponent, checkProtocolReport, checkProtocolDisplay } from './p2-protocol.mjs';
import { checkLedgerReport } from './ledger-rejections.mjs';
import { readRegistryProtocol, projectRegistryProtocol, checkRegistryDisplay, registryMetadataCorruptions, registryScope, registryRoutes, registryAttr, readRegistryBytes, registryPageLimit, registryHTMLLimit } from '../src/lib/registry-protocol.mjs';
const root=fileURLToPath(new URL('../../',import.meta.url));
let reportPath,built=false;
for(let i=2;i<process.argv.length;i++){if(process.argv[i]==='--built'){assert(!built);built=true;continue;}assert(process.argv[i]==='--report'&&reportPath===undefined&&i+1<process.argv.length);reportPath=path.resolve(process.argv[++i]);}
assert(!(built&&reportPath),'Built evidence must use the canonical current report, never alternate inputs');
const directory=mkdtempSync(path.join(tmpdir(),'registry-transport-'));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
let controls=0;
try{
  const fixture=`import sys,json\nfrom pathlib import Path\nsys.path.insert(0,'scripts')\nimport test_registry_protocol as t\nr=t.make_report();j={f'registry-slot-protocol/{m}-{lane}.json':b'{}' for _,_,m in t.r.MODES for lane in ('ordinary','adversarial')}\ni=t.make_index(r,j);t.materialize(Path(sys.argv[1]),r,i,j)\nprint(json.dumps(r))\n`;
  const report=JSON.parse(execFileSync('python3',['-B','-c',fixture,directory],{cwd:root,encoding:'utf8'}));
  const indexPath=path.join(directory,'registry-slot-protocol/index.json'),original=readFileSync(indexPath);
  const options={sidecarRoot:directory};
  assert.equal(checkRegistryTransport(report,options).files.length,11);
  function indexMutation(mutate){const bad=structuredClone(report),index=JSON.parse(original);mutate(index);const bytes=Buffer.from(JSON.stringify(index));writeFileSync(indexPath,bytes);for(const p of bad.puzzles)p.protocol_validation.index={path:'registry-slot-protocol/index.json',bytes:bytes.length,sha256:sha(bytes)};assert.throws(()=>checkRegistryTransport(bad,options));writeFileSync(indexPath,original);controls++;}
  for(const mutate of [i=>i.builds[4].journals.adversarial.bytes=registryLimits.member+1,
    i=>i.builds[0].journals.ordinary.bytes=true,i=>i.builds[0].journals.ordinary.path='../escape',
    i=>i.builds[4].artifacts[2].module_tree_hash='00'.repeat(32),i=>i.builds[0].optimize_lir=0,
    i=>i.builds.pop(),i=>i.builds[0].journals.extra=i.builds[0].journals.ordinary])indexMutation(mutate);
  for(const mutate of [r=>r.puzzles.pop(),r=>r.puzzles.push(structuredClone(r.puzzles[0])),
    r=>r.puzzles[0].kind='pure_function',r=>r.puzzles[0].protocol_validation.role=registrySlugs[2],
    r=>r.puzzles[2].artifacts[4].clvm_hex='8080',r=>r.puzzles[1].protocol_validation.index.sha256='00'.repeat(32)]){
    const bad=structuredClone(report);mutate(bad);assert.throws(()=>checkRegistryTransport(bad,options));controls++;
  }
  const extra=path.join(directory,'registry-slot-protocol/extra.json');writeFileSync(extra,'{}');assert.throws(()=>checkRegistryTransport(report,options));rmSync(extra);controls++;
  const extras=Array.from({length:100},(_,i)=>path.join(directory,`registry-slot-protocol/extra-${i}.json`));for(const name of extras)writeFileSync(name,'{}');assert.throws(()=>checkRegistryTransport(report,options),/Bounded physical registry inventory/);for(const name of extras)rmSync(name);controls++;
  assert.throws(()=>parseRegistryIndex(Buffer.from('['.repeat(129)+']'.repeat(129))),/depth bound before parse/);controls++;
  assert.throws(()=>parseRegistryIndex(Buffer.from('['+'0,'.repeat(250001)+'0]')),/structure bound before parse/);controls++;
  const member=path.join(directory,'registry-slot-protocol/rust-off-ordinary.json');rmSync(member);symlinkSync(indexPath,member);assert.throws(()=>checkRegistryTransport(report,options));rmSync(member);writeFileSync(member,'{}');controls++;
  // A transport-positive synthetic packet still fails the actual Python wire gate.
  assert.throws(()=>checkRegistryReport(report,options));controls++;
  assert.throws(()=>checkProtocolComponent(report.puzzles[0]));controls++;
  assert.throws(()=>checkProtocolDisplay('<section/>',report.puzzles[0],report));controls++;
  const missing={simulator:report.simulator,puzzles:report.puzzles.slice(1)};
  assert.throws(()=>checkProtocolReport(missing,options));assert.throws(()=>checkLedgerReport(missing,options));controls+=2;
  assert.deepEqual(checkProtocolReport({puzzles:[]}),{groups:0,attempts:0,accepted:0,rejected:0,codes:{}});
  if(reportPath){const bytes=readFileSync(reportPath);assert(bytes.length<=registryLimits.total);const actual=JSON.parse(bytes),opts={sidecarRoot:path.dirname(reportPath)};checkRegistryReport(actual,opts);checkLedgerReport(actual,opts);checkProtocolReport(actual,opts);console.log('PASS explicit report complete registry transport/Python admission and generic dispatch; recorded readers only.');}
  console.log(`PASS ${controls} constructed registry transport/dispatch controls; no current execution or independently complete JS semantics credit.`);
}finally{rmSync(directory,{recursive:true,force:true});}


const options=reportPath?{report:JSON.parse(readRegistryBytes(reportPath,registryLimits.total)),sidecarRoot:path.dirname(reportPath)}:{};
const evidence=await readRegistryProtocol(options);
if(built)assert(evidence,'Current complete registry packet missing: pending local UI cannot pass the built publication gate');
if(evidence){
  projectRegistryProtocol(evidence.index,evidence.journals,evidence.report);
  for(const [name,mutate]of registryMetadataCorruptions){const inputs=structuredClone({index:evidence.index,journals:evidence.journals,report:evidence.report});mutate(inputs);assert.throws(()=>projectRegistryProtocol(inputs.index,inputs.journals,inputs.report),undefined,`Accepted registry projection corruption: ${name}`);}
  const escaped=value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
  const field=(attribute,value)=>`<code ${attribute}>${escaped(value)}</code>`;
  const fields=(prefix,value)=>Object.entries(value).map(([key,item])=>field(`data-registry-${prefix}-${registryAttr(key)}`,item)).join('');
  const projectionMarkup=()=>`<section data-registry-protocol><p>${registryScope.join('; ')}</p><p>Prosperity Public License 3.0.0; Cargo metadata says MIT; Apache-2.0 does not establish an upstream relicense. Primary VM cost excludes charges; generator costs are not recorded. Mode 18 binds puzzle hashes, not coin IDs.</p>${Object.entries(evidence.counts).map(([key,value])=>field(`data-registry-count="${key}"`,value)).join('')}${field('data-registry-index-sha',evidence.indexSHA)}${field('data-registry-source-controls-scope',evidence.sourceControlScope)}${field('data-registry-owner',evidence.owner)}${field('data-registry-nonce',evidence.nonce)}<p>${Object.values(evidence.versions).join('; ')}</p>${evidence.builds.map(b=>`<details data-registry-build="${b.mode}"><summary>${b.label}</summary>${field('data-registry-root',b.root)}<table>${b.artifacts.map(a=>`<tr data-registry-module="${a.slug}"><td>${field('data-registry-module-hash',a.moduleHash)}${field('data-registry-module-bytes',a.bytes)}${field('data-registry-leaf-hash',a.curriedHash)}</td></tr>`).join('')}${b.progress.map(p=>`<tr data-registry-progress="${p.name}"><td>${fields('state',p)}</td></tr>`).join('')}${b.slots.map(p=>`<tr data-registry-slot="${p.name}"><td>${fields('slot',p)}</td></tr>`).join('')}${b.attempts.map(a=>`<tr data-registry-attempt="${a.lane}/${a.name}"><td>${fields('attempt',Object.fromEntries(Object.entries(a).filter(([key])=>key!=='costs')))}${field('data-registry-attempt-costs',a.costs.join(' / '))}</td></tr>`).join('')}${b.causal.map(c=>`<tr data-registry-causal="${c.name}"><td>${fields('causal',c)}</td></tr>`).join('')}</table>${fields('binding',b.binding)}${fields('isolation',b.isolation)}${b.signatures.map(s=>`<article data-registry-signature="${s.name}">${fields('signature',s)}</article>`).join('')}${b.mutations.map(m=>`<article data-registry-mutation="${m.name}"><h4>${m.name}</h4>${m.values.map(({key,value})=>field(`data-registry-mutation-${registryAttr(key)}`,value)).join('')}</article>`).join('')}${fields('cost',b.cost)}</details>`).join('')}<table>${evidence.canonical.map(c=>`<tr data-registry-canonical="${c.name}"><td>${fields('canonical',c)}</td></tr>`).join('')}</table>${evidence.downloads.map(f=>`<a data-registry-download="${f.path}" href="/chialisp-oxide/reports/${f.path}">file</a>${field(`data-registry-download-bytes="${f.path}"`,f.bytes)}${field(`data-registry-download-sha="${f.path}"`,f.sha256)}`).join('')}</section>`;
  const markup=projectionMarkup();checkRegistryDisplay(markup,evidence);
  assert.throws(()=>registryHTMLLimit('lab/other'),'Unknown routes cannot acquire the lab budget');
  for(const route of registryRoutes){
    const limit=registryHTMLLimit(route);
    assert.equal(limit,route.startsWith('lab/')?2*1024*1024:512*1024);
    checkRegistryDisplay(markup+' '.repeat(limit-Buffer.byteLength(markup)),evidence,{route});
    assert.throws(()=>checkRegistryDisplay(markup+' '.repeat(limit+1-Buffer.byteLength(markup)),evidence,{route}),/route HTML display bound/);
  }
  const oversizedSection=markup.replace('</section>',' '.repeat(registryPageLimit)+'</section>');
  assert.throws(()=>checkRegistryDisplay(oversizedSection,evidence,{route:'lab/registry-initialize'}),/section exceeds512KiB/);
  const replaceVisible=(attribute,value)=>html=>html.replace(new RegExp(`(<[^>]+\\b${attribute}(?:="[^"]*")?[^>]*>)[^<]*`),(_,prefix)=>prefix+value);
  const visibleControls=[
    replaceVisible('data-registry-count="ordinarySdk"','56'),replaceVisible('data-registry-count="adversarialRuntime"','0'),
    replaceVisible('data-registry-leaf-hash','00'.repeat(32)),replaceVisible('data-registry-state-alpha-hash','00'.repeat(32)),replaceVisible('data-registry-state-coin-id','00'.repeat(32)),
    replaceVisible('data-registry-slot-parent-id','00'.repeat(32)),replaceVisible('data-registry-binding-creator','00'.repeat(32)),
    replaceVisible('data-registry-binding-current-inner','00'.repeat(32)),replaceVisible('data-registry-binding-mode','19'),
    replaceVisible('data-registry-isolation-fresh','00'.repeat(32)),replaceVisible('data-registry-isolation-live','00'.repeat(32)),
    replaceVisible('data-registry-isolation-negative','GeneratorRuntimeError'),replaceVisible('data-registry-isolation-full-equality','false'),
    replaceVisible('data-registry-signature-signature','ab'.repeat(96)),replaceVisible('data-registry-attempt-code','Unknown'),
    replaceVisible('data-registry-attempt-costs','0'),replaceVisible('data-registry-causal-ephemeral','changed'),
    replaceVisible('data-registry-causal-status','changed'),replaceVisible('data-registry-causal-environment','80'),
    replaceVisible('data-registry-cost-ordinary','0'),replaceVisible('data-registry-canonical-module-hash','00'.repeat(32)),
    replaceVisible('data-registry-index-sha','00'.repeat(32)),html=>html.replace('Rust / off','Other / off'),
    html=>html.replace('this base view adds no Rust source-mutant credit','Rust mutants are credited'),
    html=>html.replace('clvmr 0.16.2','clvmr 0.21.0'),
  ];
  for(const change of visibleControls){const bad=change(markup);assert.notEqual(bad,markup,'Inert registry visible corruption');assert.throws(()=>checkRegistryDisplay(bad,evidence));}
  if(built){
    for(const route of registryRoutes)checkRegistryDisplay(readRegistryBytes(path.join(evidence.sourceRoot,'www/dist',route,'index.html'),registryHTMLLimit(route)).toString('utf8'),evidence,{route});
    const {opendirSync}=await import('node:fs');const directory=opendirSync(path.join(evidence.sourceRoot,'www/dist/reports/registry-slot-protocol')),names=[];
    try{for(let entry;(entry=directory.readSync())!==null;){assert(names.length<11,'Built registry download inventory exceeds11');assert(entry.isFile()&&!entry.isSymbolicLink());names.push('registry-slot-protocol/'+entry.name);}}finally{directory.closeSync();}
    assert.deepEqual(names.sort(),[...evidence.physicalNames].sort(),'Missing or extra physical registry downloads');
    for(const name of evidence.physicalNames){const expected=evidence.payloads.get(name),actual=readRegistryBytes(path.join(evidence.sourceRoot,'www/dist/reports',name),registryLimits.member);assert(actual.equals(expected),`Built registry bytes changed: ${name}`);assert.equal(sha(actual),sha(expected));}
    const published=JSON.parse(readRegistryBytes(path.join(evidence.sourceRoot,'www/dist/reports/validation.json'),registryLimits.total));
    for(const slug of registrySlugs)assert.deepEqual(published.puzzles.find(p=>p.slug===slug).protocol_validation,evidence.report.puzzles.find(p=>p.slug===slug).protocol_validation);
  }
  console.log(`PASS ${registryMetadataCorruptions.length} registry projection, ${visibleControls.length} visible controls, exact route boundaries and oversized-section rejection${built?', four actual built views (tutorial512KiB/labs2MiB, protocol sections512KiB) and eleven byte-exact physical downloads':'; constructed markup only'}. Complete Python admission remains required; no new execution credit.`);
}else console.log('Registry display data pending locally; metadata/visible/built evidence checks remain uncredited until the complete current packet is installed.');
