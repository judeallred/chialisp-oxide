"""Constructed-reader corruption controls, never compiler/VM execution evidence."""
import sys
sys.dont_write_bytecode = True
import copy
import unittest
from unittest.mock import patch
import tempfile
from pathlib import Path
from types import SimpleNamespace

import raw_evidence
import vectors
from compiled_codec import inspect
from registry_codegen_evidence import decode


def constructed():
    modules=[]
    for control in raw_evidence.CONTROLS:
        for mode in ('off','on'):
            for export in raw_evidence.EXPORTS:
                modules.append({'id':control+'/'+mode+'/'+export,'control':control,'mode':mode,'export':export,
                    'clvm_hex':'01','inspection':inspect(b'\x01'),'ir':{},'manifest':{},'source_receipt':None})
    for export in raw_evidence.EXPORTS:
        modules.append({'id':'original/chialisp/'+export,'control':'original','mode':'chialisp','export':export,
            'clvm_hex':'01','inspection':inspect(b'\x01')})
    from capture_support import identity
    for mode in ('rue-off','rue-on'):
        for export in raw_evidence.EXPORTS:
            modules.append({'id':'original/'+mode+'/'+export,'control':'original','mode':mode,'export':export,
                'optimize_lir':mode=='rue-on','source':identity(raw_evidence.HERE.parents[1]/'puzzles'/('counter-'+export)/'puzzle.rue'),
                'clvm_hex':'01','inspection':inspect(b'\x01')})
    packet={'protocol':'chialisp-oxide-action-counter-artifacts/v2','compiler':'rustc constructed-reader-fixture\n',
        'driver_declaration':'rustc constructed-reader-fixture\n','artifacts':modules}
    rows=[]
    for module in modules:
        for vector in vectors.cases()['cases']:
            if vector['export']!=module['export']:
                continue
            expected=vectors.expected(vector['export'],decode(bytes.fromhex(vector['environment_hex'])),module['control'])
            if expected['status']=='accepted':
                execution={'outcome':expected,'cost':1,'error':None}
            else:
                execution={'outcome':{'status':'rejected'},'cost':None,'error':'constructed, not executed',
                    'failure':{'kind':'program','code':'raise'}}
            rows.append({'module_id':module['id'],'export':vector['export'],'name':vector['name'],'execution':execution})
    report={'protocol':'chialisp-oxide-action-counter-raw-vm/v1','vm_failure_protocol':'clvmr-0.21.0/v1',
        'vm':'clvmr 0.21.0','flags':raw_evidence.MEMPOOL_FLAGS,'max_cost':50_000_000,
        'allocator_limit_bytes':32*1024*1024,'observations':rows}
    return packet,report


class RecordedCorruptionControls(unittest.TestCase):
    def setUp(self):
        self.packet,self.report=constructed()

    def test_complete_constructed_inventory_and_finite_partition(self):
        result=raw_evidence.validate_report(self.report,self.packet)
        self.assertEqual((result['original_observations'],result['mutant_observations'],result['accepted'],result['program_rejections']),
                         (495,396,503,388))

    def test_missing_reordered_duplicate_wrong_output_and_native_stage_are_rejected(self):
        for control in ('missing','reordered','duplicate','output','native-stage'):
            report=copy.deepcopy(self.report)
            rows=report['observations']
            if control=='missing':rows.pop()
            elif control=='reordered':rows[0],rows[1]=rows[1],rows[0]
            elif control=='duplicate':rows[1]=copy.deepcopy(rows[0])
            elif control=='output':rows[0]['execution']['outcome']['output_hex']='80'
            else:
                rejected=next(r for r in rows if r['execution']['outcome']['status']=='rejected')
                rejected['execution']['outcome']['stage']='abi'
            with self.subTest(control=control),self.assertRaises(ValueError):
                raw_evidence.validate_report(report,self.packet)

    def test_resource_infrastructure_unknown_and_boolean_metadata_are_rejected(self):
        for control in ('resource','infrastructure','unknown','bool-cost','bool-flags','missing-code'):
            report=copy.deepcopy(self.report)
            rejected=next(r for r in report['observations'] if r['execution']['outcome']['status']=='rejected')
            if control=='resource':rejected['execution']['failure']={'kind':'resource','code':'cost_exceeded'}
            elif control=='infrastructure':rejected['execution']['failure']={'kind':'infrastructure','code':'internal_error'}
            elif control=='unknown':rejected['execution']['failure']['code']='unknown_code'
            elif control=='bool-cost':report['observations'][0]['execution']['cost']=True
            elif control=='bool-flags':report['flags']=True
            else:del rejected['execution']['failure']
            with self.subTest(control=control),self.assertRaises(ValueError):
                raw_evidence.validate_report(report,self.packet)

    def test_identity_max_and_signature_mutant_witnesses_are_checked(self):
        for target in ('increment_no_change/off/increment','reset_drop_signature/on/reset'):
            report=copy.deepcopy(self.report)
            name='nil-count-'+str(vectors.MAX_COUNTER) if target.startswith('increment') else 'nil-count-1'
            row=next(r for r in report['observations'] if r['module_id']==target and r['name']==name)
            original_id='original/'+target.split('/',1)[1]
            original=next(r for r in report['observations'] if r['module_id']==original_id and r['name']==name)
            row['execution']=copy.deepcopy(original['execution'])
            with self.subTest(target=target),self.assertRaises(ValueError):
                raw_evidence.validate_report(report,self.packet)

    def test_corrupt_compiled_identity_extra_export_and_noncanonical_serialization(self):
        for control in ('identity','export','trailing','backref','bool-stats'):
            packet=copy.deepcopy(self.packet)
            if control=='identity':packet['artifacts'][0]['inspection']['identity']['module_tree_hash']='00'*32
            elif control=='export':packet['artifacts'][0]['export']='extra'
            elif control=='trailing':packet['artifacts'][0]['clvm_hex']='0180'
            elif control=='backref':packet['artifacts'][0]['clvm_hex']='fe01'
            else:packet['artifacts'][0]['inspection']['stats']['nodes']=True
            with self.subTest(control=control),self.assertRaises(ValueError):
                raw_evidence.validate_report(self.report,packet)


class IndependentCompilerQueryControl(unittest.TestCase):
    def test_coherent_metadata_substitution_cannot_replace_independent_queries(self):
        import hashlib
        compiler=b'rustc reviewed-query-fixture\nbinary: rustc\n'
        driver=b'rustc reviewed-query-fixture\n'
        packet={'compiler':compiler.decode(),'driver_declaration':driver.decode()}
        raw_evidence.validate_queries(packet,compiler,driver)
        substituted=b'rustc coherently-substituted-metadata\nbinary: rustc\n'
        # Model all metadata copies moving together, while actual query logs
        # remain fixed. Bundle re-admission must never get to trust these copies.
        receipt={'compiler_sha256':hashlib.sha256(substituted).hexdigest()}
        manifest={'compiler':substituted.decode()}
        packet={'compiler':manifest['compiler'],
                'driver_declaration':substituted.decode().splitlines()[0]+'\n'}
        self.assertEqual(receipt['compiler_sha256'],hashlib.sha256(packet['compiler'].encode()).hexdigest())
        with self.assertRaisesRegex(ValueError,'independent actual rustc query'):
            raw_evidence.validate_queries(packet,compiler,driver)
        packet={'compiler':compiler.decode(),'driver_declaration':'rustc forged-driver\n'}
        with self.assertRaisesRegex(ValueError,'independent actual driver query'):
            raw_evidence.validate_queries(packet,compiler,driver)


class CaptureRelationCorruptionControls(unittest.TestCase):
    def test_opaque_executable_identities_require_typed_length_and_complete_digest(self):
        import action_counter_packet as packet
        good={'path':'/opaque/not-followed','bytes':1,'sha256':'ab'*32}
        packet.host_identity(good)
        for field,value in (('bytes',True),('bytes',1.0),('bytes',0),('bytes',512*1024*1024+1),('sha256','garbled'),('sha256','AB'*32)):
            bad=copy.deepcopy(good);bad[field]=value
            with self.subTest(field=field,value=value),self.assertRaises(ValueError):packet.host_identity(bad)
        bad=copy.deepcopy(good);del bad['sha256']
        with self.assertRaises(ValueError):packet.host_identity(bad)

    def test_sdk_binary_is_bound_by_native_last_build_before_extraction(self):
        import capture
        from capture_support import identity
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            args=SimpleNamespace(output=root/'output',native_target=root/'target',driver=root/'driver',cargo_cli=root/'cargo')
            args.output.mkdir()
            for binary in (capture.runner_path(args),capture.protocol_path(args)):
                binary.parent.mkdir(parents=True,exist_ok=True);binary.write_bytes(b'original-native-build')
            record={'runner':identity(capture.runner_path(args)),'protocol_runner':identity(capture.protocol_path(args)),'source_window':{}}
            (args.output/'native.json').write_text(__import__('json').dumps(record))
            for end in ('before','after'):(args.output/('native-window-'+end+'.json')).write_text('{}')
            with patch.object(capture,'verify_native'),patch.object(capture,'quiet',return_value={}):
                self.assertEqual(capture.tools(args)[2],capture.protocol_path(args))
                capture.protocol_path(args).write_bytes(b'replaced-after-native-before-sdk')
                with self.assertRaisesRegex(ValueError,'LAST protocol'):
                    capture.tools(args)

    def test_recorded_protocol_receipt_closes_all_capture_paths(self):
        import protocol_capture as protocol
        args=SimpleNamespace(output=Path('/captured/raw'),**{name:Path('/captured/'+name) for name in protocol.PATH_FIELDS if name!='raw_capture'})
        receipt={'protocol':'example','status':'example',**{name:str(args.output if name=='raw_capture' else getattr(args,name)) for name in protocol.PATH_FIELDS},'first':{},'repeat':{},'source_window_unchanged':True,'repeat_byte_identical':True,'scope':protocol.SCOPE}
        protocol.check_receipt_paths(receipt,args)
        for name in protocol.PATH_FIELDS:
            bad=copy.deepcopy(receipt);bad[name]='/different/coherently-reindexed-path'
            with self.subTest(name=name),self.assertRaisesRegex(ValueError,'exact retained capture path'):
                protocol.check_receipt_paths(bad,args)

    def test_generated_caches_do_not_enter_owned_sources_but_pinned_blobs_do(self):
        import capture
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            owned=root/'experiments/action-counter'
            owned.mkdir(parents=True)
            (owned/'policy.rs').write_text('pub fn policy() {}\n')
            for name in ('Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo/config.toml'):
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('# constructed source-window fixture\n')
            for repository,pin in (('slot-machine','6edc94856306d0f77e065c6c99aacf1055e51e2f'),('chia-wallet-sdk','e9a08f6e54ea5132f7da06327836a3b386a944b1')):
                path=root/'corpus/upstream'/repository/pin/'source-inventory.json';path.parent.mkdir(parents=True);path.write_text('{"files":[]}')
            before=capture.source_names(root,['experiments/action-counter'])
            with patch.object(capture,'ROOT',root):quiet_before=capture.source_window(root)
            cache=owned/'__pycache__';cache.mkdir()
            (cache/'generated.pyc').write_bytes(b'host-specific cache')
            (owned/'generated.pyo').write_bytes(b'host-specific cache')
            self.assertEqual(capture.source_names(root,['experiments/action-counter']),before)
            with patch.object(capture,'ROOT',root):self.assertEqual(capture.source_window(root),quiet_before)
            pinned='experiments/action-counter/__pycache__/published.pyc'
            (root/pinned).write_bytes(b'explicit Git-declared fixture')
            self.assertEqual(capture.source_names(root,['experiments/action-counter'],declared_cache=[pinned]),sorted([*before,pinned]))
            alias=owned/'cache-link.pyc';alias.symlink_to(cache/'generated.pyc')
            with self.assertRaisesRegex(ValueError,'symlinks'):
                capture.source_names(root,['experiments/action-counter'])

    def test_exact_request_rejects_coherent_input_change(self):
        import capture
        packet,_=constructed()
        request=capture.request_recipe(packet)
        raw_evidence.validate_request(request,packet)
        for field in ('max_cost','module','vector'):
            bad=copy.deepcopy(request)
            if field=='max_cost':bad['max_cost']=1
            elif field=='module':bad['modules'][0]['clvm_hex']='80'
            else:bad['vectors'][0]['environment_hex']='80'
            # Updating the request's outer retained digest/window cannot change
            # the independently reconstructed actual-artifact/vector recipe.
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'exact retained'):
                raw_evidence.validate_request(bad,packet)

    def test_empty_or_omitted_retained_inventory_cannot_pass_matching_windows(self):
        import capture
        from capture_support import retained_identities
        with tempfile.TemporaryDirectory() as directory:
            args=SimpleNamespace(output=Path(directory))
            for name in capture.retained_paths(args):
                path=args.output/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('constructed-not-executed')
            copied=args.output/'projects/original/src/lib.rs'
            copied.parent.mkdir(parents=True)
            copied.write_text('constructed source')
            paths=capture.retained_paths(args)
            retained=retained_identities(args.output,paths)
            raw_evidence.validate_retained_inputs(args,retained)
            for bad in ([],[r for r in retained if r['path']!=str(copied)]):
                with self.assertRaisesRegex(ValueError,'complete independently rediscovered'):
                    raw_evidence.validate_retained_inputs(args,bad)
            copied.write_text('changed materialized source')
            with self.assertRaises(ValueError):
                raw_evidence.validate_retained_inputs(args,retained)

    def test_native_reference_raw_roles_require_exact_command_and_caps(self):
        import capture
        from capture_support import check_receipt
        args=SimpleNamespace(output=Path('/constructed/output'),native_target=Path('/constructed/native'),
            compiler_target=Path('/constructed/compiler'),rustc=Path('/constructed/rustc'),
            driver=Path('/constructed/driver'),cargo_cli=Path('/constructed/cargo-cli'))
        roles=capture.command_roles(args,all_phases=True)
        native_targets=[roles['native-'+c.replace('_','-')][1] for c in capture.CONTROLS]
        self.assertEqual(len(set(native_targets)),3)
        self.assertTrue(all(target != args.native_target and target.is_relative_to(args.native_target) for target in native_targets))
        self.assertEqual(roles['last-host-build'][1],native_targets[0])
        self.assertEqual(roles['raw-attempt1'][0][0],native_targets[0]/'debug/examples/raw-vm')
        for role in ('native-original','compile-reference','raw-attempt1','extract-original-off'):
            argv,target,driver=roles[role]
            receipt={'argv':[str(v) for v in argv],'cwd':str(capture.ROOT),'target':str(target),
                'jobs':2,'offline':True,'rustc':str(args.rustc),'driver':str(driver) if driver else None,
                'exit_code':0,'status':'passed','failure_kind':None,'failure_detail':None,
                'timeout_seconds':600,'cleanup_timeout_seconds':5,'max_stream_bytes':16*1024*1024,
                'stdout':{},'stderr':{}}
            check_receipt(receipt,argv,target=target,rustc=args.rustc,driver=driver)
            for field,value in [('argv',['cargo','test','--lib']),('cwd','/elsewhere'),('target','/elsewhere'),
                                ('timeout_seconds',601),('max_stream_bytes',32*1024*1024),
                                ('offline',False),('rustc','/other/rustc'),('driver','/other/driver'),('exit_code',False),('jobs',2.0)]:
                bad=copy.deepcopy(receipt);bad[field]=value
                with self.subTest(role=role,field=field),self.assertRaisesRegex(ValueError,'exact command role/profile'):
                    check_receipt(bad,argv,target=target,rustc=args.rustc,driver=driver)


class ProtocolExecutableIdentity(unittest.TestCase):
    def test_protocol_state_uses_existing_executable_bound_not_data_bound(self):
        import hashlib
        import protocol_capture as protocol
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            args=SimpleNamespace(output=root,native_target=root/'native')
            binary=protocol.binary(args);binary.parent.mkdir(parents=True)
            size=protocol.support.MAX_FILE+1
            with binary.open('wb') as stream:stream.truncate(size)
            (root/'artifacts.json').write_bytes(b'{}\n')
            # The generic data limit stays 16MiB. Only the production state's
            # executable role gets the same 512MiB limit as native/tools.
            with self.assertRaisesRegex(ValueError,'bounded regular file'):
                protocol.support.identity(binary)
            with patch.object(protocol.capture,'tools',return_value=()), \
                 patch.object(protocol.capture,'quiet',return_value={}):
                state=protocol.state(args)
            self.assertEqual(state['protocol_binary'],
                {'path':str(binary),'bytes':size,'sha256':hashlib.sha256(bytes(size)).hexdigest()})
            self.assertEqual(protocol.support.MAX_FILE,16*1024*1024)

    def test_oversized_protocol_executable_rejects_before_read_or_hash(self):
        import protocol_capture as protocol
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            args=SimpleNamespace(output=root,native_target=root/'native')
            binary=protocol.binary(args);binary.parent.mkdir(parents=True)
            with binary.open('wb') as stream:stream.truncate(512*1024*1024+1)
            with patch.object(protocol.capture,'tools',return_value=()), \
                 patch.object(protocol.capture,'quiet',return_value={}), \
                 patch.object(Path,'open',side_effect=AssertionError('payload opened')) as opened, \
                 patch.object(protocol.support,'digest',side_effect=AssertionError('payload hashed')) as hashed:
                with self.assertRaisesRegex(ValueError,'bounded regular file'):
                    protocol.state(args)
            opened.assert_not_called();hashed.assert_not_called()


class NativeZeroTestControl(unittest.TestCase):
    def test_native_success_with_zero_actual_tests_cannot_confirm_hardcoded_counts(self):
        import capture
        from capture_support import identity
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)
            log=output/'native-original.stdout'
            log.write_text('test result: ok. 0 passed; 0 failed; 0 ignored;\n')
            record={'status':'passed','test_groups':3,'actual_export_calls':177,'host_abi_rows':120,
                'groups':[{'control':c,'stdout':identity(log),'observations':[]} for c in capture.CONTROLS]}
            with self.assertRaisesRegex(ValueError,'one actual native test'):
                capture.verify_native(SimpleNamespace(output=output),record)


if __name__=='__main__':
    unittest.main()
