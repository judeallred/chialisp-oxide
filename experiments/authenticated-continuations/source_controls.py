#!/usr/bin/env python3
"""Generate, natively run, compile and replay actual source omissions.

All packages use the same original body except the exact declared recipe.
Source controls are separate from evidence corruption tests and the base chain.
"""
import argparse
import hashlib
import bounded_process
import json
import os
from pathlib import Path
import subprocess
import sys
import oracle as o
import replay
import admission

KINDS=("drop_opening","drop_terminal","drop_amount","wrong_successor")


def edited(source,kind):
    if kind=="original": return source
    recipes={
      "drop_opening":[("    assert!(item(recipient, next).0 == head.0);\n","")],
      "drop_terminal":[("        assert!(next.0 == terminal().0);\n","")],
      "wrong_successor":[("            instance_hash((module, next, count)),","            PuzzleHash([0x77; 32]),")],
      "drop_amount":[
        ("pub type Conditions = Cons<AssertMyAmount, Cons<CreateCoin, Option<CreateCoin>>>;","pub type Conditions = Cons<CreateCoin, Option<CreateCoin>>;"),
        ("    Cons {\n        first: assert_my_amount(remaining_budget(remaining)),\n        rest: Cons {\n            first: create_coin(recipient, PAYOUT),\n            rest: continuation,\n        },\n    }", "    Cons {\n        first: create_coin(recipient, PAYOUT),\n        rest: continuation,\n    }")],
    }
    for before,after in recipes[kind]:
        if source.count(before)!=1: raise ValueError(f"nonunique recipe {kind}")
        source=source.replace(before,after)
    return source


def queue():
    result=[o.terminal()]
    for value in reversed([0x11,0x22,0x33,0x44]):
        result.insert(0,o.item(bytes([value])*32,result[0]))
    return result


def case(kind):
    heads=queue()
    return dict(head=heads[0],next=heads[1],recipient=bytes([0x55 if kind=="drop_opening" else 0x11])*32,
                count=1 if kind=="drop_terminal" else 4)


def expected(kind,mutated):
    c=case(kind)
    if not mutated and kind in {"drop_opening","drop_terminal"}:
        return dict(status="native_panic")
    module=bytes([0xaa])*32
    conditions=[[73,1000*c["count"]],[51,c["recipient"].hex(),1000]]
    if c["count"]>1:
        successor=(bytes([0x77])*32 if mutated and kind=="wrong_successor" else
                   o.instance_hash(module,c["next"],c["count"]-1))
        conditions.append([51,successor.hex(),1000*(c["count"]-1)])
    if mutated and kind=="drop_amount": conditions=conditions[1:]
    return dict(status="accepted",output_hex=replay.encode(replay.condition_tree(conditions)).hex())


def native_example(kind,mutated):
    c=case(kind)
    array=lambda b:",".join(str(x) for x in b)
    access='''let mut conditions=vec![
        list(&[uint(out.first.0),uint(out.first.1.mojos())]),
        list(&[uint(out.rest.first.0),atom(&out.rest.first.1.0),uint(out.rest.first.2.mojos())])];
        if let Some(next)=out.rest.rest {conditions.push(list(&[uint(next.0),atom(&next.1.0),uint(next.2.mojos())]));}'''
    if mutated and kind=="drop_amount":
        access='''let mut conditions=vec![list(&[uint(out.first.0),atom(&out.first.1.0),uint(out.first.2.mojos())])];
        if let Some(next)=out.rest {conditions.push(list(&[uint(next.0),atom(&next.1.0),uint(next.2.mojos())]));}'''
    return '''use chialisp_oxide_authenticated_continuations as p;
use chialisp_oxide::{PuzzleHash,Message32};
fn atom(b:&[u8])->Vec<u8>{if b.is_empty(){vec![128]}else if b.len()==1&&b[0]<=127{b.to_vec()}else{let mut v=vec![128+u8::try_from(b.len()).unwrap()];v.extend(b);v}}
fn uint(n:u64)->Vec<u8>{if n==0{return atom(&[])}let b=n.to_be_bytes();let first=b.iter().position(|v|*v!=0).unwrap();let mut payload=b[first..].to_vec();if payload[0]&128!=0{payload.insert(0,0)}atom(&payload)}
fn list(items:&[Vec<u8>])->Vec<u8>{let mut v=Vec::new();for item in items{v.push(255);v.extend(item)}v.push(128);v}
fn main(){
let result=std::panic::catch_unwind(||p::step((PuzzleHash([0xaa;32]),Message32(['''+array(c['head'])+''']),'''+str(c['count'])+'''),(PuzzleHash(['''+array(c['recipient'])+''']),Message32(['''+array(c['next'])+''']))));
match result{Err(_)=>println!(r#"{{"status":"native_panic"}}"#),Ok(out)=>{'''+access+'''
let hex=list(&conditions).iter().map(|b|format!("{b:02x}")).collect::<String>();
println!(r#"{{"status":"accepted","output_hex":"{hex}"}}"#);}}}
'''


def run(argv,env,log):
    try:
        completed=bounded_process.run([str(x) for x in argv],env=env,timeout=180)
    except (bounded_process.OutputLimit,subprocess.TimeoutExpired) as error:
        log.with_suffix('.stdout').write_bytes(error.stdout or b'')
        log.with_suffix('.stderr').write_bytes(error.stderr or b'')
        raise
    log.with_suffix('.stdout').write_bytes(completed.stdout)
    log.with_suffix('.stderr').write_bytes(completed.stderr)
    if completed.returncode: raise SystemExit(f"command failed; preserve {log}.*")
    return completed.stdout


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--tools',type=Path,required=True)
    parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--target',type=Path,required=True)
    parser.add_argument('--ledger',type=Path,required=True)
    args=parser.parse_args()
    args.work=args.work.resolve();args.work.mkdir(parents=True,exist_ok=False)
    root=Path(__file__).resolve().parents[2]
    base=Path(__file__).resolve().parent
    source=(base/'src/lib.rs').read_text()
    env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(args.target.resolve()),CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true')
    compiler=bounded_process.run(['rustup','run',admission.TOOLCHAIN,'rustc','-vV'],check=True,timeout=60).stdout.decode()
    driver_sha=hashlib.sha256((args.tools/'chialisp-oxide-driver').read_bytes()).hexdigest()
    verifier_sha=hashlib.sha256((args.tools/'chialisp-oxide-verifier').read_bytes()).hexdigest()
    tools_before={name:hashlib.sha256((args.tools/name).read_bytes()).hexdigest() for name in ('cargo-chialisp-oxide','chialisp-oxide-driver','chialisp-oxide')}
    records=[];control_input=[];native=[];vm=[]
    for kind in KINDS:
        for mutated in (False,True):
            role='mutant' if mutated else 'original'
            package=args.work/f'{kind}-{role}';(package/'src').mkdir(parents=True);(package/'examples').mkdir()
            body=edited(source,kind if mutated else 'original')
            (package/'src/lib.rs').write_text(body)
            manifest='''[package]\nname="chialisp-oxide-authenticated-continuations"\nversion="0.0.0"\nedition="2024"\npublish=false\n[workspace]\n[dependencies]\nchialisp-oxide={path='''+json.dumps(str(root/'crates/chialisp-oxide'))+''',version="0.1.0"}\n'''
            (package/'Cargo.toml').write_text(manifest)
            (package/'Cargo.lock').write_bytes((base/'Cargo.lock').read_bytes())
            (package/'examples/native_case.rs').write_text(native_example(kind,mutated))
            stdout=run(['cargo','run','--manifest-path',package/'Cargo.toml','--offline','--example','native_case'],env,package/'native')
            observed=json.loads(stdout)
            if observed!=expected(kind,mutated):raise SystemExit(f'{kind}/{role}: native mismatch {observed}')
            native.append(dict(kind=kind,role=role,observed=observed))
            for mode in ('on','off'):
                build=package/f'bundle-{mode}'
                command=[args.tools/'cargo-chialisp-oxide','build','--manifest-path',package/'Cargo.toml','--package','chialisp-oxide-authenticated-continuations','--locked','--output-dir',build]
                if mode=='off':command+=['--no-optimize']
                before=admission.source_snapshot(package)
                run(command,env,package/f'build-{mode}')
                admission.require(before==admission.source_snapshot(package),'source changed during extraction')
                loaded=admission.fresh(build,package,mode,compiler=compiler,driver_sha256=driver_sha,
                    cargo_options=['--manifest-path',str(package/'Cargo.toml'),'--package',admission.PACKAGE,'--locked'],drop_amount=mutated and kind=='drop_amount')
                manifest_path=loaded.manifest_path
                actual_manifest=loaded.manifest
                entry=actual_manifest['entries'][0]
                program=manifest_path.parent/entry['clvm']
                recompiled=admission.recompile(manifest_path.parent,mode,args.tools/'chialisp-oxide-verifier')
                admission.require(recompiled['verifier_sha256']==verifier_sha,'independent frozen verifier identity')
                if not mutated:
                    retained=next((base/'evidence'/f'rust-{mode}').glob('*.clvm.hex'))
                    if bytes.fromhex(program.read_text().strip()) != bytes.fromhex(retained.read_text().strip()):
                        raise SystemExit(f'{kind}/original/{mode}: changed baseline artifact')
                c=case(kind)
                solution=f"((0x{'aa'*32} 0x{c['head'].hex()} {c['count']}) (0x{c['recipient'].hex()} 0x{c['next'].hex()}))"
                solutions=package/'solution.json';solutions.write_text(json.dumps([solution]))
                result=json.loads(run([args.tools/'chialisp-oxide','run-batch',program,'--solutions',solutions,'--max-cost','10000000'],env,package/f'vm-{mode}'))[0]
                expected_native=expected(kind,mutated)
                if expected_native['status']=='native_panic':
                    if result['outcome']!={'status':'rejected'} or result.get('failure')!={'kind':'program','code':'raise'}:raise SystemExit(f'{kind}/{role}/{mode}: exact VM Raise required')
                elif result['outcome']!=expected_native:raise SystemExit(f'{kind}/{role}/{mode}: exact VM output required {result}')
                vm.append(dict(kind=kind,role=role,mode=mode,execution=result))
                records.append(dict(kind=kind,role=role,mode=mode,manifest=str(manifest_path),program=str(program),recompiled=recompiled,source_sha256=hashlib.sha256(body.encode()).hexdigest()))
    for kind in KINDS:
        for mode in ('on','off'):
            select=lambda role:next(r for r in records if r['kind']==kind and r['mode']==mode and r['role']==role)
            control_input.append(dict(kind=kind,mode=mode,original_path=select('original')['program'],mutant_path=select('mutant')['program']))
    input_path=args.work/'ledger-input.json';input_path.write_text(json.dumps(dict(controls=control_input),indent=2)+'\n')
    run([args.ledger,'--controls',input_path,args.work/'ledger'],env,args.work/'ledger-run')
    admission.require(tools_before=={name:hashlib.sha256((args.tools/name).read_bytes()).hexdigest() for name in tools_before},'tools changed during capture')
    report=dict(compiler=compiler,verifier_sha256=verifier_sha,tools=tools_before,schema=1,scope='actual-source own-reveal controls; native, primary VM and SDK attempts are separate',base_source_sha256=hashlib.sha256(source.encode()).hexdigest(),records=records,native=native,primary_vm=vm,counts=dict(native_observations=8,primary_vm_observations=16,sdk_attempts=16))
    (args.work/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report['counts']))

if __name__=='__main__':main()
