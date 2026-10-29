"""Pure recorded native/typed-VM witness checks, not execution authentication."""
import hashlib
import admission as a
import replay
import source_controls as s
from vm_failures import check_execution


def execution(value,expected,label,*,failure=None):
    outcome=check_execution(value,label)
    a.require(outcome==expected,label+': exact independently expected outcome')
    if outcome['status']=='accepted':
        a.require(value['cost']<=10_000_000,label+': fixed primary VM cap')
    else:
        a.require(value['failure']==failure,label+': exact typed failure specification')


def base(vectors,report,results,program_hex):
    expected=replay.vectors()
    a.require(vectors==expected,'complete independently reconstructed raw-tree vectors')
    a.require(type(report['schema']) is int and report['schema']==1 and report['status']=='passed'
        and type(report['cases']) is int and report['cases']==len(expected),'exact primary VM report schema/case count')
    a.require(set(results)==set(program_hex)==set(report['programs'])=={'rust_on','rust_off','chialisp'},'complete actual program-mode inventory')
    for mode,rows in results.items():
        a.require(type(rows) is list and len(rows)==len(expected),'complete primary VM result inventory')
        costs=[]
        for case,row in zip(expected,rows):
            outcome={k:v for k,v in case['expected'].items() if k!='failure'}
            failure=case.get('failure_by_mode',{}).get(mode,case['expected'].get('failure'))
            execution(row,outcome,mode+'/'+case['label'],failure=failure)
            if outcome['status']=='accepted':costs.append(dict(label=case['label'],cost=row['cost']))
        raw=program_hex[mode]
        a.require(report['programs'][mode]==dict(program_hex_sha256=hashlib.sha256(raw).hexdigest(),
            serialized_bytes=len(bytes.fromhex(raw.decode().strip())),accepted_costs=costs),'program bytes/cost/report relation')


def controls(source):
    a.require(type(source['schema']) is int and source['schema']==1,'source witness schema')
    expected_native=[dict(kind=k,role=role,observed=s.expected(k,role=='mutant')) for k in s.KINDS for role in ('original','mutant')]
    a.require(source['native']==expected_native,'exact independently expected native observation rows')
    keys=[(k,role,mode) for k in s.KINDS for role in ('original','mutant') for mode in ('on','off')]
    rows=source['primary_vm']
    a.require(type(rows) is list and len(rows)==len(keys),'complete actual-source primary VM inventory')
    for key,row in zip(keys,rows):
        a.require(type(row) is dict and set(row)=={'kind','role','mode','execution'} and tuple(row[k] for k in ('kind','role','mode'))==key,'exact primary VM row identity/order')
        expected=s.expected(key[0],key[1]=='mutant')
        if expected['status']=='native_panic':
            execution(row['execution'],dict(status='rejected'),'/'.join(key),failure=dict(kind='program',code='raise'))
        else:execution(row['execution'],expected,'/'.join(key))
