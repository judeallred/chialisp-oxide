"""Bounded consistency reader for retained finite raw-action observations.

No execution authentication or semantic equivalence follows from this reader.
Program failures retain their actual typed codes; native ABI/panic stages are
independent categories and do not prescribe cross-language failure identity.
"""
import collections
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parents[1]/'scripts'))
from registry_codegen_evidence import decode
from vm_failures import check_execution
from capture_support import read, require, identity, same_json
from compiled_codec import inspect
import vectors

CONTROLS = ('original','increment_no_change','reset_drop_signature')
EXPORTS = ('increment','reset')
MEMPOOL_FLAGS = 0x217  # Pinned clvmr 0.21.0 MEMPOOL_MODE, not SDK generator flags.


def validate_inventory(packet):
    require(type(packet) is dict and set(packet)=={'protocol','compiler','driver_declaration','artifacts'},'exact artifact envelope')
    require(packet['protocol']=='chialisp-oxide-action-counter-artifacts/v2','artifact protocol')
    require(type(packet['compiler']) is str and packet['compiler'].startswith('rustc ')
            and packet['driver_declaration']==packet['compiler'].splitlines()[0]+'\n','compiler/driver declaration')
    expected=[(c,m,e) for c in CONTROLS for m in ('off','on') for e in EXPORTS]
    expected += [('original','chialisp',e) for e in EXPORTS]
    expected += [('original',m,e) for m in ('rue-off','rue-on') for e in EXPORTS]
    require(type(packet['artifacts']) is list and len(packet['artifacts'])==18,'exact complete18 artifact inventory count')
    remaining=16*1024*1024
    for row,(c,m,e) in zip(packet['artifacts'],expected,strict=True):
        require(type(row) is dict and row.get('id')==c+'/'+m+'/'+e and row.get('control')==c
                and row.get('mode')==m and row.get('export')==e,'ordered exact source/mode/export inventory')
        text=row.get('clvm_hex')
        require(type(text) is str and 0<len(text)<=2*1024*1024 and len(text)%2==0
                and all(b in '0123456789abcdef' for b in text),'bounded canonical artifact hex')
        length=len(text)//2
        require(length<=remaining,'artifact aggregate byte charge before decoding')
        remaining-=length
        require(json.dumps(inspect(bytes.fromhex(text)),sort_keys=True)==json.dumps(row['inspection'],sort_keys=True),'independently bounded complete artifact identity/profile and numeric types')
        if m=='chialisp':
            require(set(row)=={'id','control','mode','export','clvm_hex','inspection'},'exact Chialisp artifact fields')
        elif m in ('rue-off','rue-on'):
            require(set(row)=={'id','control','mode','export','optimize_lir','clvm_hex','inspection','source'},'exact actual Rue source artifact fields')
            require(type(row['optimize_lir']) is bool and row['optimize_lir']==(m=='rue-on'),'actual Rue optimization mode')
            require(same_json(row['source'],identity(HERE.parents[1]/'puzzles'/('counter-'+e)/'puzzle.rue')),'fresh actual paired Rue source')
        else:
            require(set(row)=={'id','control','mode','export','clvm_hex','inspection','ir','manifest','source_receipt'},'exact Rust artifact fields')
    return packet['artifacts']


def validate_report(report, packet):
    modules=validate_inventory(packet)
    require(type(report) is dict and set(report)=={'protocol','vm_failure_protocol','vm','flags','max_cost','allocator_limit_bytes','observations'},'exact replay envelope')
    require(report['protocol']=='chialisp-oxide-action-counter-raw-vm/v1'
            and report['vm_failure_protocol']=='clvmr-0.21.0/v1' and report['vm']=='clvmr 0.21.0','explicit primary VM protocols')
    require(type(report['flags']) is int and report['flags']==MEMPOOL_FLAGS
            and type(report['max_cost']) is int and report['max_cost']==50_000_000
            and type(report['allocator_limit_bytes']) is int and report['allocator_limit_bytes']==32*1024*1024,'exact VM resource/flag profile')
    rows=report['observations']
    require(type(rows) is list and len(rows)==891,'exact finite original/mutant observation count')
    expected=[]
    for module in modules:
        for vector in vectors.cases()['cases']:
            if vector['export']==module['export']:
                oracle=vectors.expected(vector['export'],decode(bytes.fromhex(vector['environment_hex'])),module['control'])
                expected.append((module,vector,oracle))
    counts=collections.Counter();failures=collections.Counter();outcomes={}
    for actual,(module,vector,oracle) in zip(rows,expected,strict=True):
        require(type(actual) is dict and set(actual)=={'module_id','export','name','execution'},'exact observation fields')
        require(actual['module_id']==module['id'] and actual['export']==vector['export'] and actual['name']==vector['name'],'exact ordered module/vector identities')
        result=check_execution(actual['execution'],require_conclusive=True,require_typed=True)
        require(result['status']==oracle['status'],'independent whole-environment acceptance')
        if oracle['status']=='accepted':
            require(result==oracle,'exact independent output bytes, including E/condition preservation')
            require(actual['execution']['cost']<=report['max_cost'],'actual successful cost within profile')
        else:
            failures[(module['control'],module['mode'],actual['execution']['failure']['code'])]+=1
        counts[(module['control'],module['mode'],result['status'])]+=1
        outcomes[(module['id'],vector['name'])]=result
    # Named distinguishing witnesses supplement full-row coverage. These are
    # policy/output differences, never claims that consensus rejects a mutant.
    for mode in ('off','on'):
        for e,control in [('increment','increment_no_change'),('reset','reset_drop_signature')]:
            original=outcomes[('original/'+mode+'/'+e,'nil-count-1')]
            mutant=outcomes[(control+'/'+mode+'/'+e,'nil-count-1')]
            require(original['status']==mutant['status']=='accepted' and original!=mutant,'actual-source distinguishing output')
        require(outcomes[('original/'+mode+'/increment','nil-count-'+str(vectors.MAX_COUNTER))]=={'status':'rejected'}
                and outcomes[('increment_no_change/'+mode+'/increment','nil-count-'+str(vectors.MAX_COUNTER))]['status']=='accepted','identity mutant MAX acceptance versus original overflow')
    return {'original_observations':495,'mutant_observations':396,'primary_vm_observations':len(rows),
            'accepted':sum(n for (c,m,s),n in counts.items() if s=='accepted'),
            'program_rejections':sum(n for (c,m,s),n in counts.items() if s=='rejected'),
            'by_control_mode_status':[{'control':c,'mode':m,'status':s,'count':n} for (c,m,s),n in sorted(counts.items())],
            'observed_failure_codes':[{'control':c,'mode':m,'code':code,'count':n} for (c,m,code),n in sorted(failures.items())],
            'scope':'Finite consistency only; source/native panic categories do not establish exact cross-language VM failure identity; no SDK/live or compiler-preservation credit.'}


def validate_queries(packet, compiler_stdout, driver_stdout):
    """Metadata must agree with separately retained actual tool query bytes."""
    require(type(compiler_stdout) is bytes and type(driver_stdout) is bytes,'exact retained query bytes')
    compiler=compiler_stdout.decode('utf-8')
    driver=driver_stdout.decode('utf-8')
    require(compiler.startswith('rustc ') and compiler.endswith('\n')
            and packet['compiler']==compiler,'packet compiler differs from independent actual rustc query')
    require(driver==compiler.splitlines()[0]+'\n' and packet['driver_declaration']==driver,
            'packet driver declaration differs from independent actual driver query')


def validate_request(request, packet):
    from capture import request_recipe
    require(json.dumps(request,sort_keys=True)==json.dumps(request_recipe(packet),sort_keys=True),
            'exact retained max-cost/module/vector request recipe')


def recorded_queries(args, packet):
    from capture_support import recorded_command
    validate_queries(packet,read(args.output/'compiler-version.stdout'),read(args.output/'driver-version.stdout'))
    for label,argv in [('compiler-version',[str(args.rustc),'-vV']),
                       ('driver-version',[str(args.driver),'--compiler-version'])]:
        recorded_command(args.output,label,argv,target=args.compiler_target,rustc=args.rustc)


def validate_retained_inputs(args, claimed):
    import capture
    from capture_support import retained_identities
    expected=retained_identities(args.output,capture.retained_paths(args))
    require(json.dumps(expected,sort_keys=True)==json.dumps(claimed,sort_keys=True),
            'complete independently rediscovered retained inventory/aggregate bounds')


def recorded(args):
    """Re-admit current source, logs, bundles and both immutable reports; no run."""
    import capture
    from capture_support import identity, retained_identities
    capture.verify(args)
    active_tools=capture.tools(args)
    before=json.loads(read(args.output/'extract-window-before.json'))
    require(same_json(before,json.loads(read(args.output/'extract-window-after.json'))) and same_json(before,capture.quiet(args,active_tools)),'current complete extraction source/tool window')
    replay_before=json.loads(read(args.output/'replay-window-before.json'))
    require(same_json(replay_before,json.loads(read(args.output/'replay-window-after.json'))) and same_json(replay_before['source_tools'],before),'matching replay source/tool windows')
    validate_retained_inputs(args,replay_before['retained'])
    packet=json.loads(read(args.output/'artifacts.json'))
    recorded_queries(args,packet)
    capture.check_commands(args,all_phases=True)
    validate_request(json.loads(read(args.output/'request.json')),packet)
    rust=[]
    for c in CONTROLS:
        for m in ('off','on'):
            rust+=capture.admit_bundle(args,c,m,packet['compiler'],before['tools'][0]['sha256'])
    require(same_json(rust,packet['artifacts'][:12]),'all source-receipted retained Rust bundles readmitted')
    references=json.loads(read(args.output/'compile-reference.stdout'))
    require(references['compiler']=='chialisp 0.4.6' and references['artifacts']==[
        {k:row[k] for k in ('export','clvm_hex')} for row in packet['artifacts'][12:14]],'exact retained independently compiled reference output')
    rue=json.loads(read(args.output/'compile-rue.stdout'))
    require(rue['rue_revision']=='245c107a8e795229ea08097142f005a4c0d91075' and same_json(rue['artifacts'],[{k:r[k] for k in ('mode','export','optimize_lir','clvm_hex')} for r in packet['artifacts'][14:]]), 'retained actual Rue compiler output matches inventory')
    first=read(args.output/'raw-attempt1.stdout');second=read(args.output/'raw-attempt2.stdout')
    require(first==second,'independent raw report repeats byte-identical')
    summary=validate_report(json.loads(first),packet)
    execution=json.loads(read(args.output/'execution.json'))
    require(execution['status']=='raw_vm_executed_repeat_identical' and json.dumps(execution['summary'],sort_keys=True)==json.dumps(summary,sort_keys=True)
            and same_json(execution['first'],identity(args.output/'raw-attempt1.stdout'))
            and same_json(execution['repeat'],identity(args.output/'raw-attempt2.stdout')),'retained final summary/identities')
    print(json.dumps({'status':'recorded_consistency_passed','summary':summary,'scope':'Reader adds no execution authentication or new execution credit.'},indent=2))
