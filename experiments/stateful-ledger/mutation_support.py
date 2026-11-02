"""Capture exact source recipes, native outputs, two VM modes and SDK witnesses.

No compiler, formatter, VM or simulator runs on import. Only deliberately copied
private Cargo packages are mutated. Original source and historical packets are
never edited by this capture.
"""
from pathlib import Path
import argparse
import importlib.util
import json
import shutil
import sys
from capture_support import Capture, ROOT, read, sha, write, require, source_window,admit_retained_bundle,validate_source_window,validate_compiler_window,source_corruption_controls,COMMON_RECEIPT,closed,validate_inventory,validate_commands,validate_reader_controls,SOURCE_READER_CONTROLS
sys.path.insert(0,str(ROOT/'scripts'))
import raw_tree
from vm_failures import check_execution
import ledger_reader
PROTOCOL='chialisp-oxide-stateful-mutations/v1'

def module(path,name):
    spec=importlib.util.spec_from_file_location(name,path);result=importlib.util.module_from_spec(spec);spec.loader.exec_module(result);return result

def expected(conditions):
    if conditions is None:return dict(status='rejected')
    def atom(v):return raw_tree.parse(str(v)) if type(v) is int else bytes.fromhex(v)
    def proper(xs):
        t=b''
        for x in reversed(xs):t=x,t
        return t
    return dict(status='accepted',output_hex=raw_tree.serialize(proper([proper([atom(x) for x in c]) for c in conditions])).hex())

def array(h):return '['+','.join(str(v) for v in bytes.fromhex(h))+']'

NATIVE_SERIALIZER=r'''
fn atom(v:&[u8])->Vec<u8>{if v.len()==1&&v[0]<128{return v.to_vec()}if v.len()<64{let mut out=vec![128+v.len() as u8];out.extend(v);out}else{panic!("finite atom bound")}}
fn uint(v:u64)->Vec<u8>{let b=v.to_be_bytes();let i=b.iter().position(|x|*x!=0).unwrap_or(8);let mut x=b[i..].to_vec();if x.first().is_some_and(|x|x&128!=0){x.insert(0,0);}atom(&x)}
fn list(xs:Vec<Vec<u8>>)->Vec<u8>{let mut t=vec![128];for x in xs.into_iter().rev(){let mut p=vec![255];p.extend(x);p.extend(t);t=p;}t}
fn c2(op:u64,v:u64)->Vec<u8>{list(vec![uint(op),uint(v)])}
fn create(op:u64,p:&[u8],v:u64)->Vec<u8>{list(vec![uint(op),atom(p),uint(v)])}
fn main(){let result=std::panic::catch_unwind(probe);match result{Ok(v)=>{print!("{{\"status\":\"accepted\",\"output_hex\":\"");for b in v{print!("{b:02x}");}println!("\"}}");},Err(_)=>println!("{{\"status\":\"rejected\"}}")}}
'''

def native_probe(project,w):
    if project=='continuation-batches':
        body=f'''let x=chialisp_oxide_continuation_batches::step((PuzzleHash({array(w['module'])}),Message32({array(w['head'])}),{w['remaining']}),((PuzzleHash({array(w['first'])}),Message32({array(w['middle'])})),(PuzzleHash({array(w['second'])}),Message32({array(w['next'])}))));
        let mut cs=vec![c2(x.first.0.into(),x.first.1.mojos()),create(x.rest.first.0.into(),&x.rest.first.1.0,x.rest.first.2.mojos()),create(x.rest.rest.first.0.into(),&x.rest.rest.first.1.0,x.rest.rest.first.2.mojos())];if let Some(c)=x.rest.rest.rest{{cs.push(create(c.0.into(),&c.1.0,c.2.mojos()));}}list(cs)'''
        imports='use chialisp_oxide::{PuzzleHash,Message32};'
    else:
        body=f'''let x=chialisp_oxide_spuds::step((PuzzleHash({array(w['module'])}),PublicKey({array(w['key'])}),PuzzleHash({array(w['payout'])}),1),{str(w['take']).lower()},PublicKey({array(w['next_key'])}),PuzzleHash({array(w['next_payout'])}));
        list(vec![list(vec![uint(x.0.0.into()),atom(&x.0.1.0),atom(&x.0.2.0)]),c2(x.1.0.into(),x.1.1.mojos()),c2(x.2.0.into(),x.2.1.blocks().into()),c2(x.3.0.into(),x.3.1.blocks().into()),create(x.4.0.into(),&x.4.1.0,x.4.2.mojos()),list(vec![uint(x.5.0.into()),atom(&x.5.1.0)])])'''
        imports='use chialisp_oxide::{PuzzleHash,PublicKey};'
    return imports+'\nfn probe()->Vec<u8>{'+body+'}\n'+NATIVE_SERIALIZER

def capture(base,args):
    recipes=module(base/'mutation_cases.py','stateful_recipes_'+base.name)
    oracle=module(base/'oracle.py','stateful_oracle_'+base.name)
    witnesses=recipes.witnesses(oracle);before=source_window(base)
    c=Capture(args.output,args.tools);reports=[];sdk=[]
    for w in witnesses:
        name=w['name'];old,new=recipes.RECIPES[name]
        original=read(base/'src/lib.rs').decode();require(original.count(old)==1,'unique exact source mutation: '+name)
        source_dir=c.out/name;source_dir.mkdir()
        write(source_dir/'witness.json',w);write(source_dir/'recipe.json',dict(before=old,after=new,occurrences=1))
        item=dict(name=name,variants={})
        for variant in ('original','mutant'):
            project=source_dir/variant;project.mkdir();shutil.copytree(base/'src',project/'src')
            manifest=read(base/'Cargo.toml').decode().replace('../../crates/',str(ROOT/'crates')+'/')
            (project/'Cargo.toml').write_text(manifest);shutil.copyfile(base/'Cargo.lock',project/'Cargo.lock')
            (project/'src/lib.rs').write_text(original if variant=='original' else original.replace(old,new,1))
            (project/'src/bin').mkdir(exist_ok=True);(project/'src/bin/mutation-probe.rs').write_text(native_probe(base.name,w))
            native=c.command(name+'-'+variant+'-native',['cargo','run','--manifest-path',project/'Cargo.toml','--locked','--bin','mutation-probe'],target=c.out/'native-target')
            native_out=json.loads(native.stdout);want=expected(w[variant+'_conditions']);require(native_out==want,'native exact independent witness: '+name+'/'+variant)
            modes=c.build(project,'chialisp-oxide-'+base.name,project/'artifacts',name+'-'+variant)
            observations={};solutions=project/'solutions.json';write(solutions,[w['solution']])
            for mode,path in modes.items():
                result=c.command(name+'-'+variant+'-'+mode+'-vm',[c.tools/'chialisp-oxide','run-batch',path,'--solutions',solutions,'--max-cost','100000000'])
                vm=json.loads(result.stdout);require(len(vm)==1 and check_execution(vm[0])==want,'exact source VM witness '+name+'/'+variant+'/'+mode)
                if want['status']=='rejected':require(vm[0]['failure']==dict(kind='program',code='raise'),'original actual policy Raise')
                observations[mode]=vm[0]
                sdk.append(dict(project=base.name,case=name,variant=variant,mode=mode,module_hex_path=str(path)))
            item['variants'][variant]=dict(native=native_out,primary_vm=observations,source_sha256=sha(project/'src/lib.rs'))
        reports.append(item)
    write(c.out/'sdk-input.json',dict(cases=sdk))
    c.command('sdk',['cargo','run','--manifest-path',ROOT/'experiments/stateful-ledger/Cargo.toml','--locked','--offline','--','--mutations',c.out/'sdk-input.json',c.out/'sdk-report.json'],target=ROOT/'target/stateful-ledger-private')
    require(source_window(base)==before,'mutation capture source window changed')
    packet=dict(protocol=PROTOCOL,status='actual_source_mutations_captured',experiment=base.name,source_files=before,cases=reports,sdk=json.loads(read(c.out/'sdk-report.json')),scope='finite actual-source policy controls; native outputs, two primary VM modes and own-reveal SDK witnesses are distinct; no compiler-preservation or execution-authentication credit')
    packet['reader_controls']=corruption_controls(packet['sdk'],c.out,base.name,set(recipes.RECIPES))|source_corruption_controls(before,source_window(base))
    c.finish(packet);validate(c.out/'receipt.json')
    return packet

def validate(path):
    base=path.resolve().parent;r=json.loads(read(path));closed(r,COMMON_RECEIPT|{'protocol','status','experiment','source_files','cases','sdk','scope','reader_controls'},'mutation receipt');validate_inventory(r,base);require(r.get('protocol')==PROTOCOL and r.get('status')=='actual_source_mutations_captured','mutation receipt protocol/status')
    require(r.get('compiler_preservation') is False and r.get('execution_authentication') is False,'scope limits')
    for label,digest in r['files'].items():
        p=Path(label);require(not p.is_absolute() and '..' not in p.parts and p.as_posix()==label,'confined path');require((base/p).resolve().is_relative_to(base) and sha(base/p)==digest,'retained mutation identity')
    require(json.loads(read(base/'sdk-report.json'))==r['sdk'],'SDK report identity')
    require(len(r['cases'])==(4 if r['experiment']=='continuation-batches' else 5),'complete mutation case set')
    labels=['compiler']
    for case in r['cases']:
        for variant in ('original','mutant'):labels += [case['name']+'-'+variant+'-'+suffix for suffix in ('native','check','on','off','rust_on-vm','rust_off-vm')]
    labels.append('sdk');validate_commands(base,labels)
    validate_reader_controls(r['reader_controls'],SOURCE_READER_CONTROLS|{'wrong_code','accepted_bad_kind','count','mode','own_reveal','artifact','paid_amount','solution','funding_coin_id','altered_source_recipe'})
    require(r['experiment'] in ('continuation-batches','spuds'),'closed mutation experiment identity')
    validate_source_window(r['source_files'],source_window(ROOT/'experiments'/r['experiment']));validate_compiler_window(r)
    project=ROOT/'experiments'/r['experiment'];recipes=module(project/'mutation_cases.py','reader_recipes');oracle=module(project/'oracle.py','reader_oracle')
    witnesses={w['name']:w for w in recipes.witnesses(oracle)}
    require({row['name'] for row in r['cases']}==set(recipes.RECIPES),'exact complete mutation names')
    for row in r['cases']:
        require(set(row['variants'])=={'original','mutant'},'paired source variants')
        name=row['name'];w=witnesses[name];old,new=recipes.RECIPES[name]
        require(json.loads(read(base/name/'witness.json'))==w,'independent source witness')
        check_recipe(json.loads(read(base/name/'recipe.json')),old,new)
        original=read(project/'src/lib.rs').decode();require(original.count(old)==1,'unique retained source recipe')
        for variant,v in row['variants'].items():
            want=expected(w[variant+'_conditions']);require(v['native']==want,'independent expected native output')
            require(json.loads(read(base/(name+'-'+variant+'-native.stdout')))==v['native'],'native actual command stdout identity')
            require(read(base/name/variant/'src/bin/mutation-probe.rs').decode()==native_probe(r['experiment'],w),'exact current generated native witness probe')
            require(read(base/name/variant/'Cargo.lock')==read(project/'Cargo.lock'),'exact copied native/compiler lockfile')
            require(read(base/name/variant/'Cargo.toml').decode()==read(project/'Cargo.toml').decode().replace('../../crates/',r['source_origin']+'/crates/'),'exact declared copied private Cargo manifest')
            expected_sources={p.relative_to(project/'src').as_posix():read(p) for p in (project/'src').rglob('*.rs')}
            expected_sources['lib.rs']=(original if variant=='original' else original.replace(old,new,1)).encode();expected_sources['bin/mutation-probe.rs']=native_probe(r['experiment'],w).encode()
            actual_sources={p.relative_to(base/name/variant/'src').as_posix():read(p) for p in (base/name/variant/'src').rglob('*.rs')}
            require(actual_sources==expected_sources,'complete exact copied/generated Rust source inventory')
            for mode,out in v['primary_vm'].items():
                retained=json.loads(read(base/(name+'-'+variant+'-'+mode+'-vm.stdout')));require(retained==[out],'actual VM command stdout identity')
                if want['status']=='rejected':require(out['failure']==dict(kind='program',code='raise'),'exact source-policy Program/Raise outcome')
            code=original if variant=='original' else original.replace(old,new,1)
            require(read(base/name/variant/'src/lib.rs').decode()==code and sha(base/name/variant/'src/lib.rs')==v['source_sha256'],'exact actual source mutation bytes')
            for mode in ('on','off'):admit_retained_bundle(base/name/variant/'artifacts'/('rust-'+mode),r,'chialisp-oxide-'+r['experiment'],mode=='on',base/name/variant)
            require(set(v['primary_vm'])=={'rust_on','rust_off'},'both actual source VM modes')
            require(all(check_execution(out)==v['native'] for out in v['primary_vm'].values()),'native vs retained VM witness')
    validate_sdk(r['sdk'],base,r['experiment'],set(recipes.RECIPES))
    return dict(status='retained_mutation_packet_valid',execution_credit=False)

def proper(tree):
    out=[]
    while type(tree) is tuple:out.append(tree[0]);tree=tree[1]
    require(tree==b'', 'proper list required');return out

def validate_sdk(sdk,base,project,names):
    require(sdk.get('protocol')=='chialisp-oxide-stateful-mutation-sdk/v1' and sdk.get('status')=='passed','SDK protocol/status')
    rows=sdk['records'];require(len(rows)==len(names)*4,'exact SDK attempt count')
    wanted={(n,v,m) for n in names for v in ('original','mutant') for m in ('rust_on','rust_off')};seen=set()
    for row in rows:
        name,variant,mode=row['case'],row['variant'],row['mode'];identity=(name,variant,mode);require(identity in wanted and identity not in seen and row['project']==project,'exact paired SDK mode inventory');seen.add(identity)
        root=base/name/variant/'artifacts'/mode.replace('_','-');ptr=json.loads(read(root/'latest.json'));manifest=json.loads(read(root/ptr['manifest']));payload=bytes.fromhex(read((root/ptr['manifest']).parent/manifest['entries'][0]['clvm']).decode().strip());tree=raw_tree.deserialize(payload)
        import hashlib
        require(row['artifact_blob_sha256']==hashlib.sha256(payload).hexdigest() and row['module_hash']==raw_tree.tree_hash(tree).hex(),'SDK exact selected artifact identity')
        ledger_reader.mutation_witness(row)
        actual=row['actual'];reveal=raw_tree.deserialize(bytes.fromhex(actual['curried_reveal_hex']));parts=proper(reveal);require(len(parts)==3 and parts[0]==b'\x02' and parts[1]==(b'\x01',tree),'SDK own compiled module in curry');env=proper(parts[2]);require(len(env)==3 and env[0]==b'\x04' and env[2]==b'\x01' and env[1][0]==b'\x01','standard one-argument curry');terms=proper(env[1][1]);require(terms[0].hex()==row['module_hash'],'own selected module in terms');require(raw_tree.tree_hash(reveal).hex()==actual['own_reveal_funding']['puzzle_hash'],'own reveal funding PH')
        sub=actual['submission'];mutant=variant=='mutant'
        umbrella=project=='continuation-batches' and not mutant and name in ('second_opening','terminal')
        code=None
        if not mutant:
            if project=='continuation-batches':code={'second_opening':'GeneratorRuntimeError','terminal':'GeneratorRuntimeError','amount':'AssertMyAmountFailed','successor':None}[name]
            else:code={'amount':'AssertMyAmountFailed','age':'AssertHeightRelativeFailed','nextstate':None,'announcement':'AssertCoinAnnouncementFailed','signature_binding':'BadAggregateSignature'}[name]
        if umbrella:obs=sub
        elif code is not None:require(sub['status']=='consensus_rejected','original actual SDK rejection');obs=sub['observation']
        else:
            require(sub['status']=='accepted','mutant/positive SDK acceptance');obs=sub['observation'];require(len(obs['outputs'])>=1,'accepted exact outputs retained');continue
        require(obs['code']==code and obs['full_simulator_equal'] is True,'exact typed rejection and full state equality')
        require(obs['classification']==('inconclusive_runtime_umbrella' if umbrella else 'typed_consensus_rejection') and obs['policy_rejection_credit'] is (not umbrella),'runtime umbrella claim boundary')
    require(seen==wanted,'complete SDK set')


def check_recipe(actual,old,new):
    require(actual==dict(before=old,after=new,occurrences=1),'exact retained mutation recipe')

def corruption_controls(sdk,base,project,names):
    import copy
    results={}
    for kind in ('wrong_code','accepted_bad_kind','count','mode','own_reveal','artifact','paid_amount','solution','funding_coin_id'):
        bad=copy.deepcopy(sdk)
        if kind=='wrong_code':
            row=next(x for x in bad['records'] if x['variant']=='original' and x['case']=='amount');row['actual']['submission']['observation']['code']='BadAggregateSignature'
        elif kind=='accepted_bad_kind':
            row=next(x for x in bad['records'] if x['variant']=='mutant');row['actual']['submission']['status']='consensus_rejected'
        elif kind=='count':bad['records'].pop()
        elif kind=='mode':bad['records'][0]['mode']='invented_mode'
        elif kind=='own_reveal':bad['records'][0]['actual']['curried_reveal_hex']='80'
        elif kind=='paid_amount':next(x for x in bad['records'] if x['variant']=='mutant')['actual']['submission']['observation']['outputs'][0]['amount']+=1
        elif kind=='solution':bad['records'][0]['actual']['instance_solution_hex']='80'
        elif kind=='funding_coin_id':bad['records'][0]['actual']['own_reveal_funding']['coin_id']='00'*32
        else:bad['records'][0]['artifact_blob_sha256']='00'*32
        try:validate_sdk(bad,base,project,names)
        except (ValueError,KeyError,IndexError,TypeError):results[kind]='rejected'
        else:raise ValueError('reader corruption accepted: '+kind)
    # Recipe matcher is independently reconstructed from current actual source.
    recipes=module(ROOT/'experiments'/project/'mutation_cases.py','corruption_recipes');name=next(iter(recipes.RECIPES));old,new=recipes.RECIPES[name]
    recipe=json.loads(read(base/name/'recipe.json'));require(recipe==dict(before=old,after=new,occurrences=1),'positive exact recipe control');bad=dict(recipe);bad['after']=old
    try:check_recipe(bad,old,new)
    except ValueError:results['altered_source_recipe']='rejected'
    else:raise ValueError('altered source recipe accepted')
    return results


def main(base):
    p=argparse.ArgumentParser();p.add_argument('--tools',type=Path);p.add_argument('--output',type=Path);p.add_argument('--check',type=Path);args=p.parse_args()
    if args.check:require(not args.tools and not args.output,'reader never executes tools');result=validate(args.check)
    else:require(args.tools and args.output,'tools/output required');result=capture(base,args)
    print(json.dumps(result,indent=2))
