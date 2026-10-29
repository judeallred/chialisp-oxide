"""Independent visible coin/cost witnesses; metadata is not execution authentication."""
import hashlib
import oracle as o
import admission as a


def coin(parent,ph,amount):
    return dict(parent_id=parent,puzzle_hash=ph,amount=amount,
        coin_id=hashlib.sha256(bytes.fromhex(parent)+bytes.fromhex(ph)+o.integer(amount)).hexdigest())


def verify_coin(value):
    a.require(set(value)=={'parent_id','puzzle_hash','amount','coin_id'} and value==coin(value['parent_id'],value['puzzle_hash'],value['amount']),'exact coin id/parent/puzzle/amount')


def costs(value,height):
    a.require(value['previous_transaction_height']==height and value['max_cost']==5_500_000_000
        and value['cost_per_byte']==12000,'pinned generator cost/height rules')
    rows=value['generators']
    a.require(len(rows)==2 and {r['encoding'] for r in rows}=={'uncompressed','backrefs'},'both generator encodings')
    for r in rows:
        size=len(bytes.fromhex(r['generator_hex']));m=r['metrics']
        a.require(r['generator_bytes']==size==m['charged_bytes'] and m['signature_verified'] is True,'actual generator length/BLS flag')
        a.require(m['byte_cost']==size*12000 and m['total_cost']==m['byte_cost']+m['execution_cost']+m['condition_cost'],'exact cost decomposition')
        a.require(0<m['total_cost']<=5_500_000_000,'measured bounded generator cost')
        a.require(r['signature_controls']==[dict(signature='unrelated_nonempty',code='BadAggregateSignature')],'separate BLS controls')
        a.require(r['budget_control']==dict(exact_cost_succeeded=True,one_less_code='CostExceeded'),'exact/below budget controls')
    plain=next(r for r in rows if r['encoding']=='uncompressed')
    helper=value['helper']
    a.require(helper['signature_verified'] is False and helper['charged_bytes']==plain['generator_bytes']-2,'helper skips BLS; quoted byte rule')
    a.require(helper['byte_cost']==helper['charged_bytes']*12000 and helper['total_cost']==helper['byte_cost']+helper['execution_cost']+helper['condition_cost'],'helper exact decomposition')
    return {r['encoding']:r['metrics'] for r in rows}


def rejected(value,code):
    a.require(value['code']==code and value['full_simulator_equal'] is True,'exact typed code/full simulator flag')
    if code=='GeneratorRuntimeError':
        a.require(value['classification']=='inconclusive_runtime_umbrella' and value['policy_rejection_credit'] is False,'runtime umbrella has no policy rejection credit')


def queue():
    heads=[o.terminal()]
    for byte in reversed((0x11,0x22,0x33,0x44)):
        heads.insert(0,o.item(bytes([byte])*32,heads[0]))
    return heads


def base(report,modules):
    versions(report)
    heads=queue()
    a.require(len(report['lanes'])==3 and {x['name'] for x in report['lanes']}==set(modules),'exact three base lanes')
    for lane in report['lanes']:
        module,size=modules[lane['name']]
        a.require(lane['module_hash']==module.hex() and lane['module_bytes']==size,'actual selected module identity')
        current=lane['initial_coin'];verify_coin(current)
        a.require(current['amount']==4000 and current['puzzle_hash']==o.instance_hash(module,heads[0],4).hex(),'initial complete queue terms')
        a.require(len(lane['steps'])==4,'four actual accepted spends')
        totals={name:dict(charged_bytes=0,total_cost=0) for name in ('uncompressed','backrefs')}
        for i,step in enumerate(lane['steps']):
            a.require(step['index']==i and step['previous_transaction_height']==i and step['coin']==current and step['fees']==0,'ordered exact current coin/height/fees')
            payout=coin(current['coin_id'],(bytes([0x11*(i+1)])*32).hex(),1000)
            a.require(step['payout']==payout,'fixed ordered payout recipient/amount')
            if i==3:a.require(step['successor'] is None,'terminal no successor')
            else:
                current=coin(current['coin_id'],o.instance_hash(module,heads[i+1],3-i).hex(),1000*(3-i))
                a.require(step['successor']==current,'derived exact successor state/module/amount')
            for encoding,metrics in costs(step['costs'],i).items():
                for key in totals[encoding]:totals[encoding][key]+=metrics[key]
        a.require(lane['chain_totals']==totals and lane['total_fees']==0,'exact measured chain totals')
        a.require([r['code'] for r in lane['typed_rejections']]==['BadAggregateSignature','AssertMyAmountFailed','WrongPuzzleHash',*(['DoubleSpend']*4)],'exact rejection suite')
        for r in lane['typed_rejections']:rejected(r,r['code'])
        a.require(len(lane['runtime_umbrellas'])==4,'four opening runtime umbrellas')
        for r in lane['runtime_umbrellas']:rejected(r,'GeneratorRuntimeError')
        a.require([r['kind'] for r in lane['resume']]==['persisted_cursor_in_process_resume','consumed_coin_reconciled_without_push'],'separate explicit resume scopes')
        a.require(lane['counts']==dict(accepted_submissions=4,typed_rejected_submissions=7,runtime_umbrella_submissions=4,successor_coins=3,total_sdk_attempts=15),'base per-build counts')


def controls(report,modules):
    versions(report)
    heads=queue()
    a.require(len(report['controls'])==8 and {(r['kind'],r['mode']) for r in report['controls']}=={(k,m) for k in ('drop_opening','drop_terminal','drop_amount','wrong_successor') for m in ('on','off')},'exact actual-source SDK inventory')
    for row in report['controls']:
        kind=row['kind'];count=1 if kind=='drop_terminal' else 4
        for role in ('original','mutant'):
            selected=row[role];module,size=modules[(kind,role,row['mode'])]
            verify_coin(selected['coin'])
            a.require(selected['module_hash']==module.hex() and selected['coin']['puzzle_hash']==o.instance_hash(module,heads[0],count).hex(),'own actual selected reveal funding')
            amount=4001 if kind=='drop_amount' else 1000*count
            a.require(selected['coin']['amount']==amount,'exact control amount')
            if role=='original' and kind!='wrong_successor':
                rejected(selected['negative'],'AssertMyAmountFailed' if kind=='drop_amount' else 'GeneratorRuntimeError')
                continue
            to=(bytes([0x55 if kind=='drop_opening' else 0x11])*32).hex()
            a.require(selected['accepted'] is True and selected['payout']==coin(selected['coin']['coin_id'],to,1000),'actual source counterexample payout')
            if count==1:a.require(selected['successor'] is None,'source premature end witness')
            else:
                ph=(bytes([0x77])*32).hex() if role=='mutant' and kind=='wrong_successor' else o.instance_hash(module,heads[1],count-1).hex()
                a.require(selected['successor']==coin(selected['coin']['coin_id'],ph,1000*(count-1)),'source next-state witness')
            a.require(selected['fees']==(1 if kind=='drop_amount' else 0),'source fee witness')
            costs(selected['costs'],0)


def versions(report):
    a.require(type(report['schema']) is int and report['schema']==1
        and report['sdk']=='chia-sdk-test 0.36.0' and report['consensus']=='chia-consensus 0.36.1'
        and report['vm']=='clvmr 0.16.2' and report['network']=='TESTNET11','exact SDK/consensus/VM/network report protocol')
