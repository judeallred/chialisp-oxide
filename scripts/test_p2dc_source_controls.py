"""Constructed protocol/recipe controls; no compiler, native policy or VM runs.

The synthetic retained packet intentionally supplies opaque IR and observed
results without executing them. Its positive is reader consistency, never an
execution, artifact-preservation, authorization, SDK or proof result.
"""
from pathlib import Path
import ast
import copy
import json
import os
import shutil
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

import p2dc_source_controls as c

COMPILER = ('rustc 1.99.0-nightly (daf2e5e18 2026-07-13)\n'
    'binary: rustc\ncommit-hash: daf2e5e18b3abc4311d9da3d96fd64138f43d508\n'
    'commit-date: 2026-07-13\nhost: aarch64-apple-darwin\n'
    'release: 1.99.0-nightly\nLLVM version: 22.1.8\n')


def encoded(value):return (json.dumps(value,indent=2)+'\n').encode()


def put(base,path,payload):
    destination=base/path;destination.parent.mkdir(parents=True,exist_ok=True)
    destination.write_bytes(payload);return destination


def packet(base):
    """Entirely constructed receipt, including purported observations/commands."""
    source=c.ROOT;package=Path('/constructed/p2dc-controls-package');capture=Path('/constructed/p2dc-control-capture')
    files,raw,witnesses=c.package_files(source);files['Cargo.lock']=b'version = 4\n'
    for name,payload in files.items():put(base,'package/'+name,payload)
    baseline,_,_,_=c.inputs()
    formatter=dict(path='/constructed/toolchain/bin/rustfmt',sha256='1'*64,version='constructed rustfmt identity',
        argv=['/constructed/toolchain/bin/rustfmt','--edition','2024','--emit','stdout','--config','skip_children=true'],
        relations=[dict(path=f'src/{n}.rs',raw_sha256=c.digest((baseline if n=='baseline' else raw[n]).encode()),formatted_sha256=c.digest(files[f'src/{n}.rs'])) for n in sorted(c.NAMES)])
    prepared=dict(formatter=formatter,source_files=[dict(path=p,sha256=c.digest(v)) for p,v in sorted(files.items())],witnesses=witnesses)
    window=c.source_window();tools={n:dict(path='/constructed/target/debug/'+n,sha256=c.digest(n.encode())) for n in c.TOOLS}
    log=''.join('P2DC_MUTATION_NATIVE '+json.dumps(dict(name=s['name'],witness=s['witness'],baseline=s['baseline'],actual=s['mutant']))+'\n' for s in witnesses)+'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n'
    put(base,'native.log',log.encode())
    native=dict(argv=['rustup','run','nightly-2026-07-14','cargo','test','--manifest-path',str(package/'Cargo.toml'),'--locked','--offline','--','--nocapture'],exit_code=0,native_test_groups=1,native_policy_observations=3,log='native.log',log_sha256=c.digest(log.encode()))
    native_commands=[]
    for flags,tail in [(['generate-lockfile','--offline'],[]),(['fmt'],['--all','--check']),(['check'],['--locked','--offline']),(['test'],['--locked','--offline','--','--nocapture']),(['clippy'],['--locked','--offline','--all-targets','--','-D','warnings'])]:
        native_commands.append(dict(argv=['rustup','run','nightly-2026-07-14','cargo',*flags,'--manifest-path',str(package/'Cargo.toml'),*tail],exit_code=0,log_sha256=native['log_sha256'] if flags==['test'] else '2'*64))
    receipt=dict(schema_version=1,protocol='chialisp-oxide-p2dc-controls-native/v1',status='native_phase_passed_before_LAST_host_build',source_root=str(source),package_root=str(package),toolchain='nightly-2026-07-14',source_window=window,prepared=prepared,native=native,commands=native_commands)
    put(base,'native-receipt.json',encoded(receipt))
    quiet=dict(schema_version=1,protocol='chialisp-oxide-p2dc-controls-quiet-window/v1',status='LAST_host_build_terminal_tools_quiet',source_root=str(source),package_root=str(package),compiler=COMPILER,toolchain='nightly-2026-07-14',source_window=window,tools=tools,native_receipt_sha256=c.sha(base/'native-receipt.json'),scope='CONSTRUCTED receipt; no build ran.')
    put(base,'last-host-window.json',encoded(quiet))
    context=dict(source_root=str(source),package_root=str(package),capture_root=str(capture),compiler=COMPILER,toolchain='nightly-2026-07-14',source_window=window,tools=tools,last_host_window=dict(path='/constructed/last-host-window.json',sha256=c.sha(base/'last-host-window.json')))
    bundles={};runs=[];commands=[];paths=set('package/'+p for p in files)|{'native.log','native-receipt.json','last-host-window.json'}
    blob=bytes.fromhex('ff0180');tree=c.sha_tree(c.deserialize(blob)).hex()
    def record(role,argv,result):
        index=len(commands);stdout=f'logs/{index:02}.stdout';stderr=f'logs/{index:02}.stderr'
        put(base,stdout,b'constructed Cargo log\n' if result is None else encoded(result));put(base,stderr,b'')
        commands.append(dict(role=role,argv=list(map(str,argv)),exit_code=0,stdout=stdout,stdout_sha256=c.sha(base/stdout),stderr=stderr,stderr_sha256=c.sha(base/stderr)));paths.update((stdout,stderr))
    for mode in ('off','on'):
        entries=[];recompiled=[]
        for name in c.NAMES:
            entry=f'chialisp_oxide_p2dc_controls::{name}::puzzle'
            # Deliberately only a metadata fixture; ordinary Program validation
            # belongs to the separately executed producer, never these tests.
            program=dict(ir_version=5,environment='raw',program=dict(name=entry,parameters=[dict(name='environment',ty=dict(kind='tree'))],result=dict(kind='pair',first=dict(kind='tuple',items=[dict(kind='int',bits=64,signed=False),dict(kind='tree'),dict(kind='bytes',len=32)]),rest=dict(kind='tree')),body=dict(kind='constructed_not_executed'),functions=[]))
            ir=f'{name}.ir.json';clvm=f'{name}.clvm.hex'
            put(base,f'bundles/{mode}/{ir}',encoded(program));put(base,f'bundles/{mode}/{clvm}',b'ff0180\n')
            artifact=dict(backend='rust_mir',optimize_lir=mode=='on',bytes=3,clvm_hex=blob.hex(),tree_hash=tree,disassembly='(q)')
            entries.append(dict(entry=entry,ir=ir,clvm=clvm,artifact=artifact))
            compiled=[dict(optimize=flag,bytes=3,clvm_hex=blob.hex(),tree_hash=tree) for flag in (False,True)]
            recompiled.append(dict(entry=entry,ir_sha256=c.sha(base/f'bundles/{mode}/{ir}'),artifacts=compiled));paths.update((f'bundles/{mode}/{ir}',f'bundles/{mode}/{clvm}'))
        manifest=dict(analysis_target='aarch64-apple-darwin',cargo_options=c.options(package),compiler=COMPILER,driver_sha256=tools['chialisp-oxide-driver']['sha256'],entries=entries,manifest_path=str(package/'Cargo.toml'),package=c.PACKAGE,package_id=f'path+{package.as_uri()}#{c.PACKAGE}@0.1.0',rue_revision='245c107a8e795229ea08097142f005a4c0d91075',schema_version=1,status='wip_prototype_not_ready_for_use',toolchain='nightly-2026-07-14',workspace_root=str(package))
        path=f'bundles/{mode}/manifest.json';put(base,path,encoded(manifest));paths.add(path)
        bundles[mode]=dict(manifest=path,manifest_sha256=c.sha(base/path),recompiled=recompiled)
        record(f'export-{mode}',[tools['cargo-chialisp-oxide']['path'],'build',*c.options(package),'--output-dir',str(capture/f'fresh-{mode}')]+(['--no-optimize'] if mode=='off' else []),None)
        for name,entry,compiled in zip(c.NAMES,entries,recompiled,strict=True):
            record(f'recompile-{mode}-{name}',[tools['chialisp-oxide-verifier']['path'],str(capture/f'bundles/{mode}'/entry['ir']),'--artifacts'],compiled['artifacts'])
        for spec in witnesses:
            for name in ('baseline',spec['name']):
                expected=copy.deepcopy(spec['baseline'] if name=='baseline' else spec['mutant'])
                execution=dict(outcome=expected,cost=7,error=None) if expected['status']=='accepted' else dict(outcome=expected,cost=None,error='constructed Raise',failure=dict(kind='program',code='raise'))
                runs.append(dict(mode=mode,name=name,witness=spec['witness'],solution=spec['solution'],artifact_sha256=c.digest(blob),execution=execution))
                path=f'{mode}-{name}-{spec["name"]}-solution.json';put(base,path,encoded([spec['solution']]));paths.add(path)
                record(f'vm-{mode}-{name}-{spec["name"]}',[tools['chialisp-oxide']['path'],'run-batch',str(capture/f'bundles/{mode}/{name}.clvm.hex'),'--solutions',str(capture/path),'--max-cost','1000000'],[execution])
    report=dict(schema_version=1,protocol=c.PROTOCOL,status='actual_source_controls_captured',scope=c.SCOPE,context=context,prepared=prepared,native=native,bundles=bundles,runs=runs,commands=commands,files=[dict(path=p,sha256=c.sha(base/p)) for p in sorted(paths)])
    return report


class ProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory=tempfile.TemporaryDirectory(prefix='p2dc-CONSTRUCTED-controls-');cls.base=Path(cls.directory.name);cls.report=packet(cls.base)

    @classmethod
    def tearDownClass(cls):cls.directory.cleanup()

    def check(self,report=None,base=None):return c.validate_recorded(self.report if report is None else report,self.base if base is None else base)

    def bad(self,action):
        report=copy.deepcopy(self.report);action(report)
        self.assertNotEqual(c.canonical(report),c.canonical(self.report))
        with self.assertRaises((ValueError,TypeError,KeyError)):self.check(report)

    def changed_file(self,path,action):
        with tempfile.TemporaryDirectory(prefix='p2dc-CONSTRUCTED-mutant-') as temporary:
            base=Path(temporary)/'evidence';shutil.copytree(self.base,base);report=copy.deepcopy(self.report)
            action(report,base/path)
            for row in report['files']:
                if row['path']==path:row['sha256']=c.sha(base/path)
            if path.startswith('bundles/') and path.endswith('manifest.json'):report['bundles'][path.split('/')[1]]['manifest_sha256']=c.sha(base/path)
            with self.assertRaises((ValueError,TypeError,KeyError)):self.check(report,base)

    def test_constructed_positive_no_processes(self):
        with patch.object(subprocess,'run',side_effect=AssertionError('no process credit')),patch.object(subprocess,'Popen',side_effect=AssertionError('no process credit')):
            value=self.check()
        self.assertEqual([value[k] for k in ('native_test_groups','native_policy_observations','exports','export_modes','retained_VM_observations')],[1,3,4,8,12]);self.assertEqual(len(self.report['files']),86)

    def test_closed_schema_and_boolean_counts(self):
        for key,value in [('schema_version',True),('status','native_only'),('SDK_submissions',60)]:
            with self.subTest(key=key):self.bad(lambda r:r.update({key:value}))
        for key,value in [('native_test_groups',3),('native_test_groups',True),('native_policy_observations',True)]:
            with self.subTest(key=key,value=value):self.bad(lambda r:r['native'].update({key:value}))

    def test_source_and_authoring_staleness(self):
        self.bad(lambda r:r['context']['source_window']['sources'][0].update(sha256='0'*64))
        self.bad(lambda r:r['context']['source_window']['authoring']['exports'].pop(next(iter(r['context']['source_window']['authoring']['exports']))))

    def test_context_host_paths_toolchain(self):
        for key,value in [('compiler',COMPILER.replace('host: aarch64-apple-darwin','host: constructed-other')),('toolchain','nightly-2099-01-01'),('package_root','/constructed/elsewhere'),('source_root','/constructed/../source')]:
            with self.subTest(key=key):self.bad(lambda r:r['context'].update({key:value}))
        self.bad(lambda r:r['context']['tools']['chialisp-oxide-driver'].update(path='/other/debug/chialisp-oxide-driver'))

    def test_manifest_independent_binding_corruptions(self):
        for key,value in [('analysis_target','constructed-other'),('cargo_options',c.options(Path('/constructed/p2dc-controls-package'))[:-1]),('compiler',COMPILER.replace('LLVM version: 22.1.8','LLVM version: 22.1.9')),('driver_sha256','0'*64),('workspace_root','/constructed/other'),('manifest_path','/constructed/other/Cargo.toml'),('package_id','path+file:///other#wrong@0.1.0'),('schema_version',True),('rue_revision','0'*40)]:
            def change(report,path,key=key,value=value):
                manifest=c.load(path);manifest[key]=value;path.write_bytes(encoded(manifest))
            with self.subTest(key=key):self.changed_file('bundles/on/manifest.json',change)

    def test_manifest_modes_and_export_set(self):
        for mutate in [lambda m:m['entries'].pop(),lambda m:m['entries'][0]['artifact'].update(optimize_lir=False),lambda m:m['entries'][0].update(entry=m['entries'][1]['entry'])]:
            def change(report,path,mutate=mutate):m=c.load(path);mutate(m);path.write_bytes(encoded(m))
            self.changed_file('bundles/on/manifest.json',change)

    def test_artifact_and_recompile_receipts(self):
        self.bad(lambda r:r['bundles']['on']['recompiled'][0].update(ir_sha256='0'*64))
        self.bad(lambda r:r['bundles']['on']['recompiled'][0]['artifacts'][1].update(tree_hash='0'*64))
        self.bad(lambda r:r['bundles']['on']['recompiled'][0]['artifacts'][1].update(optimize=1))
        self.changed_file('bundles/on/baseline.clvm.hex',lambda r,p:p.write_bytes(b'80\n'))

    def test_raw_abi_and_source_ir_consistency(self):
        for field,value in [('environment','strict'),('ir_version',True)]:
            def change(r,p,field=field,value=value):v=c.load(p);v[field]=value;p.write_bytes(encoded(v))
            self.changed_file('bundles/on/baseline.ir.json',change)
        def change(r,p):v=c.load(p);v['program']['result']={'kind':'tree'};p.write_bytes(encoded(v))
        self.changed_file('bundles/off/baseline.ir.json',change)

    def test_exact_recipe_format_and_native_template(self):
        for path in ['package/src/reject_ignored_improper_tail.rs','package/tests/native.rs','package/Cargo.toml']:
            def change(r,p,path=path):
                p.write_bytes(c.read(p)+b'\n');relative=path.removeprefix('package/')
                for row in r['prepared']['source_files']:
                    if row['path']==relative:row['sha256']=c.sha(p)
            with self.subTest(path=path):self.changed_file(path,change)
        self.bad(lambda r:r['prepared']['formatter']['relations'][0].update(formatted_sha256='0'*64))

    def test_pre_execution_prepared_metadata_checks(self):
        c.prepared_metadata(self.report['prepared'],self.base/'package',locked=True)
        for mutate in [lambda v:v['formatter']['argv'].pop(),lambda v:v['formatter']['relations'][0].update(raw_sha256='0'*64),lambda v:v['witnesses'][0].update(mutant={'status':'rejected'}),lambda v:v['source_files'].pop()]:
            value=copy.deepcopy(self.report['prepared']);mutate(value)
            with self.assertRaises(ValueError):c.prepared_metadata(value,self.base/'package',locked=True)

    def test_native_rows_meaningful_corruptions(self):
        _,_,_,witnesses=c.inputs();log=c.read(self.base/'native.log').decode();self.assertEqual(len(c.native_rows(log,witnesses)),3)
        for value in [log.replace('1 passed','3 passed'),log.split('\n',1)[1],log+log.splitlines()[0]+'\n',log.replace('wrong_atom_domain','wrong_other_domain',1)]:
            self.assertNotEqual(value,log)
            with self.assertRaises(ValueError):c.native_rows(value,witnesses)

    def test_vm_outputs_typed_faults_and_resources(self):
        self.bad(lambda r:r['runs'][0]['execution']['outcome'].update(output_hex='80'))
        index=next(i for i,row in enumerate(self.report['runs']) if row['execution']['outcome']['status']=='rejected')
        self.bad(lambda r:r['runs'][index]['execution']['failure'].update(code='path_into_atom'))
        self.bad(lambda r:r['runs'][index].update(execution=dict(outcome={'status':'inconclusive'},cost=None,error='constructed cost exhausted',failure=dict(kind='resource',code='cost_exceeded'))))
        self.bad(lambda r:r['runs'][0].update(artifact_sha256='0'*64))

    def test_command_order_options_and_streams(self):
        self.bad(lambda r:r['commands'][0]['argv'].remove('--offline'))
        self.bad(lambda r:r['commands'][1].update(exit_code=-9))
        self.bad(lambda r:r['commands'][1].update(role='recompile-off-other'))
        self.changed_file('logs/01.stdout',lambda r,p:p.write_bytes(encoded([])))
        self.changed_file('logs/01.stderr',lambda r,p:p.write_bytes(b'warning: constructed\n'))
        self.changed_file('off-baseline-wrong_atom_domain-solution.json',lambda r,p:p.write_bytes(encoded([{'atom':'00'}])))

    def test_retained_inventory_and_receipt_linkage(self):
        self.bad(lambda r:r['files'].pop())
        self.bad(lambda r:r['context']['last_host_window'].update(sha256='0'*64))
        def change(r,p):v=c.load(p);v['schema_version']=True;p.write_bytes(encoded(v));r['context']['last_host_window']['sha256']=c.sha(p)
        self.changed_file('last-host-window.json',change)
        def change(r,p):v=c.load(p);v['commands'][1]['argv'].remove('--check');p.write_bytes(encoded(v))
        self.changed_file('native-receipt.json',change)


class BoundaryTests(unittest.TestCase):
    def test_declared_source_target_admission_is_closed(self):
        c.declared_source_entries('puzzles',{'package':{},'lib':{'path':'lib.rs'}})
        c.declared_source_entries('crates/example',{'package':{'build':'build.rs'},'lib':{},'bin':[{'path':'src/bin/main.rs'}],'example':[{'path':'examples/demo.rs'}],'test':[{'path':'tests/check.rs'}],'bench':[{'path':'benches/measure.rs'}]})
        for kind in ('lib','bin','example','test','bench'):
            for path in ('target/foo.rs','../escape.rs','/absolute.rs','src/../target.rs','src/non-rust.txt'):
                entries={'path':path};metadata={'package':{},kind:entries if kind=='lib' else [entries]}
                with self.subTest(kind=kind,path=path),self.assertRaises(ValueError):c.declared_source_entries('crates/example',metadata)
        for build in ('target/build.rs','../build.rs',True,0):
            with self.subTest(build=build),self.assertRaises(ValueError):c.declared_source_entries('crates/example',{'package':{'build':build}})

    def test_source_window_rejects_custom_declared_inputs_before_capture(self):
        original=c.vectors.read;path='crates/chialisp-oxide-cli/Cargo.toml'
        for fragment in ('\n[lib]\npath="target/custom.rs"\n','\n[[example]]\nname="custom"\npath="target/custom.rs"\n','\n[[test]]\nname="custom"\npath="target/custom.rs"\n','\n[[bench]]\nname="custom"\npath="target/custom.rs"\n'):
            def changed(root,name,fragment=fragment):return original(root,name)+(fragment.encode() if name==path else b'')
            with self.subTest(fragment=fragment),patch.object(c.vectors,'read',side_effect=changed),self.assertRaisesRegex(ValueError,'unsupported declared'):c.source_window()
        def changed(root,name):
            value=original(root,name)
            return value.replace(b'path = "src/main.rs"',b'path = "target/main.rs"') if name==path else value
        with patch.object(c.vectors,'read',side_effect=changed),self.assertRaisesRegex(ValueError,'unsupported declared bin'):c.source_window()
        window=c.source_window();paths={r['path'] for r in window['sources']}
        self.assertIn('crates/chialisp-oxide-corpus/examples/p2_singleton_controls.rs',paths)
        self.assertIn('crates/chialisp-oxide-driver/tests/cons.rs',paths)

    def test_exact_recipes_closed_formatter_relation(self):
        baseline,raw,sources,witnesses=c.inputs();self.assertEqual(c.digest(c.read(c.ROOT/c.TEMPLATE)),c.TEMPLATE_SHA)
        self.assertEqual(sources['src/baseline.rs'],baseline)
        for name in c.NAMES[1:3]:self.assertEqual(sources[f'src/{name}.rs'],raw[name])
        self.assertNotEqual(sources['src/reject_ignored_improper_tail.rs'],raw[c.NAMES[-1]])
        self.assertTrue(all(row['baseline']!=row['mutant'] for row in witnesses));self.assertEqual(witnesses[-1]['mutant'],{'status':'rejected'})

    def test_bounded_regular_io_and_confinement(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);(base/'valid').write_bytes(b'abc');self.assertEqual(c.read(base/'valid'),b'abc')
            for path in [base,base/'valid']:
                with self.assertRaises(ValueError):c.read(path,limit=2)
            if hasattr(os,'mkfifo'):
                os.mkfifo(base/'fifo')
                with self.assertRaises(ValueError):c.read(base/'fifo')
            (base/'link').symlink_to(base/'valid')
            for path in ['../valid','/valid','link']:
                with self.assertRaises(ValueError):c.relative(base,path)

    def test_compiler_exact_fields(self):
        self.assertEqual(c.compiler_host(COMPILER),'aarch64-apple-darwin')
        for value in [COMPILER+'host: other\n',COMPILER.replace('binary: rustc','host: other'),COMPILER.replace('22.1.8','22.1.9'),COMPILER.rstrip()]:
            with self.assertRaises(ValueError):c.compiler_host(value)

    def test_fresh_loader_complete_independent_requested_identity(self):
        observed=[];private=dict(exports={n:dict(entry=f'controls::{n}') for n in c.NAMES})
        loaded=types.SimpleNamespace(manifest={'entries':[{}]*4},manifest_path=Path('/constructed/manifest.json'))
        def load(*args,**kwargs):
            observed.append(kwargs);self.assertEqual(kwargs['current_authoring'](),private);return loaded
        with patch.object(c,'source_window',return_value={'fresh':'source metadata'}),patch.object(c,'private_snapshot',return_value=private),patch.object(c.cargo_bundle,'load_declared_export',side_effect=load):
            value=c.fresh_bundle(Path('/constructed/output'),Path('/constructed/package'),root=c.ROOT,compiler=COMPILER,driver_sha256='3'*64,toolchain='nightly-2026-07-14',optimize=False)
        self.assertIs(value,loaded);self.assertEqual(len(observed),4)
        for row in observed:
            self.assertEqual(row['cargo_options'],c.options(Path('/constructed/package')));self.assertEqual(row['compiler'],COMPILER);self.assertEqual(row['driver_sha256'],'3'*64);self.assertIs(row['optimize'],False)

    def test_source_window_and_runner_syntax_order(self):
        window=c.source_window();paths={row['path'] for row in window['sources']}
        import corpus_append
        self.assertEqual(set(window['authoring']['exports']),set(corpus_append.current_slugs()));self.assertTrue(set(c.CODE)<=paths)
        for name in ['run-p2dc-source-controls.py','check-p2dc-source-controls.py']:
            tree=ast.parse(c.read(c.ROOT/'scripts'/name));self.assertIsInstance(tree,ast.Module)
        text=c.read(c.ROOT/'scripts/run-p2dc-source-controls.py').decode()
        self.assertLess(text.index('actual=command(argv'),text.index('package.mkdir(parents=True)'))
        self.assertLess(text.index("c.closed(quiet,"),text.index("compiler=command(['rustup'"))
        self.assertIn("(['fmt'],['--all','--check'])",text)


if __name__=='__main__':unittest.main()
