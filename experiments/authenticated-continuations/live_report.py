#!/usr/bin/env python3
"""Bounded offline export of the completed four-step Testnet11 trial.

No HTTP, signing, submission or active-journal writes. Captured HTTPS assertions,
acknowledgements, source hashes and command declarations do not authenticate
chain inclusion, transaction causality, binaries, finality or compiler behavior.
Inventory snapshots assume a private directory and cooperating writers; they
are not a hostile-directory/TOCTOU or power-loss guarantee.
"""
import argparse
import copy
import hashlib
import json
import os
import stat
from pathlib import Path
from datetime import datetime

import ledger_checks
import live_workflow as live
import oracle as o
import workflow as w

BASE=Path(__file__).resolve().parent
MODULE='b3329cbb066c43c84501a5aaf128343f052dadc6636a58fc35d02b6d333631c4'
RECIPIENT='dc2eeaf3acf70a3e5924fd38ef2b3c1ffb26d2edc7e02456abb79abd2bd3cbc1'
ADDRESS='txch1mshw4uav7u9rukfyl5uw72eurlajd5hdclszg44tk7dt627ne0qsynqcqt'
FUNDING_COIN='1da9b30e5d5426f8b2ada218b9c0eb88978a90ae9e57f8e4472457e82f22c72b'
MISSING_RESPONSE_SHA='d8f854fcb0ceb5ba35777d9210da2d6f4c2bae8a36cb6026bd05effd2666754d'
SCOPE='retained fixed-provider observations; no inclusion authentication, submission causality or finality'
MAX_FILES=32
MAX_TOTAL=16*1024*1024
SOURCE_FILES={'live_report.py','test_live_report.py','workflow.py','prepare_step.py','live_workflow.py','recovery_step.py',
              'oracle.py','ledger_checks.py','admission.py','check_retained.py','bounded_process.py','src/lib.rs','Cargo.toml','Cargo.lock',
              'ledger/src/bin/prepare-step.rs','ledger/Cargo.toml','ledger/Cargo.lock'}
DEPENDENCIES=set(w.check_retained.DEPENDENCIES)|{'experiments/spend-costs/src/measurement.rs','crates/chialisp-oxide-simulator/src/lib.rs'}
TOOLS={'chialisp-oxide','chialisp-oxide-driver','cargo-chialisp-oxide','chialisp-oxide-verifier'}


class PartialTrial(ValueError):
    """Incomplete capture; no completed-trial report may be produced."""


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def inventory(directory):
    info=directory.lstat()
    w.require(stat.S_ISDIR(info.st_mode) and not directory.is_symlink(),'regular nonsymlink packet directory')
    pending=[(directory,'')];files=set();entries=0
    while pending:
        parent,prefix=pending.pop()
        with os.scandir(parent) as rows:
            for entry in rows:
                entries+=1
                w.require(entries<=MAX_FILES+4,'packet inventory entry budget')
                name=entry.name
                w.require(0<len(name)<=256 and name not in ('.','..') and all(char.isascii() and (char.isalnum() or char in '._-') for char in name),'canonical bounded inventory name')
                w.require(not entry.is_symlink(),'symlink packet entry forbidden before reads')
                info=entry.stat(follow_symlinks=False)
                relative=prefix+name
                if stat.S_ISDIR(info.st_mode):
                    w.require(not prefix and name in ('responses','monitor','operator'),'closed packet directory depth')
                    pending.append((Path(entry.path),relative+'/'))
                else:
                    w.require(stat.S_ISREG(info.st_mode) and info.st_nlink==1,'regular single-link packet entries')
                    w.require(len(files)<MAX_FILES+1,'packet file count before allocation')
                    files.add(relative)
    return files


def fixed_files(files):
    w.require(type(files) is dict and len(files)<=MAX_FILES and sum(len(raw) for raw in files.values())<=MAX_TOTAL,'bounded public packet')
    for name,raw in files.items():
        w.require(type(name) is str and type(raw) is bytes and len(raw)<=live.MAX_BYTES
                  and not Path(name).is_absolute() and '..' not in Path(name).parts,'bounded relative regular packet names')


def parsed(files,name):
    w.require(name in files,'missing required public capture '+name)
    return live.decode(files[name])


def push_receipt(intent):
    rows=[row for row in intent['http_receipts'] if row['endpoint']=='push_tx']
    w.require(len(rows)==1,'one retained push receipt per selected attempt')
    return rows[0]


def rpc_request(bundle,prefixed):
    value=copy.deepcopy(bundle)
    if prefixed:
        for spend in value['coin_spends']:
            for key in ('parent_coin_info','puzzle_hash'):spend['coin'][key]='0x'+spend['coin'][key]
            for key in ('puzzle_reveal','solution'):spend[key]='0x'+spend[key]
        value['aggregated_signature']='0x'+value['aggregated_signature']
    return w.encode(dict(spend_bundle=value))


def check_requests(rows,expected):
    w.require(len(rows)==len(expected),'complete read request inventory')
    for row,(endpoint,body) in zip(rows,expected):
        payload=w.encode(body)
        w.require(row['endpoint']==endpoint and row['request_bytes']==len(payload)
                  and row['request_sha256']==digest(payload),'read receipt request binds exact coin/height')


def expected_step(state,index,module_blob):
    # Independently reconstruct all four fixed-byte openings and rooted coins.
    heads=[o.terminal()]
    recipient=bytes.fromhex(RECIPIENT)
    for _ in range(4):heads.insert(0,o.item(recipient,heads[0]))
    parent=state['funding_parent']
    for position in range(index+1):
        current=ledger_checks.coin(parent,o.instance_hash(bytes.fromhex(MODULE),heads[position],4-position).hex(),1000*(4-position))
        parent=current['coin_id']
    conditions=o.step(bytes.fromhex(MODULE),heads[index],4-index,recipient,heads[index+1])
    children=[ledger_checks.coin(current['coin_id'],RECIPIENT,1000)]
    if index<3:children.append(ledger_checks.coin(current['coin_id'],conditions[2][1],1000*(3-index)))
    raw=w.check_retained.a.raw_tree
    terms=o.proper(bytes.fromhex(MODULE),heads[index],o.integer(4-index))
    reveal=raw.serialize(o.proper(b'\x02',(b'\x01',raw.deserialize(module_blob)),o.proper(b'\x04',(b'\x01',terms),b'\x01'))).hex()
    solution=raw.serialize(o.proper(o.proper(recipient,heads[index+1]))).hex()
    return current,children,reveal,solution


def monitor_context(files,steps):
    raw=files['monitor/advancement-events.jsonl']
    lines=raw.splitlines()
    w.require(0<len(lines)<=512,'bounded monitor event list')
    contexts=[];confirmed=set();submitted=set();completed=False;previous_time=None
    fields={'started':'kind time policy runtime_sha256','confirmed':'kind time bundle_id index spent_height',
            'submitted_once':'kind time bundle_id index status','waiting':'kind time index status',
            'inconclusive_read_or_preflight':'kind time detail','complete':'kind time completed payout_mojos',
            'bounded_monitor_stopped':'kind time detail'}
    for line in lines:
        value=live.decode(line)
        w.require(type(value) is dict and value.get('kind') in fields,'known monitor event')
        w.fields(value,fields[value['kind']])
        w.require(type(value['time']) is str and len(value['time'])<=64,'bounded event timestamp')
        stamp=datetime.fromisoformat(value['time'])
        w.require(stamp.tzinfo is not None and (previous_time is None or stamp>=previous_time),'monotone timezone-aware monitor timestamps')
        previous_time=stamp
        if value['kind']=='started':
            hashes=value['runtime_sha256']
            w.require(type(hashes) is dict and len(hashes)==3 and {Path(name).name for name in hashes}=={'live_workflow.py','workflow.py','prepare_step.py'},'exact three recorded runtime sources')
            for sha in hashes.values():w.hex32(sha)
            w.require(value['policy']=='no automatic retry; zero-fee fixed queue only','recorded monitor policy')
            contexts.append(dict(time=value['time'],runtime_sha256={Path(name).name:sha for name,sha in hashes.items()}))
        elif value['kind'] in ('confirmed','submitted_once','waiting'):
            index=value['index'];w.require(type(index) is int and 0<=index<4,'monitor event index')
            if value['kind']!='waiting':w.require(value['bundle_id']==steps[index]['bundle_id'],'monitor selected bundle identity')
            if value['kind']=='confirmed':
                w.require(type(value['spent_height']) is int and value['spent_height']==steps[index]['spent_height'],'monitor observed height')
                confirmed.add(index)
            else:
                if value['kind']=='submitted_once':
                    w.require(index not in submitted,'duplicate submitted-once monitor declaration')
                    submitted.add(index)
                w.require(value['status'] in ('submitted','ambiguous','prepared','push_started','confirmed'),'recorded monitor status')
        elif value['kind']=='complete':
            w.require(type(value['completed']) is int and value['completed']==4 and type(value['payout_mojos']) is int and value['payout_mojos']==4000,'monitor final declaration')
            completed=True
        else:w.require(type(value['detail']) is str and len(value['detail'])<=4096,'bounded monitor diagnostic')
    if not completed:raise PartialTrial('monitor has not recorded completion')
    w.require(contexts and confirmed==set(range(4)),'all four confirmation events and runtime context')
    w.require(len(files['monitor/advance.py'])>0,'retained public monitor source')
    return dict(scope='Recorded runtime/source-epoch statements, not authentication of execution or earlier source bodies.',
                runtime_contexts=contexts,events=len(lines),local_observer_traceback='monitor/advance.stderr',
                additional_push_or_spend_credit=0)


def summarize(files,artifact,module_blob):
    fixed_files(files)
    journal=parsed(files,'journal.json')
    w.validate(journal,artifact)
    w.require(journal['completed']==4 and journal['revision']==5,'completed four-step journal required')
    w.require(artifact['mode']=='on' and artifact['module_hash']==MODULE
              and artifact['blob_sha256']==digest(module_blob),'exact selected admitted optimized Rust module')
    w.require(journal['plan']['recipients']==[RECIPIENT]*4,'exact four-recipient public trial')
    funding=parsed(files,'funding-observed.json')
    w.fields(funding,'scope tip coin journal_revision_token http_receipts')
    initial=copy.deepcopy(journal);initial.update(completed=0,revision=1)
    first,_,_,_=expected_step(initial,0,module_blob)
    w.require(first['coin_id']==FUNDING_COIN and first['amount']==4000,'fixed observed funding coin')
    w.require(funding['scope']==live.SCOPE and funding['coin']['coin']==first and funding['coin']['spent']==0
              and funding['journal_revision_token']==w.token(initial),'exact funding observation')
    w.fields(funding['coin'],'coin confirmed spent')
    w.fields(funding['tip'],'height header_hash previous_transaction_height')
    created=live.uint(funding['coin']['confirmed'])
    w.require(0<created<=live.uint(funding['tip']['height']),'positive funding confirmation')
    w.hex32(funding['tip']['header_hash'])
    w.require(live.uint(funding['tip']['previous_transaction_height'])<=funding['tip']['height'],'funding previous-height relation')
    live.validate_receipt_rows(funding['http_receipts'])
    w.require([row['endpoint'] for row in funding['http_receipts']]==['get_network_info','get_blockchain_state','get_coin_record_by_name','get_blockchain_state']
              and all(row['http_status']==200 for row in funding['http_receipts']),'funding stable-tip read set')
    check_requests(funding['http_receipts'],[('get_network_info',{}),('get_blockchain_state',{}),('get_coin_record_by_name',{'name':'0x'+first['coin_id']}),('get_blockchain_state',{})])
    expected_names={'journal.json','funding-observed.json','step-0-submit.json','step-0-explicit-retry.json','monitor/advancement-events.jsonl','monitor/advance.py','monitor/advance.stderr'}
    for optional in ('monitor/advance-a02.stderr','operator/step-1-manual-reconcile.json'):
        if optional in files:expected_names.add(optional)
    steps=[];intents=[];bundle_ids=[]
    for index in range(4):
        name=f'step-{index}-confirmed.json';expected_names.add(name)
        intent=parsed(files,name);live.check_intent(intent)
        expected_state=copy.deepcopy(initial);expected_state.update(completed=index,revision=index+1)
        w.require(intent['status']=='confirmed' and intent['original_state']==expected_state,'exact source-admitted original state/prefix')
        current,children,reveal,solution=expected_step(initial,index,module_blob)
        prepared=intent['prepared'];confirmation=intent['confirmation']
        w.require(prepared['current_coin']==current and prepared['children']==children,'independent exact coin/payout/successor lineage')
        spend=prepared['spend_bundle']['coin_spends'][0]
        w.require(spend['puzzle_reveal']==reveal and spend['solution']==solution,'actual selected module curry and exact opening bytes')
        w.require(confirmation['parent']['confirmed']==created and confirmation['spent_height']>created,'monotone creation/spend heights across distinct blocks')
        spent=confirmation['spent_height']
        check_requests(confirmation['http_receipts'],[('get_network_info',{}),('get_blockchain_state',{}),('get_coin_record_by_name',{'name':'0x'+current['coin_id']}),('get_puzzle_and_solution',{'coin_id':'0x'+current['coin_id'],'height':spent}),('get_coin_records_by_parent_ids',{'parent_ids':['0x'+current['coin_id']],'include_spent_coins':True}),('get_blockchain_state',{})])
        observed={row['coin']['coin_id']:row for row in confirmation['children']}
        w.require(len(observed)==len(children) and all(observed[coin['coin_id']]['confirmed']==spent for coin in children),'complete child creation heights')
        if index<3 and observed[children[1]['coin_id']]['spent']:
            later=parsed(files,f'step-{index+1}-confirmed.json')
            w.require(observed[children[1]['coin_id']]['spent']==later['confirmation']['spent_height'],'consistent already-spent successor observation')
        delta=current['amount']-sum(coin['amount'] for coin in children)
        w.require(delta==0,'zero policy-coin delta/prepared-bundle fee')
        receipt=push_receipt(intent);request=rpc_request(prepared['spend_bundle'],True)
        w.require(receipt['http_status']==200 and receipt['request_bytes']==len(request) and receipt['request_sha256']==digest(request),'exact prefixed RPC request bytes')
        response_name='responses/'+receipt['request_sha256']+'-'+receipt['response_sha256']+'.json';expected_names.add(response_name)
        captured=parsed(files,response_name)
        w.fields(captured,'schema scope receipt response_hex')
        response_hex=captured['response_hex']
        w.require(type(response_hex) is str and len(response_hex)<=2*live.MAX_PUSH_RESPONSE and len(response_hex)%2==0,'raw push response transport cap before hex allocation')
        response=bytes.fromhex(response_hex)
        w.require(response.hex()==response_hex,'canonical raw response hex')
        w.require(type(captured['schema']) is int and captured['schema']==1 and captured['scope']==live.SCOPE
                  and captured['receipt']==receipt and len(response)==receipt['response_bytes'] and digest(response)==receipt['response_sha256'],'retained complete raw push response bytes/hash')
        ack=live.decode(response)
        w.fields(ack,'status success tx_id')
        bundle_id=prepared['spend_bundle_id'];bundle_ids.append(bundle_id)
        w.require(ack.get('success') is True and ack.get('status')=='SUCCESS' and ack.get('error') is None
                  and live.normalized(ack['tx_id'])==bundle_id,'provider acknowledgement binds selected bundle ID')
        steps.append(dict(index=index,coin=current,payout=children[0],successor=children[1] if index<3 else None,
                          created_height=created,spent_height=spent,observed_tip=confirmation['tip'],bundle_id=bundle_id,
                          policy_coin_delta_mojos=delta,prepared_bundle_fee_mojos=0,success_response=response_name,
                          preflight_host_executable_sha256=prepared['host_executable_sha256']))
        intents.append(intent);created=spent
    original=parsed(files,'step-0-submit.json');live.check_intent(original)
    w.require(original['status']=='ambiguous' and original['original_state']==initial,'preserved original ambiguous attempt')
    w.require(original['prepared']['spend_bundle_hex']==intents[0]['prepared']['spend_bundle_hex'],'retry keeps exact original streamable bundle')
    original_receipt=push_receipt(original);original_request=rpc_request(original['prepared']['spend_bundle'],False)
    w.require(original_receipt['http_status']==200 and original_receipt['request_bytes']==len(original_request)
              and original_receipt['request_sha256']==digest(original_request) and original_receipt['response_bytes']==757 and original_receipt['response_sha256']==MISSING_RESPONSE_SHA,'original unprefixed request and missing 757-byte response metadata')
    archive=f'journal.json.retry-original-{bundle_ids[0]}.json';context=archive+'.context.json'
    expected_names.update((archive,context))
    w.require(parsed(files,archive)==original,'exact archived original intent')
    retry=parsed(files,'step-0-explicit-retry.json')
    w.fields(retry,'action original_intent_archive recovery_context_archive intent')
    w.require(retry['action']=='one_explicit_exact_retry' and Path(retry['original_intent_archive']).name==archive
              and Path(retry['recovery_context_archive']).name==context,'explicit exact-retry archive identities')
    live.check_intent(retry['intent'])
    w.require(retry['intent']['status']=='submitted' and retry['intent']['prepared']==intents[0]['prepared']
              and retry['intent']['http_receipts']==intents[0]['http_receipts'],'retained retry and later confirmation preserve exact prepared/submission records')
    recovery=parsed(files,context)
    w.fields(recovery,'schema scope bundle_id revision_token mempool tip http_receipts')
    w.require(type(recovery['schema']) is int and recovery['schema']==1 and recovery['bundle_id']==bundle_ids[0]
              and recovery['revision_token']==w.token(initial) and recovery['scope']=='explicit exact-bundle retry context; absence does not prove failure'
              and recovery['mempool']=='absent; not proof of previous submission failure','explicit recovery uncertainty/context')
    live.validate_receipt_rows(recovery['http_receipts'])
    w.require(any(row['endpoint']=='get_mempool_item_by_tx_id' for row in recovery['http_receipts'])
              and all(row['http_status']==200 for row in recovery['http_receipts']),'retained read-only retry context')
    w.require(set(files)==expected_names and len(set(bundle_ids))==4,'closed four-step packet/unique bundles')
    monitor=monitor_context(files,steps)
    if 'operator/step-1-manual-reconcile.json' in files:
        manual=parsed(files,'operator/step-1-manual-reconcile.json');live.check_intent(manual)
        w.require(manual['status']=='confirmed' and manual['original_state']==intents[1]['original_state'] and manual['prepared']==intents[1]['prepared']
                  and manual['confirmation']['spent_height']==steps[1]['spent_height'],'manual read-only reconciliation repeats step one without extra spend credit')
        monitor['operator_reconciliation']='operator/step-1-manual-reconcile.json'
    return dict(schema=1,status='completed_trial_observed',scope=SCOPE,network='testnet11',wallet_address=ADDRESS,
                selected_module_hash=MODULE,monitor=monitor,funding_coin=first,funding_confirmed_height=funding['coin']['confirmed'],steps=steps,
                counts=dict(provider_observed_spends=4,retained_http_push_attempts=5,unique_prepared_bundles=4,success_acknowledgements=4),
                original_ambiguous_response=dict(availability='not_retained',receipt=original_receipt,classification='inconclusive_unknown_body',policy_rejection_credit=False),
                causality='Acknowledgements and matching spends do not establish that these submissions caused inclusion.',
                fees=dict(policy_coin_delta_mojos=0,prepared_bundles_mojos=0,funding_fee='not_measured',containing_chain_bundle_fees='not_authenticated_or_measured'),
                chain_authentication=False,finality=False,compiler_preservation=False)


def admitted():
    artifact=w.selected('on')
    manifest=live.decode(w.payload(BASE/'evidence/rust-on/manifest.json',256*1024))
    blob=bytes.fromhex(manifest['entries'][0]['artifact']['clvm_hex'])
    w.require(digest(blob)==artifact['blob_sha256'],'actual selected retained blob binding')
    return artifact,blob


def collect(directory):
    files={}
    w.require(directory.is_dir() and not directory.is_symlink(),'regular capture directory')
    def read(name,source=None):
        w.require(len(files)<MAX_FILES,'capture file count before read')
        remaining=MAX_TOTAL-sum(len(raw) for raw in files.values())
        try:files[name]=w.payload(source or directory/name,min(live.MAX_BYTES,remaining))
        except FileNotFoundError as error:raise PartialTrial('trial capture incomplete: '+name) from error
        fixed_files(files)
    read('journal.json');journal=parsed(files,'journal.json')
    if journal.get('completed')!=4:raise PartialTrial('trial has not completed all four steps')
    for name in ['funding-observed.json','step-0-submit.json','step-0-explicit-retry.json',*[f'step-{index}-confirmed.json' for index in range(4)]]:read(name)
    first=parsed(files,'step-0-confirmed.json')['prepared']['spend_bundle_id'];w.hex32(first)
    archive=f'journal.json.retry-original-{first}.json'
    read(archive);read(archive+'.context.json')
    for name in ('advancement-events.jsonl','advance.py','advance.stderr'):
        read('monitor/'+name,directory/name)
    for name in ('advance-a02.stderr','step-1-manual-reconcile.json'):
        if (directory/name).exists():read(('monitor/' if name.endswith('.stderr') else 'operator/')+name,directory/name)
    response_directory=directory/'journal.json.responses'
    w.require(response_directory.is_dir() and not response_directory.is_symlink(),'regular public response directory')
    for index in range(4):
        receipt=push_receipt(parsed(files,f'step-{index}-confirmed.json'))
        for key in ('request_sha256','response_sha256'):w.hex32(receipt[key])
        basename=receipt['request_sha256']+'-'+receipt['response_sha256']+'.json'
        read('responses/'+basename,directory/'journal.json.responses'/basename)
    w.require(w.payload(directory/'journal.json')==files['journal.json'],'active journal changed while copying; retry read-only export')
    return files


def hash_file(path):
    result=hashlib.sha256()
    with path.open('rb') as stream:
        while chunk:=stream.read(65536):result.update(chunk)
    return result.hexdigest()


def export(directory,output,tools,host_executable):
    w.require(not output.exists(),'new report directory required')
    before={name:hash_file(BASE/name) for name in SOURCE_FILES}
    dependencies={name:hash_file(BASE.parents[1]/name) for name in DEPENDENCIES}
    tool_hashes={name:hash_file(tools/name) for name in TOOLS}
    host_hash=hash_file(host_executable)
    artifact,blob=admitted();files=collect(directory);summary=summarize(files,artifact,blob)
    w.require(all(step['preflight_host_executable_sha256']==host_hash for step in summary['steps']),'current selected host executable consistency')
    w.require(before=={name:hash_file(BASE/name) for name in SOURCE_FILES} and dependencies=={name:hash_file(BASE.parents[1]/name) for name in DEPENDENCIES} and tool_hashes=={name:hash_file(tools/name) for name in TOOLS}
              and hash_file(host_executable)==host_hash,'source/tool window changed')
    report=dict(summary=summary,files={name:dict(bytes=len(raw),sha256=digest(raw)) for name,raw in files.items()},
                current_source_files=before,current_dependency_files=dependencies,current_tool_files=tool_hashes,current_host_executable_sha256=host_hash,
                freshness_scope='Current exporter/source/tool consistency, not authentication of prior executions or runtime source epochs.')
    output.mkdir(parents=True,exist_ok=False)
    for name,raw in files.items():
        target=output/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
    (output/'report.json').write_bytes(w.encode(report))
    check(output)
    return report


def check(directory):
    actual=inventory(directory)
    report=live.decode(w.payload(directory/'report.json'))
    w.fields(report,'summary files current_source_files current_dependency_files current_tool_files current_host_executable_sha256 freshness_scope')
    w.require(type(report['files']) is dict and len(report['files'])<=MAX_FILES,'bounded report inventory')
    w.require(actual==set(report['files'])|{'report.json'},'closed packet inventory before declared reads')
    files={}
    for name,row in report['files'].items():
        w.require(type(name) is str and not Path(name).is_absolute() and '..' not in Path(name).parts,'contained report path')
        w.fields(row,'bytes sha256');w.hex32(row['sha256'])
        w.require(type(row['bytes']) is int and 0<=row['bytes']<=live.MAX_BYTES,'declared file byte budget')
        w.require(sum(len(value) for value in files.values())+row['bytes']<=MAX_TOTAL,'aggregate charge before file read')
        raw=w.payload(directory/name,row['bytes'])
        w.require(len(raw)==row['bytes'] and digest(raw)==row['sha256'],'retained file bytes/hash')
        files[name]=raw
    w.require(set(report['current_source_files'])==SOURCE_FILES and report['current_source_files']=={name:hash_file(BASE/name) for name in SOURCE_FILES},'current declared source consistency')
    w.require(set(report['current_dependency_files'])==DEPENDENCIES and report['current_dependency_files']=={name:hash_file(BASE.parents[1]/name) for name in DEPENDENCIES},'current declared reader/native dependency consistency')
    w.require(set(report['current_tool_files'])==TOOLS,'closed recorded tool inventory')
    for value in report['current_tool_files'].values():w.hex32(value)
    w.hex32(report['current_host_executable_sha256'])
    artifact,blob=admitted()
    w.require(report['summary']==summarize(files,artifact,blob),'independent complete report reconstruction')
    w.require(all(step['preflight_host_executable_sha256']==report['current_host_executable_sha256'] for step in report['summary']['steps']),'recorded current host consistency')
    w.require(report['freshness_scope']=='Current exporter/source/tool consistency, not authentication of prior executions or runtime source epochs.','limited source epoch claim')
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input',type=Path);parser.add_argument('--output',type=Path)
    parser.add_argument('--tools',type=Path);parser.add_argument('--host-executable',type=Path)
    parser.add_argument('--check',type=Path)
    args=parser.parse_args()
    if args.check:result=check(args.check)
    else:
        if not all((args.input,args.output,args.tools,args.host_executable)):parser.error('export requires input/output/tools/host-executable')
        try:result=export(args.input,args.output,args.tools,args.host_executable)
        except PartialTrial as error:parser.exit(2,str(error)+'\n')
    print(json.dumps(result['summary'],indent=2))
