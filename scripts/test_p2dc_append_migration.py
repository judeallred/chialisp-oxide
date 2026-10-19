"""Pure constructed append/schema/order controls; no actual23 capture credit.

The old22 input is exact retained227. The appended component below is an explicit
minimal constructed value, NOT a fabricated native/VM/SDK report or currentpositive.
"""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import p2dc_append_migration as migration
import runtime_cost_migration as cost


def pending_profile():
    """Test-only pending metadata, separate from the real on-disk authority.

    The loaded basis may be pending or reviewed FINAL. This copy never grants
    current admission; constructed append values only exercise pure algorithms.
    """
    value = copy.deepcopy(migration.load())
    value['status'] = migration.PENDING
    value['reviewed_after'] = None
    migration.validate_profile(value)
    return value


def constructed(before,profile):
    cost.require(profile['status'] == migration.PENDING and profile['reviewed_after'] is None,
                 'Constructed metadata requires an explicit test-only pending profile')
    current=copy.deepcopy(before);transition=profile['source_transition']
    current['authoring']['exports'][migration.SLUG]=dict(entry=migration.ENTRY,source=f'puzzles/{migration.SLUG}/puzzle.rs')
    sources={r['path']:r['sha256'] for r in current['authoring']['sources']}
    sources.update({r['path']:r['sha256'] for r in transition['added']})
    sources.update({r['path']:r['after_sha256'] for r in transition['changed']})
    current['authoring']['sources']=[dict(path=p,sha256=h) for p,h in sorted(sources.items())]
    changed={r['path']:r['after_sha256'] for r in transition['changed']}
    for puzzle in current['puzzles']:
        for row in puzzle.get('native_sources',[]):
            if row['path'] in migration.NATIVE:row['sha256']=changed[row['path']]
    current['puzzles'].append(dict(slug=migration.SLUG,protocol_validation={'constructed_no_execution':True}))
    return current


class SchemaAndHistory(unittest.TestCase):
    def test_pending_default_is_explicit_and_never_falls_back22(self):
        actual = migration.load()
        self.assertIn(actual['status'], (migration.PENDING, migration.FINAL))
        profile = pending_profile()
        for report in ({},migration.predecessor(profile)):
            with patch.object(migration,'load',return_value=profile), \
                    patch.object(migration.composition,'check_sidecars',side_effect=AssertionError('legacy22 fallback')),self.assertRaisesRegex(ValueError,'pending'):
                cost.validate_current(report)
        self.assertEqual(migration.load(), actual)
        self.assertEqual(migration.composition.load()['status'],migration.composition.FINAL)
        self.assertEqual(migration.composition.guarded.load()['status'],migration.composition.guarded.PENDING)

    def test_pending_copy_does_not_mutate_the_loaded_authority(self):
        actual = migration.load()
        migration.validate_profile(actual)
        with patch.object(migration, 'load', return_value=actual):
            profile = pending_profile()
        self.assertIsNot(profile, actual)
        self.assertIsNot(profile['source_transition'], actual['source_transition'])
        self.assertEqual(profile['status'], migration.PENDING)
        self.assertIsNone(profile['reviewed_after'])
        final_shape = copy.deepcopy(profile)
        final_shape.update(status=migration.FINAL, reviewed_after={})
        with self.assertRaisesRegex(ValueError, 'explicit test-only pending'):
            constructed(migration.predecessor(profile), final_shape)
        profile['source_transition']['changed'][0]['after_sha256'] = '0' * 64
        self.assertEqual(migration.load(), actual)

    def test_closed_schema_no_fake_signed_checkpoint_or_mixed_capture(self):
        base=pending_profile();migration.validate_profile(base)
        for name,mutate in [
            ('bool-version',lambda v:v.update(schema_version=True)),('unknown',lambda v:v.update(extra=True)),
            ('fake227commit',lambda v:v['baseline'].update(commit='1'*40)),
            ('signed-relabel',lambda v:v['baseline'].update(kind='signed_git_checkpoint')),
            ('wrong-anchor',lambda v:v['baseline']['signed_anchor'].update(commit='0'*40)),
            ('mixed-pending',lambda v:v.update(reviewed_after={})),('empty-final',lambda v:v.update(status=migration.FINAL,reviewed_after={})),
            ('extra-source',lambda v:v['source_transition']['added'].append(dict(path='constructed.rs',sha256='1'*64))),
            ('wrong-native-path',lambda v:v['source_transition']['embedded_native_paths'].append('constructed.rs'))]:
            bad=copy.deepcopy(base);mutate(bad);self.assertFalse(cost.same(bad,base))
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):migration.validate_profile(bad)

    def test_unsigned227_checkpoint_and_signed_E20_link_are_pure(self):
        with patch.object(cost,'validate_current',side_effect=AssertionError('current on history')), \
                patch.object(migration,'current_native_sources',side_effect=AssertionError('native on history')), \
                patch.object(migration,'check_sidecars',side_effect=AssertionError('sidecars on history')), \
                patch.object(subprocess,'run',side_effect=AssertionError('no processes')):
            before=migration.predecessor();source=cost.source_checkpoint(before);anchor=migration.composition.anchor_checkpoint(source)
        self.assertEqual(cost.sha(cost.canonical(before)),migration.BEFORE_CANONICAL)
        self.assertEqual(cost.sha(cost.canonical(anchor)),migration.composition.ANCHOR['canonical_report_sha256'])

    def test_raw_predecessor_and_hash_checks_do_not_project_history_early(self):
        profile=pending_profile();before=migration.predecessor(profile);report=constructed(before,profile)
        with patch.object(migration.composition,'project_validated',side_effect=AssertionError('early composition history')), \
                patch.object(migration.composition.source,'project_validated',side_effect=AssertionError('early source history')), \
                patch.object(migration,'reviewed_after',return_value=(profile,{})):
            self.assertEqual(migration.predecessor(profile),before)
            self.assertTrue(migration.hash_diagnostic_profile(report))

    def test_checkpoint_rebound_raw_bytes_reject_before_use(self):
        original=migration.read
        def changed(path,*args,**kwargs):
            payload=original(path,*args,**kwargs)
            return payload+b'\n' if path==migration.CHECKPOINT else payload
        migration.predecessor()
        with patch.object(migration,'read',side_effect=changed),self.assertRaisesRegex(ValueError,'unsigned22 bytes'):migration.predecessor()


class ConstructedClosedDelta(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profile=pending_profile();cls.before=migration.predecessor(cls.profile);cls.current=constructed(cls.before,cls.profile)

    def test_closed10plus4plus2_positive_has_no_source_reads(self):
        # This is ONLY the append metadata algorithm on a constructed component.
        with patch.object(migration,'read',side_effect=AssertionError('comparison file read')), \
                patch.object(migration,'snapshot',side_effect=AssertionError('current snapshot on history')), \
                patch.object(subprocess,'run',side_effect=AssertionError('no processes')):
            substitutions=migration.compare_append(self.profile,self.before,self.current)
        self.assertTrue(substitutions)
        self.assertEqual({r['path'] for r in substitutions},set(migration.NATIVE))

    def test_non_noop_old22_source_typed_artifact_and_coverage_changes_reject(self):
        migration.compare_append(self.profile,self.before,self.current)
        for name in ('source','native','solution','order','missing','unknown','off','rue','failure','rollback'):
            bad=copy.deepcopy(self.current);p=bad['puzzles'][0]
            if name=='source':p['source']+='\n'
            elif name=='native':p['cases'][0]['native']={'status':'rejected'}
            elif name=='solution':p['cases'][0]['solution']='(99)'
            elif name=='order':bad['puzzles'][0:2]=bad['puzzles'][1::-1]
            elif name=='missing':bad['puzzles'].pop()
            elif name=='unknown':p['constructed_extra']=True
            elif name=='off':p['artifacts'][0]['bytes']+=1
            elif name=='rue':p['cases'][0]['runs'][3]['execution']['cost']+=1
            elif name=='failure':p['cases'][1]['runs'][1]['execution']['failure']={'kind':'resource','code':'cost_exceeded'}
            else:bad['puzzles'][-2]['protocol_validation']['observations'][1]['negatives'][0]['full_simulator_equality']=False
            self.assertFalse(cost.same(bad,self.current))
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):migration.compare_append(self.profile,self.before,bad)

    def test_native_and_authoring_changes_bound_to_exact_profile_tables(self):
        migration.compare_append(self.profile,self.before,self.current)
        for name in ('old-source','added-source','missing-source','extra-source','duplicate-source','export','native-path','native-hash'):
            bad=copy.deepcopy(self.current)
            if name=='old-source':bad['authoring']['sources'][0]['sha256']='0'*64
            elif name=='added-source':next(r for r in bad['authoring']['sources'] if r['path']==migration.ADDED[0])['sha256']='0'*64
            elif name=='missing-source':bad['authoring']['sources'].pop()
            elif name=='extra-source':bad['authoring']['sources'].append(dict(path='zz-constructed.rs',sha256='0'*64))
            elif name=='duplicate-source':bad['authoring']['sources'].append(copy.deepcopy(bad['authoring']['sources'][0]))
            elif name=='export':bad['authoring']['exports'][migration.SLUG]['entry']='constructed::wrong'
            else:
                row=next(r for p in bad['puzzles'][:-1] for r in p.get('native_sources',[]) if r['path'] in migration.NATIVE)
                if name=='native-path':row['path']='constructed.rs'
                else:row['sha256']='0'*64
            self.assertFalse(cost.same(bad,self.current))
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):migration.compare_append(self.profile,self.before,bad)

    def test_constructed_final_schema_and_added_whole_component_are_closed(self):
        actual = migration.load()
        profile=copy.deepcopy(self.profile);profile['status']=migration.FINAL
        raw={r['entry']:r['sha256'] for r in migration.composition.append.load()['raw_ir_before']}
        raw[migration.composition.source.ENTRY]=migration.composition.source.load()['reviewed_after']['raw_ir_sha256']
        raw[migration.ENTRY]='1'*64
        profile['reviewed_after']=dict(report_sha256='2'*64,canonical_report_sha256=cost.sha(cost.canonical(self.current)),
            added_puzzle_sha256=cost.sha(cost.canonical(self.current['puzzles'][-1])),protocol_sha256=cost.sha(cost.canonical(self.current['puzzles'][-1]['protocol_validation'])),
            metrics_sha256='3'*64,controls_sha256='4'*64,raw_ir=[dict(entry=e,sha256=h) for e,h in sorted(raw.items())],
            bundles={name:dict(manifest_sha256='5'*64,optimize_lir=mode,cargo_options=list(migration.OPTIONS)+(['--offline'] if mode else [])) for name,mode in [('on',True),('off',False)]})
        migration.validate_profile(profile);migration.compare_append(profile,self.before,self.current)
        for name,mutate in [
            ('missing',lambda v:v['reviewed_after'].pop('report_sha256')),
            ('bool-mode',lambda v:v['reviewed_after']['bundles']['on'].update(optimize_lir=1)),
            ('unknown-capture',lambda v:v['reviewed_after'].update(extra=True)),
            ('short-IR',lambda v:v['reviewed_after']['raw_ir'].pop()),
            ('duplicate-IR',lambda v:v['reviewed_after']['raw_ir'].__setitem__(0,copy.deepcopy(v['reviewed_after']['raw_ir'][1]))),
            ('options-not-vector',lambda v:v['reviewed_after']['bundles']['off'].update(cargo_options='--locked'))]:
            bad=copy.deepcopy(profile);mutate(bad);self.assertFalse(cost.same(bad,profile))
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):migration.validate_profile(bad)
        bad=copy.deepcopy(self.current);bad['puzzles'][-1]['protocol_validation']['constructed_no_execution']=False
        with self.assertRaisesRegex(ValueError,'added component differs'):migration.compare_append(profile,self.before,bad)
        self.assertEqual(migration.load(), actual)

    def test_component_view_preserves_full23_metadata_and_same_actual_object(self):
        view=migration.p2_component_view(self.current)
        self.assertIs(view['authoring'],self.current['authoring'])
        self.assertEqual({k:v for k,v in view.items() if k!='puzzles'},{k:v for k,v in self.current.items() if k!='puzzles'})
        self.assertIs(view['puzzles'][0],self.current['puzzles'][-2])
        with self.assertRaises(ValueError):migration.p2_component_view(self.before)


class ConstructedOrderControls(unittest.TestCase):
    def test_full23_current_checker_order_is_explicit_constructed_spies(self):
        report={'puzzles':[{'slug':s} for s in migration.component.OLD_SLUGS+[migration.SLUG]]};calls=[]
        def step(name):
            def checked(value,*args,**kwargs):self.assertIs(value,report);calls.append(name);return (23,459,40)
            return checked
        def script(path):
            name='hash' if 'hashing' in str(path) else 'existing-P2'
            return {'validate':step(name)}
        with patch.object(migration,'reviewed_after',return_value=({},{})), \
                patch.dict(cost.READABLE,check_report=step('generic'),check_sources=step('source')) , \
                patch.object(migration,'current_native_sources',side_effect=step('native')) , \
                patch.object(cost,'password_evidence',side_effect=step('password')), \
                patch.object(migration.runpy,'run_path',side_effect=script), \
                patch.object(migration.component,'validate_current',side_effect=step('new-P2dc')), \
                patch.object(migration,'check_sidecars',side_effect=step('current23-sidecars')), \
                patch.object(migration,'project_validated',side_effect=AssertionError('early history')):
            migration.validate_current(report)
        self.assertEqual(calls,['generic','source','native','password','hash','existing-P2','new-P2dc','current23-sidecars'])

    def test_old22_sidecar_is_stale_in23_sources_not_a_historical_current_positive(self):
        profile=pending_profile();before=migration.predecessor(profile);report=constructed(before,profile)
        controls=json.loads(migration.read(migration.composition.CONTROLS))
        with self.assertRaisesRegex(ValueError,'Stale control'):
            migration.composition.source.validate_controls(migration.p2_component_view(report),controls)

    def test_requested_fresh_after_current_before_pure_history_and_failure_stops(self):
        path=migration.ROOT/'scripts/check-p2dc-append.py';spec=importlib.util.spec_from_file_location('constructed_p2dc_append_cli',path);cli=importlib.util.module_from_spec(spec);spec.loader.exec_module(cli)
        report={'constructed':True};calls=[];before={'constructed22':True};intermediate={'constructedM1':True};anchor={}
        def add(name,result=None):
            def spy(*args,**kwargs):calls.append(name);return result
            return spy
        with patch.object(cost,'validate_current',side_effect=add('current')), \
                patch.object(cost,'check_report_bytes',side_effect=add('raw')), \
                patch.object(cli.p2_cli,'_check_after_current_validation',side_effect=add('fresh',{})), \
                patch.object(cost,'current_checkpoint',side_effect=lambda value:value), \
                patch.object(migration,'reviewed_after',return_value=None), \
                patch.object(migration,'project_validated',side_effect=add('outer',before)), \
                patch.object(cost,'source_checkpoint',side_effect=add('composition',intermediate)), \
                patch.object(migration.composition,'anchor_checkpoint',side_effect=add('anchor',anchor)), \
                patch.object(subprocess,'run',side_effect=AssertionError('no processes')):
            cli.validate(report,b'constructed',fresh_args=object())
        self.assertEqual(calls,['current','raw','fresh','outer','composition','anchor'])
        with patch.object(cost,'validate_current',return_value=None),patch.object(cost,'check_report_bytes',return_value=None), \
                patch.object(cli.p2_cli,'_check_after_current_validation',side_effect=ValueError('fresh failed')), \
                patch.object(cost,'current_checkpoint',side_effect=AssertionError('history after bad fresh')), \
                patch.object(migration,'project_validated',side_effect=AssertionError('history after bad fresh')),self.assertRaisesRegex(ValueError,'fresh failed'):
            cli.validate(report,b'constructed',fresh_args=object())

    def test_composition_CLI_checks_active23_bytes_and_fresh_before_exact22_history(self):
        path=migration.ROOT/'scripts/check-runtime-composition.py';spec=importlib.util.spec_from_file_location('constructed_composition_CLI23_order',path);cli=importlib.util.module_from_spec(spec);spec.loader.exec_module(cli)
        report={'constructed23':True};payload=b'constructed23raw';before={'constructed22':True};intermediate={'constructedM1':True};anchor={};old={};calls=[]
        def step(name,result=None):
            def spy(*args,**kwargs):calls.append(name);return result
            return spy
        with patch.object(cost,'validate_current',side_effect=step('current23')), \
                patch.object(cost,'check_report_bytes',side_effect=step('active23-raw')), \
                patch.object(cli.migration,'check_report_bytes',side_effect=AssertionError('23bytes sent to8024 rawchecker')), \
                patch.object(cli.p2_cli,'_check_after_current_validation',side_effect=step('fresh23',{})), \
                patch.object(cost,'composition_checkpoint',side_effect=step('pure23-to227',before)), \
                patch.object(cli.migration,'project_validated',side_effect=step('8024-history',intermediate)), \
                patch.object(cli.migration.source,'project_validated',side_effect=step('source-history',anchor)), \
                patch.object(cli.migration.append,'project_validated',side_effect=step('append-history',old)):
            cli.validate(report,payload,fresh_args=object())
        self.assertEqual(calls,['current23','active23-raw','fresh23','pure23-to227','8024-history','source-history','append-history'])
        with patch.object(cost,'validate_current',return_value=None),patch.object(cost,'check_report_bytes',return_value=None), \
                patch.object(cli.p2_cli,'_check_after_current_validation',side_effect=ValueError('fresh failed')), \
                patch.object(cost,'composition_checkpoint',side_effect=AssertionError('history after bad fresh')),self.assertRaisesRegex(ValueError,'fresh failed'):
            cli.validate(report,payload,fresh_args=object())

    def test_generic_composition_checkpoint_accepts_only_exact_historical227(self):
        before=migration.predecessor()
        with patch.object(cost,'validate_current',side_effect=AssertionError('current on history')), \
                patch.object(migration,'project_validated',side_effect=AssertionError('23outer on explicit22')):
            self.assertIs(cost.composition_checkpoint(before),before)
            bad=copy.deepcopy(before);bad['puzzles'][0]['artifacts'][0]['bytes']+=1
            with self.assertRaises(ValueError):cost.composition_checkpoint(bad)

    def test_pure_source_checkpoint_selects23_then22_without_current_checks(self):
        value={'puzzles':[{'slug':migration.SLUG}]};before=object();source=object();calls=[]
        def outer(report):self.assertIs(report,value);calls.append('outer');return before
        def composition(report):self.assertIs(report,before);calls.append('composition');return source
        with patch.object(migration,'project_validated',side_effect=outer), \
                patch.object(migration.composition,'project_validated',side_effect=composition), \
                patch.object(cost,'validate_current',side_effect=AssertionError('current on history')):
            self.assertIs(cost.source_checkpoint(value),source)
        self.assertEqual(calls,['outer','composition'])


class ConstructedFreshLoaderControls(unittest.TestCase):
    def test_all23_modes_options_and_non_noop_metadata_controls(self):
        # The generic identity loader is replaced by a spy. These observations
        # exercise loop/completeness/options/report bindings, NOT real bundles.
        with tempfile.TemporaryDirectory(prefix='decoder-p2dc-fresh-metadata-') as temporary:
            directory=Path(temporary);slugs=migration.component.OLD_SLUGS+[migration.SLUG]
            exports={s:{'entry':f"chialisp_oxide_puzzles::{s.replace('-','_')}::puzzle"} for s in slugs}
            report=dict(authoring={'exports':exports},rue_revision='constructed',puzzles=[])
            entries=[];raw=[]
            for index,slug in enumerate(slugs):
                entry=exports[slug]['entry'];name=f'{index}.json';payload=json.dumps({'constructed':index}).encode();(directory/name).write_bytes(payload)
                raw.append(dict(entry=entry,sha256=cost.sha(payload)))
                artifacts=[dict(backend='rust_mir',optimize_lir=mode,bytes=1,clvm_hex='80',tree_hash='1'*64,disassembly='()') for mode in (False,True)]
                report['puzzles'].append(dict(slug=slug,artifacts=artifacts));entries.append(dict(entry=entry,ir=name,artifact=artifacts[0]))
            manifest=directory/'manifest.json';manifest.write_text('{}')
            after=dict(raw_ir=raw,bundles={name:dict(manifest_sha256='2'*64,cargo_options=list(migration.OPTIONS)) for name in ('on','off')})
            calls=[];corruption=[None];expected_options=[list(migration.OPTIONS)]
            def loader(output,**kwargs):
                self.assertEqual(kwargs['compiler'],'independent-compiler');self.assertEqual(kwargs['driver_sha256'],'3'*64)
                self.assertEqual(kwargs['toolchain'],'independent-toolchain');self.assertEqual(kwargs['cargo_options'],expected_options[0])
                self.assertIs(kwargs['authoring'],report['authoring']);self.assertIs(kwargs['current_authoring'](),report['authoring'])
                self.assertEqual(kwargs['selected_entry'],migration.ENTRY)
                mode=kwargs['optimize'];calls.append(mode);rows=copy.deepcopy(entries)
                for row,puzzle in zip(rows,report['puzzles'],strict=True):row['artifact']=copy.deepcopy(puzzle['artifacts'][1 if mode else 0])
                if corruption[0]=='missing':rows.pop()
                elif corruption[0]=='extra':rows.append(copy.deepcopy(rows[0]))
                elif corruption[0]=='artifact':rows[0]['artifact']['bytes']+=1
                elif corruption[0]=='mode':rows[0]['artifact']['optimize_lir']=not mode
                return SimpleNamespace(manifest_path=manifest,manifest={'entries':rows})
            def verify(options):
                return migration.verify_fresh_bundles(report,directory,directory,compiler='independent-compiler',driver_sha256='3'*64,toolchain='independent-toolchain',cargo_options=options)
            with patch.object(migration,'reviewed_after',return_value=({},after)), \
                    patch.object(migration,'check_recorded',return_value=None), \
                    patch.object(migration,'snapshot',return_value=report['authoring']), \
                    patch.object(migration,'load_declared_export',side_effect=loader):
                self.assertEqual([r['exports'] for r in verify(expected_options[0])],[23,23]);self.assertEqual(calls,[True,False])
                expected_options[0]=list(migration.OPTIONS)+['--offline']
                self.assertEqual([r['expected_cargo_options'] for r in verify(expected_options[0])],[expected_options[0],expected_options[0]])
                for bad in ('missing','extra','artifact','mode'):
                    corruption[0]=bad
                    with self.subTest(name=bad),self.assertRaises(ValueError):verify(expected_options[0])
                corruption[0]=None;original=(directory/'0.json').read_bytes();(directory/'0.json').write_bytes(original+b' ')
                with self.assertRaisesRegex(ValueError,'rawIR changed'):verify(expected_options[0])
                (directory/'0.json').write_bytes(original)
                with self.assertRaisesRegex(ValueError,'expected Cargo options'):verify([])



if __name__=='__main__':unittest.main()
