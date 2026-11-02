"""Independent finite ledger-effect checks; no VM, SDK or network execution.

Coin names, queue/curry hashes, payouts, protected top-ups and relative ages are
recomputed from the declared artifact identities and named public fixture data.
Selected artifact bytes are independently admitted by the outer capture reader.
"""
from pathlib import Path
import hashlib
from capture_support import ROOT,closed,require,sha
import raw_tree
HOLDER='ae59eb0ec2c937e59d142b74ded8adaf8732c5a76bcbc945735ad7359cb4173c9e33b0a60ec2eb05de055af65b37e161'
NEXT='95d3c5d0d0fecf9391cade8bd9d0bac9604973831ba26b809eed412fe6831d505242de10bac25577a2650182072840e5'
WRONG='93c707a8502b81234ac92d4315b2a82d213a7cf6fb6e1bd9c7e6fed3935813ca43e568a2484122a27841b453b799f561'
def digest(data):return hashlib.sha256(data).digest()
def integer(n):
    require(type(n) is int and 0<=n<2**64,'bounded unsigned amount')
    b=n.to_bytes((n.bit_length()+7)//8,'big');return b'\0'+b if b and b[0]&128 else b

def proper(xs):
    tail=b''
    for x in reversed(xs):tail=x,tail
    return tail

def fixed(h,n=32):
    require(type(h) is str and len(h)==n*2 and h==h.lower(),'canonical fixed hex')
    try:b=bytes.fromhex(h)
    except ValueError:raise ValueError('invalid fixed hex') from None
    require(len(b)==n,'exact fixed bytes');return b

def instance(module,terms):
    # Standard curry tree with an independently prehashed selected-module leaf.
    ah=lambda atom:digest(b'\1'+atom)
    ph=lambda left,right:digest(b'\2'+left+right)
    quote=ph(ah(b'\1'),module)
    env=proper([b'\4',(b'\1',terms),b'\1'])
    return ph(ah(b'\2'),ph(quote,ph(raw_tree.tree_hash(env),ah(b''))))

def coin(parent,ph,amount):
    parent=fixed(parent) if type(parent) is str else parent;ph=fixed(ph) if type(ph) is str else ph
    return dict(parent=parent.hex(),puzzle_hash=ph.hex(),amount=amount,coin_id=digest(parent+ph+integer(amount)).hex())

def check_coin(value):
    closed(value,{'coin_id','parent','puzzle_hash','amount'},'coin record');fixed(value['coin_id'])
    require(value==coin(value['parent'],value['puzzle_hash'],value['amount']),'independent exact coin name')

def effects(step,removals,outputs,height,fee=0):
    closed(step,{'height','removals','outputs','measurement'},'accepted ledger step')
    require(type(step['height']) is int and step['height']>=0 and step['height']==height,'actual exact integer accepted height')
    for c in step['removals']+step['outputs']:check_coin(c)
    order=lambda xs:sorted(xs,key=lambda c:c['coin_id'])
    require(order(step['removals'])==order(removals) and order(step['outputs'])==order(outputs),'exact independently expected ledger effects')
    require(len({c['coin_id'] for c in step['outputs']})==len(step['outputs']),'distinct output coin names')
    require(sum(c['amount'] for c in removals)==sum(c['amount'] for c in outputs)+fee,'exact no-fee chain conservation')

def batch(chain,module,count,width):
    require(all(type(chain[k]) is int for k in ('count','batch_width','payouts','payout_total')),'exact batch scalar types')
    require(chain['count']==count and chain['batch_width']==width and chain['payouts']==count and chain['payout_total']==count*1000 and chain['terminal_successor'] is False,'exact complete batch metadata')
    recipients=[bytes([i*0x11])*32 for i in range(1,count+1)]
    require(chain['recipient_queue']==[r.hex() for r in recipients] and len(chain['steps'])==count//width,'exact fixed pair-distinct queue/step inventory')
    heads=[b'']*(count+1);heads[count]=digest(b'chialisp-oxide/continuation/v1/end:1000')
    for i in range(count-1,-1,-1):heads[i]=digest(b'chialisp-oxide/continuation/v1/item:1000'+recipients[i]+heads[i+1])
    def ph(i):return instance(module,proper([module,heads[i],integer(count-i)]))
    current=None
    for number,i in enumerate(range(0,count,width)):
        step=chain['steps'][number];require(len(step['removals'])==1,'one queue removal')
        if current is None:
            current=step['removals'][0];check_coin(current);require(current['puzzle_hash']==ph(i).hex() and current['amount']==count*1000,'own exact initial queue funding')
        outputs=[coin(current['coin_id'],recipient,1000) for recipient in recipients[i:i+width]]
        successor=coin(current['coin_id'],ph(i+width),(count-i-width)*1000) if i+width<count else None
        if successor:outputs.append(successor)
        effects(step,[current],outputs,number);current=successor
    require(current is None,'complete terminal chain')

def spuds(chain,module):
    require(chain['fixture_public_keys']==dict(holder=HOLDER,next=NEXT,wrong=WRONG),'exact named disposable public fixture keys')
    require(type(chain['take_ages']) is list and all(type(n) is int for n in chain['take_ages']) and type(chain['claim_age']) is int,'exact relative age scalar types')
    require(chain['take_ages']==[1,4] and chain['claim_age']==5 and chain['timer_reset_checked'] is True and chain['exact_six_condition_VM_outputs_checked'] is True,'actual relative-age scope')
    require(chain['claim_message']==digest(b'chialisp-oxide/spuds/claim:v1').hex() and len(chain['steps'])==3,'claim intent and complete chain')
    parameters=[(HOLDER,bytes([0xa1])*32,1),(NEXT,bytes([0xa2])*32,2),(HOLDER,bytes([0xa4])*32,3)]
    hashes=[instance(module,proper([module,fixed(key,48),payout,integer(round)])) for key,payout,round in parameters]
    heights=[1,5,10];ages=[1,4,5];birth=0;current=None
    for index,step in enumerate(chain['steps']):
        if current is None:
            require(len(step['removals'])==2,'first take removals')
            candidates=[c for c in step['removals'] if c['puzzle_hash']==hashes[0].hex() and c['amount']==1000];require(len(candidates)==1,'own exact initial spuds funding');current=candidates[0]
        require(heights[index]-birth==ages[index],'independent coin birth/age reset')
        if index<2:
            require(len(step['removals'])==2 and current in step['removals'],'game plus quoted top-up removals')
            funding=next(c for c in step['removals'] if c!=current);check_coin(funding)
            message=digest(b'chialisp-oxide/spuds/take:v1'+hashes[index+1]);announcement=digest(fixed(current['coin_id'])+message)
            conditions=proper([proper([integer(61),announcement]),proper([integer(51),bytes([0xcc])*32,integer(1000)])])
            fixture_ph=raw_tree.tree_hash((b'\1',conditions))
            require(funding['puzzle_hash']==fixture_ph.hex() and funding['amount']==2000,'exact take-announcement/fixed-change top-up fixture')
            successor=coin(current['coin_id'],hashes[index+1],(index+2)*1000);change=coin(funding['coin_id'],bytes([0xcc])*32,1000)
            effects(step,[current,funding],[successor,change],heights[index]);current=successor;birth=heights[index]
        else:effects(step,[current],[coin(current['coin_id'],bytes([0xa4])*32,3000)],heights[index])

def source_identity():
    base=ROOT/'experiments/stateful-ledger'
    names={'manifest':base/'Cargo.toml','lockfile':base/'Cargo.lock','measurement':ROOT/'experiments/spend-costs/src/measurement.rs'}
    names.update({n:base/'src'/(n+'.rs') for n in ('main','common','batches','spuds','fixture_keys','mutations')})
    return {n:sha(p) for n,p in names.items()}

def validate(report):
    closed(report,{'schema','scope','network','simulator','consensus','vm','reports','source_identity','compiler_preservation_proof','standard_wallet_integration'},'SDK report')
    require(type(report['schema']) is int and report['schema']==1 and report['scope']=='three_mode_sdk_execution' and report['network']=='TESTNET11' and report['simulator']=='chia-sdk-test 0.36.0' and report['consensus']=='chia-consensus 0.36.1' and report['vm']=='clvmr 0.16.2','exact pinned SDK scope/versions')
    require(report['compiler_preservation_proof'] is False and report['standard_wallet_integration'] is False and report['source_identity']==source_identity(),'exact runner source identity and claim limits')
    require([r['mode'] for r in report['reports']]==['chialisp','rust_on','rust_off'],'complete exact SDK mode coverage')
    for row in report['reports']:
        closed(row,{'artifact_blob_sha256','batch_comparisons','mode','module_hashes','spuds'},'SDK mode row')
        closed(row['module_hashes'],{'batch_one','batch_two','spuds'},'selected module hashes');closed(row['artifact_blob_sha256'],{'batch_one','batch_two','spuds'},'selected blob hashes')
        require([c['count'] for c in row['batch_comparisons']]==[4,8],'exact per-mode four/eight queue coverage')
        for compare in row['batch_comparisons']:
            closed(compare,{'count','batch_one','batch_two','batch_two_minus_batch_one'},'batch comparison')
            require(type(compare['count']) is int,'exact comparison count scalar')
            batch(compare['batch_one'],fixed(row['module_hashes']['batch_one']),compare['count'],1)
            batch(compare['batch_two'],fixed(row['module_hashes']['batch_two']),compare['count'],2)
        spuds(row['spuds'],fixed(row['module_hashes']['spuds']))
    return True

def mutation_witness(row):
    """Reconstruct complete SDK instance inputs and all accepted child effects."""
    actual=row['actual'];module=fixed(row['module_hash']);game=actual['own_reveal_funding'];check_coin(game)
    case=row['case'];mutant=row['variant']=='mutant';project=row['project']
    if project=='continuation-batches':
        recipients=[bytes([i*0x11])*32 for i in range(1,5)];heads=[b'']*5;heads[4]=digest(b'chialisp-oxide/continuation/v1/end:1000')
        for i in range(3,-1,-1):heads[i]=digest(b'chialisp-oxide/continuation/v1/item:1000'+recipients[i]+heads[i+1])
        count=2 if case=='terminal' else 4;second=bytes([0x99])*32 if case=='second_opening' else recipients[1]
        terms=proper([module,heads[0],integer(count)]);solution=proper([proper([proper([recipients[0],heads[1]]),proper([second,heads[2]])])])
        amount=4001 if case=='amount' else count*1000;height=0;fee=1 if case=='amount' else 0
        outputs=[coin(game['coin_id'],recipients[0],1000),coin(game['coin_id'],second,1000)]
        if count>2:
            head=heads[0] if case=='successor' and mutant else heads[2];next_ph=instance(module,proper([module,head,integer(count-2)]));outputs.append(coin(game['coin_id'],next_ph,(count-2)*1000))
        removals=[game]
    else:
        taking=case in ('nextstate','signature_binding');desired=bytes([0xa3 if case=='signature_binding' else 0xa2])*32
        terms=proper([module,fixed(HOLDER,48),bytes([0xa1])*32,integer(1)]);solution=proper([integer(int(taking)),fixed(NEXT,48),desired]);amount=1001 if case=='amount' else 1000;height=1 if taking or case=='age' else 5
        require(type(actual['coin_age']) is int and actual['coin_age']==height and actual['exact_six_condition_VM_output_checked_separately'] is True,'exact mutant SDK age/VM scope')
        output_payout=bytes([0xa1])*32 if case=='nextstate' and mutant else desired
        successor=instance(module,proper([module,fixed(NEXT,48),output_payout,integer(2)]))
        claim=digest(b'chialisp-oxide/spuds/claim:v1');take=digest(b'chialisp-oxide/spuds/take:v1'+successor)
        message=take if taking and not(case=='signature_binding' and mutant) else claim
        destination=successor if taking else bytes([0xa1])*32
        outputs=[coin(game['coin_id'],destination,2000 if taking else 1000)];removals=[game];fee=1 if case=='amount' else 1000 if case=='announcement' else 0
        if taking or case=='announcement':
            sub=actual['submission']
            if sub.get('status')=='accepted':
                observed=sub['observation'];require(len(observed['removals'])==2 and game in observed['removals'],'own mutated game removal plus top-up')
                funding=next(c for c in observed['removals'] if c!=game);check_coin(funding)
                announcement=digest(fixed(game['coin_id'])+(take if case=='announcement' else message));conditions=proper([proper([integer(61),announcement]),proper([integer(51),bytes([0xcc])*32,integer(1000)])])
                require(funding['puzzle_hash']==raw_tree.tree_hash((b'\1',conditions)).hex() and funding['amount']==2000,'exact mutant top-up fixture')
                removals.append(funding);outputs.append(coin(funding['coin_id'],bytes([0xcc])*32,1000))
    require(game['amount']==amount and game['puzzle_hash']==instance(module,terms).hex(),'exact independently derived own funded instance')
    parsed=raw_tree.deserialize(bytes.fromhex(actual['curried_reveal_hex']));parts=proper_values(parsed);require(proper_values(parts[2])[1]==(b'\1',terms),'complete actual curried terms')
    require(actual['instance_solution_hex']==raw_tree.serialize(solution).hex(),'complete independently derived SDK witness solution')
    if actual['submission'].get('status')=='accepted':effects(actual['submission']['observation'],removals,outputs,height,fee)

def proper_values(tree):
    values=[]
    while type(tree) is tuple:values.append(tree[0]);tree=tree[1]
    require(tree==b'','proper list required');return values
