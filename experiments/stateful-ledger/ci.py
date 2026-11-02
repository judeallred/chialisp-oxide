#!/usr/bin/env python3
"""Fresh complete stateful captures, or read-only retained ledger consistency.

No wallet/network authority. Retained readers add no execution authentication.
"""
from pathlib import Path
import argparse
import copy
import hashlib
import json
import sys
import os
import shutil
from capture_support import ROOT, Capture,source_window,sha,read,write,require,validate_success,capture_success,validate_source_window,validate_compiler_window,source_corruption_controls,COMMON_RECEIPT,closed,validate_inventory,validate_commands,validate_reader_controls,SOURCE_READER_CONTROLS
from historical_boundary import validate as historical
from mutation_support import capture as mutations,validate as validate_mutations
import ledger_reader
import raw_tree
PROTOCOL='chialisp-oxide-stateful-ledger-capture/v1'

def metrics(report):
    ledger_reader.validate(report)
    require(report.get('schema')==1 and report.get('scope')=='three_mode_sdk_execution','complete SDK mode scope')
    require([r['mode'] for r in report['reports']]==['chialisp','rust_on','rust_off'],'exact ordered SDK modes')
    totals=dict(accepted_sdk=0,typed_rejected_sdk=0,runtime_umbrella_sdk=0)
    for row in report['reports']:
        for chain in [c[k] for c in row['batch_comparisons'] for k in ('batch_one','batch_two')]+[row['spuds']]:
            steps=chain['steps'];totals['accepted_sdk']+=len(steps)
            if 'batch_width' in chain:
                expected_batch=[('bad_amount','AssertMyAmountFailed'),('malformed_opening','GeneratorRuntimeError'),('composition_inflation','MintingCoin')]+[('stale_coin','DoubleSpend')]*len(steps)
                require([(c['label'],c['code']) for c in chain['controls']]==expected_batch,'exact ordered batch rejection codes')
            for enc in ('uncompressed','backrefs'):
                sums={k:0 for k in ('charged_bytes','execution_cost','condition_cost','byte_cost','total_cost')}
                for s in steps:
                    require(type(s['measurement']['previous_transaction_height']) is int and s['height']==s['measurement']['previous_transaction_height'],'actual integer generator height')
                    require(type(s['measurement']['cost_per_byte']) is int and s['measurement']['cost_per_byte']==12000 and type(s['measurement']['max_cost']) is int and s['measurement']['max_cost']==5500000000,'exact integer measurement constants')
                    gs=s['measurement']['generators'];require([g['encoding'] for g in gs]==['uncompressed','backrefs'],'both actual encodings')
                    g=next(g for g in gs if g['encoding']==enc);m=g['metrics']
                    require(all(type(m[k]) is int and m[k]>=0 for k in sums),'exact nonnegative integer measured costs')
                    helper=s['measurement']['helper'];closed(helper,{'total_cost','execution_cost','condition_cost','byte_cost','charged_bytes','signature_verified','per_spend'},'conditions helper cost map')
                    require(all(type(helper[k]) is int and helper[k]>=0 for k in sums) and helper['signature_verified'] is False,'helper exact integer costs; no BLS verification credit')
                    require(helper['byte_cost']==helper['charged_bytes']*12000 and helper['total_cost']==helper['execution_cost']+helper['condition_cost']+helper['byte_cost'],'helper cost decomposition')
                    require(helper['charged_bytes']==s['measurement']['generators'][0]['generator_bytes']-2 and m['execution_cost']==helper['execution_cost']+20 and m['condition_cost']==helper['condition_cost'] and m['per_spend']==helper['per_spend'],'actual generator/helper quote and per-spend relation')
                    require({p['coin_id'] for p in m['per_spend']}=={p['coin_id'] for p in s['removals']} and len(m['per_spend'])==len(s['removals']),'exact measured removal identities')
                    for spent in m['per_spend']:
                        closed(spent,{'coin_id','execution_cost','condition_cost'},'per-spend costs');require(all(type(spent[k]) is int and spent[k]>=0 for k in ('execution_cost','condition_cost')),'per-spend exact integer costs')
                    require(m['signature_verified'] is True and m['byte_cost']==m['charged_bytes']*12000 and m['total_cost']==m['execution_cost']+m['condition_cost']+m['byte_cost'],'actual BLS/cost decomposition')
                    require(g['budget_control']==dict(exact_cost_succeeded=True,one_less_code='CostExceeded') and len(g['signature_controls'])>=1 and all(x['code']=='BadAggregateSignature' for x in g['signature_controls']),'signature and exact budget controls')
                    require(len(bytes.fromhex(g['generator_hex']))==g['generator_bytes']==m['charged_bytes'],'charged actual generator bytes')
                    for field in sums:sums[field]+=m[field]
                require(all(type(chain['whole_chain'][enc][k]) is int and chain['whole_chain'][enc][k]>=0 for k in sums),'exact whole-chain cost scalar types')
                require(sums==chain['whole_chain'][enc],'exact whole-chain metric sums')
            for control in chain['controls']:
                umbrella=control['code']=='GeneratorRuntimeError';require(control['full_simulator_equal'] is True and control['policy_rejection_credit'] is (not umbrella) and control['classification']==('inconclusive_runtime_umbrella' if umbrella else 'typed_consensus_rejection'),'exact rejection scope/full state equality')
                totals['runtime_umbrella_sdk' if umbrella else 'typed_rejected_sdk']+=1
        for compare in row['batch_comparisons']:
            count=compare['count'];b1=compare['batch_one'];b2=compare['batch_two'];require(count in (4,8) and len(b1['steps'])==count and len(b2['steps'])==count//2 and b1['recipient_queue']==b2['recipient_queue'] and b1['payout_total']==b2['payout_total']==count*1000,'same complete queues')
            for enc in ('uncompressed','backrefs'):
                require(all(type(v) is int for v in compare['batch_two_minus_batch_one'][enc].values()),'signed integer metric deltas')
                require(compare['batch_two_minus_batch_one'][enc]=={k:b2['whole_chain'][enc][k]-b1['whole_chain'][enc][k] for k in b1['whole_chain'][enc]},'signed cost deltas including regressions')
        require(len(row['spuds']['steps'])==3 and row['spuds']['take_ages']==[1,4] and row['spuds']['claim_age']==5 and row['spuds']['timer_reset_checked'] is True and row['spuds']['exact_six_condition_VM_outputs_checked'] is True,'complete spuds chain/context')
        expected={'take_at_maximum_round_runtime_umbrella':'GeneratorRuntimeError','ephemeral_take_age_zero':'EphemeralRelativeCondition','take_at_age_zero':'AssertHeightRelativeFailed','wrong_key_actual_bundle':'BadAggregateSignature','transplanted_payout_old_signature':'BadAggregateSignature','under_topup':'MintingCoin','stale_taken_coin':'DoubleSpend','claim_at_age_four':'AssertHeightRelativeFailed','claim_after_take_timer_reset':'AssertHeightRelativeFailed','take_at_age_five':'AssertBeforeHeightRelativeFailed','claim_with_take_protected_topup':'AssertCoinAnnouncementFailed'}
        require({c['label']:c['code'] for c in row['spuds']['controls']}==expected and len(row['spuds']['controls'])==11,'exact complete spuds typed control set')
    require(totals==dict(accepted_sdk=63,typed_rejected_sdk=108,runtime_umbrella_sdk=15),'exact current SDK counts')
    totals['sdk_attempts']=sum(totals.values());return totals

LEDGER_CONTROLS={'altered_paid_amount','altered_parent','terminal','module_hash','compare_counts','scope_flags','source_identity','boolean_height'}

def ledger_corruption_controls(report):
    result={}
    for kind in LEDGER_CONTROLS:
        bad=copy.deepcopy(report);row=bad['reports'][0];chain=row['batch_comparisons'][0]['batch_two']
        if kind=='altered_paid_amount':chain['steps'][0]['outputs'][0]['amount']+=1
        elif kind=='altered_parent':chain['steps'][0]['outputs'][0]['parent']='00'*32
        elif kind=='terminal':chain['terminal_successor']=True
        elif kind=='module_hash':row['module_hashes']['batch_two']='00'*32
        elif kind=='compare_counts':row['batch_comparisons'][1]['count']=4
        elif kind=='scope_flags':bad['standard_wallet_integration']=True
        elif kind=='boolean_height':row['spuds']['steps'][0]['height']=True
        else:bad['source_identity'].pop('main')
        try:metrics(bad)
        except (ValueError,KeyError,IndexError,TypeError):result[kind]='rejected'
        else:raise ValueError('ledger contradiction accepted: '+kind)
    return result


def validate(path):
    base=path.resolve().parent;r=json.loads(read(path));closed(r,COMMON_RECEIPT|{'protocol','status','source_files','report_sha256','counts','reader_controls','success_receipts','mutation_receipts','recorded_packet_origins','scope'},'shared ledger receipt');validate_inventory(r,base);require(r['protocol']==PROTOCOL and r['status']=='successful_stateful_ledger_capture','ledger capture protocol/status')
    require(sha(base/'report.json')==r['report_sha256'],'exact SDK report bytes')
    require(read(base/'report.json')==read(base/'report-rerun.json'),'retained independently rerun byte equality')
    report=json.loads(read(base/'report.json'));require(metrics(report)==r['counts'],'ledger semantic counts')
    validate_reader_controls(r['reader_controls'],SOURCE_READER_CONTROLS|LEDGER_CONTROLS)
    validate_commands(base,['compiler','runner-fmt','runner-native','runner-clippy','reader-python','sdk','sdk-rerun'])
    validate_source_window(r['source_files'],source_window(ROOT/'experiments/spuds')|source_window(ROOT/'experiments/continuation-batches'));validate_compiler_window(r)
    for name,digest in r['files'].items():
        p=Path(name);require(not p.is_absolute() and '..' not in p.parts and p.as_posix()==name,'confined ledger path');require(sha(base/p)==digest,'retained ledger bytes')
    require(set(r['success_receipts'])==set(r['mutation_receipts'])=={'continuation-batches','spuds'},'closed exact project receipt maps')
    for project in ('continuation-batches','spuds'):
        success=resolve_reference(base,r['success_receipts'][project],project,'success')
        mutation=resolve_reference(base,r['mutation_receipts'][project],project,'mutations')
        validate_mutations(mutation)
        validate_success(success)
        for row in report['reports']:
            if row['mode']=='chialisp':payload=bytes.fromhex(read(success.parent/'reference.clvm.hex').decode().strip())
            else:
                output=success.parent/row['mode'].replace('_','-');ptr=json.loads(read(output/'latest.json'));man=output/ptr['manifest'];m=json.loads(read(man));payload=bytes.fromhex(read(man.parent/m['entries'][0]['clvm']).decode().strip())
            key='batch_two' if project=='continuation-batches' else 'spuds';require(hashlib.sha256(payload).hexdigest()==row['artifact_blob_sha256'][key] and raw_tree.tree_hash(raw_tree.deserialize(payload)).hex()==row['module_hashes'][key],'SDK exact blob/tree selected from complete successful capture')
            if row['mode']!='chialisp':
                controls=json.loads(read(mutation));originals=[x for x in controls['sdk']['records'] if x['variant']=='original' and x['mode']==row['mode']]
                require(len(originals)==(4 if project=='continuation-batches' else 5) and all(x['artifact_blob_sha256']==row['artifact_blob_sha256'][key] for x in originals),'every original mutation control uses exact successful base module')
    for row in report['reports']:
        old=ROOT/'experiments/authenticated-continuations/evidence'
        if row['mode']=='chialisp':path=old/'reference.clvm.hex'
        else:
            root=old/row['mode'].replace('_','-');m=json.loads(read(root/'manifest.json'));path=root/m['entries'][0]['clvm']
        payload=bytes.fromhex(read(path).decode().strip());require(hashlib.sha256(payload).hexdigest()==row['artifact_blob_sha256']['batch_one'] and raw_tree.tree_hash(raw_tree.deserialize(payload)).hex()==row['module_hashes']['batch_one'],'exact immutable baseline blob/tree identity')
    return dict(status='retained_stateful_ledger_valid',counts=r['counts'],execution_credit=False)

def resolve_reference(base,label,project,kind):
    private=f'../{project}-{kind}/receipt.json'
    retained_kind='success-evidence' if kind=='success' else 'source-controls'
    published=f'../../{project}/{retained_kind}/receipt.json'
    require(type(label) is str and label in (private,published),'exact reviewed relative retained receipt path')
    if label==published:require(base==ROOT/'experiments/stateful-ledger/evidence','published reference requires canonical ledger destination')
    else:require(base.name=='ledger' and base.is_relative_to(ROOT/'target'),'private CI references require isolated repository target capture')
    path=base/label;resolved=path.resolve(strict=True);require(resolved.is_relative_to(ROOT),'retained reference outside project')
    require(not any(p.is_symlink() for p in [path,*path.parents] if p.is_relative_to(ROOT)),'retained reference symlink')
    return resolved

def promote(capture_root):
    """Copy only inventoried packet files; retain captured origins as declarations."""
    capture_root=capture_root.resolve(strict=True);validate(capture_root/'ledger/receipt.json')
    def packet(source,dest):
        r=json.loads(read(source/'receipt.json'));dest.mkdir(parents=True,exist_ok=False)
        for label in [*r['files'],'receipt.json']:
            p=Path(label);require(not p.is_absolute() and '..' not in p.parts,'confined promotion inventory');target=dest/p;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/p,target)
    for project in ('continuation-batches','spuds'):
        packet(capture_root/(project+'-success'),ROOT/'experiments'/project/'success-evidence')
        packet(capture_root/(project+'-mutations'),ROOT/'experiments'/project/'source-controls')
    dest=ROOT/'experiments/stateful-ledger/evidence';packet(capture_root/'ledger',dest);r=json.loads(read(dest/'receipt.json'))
    r['success_receipts']={p:f'../../{p}/success-evidence/receipt.json' for p in ('continuation-batches','spuds')}
    r['mutation_receipts']={p:f'../../{p}/source-controls/receipt.json' for p in ('continuation-batches','spuds')}
    write(dest/'receipt.json',r);return validate(dest/'receipt.json')


def capture(args):
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    before=source_window(ROOT/'experiments/spuds')|source_window(ROOT/'experiments/continuation-batches')
    ledger=out/'ledger';c=Capture(ledger,args.tools);host=[s[6:] for s in c.compiler.splitlines() if s.startswith('host: ')];require(host==[args.expected_host],'actual pinned compiler host matches required host')
    manifest=ROOT/'experiments/stateful-ledger/Cargo.toml';private=ROOT/'target/stateful-ledger-private'
    c.command('runner-fmt',['cargo','fmt','--manifest-path',manifest,'--check'],target=private)
    c.command('runner-native',['cargo','test','--manifest-path',manifest,'--locked','--offline'],target=private)
    c.command('runner-clippy',['cargo','clippy','--manifest-path',manifest,'--locked','--offline','--all-targets','--','-D','warnings'],target=private)
    c.command('reader-python',[sys.executable,'-B','-m','unittest','discover','-s',ROOT/'experiments/stateful-ledger','-p','test_*.py'])
    successes={};controls={}
    for project in ('continuation-batches','spuds'):
        base=ROOT/'experiments'/project;historical(base)
        sub=argparse.Namespace(tools=args.tools,output=out/(project+'-success'))
        capture_success(base,sub);successes[project]=sub.output/'receipt.json'
        sub=argparse.Namespace(tools=args.tools,output=out/(project+'-mutations'))
        mutations(base,sub);controls[project]=sub.output/'receipt.json';validate_mutations(controls[project])
    programs=[]
    for mode in ('chialisp','rust_on','rust_off'):
        values={}
        for project,key in [('continuation-batches','batch_two'),('spuds','spuds')]:
            root=successes[project].parent
            if mode=='chialisp':path=root/'reference.clvm.hex'
            else:
                dest=root/mode.replace('_','-');ptr=json.loads(read(dest/'latest.json'));man=dest/ptr['manifest'];path=man.parent/json.loads(read(man))['entries'][0]['clvm']
            values[key]=str(path)
        old=ROOT/'experiments/authenticated-continuations/evidence'
        if mode=='chialisp':path=old/'reference.clvm.hex'
        else:path=old/mode.replace('_','-')/json.loads(read(old/mode.replace('_','-')/'manifest.json'))['entries'][0]['clvm']
        programs.append(dict(name=mode,batch_one=str(path),**values))
    write(ledger/'input.json',dict(reference_only=False,programs=programs))
    for suffix in ('','-rerun'):
        c.command('sdk'+suffix,['cargo','run','--manifest-path',ROOT/'experiments/stateful-ledger/Cargo.toml','--locked','--offline','--',ledger/'input.json',ledger/('report'+suffix+'.json')],target=ROOT/'target/stateful-ledger-private')
    require(read(ledger/'report.json')==read(ledger/'report-rerun.json'),'independently rerun complete SDK report bytes')
    require(before==(source_window(ROOT/'experiments/spuds')|source_window(ROOT/'experiments/continuation-batches')),'full current source window changed')
    report=json.loads(read(ledger/'report.json'));receipt=c.finish(dict(protocol=PROTOCOL,status='successful_stateful_ledger_capture',source_files=before,report_sha256=sha(ledger/'report.json'),counts=metrics(report),reader_controls=source_corruption_controls(before,source_window(ROOT/'experiments/spuds')|source_window(ROOT/'experiments/continuation-batches'))|ledger_corruption_controls(report),success_receipts={k:Path(os.path.relpath(v,ledger)).as_posix() for k,v in successes.items()},mutation_receipts={k:Path(os.path.relpath(v,ledger)).as_posix() for k,v in controls.items()},recorded_packet_origins={k:str(v) for k,v in successes.items()}|{k+'-mutations':str(v) for k,v in controls.items()},scope='finite native, exact primary VM and SDK observations; no live evidence or compiler preservation'))
    validate(ledger/'receipt.json');return receipt

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--tools',type=Path);p.add_argument('--expected-host');p.add_argument('--output',type=Path);p.add_argument('--check',type=Path);p.add_argument('--promote',type=Path);a=p.parse_args()
    if a.promote:require(not a.check and not a.tools and not a.expected_host and not a.output,'promotion accepts a complete captured root only');result=promote(a.promote)
    elif a.check:require(not a.tools and not a.expected_host and not a.output,'read-only check');result=validate(a.check)
    else:require(a.tools and a.expected_host and a.output,'tools, expected host and fresh output required');result=capture(a)
    print(json.dumps(result,indent=2))
if __name__=='__main__':main()
