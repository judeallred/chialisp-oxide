"""Execute retained counter CLVM bytes; no compiler or SDK submissions here."""
import sys
sys.dont_write_bytecode = True
from collections import defaultdict
import json
from pathlib import Path
import tempfile

import action_counter_packet as packet
from vm_failures import check_execution


def assembly(raw):
    tree=packet.protocol_oracle.codec.decode(raw)
    pending=[tree];output=[];size=0
    while pending:
        value=pending.pop()
        if type(value) is str:part=value
        elif type(value) is bytes:part='0x'+value.hex() if value else '()'
        else:
            pending.extend((')',value[1],' . ',value[0],'('))
            continue
        size+=len(part)
        packet.require(size<=1024*1024,'bounded exact replay assembly')
        output.append(part)
    return ''.join(output)


def replay(root,cli):
    packet.validate(root)
    data=packet.Packet(root)
    cli=Path(cli).resolve()
    before=packet.support.identity(cli,512*1024*1024)
    artifacts=data.load('raw/artifacts.json')['artifacts']
    vectors=data.load('raw/request.json')['vectors']
    rows=data.load('raw/raw-attempt1.stdout')['observations']
    modules={a['id']:a for a in artifacts}
    vector_map={(v['export'],v['name']):v for v in vectors}
    groups=defaultdict(list)
    for row in rows:
        module=modules[row['module_id']]
        vector=vector_map[(row['export'],row['name'])]
        groups[(module['clvm_hex'],50_000_000)].append((vector['environment_hex'],row['execution']))
    report=data.load('protocol/protocol-attempt1.stdout')
    protocol_count=0
    for run in [*report['runs'],*report['source_control_runs']]:
        for component in run.get('outer_components',[]):
            groups[(component['reveal_hex'],100_000_000)].append((component['solution_hex'],component['execution']))
            protocol_count+=1
        cases=[run,*run.get('lifecycle_cases',[]),*run.get('adversarial_cases',[])]
        for case in cases:
            for attempt in case.get('attempts',[]):
                for spend,execution in zip(attempt['transaction']['spends'],attempt['primary_vm'],strict=True):
                    groups[(spend['reveal_hex'],100_000_000)].append((spend['solution_hex'],execution))
                    protocol_count+=1
    executed=0
    with tempfile.TemporaryDirectory(prefix='counter-retained-replay-') as temp:
        directory=Path(temp)
        for index,((program,limit),observations) in enumerate(groups.items()):
            program_path=directory/f'{index}.hex';program_path.write_text(program+'\n')
            solutions=[]
            for environment,expected in observations:
                if expected['outcome']['status']=='accepted':
                    packet.require(expected['cost']<=limit,'retained accepted cost exceeds this replay transport cap; inconclusive')
                solutions.append(assembly(bytes.fromhex(environment)))
            encoded=json.dumps(solutions).encode()
            packet.require(len(encoded)<=8*1024*1024,'bounded CLI replay solution transport')
            solutions_path=directory/f'{index}.json';solutions_path.write_bytes(encoded)
            raw=packet.support.command(directory,f'replay-{index}',[cli,'run-batch',program_path,'--solutions',solutions_path,'--max-cost',str(limit)],target=directory)
            actual=packet.decode_json(raw)
            packet.require(len(actual)==len(observations),'complete retained replay observations')
            for execution,(_,expected) in zip(actual,observations,strict=True):
                outcome=check_execution(execution,require_conclusive=True,require_typed=True)
                packet.require(outcome==expected['outcome'],'retained exact VM acceptance/output')
                if outcome['status']=='rejected':packet.require(execution['failure']==expected['failure'],'retained exact typed primary failure')
                executed+=1
    packet.require(packet.support.identity(cli,512*1024*1024)==before,'VM executable changed during retained replay')
    return {'raw_vm_observations':len(rows),'protocol_primary_observations':protocol_count,'total':executed,
        'vm':'clvmr 0.21.0','flags':535,'raw_max_cost':50_000_000,'protocol_replay_max_cost':100_000_000,
        'captured_protocol_max_cost':250_000_000,
        'scope':'Actual finite retained CLVM replay; protocol replay uses the CLI100M cap, separately from captured250M components. No SDK submission, new lifecycle scenario, live-network, BLS or compiler-preservation credit.'}
