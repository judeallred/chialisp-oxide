"""Repeat local SDK execution after complete fresh raw Cargo capture admission.

No compiler, driver or Cargo build is run here. Recorded reading is a separate
phase and adds no execution credit. All paths/tools are explicit capture inputs.
"""
import argparse
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True

import capture
import capture_support as support
from capture_support import recorded_command
import protocol_oracle as oracle
import raw_evidence

ROOT = capture.ROOT
HERE = Path(__file__).resolve().parent
SCOPE = 'Actual finite primary-VM/local SDK observations. Repeat adds no scenarios. Source/binary freshness is not execution authentication, BLS proof, compiler preservation, slots, whole issue14 or live Testnet11 evidence.'
PATH_FIELDS = ('raw_capture','native_target','compiler_target','compiler_root','rustc','driver','cargo_cli')


def binary(args):
    return capture.native_target(args, 'original')/'debug/examples/protocol'


def state(args):
    tools = capture.tools(args)
    return {'raw_source_tools': capture.quiet(args, tools),
            'protocol_binary': support.identity(binary(args),512*1024*1024),
            'artifact_packet': support.identity(args.output/'artifacts.json'),
            'capture_python': support.identity(Path(sys.executable).resolve())}


def source_args(args):
    result = []
    for name in ('output','native_target','compiler_target','compiler_root','rustc','driver','cargo_cli'):
        result += ['--'+name.replace('_','-'), str(getattr(args,name))]
    return result


def raw_read(args, directory, label):
    return support.command(directory,label,[Path(sys.executable).resolve(),HERE/'capture.py','read',*source_args(args)],
                           target=capture.native_target(args,'original'),env_extra=capture.env(args))


def check_receipt_paths(receipt,args):
    oracle.require(set(receipt)=={'protocol','status',*PATH_FIELDS,'first','repeat','source_window_unchanged','repeat_byte_identical','scope'},'closed protocol receipt fields')
    for name in PATH_FIELDS:
        expected=args.output if name=='raw_capture' else getattr(args,name)
        oracle.require(type(receipt[name]) is str and receipt[name]==str(expected),'exact retained capture path: '+name)


def run(args, destination):
    destination.mkdir(parents=True,exist_ok=False)
    capture.verify(args)
    before=state(args)
    support.write(destination/'inputs-before.json',before)
    raw_read(args,destination,'raw-schema2-read')
    oracle.require(support.same_json(state(args),before),'source/tool window after complete raw admission')
    packet=args.output/'artifacts.json'
    oracle.admit_packet(packet)
    oracle.modules()
    reports=[]
    for index in (1,2):
        label=f'protocol-attempt{index}'
        raw=support.command(destination,label,[binary(args),packet],
                            target=capture.native_target(args,'original'),env_extra=capture.env(args))
        oracle.require(support.same_json(state(args),before),'quiet source/tool/artifact window after SDK execution')
        report=json.loads(raw)
        reader=oracle.check_source_controls(report,packet)
        support.write(destination/(label+'.reader.json'),reader)
        reports.append(raw)
    oracle.require(reports[0]==reports[1],'independent SDK repeat must be byte-identical')
    raw_read(args,destination,'raw-schema2-read-after')
    after=state(args)
    oracle.require(support.same_json(after,before),'complete source/tool admission after both executions')
    support.write(destination/'inputs-after.json',after)
    support.write(destination/'report.json',{
        'protocol':'chialisp-oxide-action-counter-protocol-capture/v1',
        'status':'five-mode-counter-repeat-passed','raw_capture':str(args.output),
        'native_target':str(args.native_target),'compiler_target':str(args.compiler_target),
        'compiler_root':str(args.compiler_root),'rustc':str(args.rustc),'driver':str(args.driver),'cargo_cli':str(args.cargo_cli),
        'first':support.identity(destination/'protocol-attempt1.stdout'),
        'repeat':support.identity(destination/'protocol-attempt2.stdout'),
        'source_window_unchanged':True,'repeat_byte_identical':True,
        'scope':SCOPE})
    print(json.dumps({'status':'passed','output':str(destination),'bytes':len(reports[0]),'sha256':support.digest(reports[0])}))


def recorded(args,destination):
    capture.verify(args)
    before=json.loads(support.read(destination/'inputs-before.json'))
    oracle.require(support.same_json(before,json.loads(support.read(destination/'inputs-after.json'))) and support.same_json(before,state(args)),'current matched protocol source/tool windows')
    # Full original37-export and mutant schema2 inventories, native witness logs
    # and independently retained compiler queries are all readmitted first.
    raw_evidence.recorded(args)
    for label in ('raw-schema2-read','raw-schema2-read-after'):
        recorded_command(destination,label,[Path(sys.executable).resolve(),HERE/'capture.py','read',*source_args(args)],
            target=capture.native_target(args,'original'),rustc=args.rustc)
    first=None
    for index in (1,2):
        label=f'protocol-attempt{index}'
        recorded_command(destination,label,[binary(args),args.output/'artifacts.json'],
            target=capture.native_target(args,'original'),rustc=args.rustc)
        raw=support.read(destination/(label+'.stdout'))
        current=oracle.check_source_controls(json.loads(raw),args.output/'artifacts.json')
        oracle.require(support.same_json(current,json.loads(support.read(destination/(label+'.reader.json')))),'retained exact independent reader summary')
        if first is None:first=raw
        else:oracle.require(raw==first,'recorded independent SDK repeat')
    receipt=json.loads(support.read(destination/'report.json'))
    check_receipt_paths(receipt,args)
    oracle.require(receipt['protocol']=='chialisp-oxide-action-counter-protocol-capture/v1'
        and receipt['status']=='five-mode-counter-repeat-passed'
        and support.same_json(receipt['first'],support.identity(destination/'protocol-attempt1.stdout'))
        and support.same_json(receipt['repeat'],support.identity(destination/'protocol-attempt2.stdout'))
        and receipt['source_window_unchanged'] is True and receipt['repeat_byte_identical'] is True
        and receipt['scope']==SCOPE,'recorded exact protocol repeat receipt')
    print(json.dumps({'status':'recorded_protocol_consistency_passed','scope':'Reader only; no new execution credit.'}))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=('run','read'))
    parser.add_argument('--protocol-output',type=Path,required=True)
    for name in ('output','native-target','compiler-target','compiler-root','rustc','driver','cargo-cli'):
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    for name in ('output','native_target','compiler_target','compiler_root','rustc','driver','cargo_cli','protocol_output'):
        setattr(args,name,getattr(args,name).resolve())
    oracle.require(not args.protocol_output.is_relative_to(ROOT/'experiments'),'outputs outside source')
    globals()['run' if args.phase=='run' else 'recorded'](args,args.protocol_output)


if __name__=='__main__':main()
