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
    fixture=o.fixtures()
    def env(**changes):
        value=dict(module=fixture['fixture_module_hash'],key=fixture['holder_key'],payout=fixture['holder_payout'],
                   round='1',take='1',next_key=fixture['next_key'],next_payout=fixture['next_payout'])
        value.update(changes)
        return f"((0x{value['module']} 0x{value['key']} 0x{value['payout']} {value['round']}) {value['take']} 0x{value['next_key']} 0x{value['next_payout']})"
    def conditions(round=1,take=True,**changes):
        value=dict(module=bytes.fromhex(fixture['fixture_module_hash']),key=bytes.fromhex(fixture['holder_key']),
                   payout=bytes.fromhex(fixture['holder_payout']),next_key=bytes.fromhex(fixture['next_key']),
                   next_payout=bytes.fromhex(fixture['next_payout']))
        value.update(changes)
        return o.step(value['module'],value['key'],value['payout'],round,take,value['next_key'],value['next_payout'])
    def add(label,solution,output=None):
        rows.append(dict(label=label,solution=solution,expected=dict(status='accepted',output_hex=output_hex(output))
                         if output is not None else dict(status='rejected'),failure=None if output is not None else dict(kind='program',code='raise')))
    for round in range(1,9):
        add(f'claim round {round} emits age/signature conditions, not consensus execution',env(round=str(round),take='()'),conditions(round,False))
        if round<8:add(f'take round {round} emits required topup and reset state',env(round=str(round)),conditions(round))
    for label,changes in [
        ('round zero',dict(round='()')),('round nine',dict(round='9')),('round u8 maximum',dict(round='255')),
        ('round outside u8',dict(round='256')),('negative round',dict(round='-1')),('padded round',dict(round='0x0001')),
        ('pair round',dict(round='(1)')),('take at cap',dict(round='8')),('invalid bool two',dict(take='2')),
        ('padded false',dict(take='0x00')),('padded true',dict(take='0x0001')),('pair bool',dict(take='(1)')),
        ('short module',dict(module='aa'*31)),('long module',dict(module='aa'*33)),
        ('short holder key',dict(key='11'*47)),('long holder key',dict(key='11'*49)),
        ('short holder payout',dict(payout='11'*31)),('long holder payout',dict(payout='11'*33)),
        ('short next key',dict(next_key='22'*47)),('long next key',dict(next_key='22'*49)),
        ('short next payout',dict(next_payout='22'*31)),('long next payout',dict(next_payout='22'*33)),
    ]:add(label,env(**changes))
    normal=env()
    for label,solution in [
        ('nil environment','()'),('atom environment','1'),('missing next payout',normal.rsplit(' ',1)[0]+')'),
        ('extra environment field',normal[:-1]+' ())'),('improper environment',normal[:-1]+' . 1)'),
        ('nil terms',normal.replace(normal[1:normal.index(')')+1],'()',1)),
        ('extra terms',normal.replace(' 1)',' 1 2)',1)),('improper terms',normal.replace(' 1)',' 1 . 2)',1)),
        ('pair holder key',normal.replace('0x'+fixture['holder_key'],'(1)',1)),
        ('pair next payout',normal.replace('0x'+fixture['next_payout'],'(1)',1)),
    ]:add(label,solution)
    add('wrong selected module accepted; initial host admission required',env(module='bb'*32),conditions(module=b'\xbb'*32))
    add('claim ignores decoded next fields',env(take='()',next_payout='99'*32),conditions(take=False,next_payout=b'\x99'*32))
    add('invalid BLS key bytes only produce conditions; consensus required',env(next_key='00'*48),conditions(next_key=b'\0'*48))
    add('new payout changes signed message and successor',env(next_payout='99'*32),conditions(next_payout=b'\x99'*32))
    return rows


def execute(cli, programs, output):
    if set(programs) != {'chialisp'}:raise ValueError('reference-only lane; no Rust artifact admitted')
    output.mkdir(parents=True,exist_ok=False)
    rows=vectors()
    (output/'vectors.json').write_text(json.dumps(rows,indent=2)+'\n')
    solutions=output/'solutions.json';solutions.write_text(json.dumps([row['solution'] for row in rows]))
    report=dict(schema=1,scope='finite primary VM observations, no SDK spends/authentication/proof',cases=len(rows),programs={})
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


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(execute(args.cli,dict(chialisp=args.reference),args.output),indent=2))
