#!/usr/bin/env python3
"""Finite actual CLI replays against independent recipe outcomes, no SDK credit."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

BASE = Path(__file__).resolve().parent
sys.path.insert(0,str(BASE.parents[1]/'scripts'))
sys.path.append(str(BASE.parent/'authenticated-continuations'))
import bounded_process
import raw_tree
from vm_failures import check_execution
import oracle as o


def integer(value):
    if value == 0: return b''
    raw = value.to_bytes((value.bit_length()+7)//8,'big')
    return b'\0'+raw if raw[0]&128 else raw


def output_hex(conditions):
    return raw_tree.serialize(o.proper(*(o.proper(*(integer(value) if type(value) is int else bytes.fromhex(value)
        for value in condition)) for condition in conditions))).hex()


def vectors():
    rows=[]
    module='aa'*32
    def env(row, **changes):
        value=dict(module=module,head=row['head'],count=str(row['remaining']),first=row['first'],middle=row['middle'],second=row['second'],next=row['next'])
        value.update(changes)
        return f"((0x{value['module']} 0x{value['head']} {value['count']}) ((0x{value['first']} 0x{value['middle']}) (0x{value['second']} 0x{value['next']})))"
    def add(label,solution,conditions=None):
        rows.append(dict(label=label,solution=solution,expected=dict(status='accepted',output_hex=output_hex(conditions)) if conditions is not None else dict(status='rejected'),failure=None if conditions is not None else dict(kind='program',code='raise')))
    for count in (2,4,6,8):
        recipients=[bytes([index+1])*32 for index in range(count)]
        for index,row in enumerate(o.plan(bytes.fromhex(module),bytes.fromhex(module),recipients)):
            add(f'complete queue {count} batch {index}',env(row),row['conditions'])
    first,last=o.fixtures()['steps']
    for label,kwargs in [
        ('wrong first recipient',dict(first='99'*32)),('wrong second recipient',dict(second='99'*32)),
        ('wrong first opening digest',dict(middle='99'*32)),('wrong second opening digest',dict(next='99'*32)),
        ('wrong committed head',dict(head='99'*32)),('swapped recipients',dict(first=first['second'],second=first['first'])),
        ('early terminal count',dict(count='2')),('zero count',dict(count='()')),('count one',dict(count='1')),
        ('odd count three',dict(count='3')),('odd count seven',dict(count='7')),('count nine',dict(count='9')),
        ('count u8 maximum',dict(count='255')),('count outside u8',dict(count='256')),('negative count',dict(count='-1')),
        ('padded count',dict(count='0x0004')),('pair count',dict(count='(4)')),
        ('short module',dict(module='aa'*31)),('long module',dict(module='aa'*33)),
        ('short head',dict(head='99'*31)),('long first recipient',dict(first='11'*33)),
        ('short middle',dict(middle='99'*31)),('long second recipient',dict(second='22'*33)),
        ('short next',dict(next='99'*31)),
    ]:add(label,env(first,**kwargs))
    add('excess terminal continuation',env(last,count='4'))
    recipient=bytes.fromhex('11'*32);middle=o.item(recipient,o.terminal());head=o.item(recipient,middle)
    duplicate=dict(remaining=2,head=head.hex(),first=recipient.hex(),middle=middle.hex(),second=recipient.hex(),next=o.terminal().hex())
    add('duplicate output with coherent openings',env(duplicate))
    middle_terminal=dict(remaining=2,head=o.item(recipient,o.terminal()).hex(),first=recipient.hex(),middle=o.terminal().hex(),second='22'*32,next=o.terminal().hex())
    add('first opening terminates early',env(middle_terminal))
    for label,solution in [
        ('nil environment','()'),('atom environment','1'),
        ('missing batch',f"((0x{module} 0x{first['head']} 4))"),
        ('extra environment field',env(first)[:-1]+' ())'),
        ('improper environment',env(first)[:-1]+' . 1)'),
        ('nil terms',env(first).replace(f'(0x{module} 0x{first["head"]} 4)','()')),
        ('extra terms field',env(first).replace(' 4)',' 4 5)',1)),
        ('improper terms',env(first).replace(' 4)',' 4 . 5)',1)),
        ('pair module',env(first).replace('0x'+module,'(1)',1)),
        ('nil batch',f"((0x{module} 0x{first['head']} 4) ())"),
        ('one opening',env(first).replace(f" (0x{first['second']} 0x{first['next']})",'')),
        ('extra batch opening',env(first)[:-2]+' ()))'),
        ('improper batch',env(first)[:-2]+' . 1))'),
        ('nil first opening',env(first).replace(f"(0x{first['first']} 0x{first['middle']})",'()')),
        ('nil second opening',env(first).replace(f"(0x{first['second']} 0x{first['next']})",'()')),
        ('extra first opening field',env(first).replace(f"0x{first['middle']})",f"0x{first['middle']} 1)",1)),
        ('extra second opening field',env(first).replace(f"0x{first['next']})",f"0x{first['next']} 1)",1)),
        ('improper first opening',env(first).replace(f"0x{first['middle']})",f"0x{first['middle']} . 1)",1)),
        ('improper second opening',env(first).replace(f"0x{first['next']})",f"0x{first['next']} . 1)",1)),
        ('pair first recipient',env(first).replace('0x'+first['first'],'(1)',1)),
        ('pair second recipient',env(first).replace('0x'+first['second'],'(1)',1)),
    ]:add(label,solution)
    # Keep reviewed ABI failure-code divergences explicit. These tails reach
    # strict nil equality in off mode, whereas reference/on shape guards raise.
    for row in rows:
        if row['label'] in ('extra environment field','extra terms field','extra batch opening','extra first opening field','extra second opening field'):
            row['failure_by_mode']={'rust_off':dict(kind='program',code='invalid_op_arg')}
    changed=bytes.fromhex('bb'*32)
    conditions=o.step(changed,bytes.fromhex(first['head']),4,bytes.fromhex(first['first']),bytes.fromhex(first['middle']),bytes.fromhex(first['second']),bytes.fromhex(first['next']))
    add('wrong selected module accepted by template; host admission required',env(first,module=changed.hex()),conditions)
    return rows


def execute(cli, programs, output):
    if set(programs) not in ({'chialisp'},{'chialisp','rust_on','rust_off'}):raise ValueError('complete three-mode set or explicit reference-only execution required')
    output.mkdir(parents=True,exist_ok=False)
    rows=vectors()
    (output/'vectors.json').write_text(json.dumps(rows,indent=2)+'\n')
    solutions=output/'solutions.json';solutions.write_text(json.dumps([row['solution'] for row in rows]))
    report=dict(schema=1,scope='finite primary VM observations, no SDK spends/authentication/proof',cases=len(rows),programs={},mode_scope='reference_only' if set(programs)=={'chialisp'} else 'three_mode')
    for mode,path in programs.items():
        payload=path.read_bytes()
        result=bounded_process.run([cli.resolve(),'run-batch',path.resolve(),'--solutions',solutions.resolve(),'--max-cost','10000000'],check=True,timeout=90)
        (output/(mode+'.json')).write_bytes(result.stdout)
        observations=json.loads(result.stdout)
        if len(observations)!=len(rows):raise ValueError('complete result count')
        costs=[]
        for case,actual in zip(rows,observations):
            if check_execution(actual,mode+'/'+case['label'])!=case['expected']:raise ValueError(f"{mode}/{case['label']}: exact independent output mismatch: {actual}")
            failure=case.get('failure_by_mode',{}).get(mode,case['failure'])
            if failure is not None and actual.get('failure')!=failure:raise ValueError(f"{mode}/{case['label']}: expected typed {failure}, got {actual}")
            if case['expected']['status']=='accepted':costs.append(dict(label=case['label'],cost=actual['cost']))
        report['programs'][mode]=dict(program_hex_sha256=hashlib.sha256(payload).hexdigest(),serialized_bytes=len(bytes.fromhex(payload.decode().strip())),accepted_costs=costs)
    report['status']='passed'
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--rust-on',type=Path)
    parser.add_argument('--rust-off',type=Path)
    parser.add_argument('--reference-only',action='store_true')
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.reference_only:
        if args.rust_on or args.rust_off:parser.error('reference-only forbids Rust programs')
        programs=dict(chialisp=args.reference)
    else:
        if not args.rust_on or not args.rust_off:parser.error('both Rust modes required without reference-only')
        programs=dict(rust_on=args.rust_on,rust_off=args.rust_off,chialisp=args.reference)
    print(json.dumps(execute(args.cli,programs,args.output),indent=2))
