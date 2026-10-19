"""Reviewed current23 admission, then PURE unsigned reviewed22 restoration.

Recorded consistency is not authenticated execution, state equality, BLS,
compiler preservation, exhaustive authorization or production readiness.
"""
import copy
import json
from pathlib import Path
import re
import runpy

from cargo_authoring import check_recorded, contained_file, snapshot, unique_object
from cargo_bundle import load_declared_export
import runtime_cost_migration as cost
import runtime_composition_migration as composition
import p2_delegated_conditions_evidence as component
import p2_singleton_evidence as ledger

ROOT = Path(__file__).resolve().parents[1]
PATH = 'corpus/p2-delegated-conditions-append-baseline.json'
PIN = 'edb7f1172d32e7277af2e42e5eddf062b4d00aa2e4707f498a4e4b10beaf9530'
PENDING = 'pending_actual23_capture_and_review'
FINAL = 'reviewed_final_p2dc_append'
PROTOCOL = 'chialisp-oxide-p2dc-append/v1'
SLUG = 'p2-delegated-conditions'
ENTRY = 'chialisp_oxide_puzzles::p2_delegated_conditions::puzzle'
CHECKPOINT = 'corpus/checkpoints/runtime-composition-validation.json'
BEFORE_RAW = '227ce91137f786919ce47cf0b0e09038efa84ee2f9c0df6a4e3fc0894084dc64'
BEFORE_CANONICAL = 'a58ea5e73d51cd9207913b29cfb122323e96a84bff670a0af3c5af52c63a3905'
CONTROLS = 'corpus/p2dc-append-existing-p2-controls.json'
METRICS = 'corpus/p2dc-append-existing-p2-costs.json'
OPTIONS = ['--manifest-path','puzzles/Cargo.toml','--package','chialisp-oxide-puzzles','--locked']
CORE = ['puzzles/lib.rs','puzzles/authoring.json','crates/chialisp-oxide-corpus/src/lib.rs','crates/chialisp-oxide-corpus/src/native.rs']
NATIVE = CORE[2:]
BASE = 'corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_delegated_conditions.clsp'
ADDED = sorted([f'puzzles/{SLUG}/{name}' for name in ('puzzle.rs','puzzle.rue','reference.clsp','puzzle.json','independent-vectors.json','mutation-recipes.json')]+[BASE,BASE+'.hex','crates/chialisp-oxide-corpus/src/p2_delegated_conditions.rs','crates/chialisp-oxide-corpus/src/p2_delegated_conditions/ledger.rs'])
BASELINE = dict(kind='unsigned_reviewed_content_addressed_checkpoint',checkpoint_path=CHECKPOINT,
    report_sha256=BEFORE_RAW,canonical_report_sha256=BEFORE_CANONICAL,
    authorizing_transition_path=composition.PATH,authorizing_transition_sha256=composition.PIN,
    signed_anchor=composition.ANCHOR)
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same
report_payload = composition.report_payload


def read(path,limit=16*1024*1024):
    payload=report_payload(contained_file(ROOT,path))
    require(len(payload)<=limit,'P2dc append input byte bound')
    return payload


def digest(value):
    return type(value) is str and re.fullmatch('[0-9a-f]{64}',value) is not None


def rows_map(rows):
    require(type(rows) is list,'source rows required')
    for row in rows:
        ledger.closed(row,['path','sha256'],'source identity')
        require(type(row['path']) is str and digest(row['sha256']),'invalid source identity')
    require([r['path'] for r in rows]==sorted({r['path'] for r in rows}),'sorted unique source identities required')
    return {r['path']:r['sha256'] for r in rows}


def capture_fields(after,slugs):
    ledger.closed(after,['report_sha256','canonical_report_sha256','added_puzzle_sha256','protocol_sha256',
        'metrics_sha256','controls_sha256','raw_ir','bundles'],'P2dc final capture')
    require(all(digest(after[k]) for k in set(after)-{'raw_ir','bundles'}),'missing actual23 capture pins')
    expected={f"chialisp_oxide_puzzles::{s.replace('-','_')}::puzzle" for s in slugs+[SLUG]}
    rows=after['raw_ir'];require(type(rows) is list and len(rows)==23,'actual23 raw IR cardinality')
    for row in rows:
        ledger.closed(row,['entry','sha256'],'rawIR')
        require(type(row['entry']) is str and digest(row['sha256']),'rawIR identity')
    require({r['entry'] for r in rows}==expected and len({r['entry'] for r in rows})==23,'rawIR export set')
    ledger.closed(after['bundles'],['on','off'],'paired actual23 bundles')
    for name,mode in [('on',True),('off',False)]:
        value=after['bundles'][name]
        ledger.closed(value,['manifest_sha256','optimize_lir','cargo_options'],'recorded bundle metadata')
        require(digest(value['manifest_sha256']) and value['optimize_lir'] is mode,'recorded bundle mode/hash')
        options=value['cargo_options']
        require(type(options) is list and 0<len(options)<=64 and all(type(v) is str and 0<len(v)<=4096 for v in options),'recorded exact Cargo options')
    return after


def validate_profile(value):
    ledger.closed(value,['schema_version','protocol','status','scope','baseline','preserved_slugs','added_slug','source_transition','reviewed_after'],'P2dc append profile')
    require(type(value['schema_version']) is int and value['schema_version']==1 and value['protocol']==PROTOCOL
        and value['status'] in (PENDING,FINAL) and type(value['scope']) is str and value['scope'],'P2dc append profile schema')
    require(same(value['baseline'],BASELINE) and value['added_slug']==SLUG
        and value['preserved_slugs']==component.OLD_SLUGS,'changed unsigned22/signedE20 checkpoint context')
    transition=value['source_transition'];ledger.closed(transition,['added','changed','embedded_native_paths'],'source transition')
    require(list(rows_map(transition['added']))==ADDED and transition['embedded_native_paths']==NATIVE,'closed10 additions/two native paths')
    require(type(transition['changed']) is list and [r['path'] for r in transition['changed']]==CORE,'closed four source changes')
    for row in transition['changed']:
        ledger.closed(row,['path','before_sha256','after_sha256'],'changed source')
        require(digest(row['before_sha256']) and digest(row['after_sha256']) and row['before_sha256']!=row['after_sha256'],'non-noop source transition')
    if value['status']==PENDING:require(value['reviewed_after'] is None,'mixed pending capture')
    else:capture_fields(value['reviewed_after'],value['preserved_slugs'])
    return value


def load(payload=None):
    payload=read(PATH) if payload is None else payload
    require(sha(payload)==PIN,'P2dc append profile bytes changed')
    value=validate_profile(json.loads(payload,object_pairs_hook=unique_object))
    require(sha(read(composition.PATH))==composition.PIN,'changed reviewed composition profile')
    authority=composition.load() # Immutable profiles/anchor metadata, never project history here.
    require(authority['reviewed_after']['report_sha256']==BEFORE_RAW
        and authority['reviewed_after']['canonical_report_sha256']==BEFORE_CANONICAL,
        'unsigned22 checkpoint not bound to exact reviewed composition')
    if value['status']==FINAL:
        prior={r['entry']:r['sha256'] for r in composition.append.load()['raw_ir_before']}
        prior[composition.source.ENTRY]=composition.source.load()['reviewed_after']['raw_ir_sha256']
        actual={r['entry']:r['sha256'] for r in value['reviewed_after']['raw_ir']}
        require(all(actual[entry]==pin for entry,pin in prior.items()),'retained22 rawIR changed')
    return value


def predecessor(profile=None):
    """Exact immutable227 data only; never current source/SDK/sidecar checks."""
    profile=load() if profile is None else profile
    payload=read(CHECKPOINT)
    require(sha(payload)==profile['baseline']['report_sha256']==BEFORE_RAW,'changed complete unsigned22 bytes')
    value=json.loads(payload,object_pairs_hook=unique_object)
    require(sha(canonical(value))==BEFORE_CANONICAL,'changed complete unsigned22 object')
    # Raw checkpoint only. The caller projects composition/source history
    # AFTER actual current/fresh gates, never inside a current validator.
    return value


def is_append(report):
    return any(p.get('slug')==SLUG for p in report.get('puzzles',[]))


def reviewed_after(report):
    profile=load();require(profile['status']==FINAL,'P2dc append pending actual23 capture/review')
    after=profile['reviewed_after'];capture_fields(after,profile['preserved_slugs'])
    require([p.get('slug') for p in report.get('puzzles',[])]==profile['preserved_slugs']+[SLUG]
        and sha(canonical(report))==after['canonical_report_sha256'],'unreviewed complete current23 report')
    added=report['puzzles'][-1]
    require(sha(canonical(added))==after['added_puzzle_sha256'] and sha(canonical(added['protocol_validation']))==after['protocol_sha256'],'changed added component/protocol')
    return profile,after


def compare_append(profile,before,current):
    """PURE closed metadata restoration; reads NO current or predecessor files."""
    validate_profile(profile)
    require(sha(canonical(before))==BEFORE_CANONICAL,'comparison requires exact unsigned22 checkpoint')
    require([p['slug'] for p in before['puzzles']]==profile['preserved_slugs']
        and [p['slug'] for p in current['puzzles']]==profile['preserved_slugs']+[SLUG],'exact23 append order')
    a,b=before['authoring'],current['authoring']
    require({k:v for k,v in a.items() if k not in ('exports','sources')}=={k:v for k,v in b.items() if k not in ('exports','sources')},'authoring header changed')
    require(set(a)==set(b) and {k:v for k,v in b['exports'].items() if k!=SLUG}==a['exports']
        and b['exports'].get(SLUG)==dict(entry=ENTRY,source=f'puzzles/{SLUG}/puzzle.rs'),'closed annotated export addition')
    old,new=rows_map(a['sources']),rows_map(b['sources']);transition=profile['source_transition']
    expected=dict(old);expected.update(rows_map(transition['added']))
    require(not set(old)&set(rows_map(transition['added'])),'new source paths already present')
    for row in transition['changed']:
        require(old.get(row['path'])==row['before_sha256'],'before source hash differs from checkpoint')
        expected[row['path']]=row['after_sha256']
    require(new==expected,'source table changed outside reviewed10+4 transition')
    if profile['status']==FINAL:
        added=current['puzzles'][-1];after=profile['reviewed_after']
        require(sha(canonical(added))==after['added_puzzle_sha256']
            and sha(canonical(added['protocol_validation']))==after['protocol_sha256'],'added component differs from reviewed capture')
    restored=copy.deepcopy(current);restored['authoring']=copy.deepcopy(a);restored['puzzles'].pop()
    changes={r['path']:r for r in transition['changed']};substitutions=[]
    for previous,actual,target in zip(before['puzzles'],current['puzzles'][:-1],restored['puzzles'],strict=True):
        require(('native_sources' in previous)==('native_sources' in actual),'native metadata presence changed')
        if 'native_sources' not in previous:continue
        require(len(previous['native_sources'])==len(actual['native_sources']),'native metadata count')
        for index,(x,y) in enumerate(zip(previous['native_sources'],actual['native_sources'],strict=True)):
            ledger.closed(x,['path','sha256'],'old native source');ledger.closed(y,['path','sha256'],'new native source')
            require(x['path']==y['path'],'native path/order changed')
            if x['path'] in NATIVE:
                row=changes[x['path']]
                require(x['sha256']==row['before_sha256'] and y['sha256']==row['after_sha256'],'embedded native source hashes not bound')
                target['native_sources'][index]=copy.deepcopy(x);substitutions.append(dict(slug=previous['slug'],path=x['path']))
            else:require(x==y,'other native source changed')
    require(same(restored,before),'whole unsigned22 source/ABI/artifact/OFF/typed/coverage/protocol/metadata regression')
    return substitutions


def project_validated(report):
    """PURE whole23→exact227. Current/fresh checks belong to caller."""
    profile,_=reviewed_after(report);before=predecessor(profile)
    compare_append(profile,before,report)
    return before


def check_report_bytes(payload,report):
    require(sha(payload)==reviewed_after(report)[1]['report_sha256'],'current23 report raw bytes changed')


def hash_diagnostic_profile(report):
    reviewed_after(report);before=predecessor()
    for slug in ('sha256tree','singleton-launcher'):
        old=next(p for p in before['puzzles'] if p['slug']==slug)
        new=next(p for p in report['puzzles'] if p['slug']==slug)
        require(new['artifacts']==old['artifacts'] and new.get('resource_controls')==old.get('resource_controls'),'retained hash slots changed')
    return True


def current_native_sources(report):
    """FULL current23 paths/bytes; never call old22 native checker on a view."""
    check_recorded(report['authoring'],ROOT,component.OLD_SLUGS+[SLUG])
    inputs=rows_map(report['authoring']['sources'])
    for puzzle in report['puzzles']:
        manifest=json.loads(read(f"puzzles/{puzzle['slug']}/puzzle.json"),object_pairs_hook=unique_object)
        paths=manifest.get('native_sources',[]);rows=puzzle.get('native_sources',[])
        require(type(paths) is list and len(paths)==len(set(paths)) and type(rows) is list and [r['path'] for r in rows]==paths,'current23 native closure')
        for row,path in zip(rows,paths,strict=True):
            ledger.closed(row,['path','sha256'],'native source')
            require(path in inputs and row['sha256']==inputs[path]==sha(read(path)),'stale current23 native source')


def p2_component_view(report):
    matches=[p for p in report['puzzles'] if p['slug']=='p2-singleton']
    require(len(report['puzzles'])==23 and len(matches)==1,'unique current23 existing P2 required')
    return {**report,'puzzles':[matches[0]]}


def check_sidecars(report):
    """Actual current23 source/typed checks must precede this pure P2-last view."""
    _,after=reviewed_after(report)
    controls,metrics=read(CONTROLS),read(METRICS)
    require(sha(controls)==after['controls_sha256'] and sha(metrics)==after['metrics_sha256'],'stale current23 oldP2 sidecar bytes')
    view=p2_component_view(report)
    counts=composition.source.validate_controls(view,json.loads(controls,object_pairs_hook=unique_object))
    measured=composition.source.validate_metrics(view,json.loads(metrics,object_pairs_hook=unique_object))
    require(sha(metrics)==composition.load()['reviewed_after']['metrics_sha256'],'existing15 conditions metrics changed')
    return {**counts,**measured}


def validate_current(report):
    """Explicit current23 root; pending/missing/malformed never falls back22."""
    reviewed_after(report)
    require([p.get('slug') for p in report['puzzles']]==component.OLD_SLUGS+[SLUG],'current23 exact coverage')
    counts=cost.READABLE['check_report'](report)
    cost.READABLE['check_sources'](report,ROOT,require_rue=True)
    current_native_sources(report)
    cost.password_evidence(report)
    runpy.run_path(str(ROOT/'scripts/check-hashing-puzzles.py'))['validate'](report)
    runpy.run_path(str(ROOT/'scripts/check-p2-singleton.py'))['validate'](report)
    component.validate_current(report,root=ROOT)
    check_sidecars(report)
    return counts


def verify_fresh_bundles(report,on,off,*,compiler,driver_sha256,toolchain,cargo_options=OPTIONS):
    _,after=reviewed_after(report)
    require(type(cargo_options) is list and 0<len(cargo_options)<=64 and all(type(x) is str and 0<len(x)<=4096 for x in cargo_options),'explicit expected Cargo options required')
    expected={r['entry']:r['sha256'] for r in after['raw_ir']}
    source=report['authoring'];check_recorded(source,ROOT,component.OLD_SLUGS+[SLUG])
    require(snapshot(ROOT)==source,'current23 snapshot differs before fresh load')
    puzzles={source['exports'][p['slug']]['entry']:p for p in report['puzzles']};result=[]
    for mode,directory in [('on',on),('off',off)]:
        optimize=mode=='on';loaded=load_declared_export(directory,root=ROOT,authoring=source,selected_entry=ENTRY,
            compiler=compiler,driver_sha256=driver_sha256,toolchain=toolchain,current_authoring=lambda:snapshot(ROOT),
            optimize=optimize,rue_revision=report['rue_revision'],cargo_options=cargo_options)
        entries=loaded.manifest['entries'];require(len(entries)==23 and {r['entry'] for r in entries}==set(expected),'fresh complete23 exports required')
        for row in entries:
            require(sha(report_payload(loaded.manifest_path.parent/row['ir']))==expected[row['entry']],'current23 fresh rawIR changed')
            artifact=cost.rust_on(puzzles[row['entry']]['artifacts']) if optimize else next(a for a in puzzles[row['entry']]['artifacts'] if a['backend']=='rust_mir' and a['optimize_lir'] is False)
            require(same(row['artifact'],artifact),'fresh23 artifact/report differs')
        result.append(dict(mode=optimize,exports=23,fresh_manifest_sha256=sha(report_payload(loaded.manifest_path)),recorded_manifest_sha256=after['bundles'][mode]['manifest_sha256'],expected_cargo_options=list(cargo_options),recorded_cargo_options=after['bundles'][mode]['cargo_options'],scope='Independent actual current compiler/driver/mode/source/options/rawIR/artifact consistency; recorded manifest/options remain capture metadata, not cross-host admission pins or authentication.'))
    require(snapshot(ROOT)==source,'current23 snapshot changed after fresh load')
    return result
