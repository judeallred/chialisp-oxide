"""Pure retained-output count and pinned source audit. Executes no VM or network."""
import collections,copy,hashlib,json,re,sys,tarfile,tomllib
from pathlib import Path
ROOT=Path('/Users/judeallred/code/clispOx');OUT=ROOT/'target/action15-create-cost-audit-a01';BASE=ROOT/'target/xchandles15-research-20261026/attempt4';REG=Path('/Users/judeallred/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chia-consensus-0.36.1');ARCHIVE=Path('/Users/judeallred/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/chia-consensus-0.36.1.crate')
sys.path.insert(0,str(ROOT/'scripts'));import registry_protocol as r

def require(v,m):
 if not v:raise ValueError(m)
def sha(v):return hashlib.sha256(v).hexdigest()
def read(p,n=2*1024*1024):
 s=p.lstat();require(p.is_file() and not p.is_symlink() and s.st_nlink==1 and s.st_size<=n,'bounded regular');b=p.read_bytes();require(len(b)==s.st_size,'quiet read');return b
def fields(v):
 out=[]
 while type(v)is tuple:require(len(out)<4096,'bounded condition list');out.append(v[0]);v=v[1]
 require(v==b'','proper list');return out
source_names=['src/opcodes.rs','src/conditions.rs','src/consensus_constants.rs','src/spendbundle_validation.rs','src/flags.rs','.cargo_vcs_info.json']
lock=tomllib.loads(read(ROOT/'Cargo.lock').decode());pkg=next(p for p in lock['package']if p['name']=='chia-consensus');ar=read(ARCHIVE);require(pkg['version']=='0.36.1'and sha(ar)==pkg['checksum']=='d942bcb20b243213becbd94630dd7122c18dea7450beab38cbb4a6fcac993cae','actual crate archive equals reviewed lock checksum');sources={};source_rows=[]
with tarfile.open(ARCHIVE)as tar:
 for n in source_names:
  m=tar.getmember('chia-consensus-0.36.1/'+n);require(m.isfile()and 0<m.size<=1024*1024,'source member precharge');b=tar.extractfile(m).read(m.size+1);require(len(b)==m.size and b==read(REG/n),'installed source equals exact locked archive');sources[n]=b
  target=OUT/'sources'/n;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b);source_rows.append({'path':n,'bytes':len(b),'sha256':sha(b)})
vcs=json.loads(sources['.cargo_vcs_info.json']);require(vcs['git']['sha1']=='56f6b676a96de8659fe05d7e4f705df553f37fa5','crate VCS metadata pin')
constant=re.search(rb'pub const CREATE_COIN_COST: Cost = ([0-9_]+);',sources['src/opcodes.rs']);rate=int(constant[1].replace(b'_',b''));require(rate==1800000,'pinned CREATE cost')
ceiling=int(re.search(rb'max_block_cost_clvm: ([0-9_]+),',sources['src/consensus_constants.rs'])[1].replace(b'_',b''));require(ceiling==11000000000,'pinned block limit')
receipt=json.loads(read(BASE/'execution-receipt.json'));require(len(receipt['steps'])==129,'retained component count');outcomes=[];rows=[];coin_ids=set()
def count_conditions(conditions,names,parent):
 found=[]
 for c in conditions:
  v=fields(c);require(v and type(v[0])is bytes,'condition opcode')
  if v[0]!=b'\x33':continue
  require(len(v)in(3,4)and type(v[1])is bytes and len(v[1])==32 and type(v[2])is bytes,'CREATE shape')
  require(v[2]in(b'',b'\1'),'observed amounts only nil0/atom1')
  found.append((v[1],v[2]))
 require(len(found)==3*names+1,'three slots per observed name plus controller')
 require(sum(amount==b''for _,amount in found)==3*names and sum(amount==b'\1'for _,amount in found)==1,'zero and positive counts')
 require(len(set(found))==len(found),'duplicate output would be rejected, not discounted')
 ids=[sha(bytes.fromhex(parent)+ph+amount)for ph,amount in found]
 return found,ids
for s in receipt['steps']:
 b=read(BASE/'steps'/f"{s['index']:03}.stdout.json");e=r.json_value(b);require(e['outcome']['status']=='accepted'and e['error']is None and type(e['cost'])is int,'actual accepted retained output');tree=r.codec.decode(bytes.fromhex(e['outcome']['output_hex']));conditions=fields(tree);creates,ids=count_conditions(conditions,len(s['handles']),s['coin_id']);require(not coin_ids.intersection(ids),'full created coin ID uniqueness across parents');coin_ids.update(ids);outcomes.append((conditions,len(s['handles']),s['coin_id']))
 rows.append({'index':s['index'],'names':len(s['handles']),'create_coin_conditions':len(creates),'zero_mojo':sum(a==b''for _,a in creates),'one_mojo':sum(a==b'\1'for _,a in creates),'condition_cost_lower_bound':len(creates)*rate,'stdout_bytes':len(b),'stdout_sha256':sha(b)})
# Constructed falsifications check only this pure count contract, never consensus.
conditions,names,parent=outcomes[1];first_zero=next(i for i,c in enumerate(conditions)if fields(c)[0]==b'\x33'and fields(c)[2]==b'');controls=[]
def rejects(label,changed):
 try:count_conditions(changed,names,parent)
 except ValueError:controls.append(label)
 else:raise ValueError('accepted falsification '+label)
changed=list(conditions);changed.pop(first_zero);rejects('drop_one_zero_creation',changed)
changed=list(conditions);changed.append(changed[first_zero]);rejects('duplicate_zero_output',changed)
changed=list(conditions);v=fields(changed[first_zero]);v[0]=b'\x01';changed[first_zero]=r.oracle.proper(*v);rejects('hide_creation_as_remark',changed)
changed=list(conditions);v=fields(changed[first_zero]);v[2]=b'\1';changed[first_zero]=r.oracle.proper(*v);rejects('zero_amount_is_not_one',changed)
changed=list(conditions);v=fields(changed[first_zero]);v[2]=b'\x00';changed[first_zero]=r.oracle.proper(*v);rejects('padded_zero_outside_observed_canonical_profile',changed)
changed=list(conditions);v=fields(changed[first_zero]);v[2]=b'\xff';changed[first_zero]=r.oracle.proper(*v);rejects('negative_amount_outside_observed_profile',changed)
changed=list(conditions);v=fields(changed[first_zero]);v[1]=b'\x01';changed[first_zero]=r.oracle.proper(*v);rejects('invalid_created_hash_width',changed)
total=sum(x['create_coin_conditions']for x in rows);zero=sum(x['zero_mojo']for x in rows);one=sum(x['one_mojo']for x in rows);require(total==11637 and zero==11508 and one==129 and zero==3*3836,'aggregate exact count');require(total*rate==20946600000 and zero*rate==20714400000 and zero*rate>ceiling,'strict lower bound exceeds ceiling')
result={'status':'executed_pure_count_and_locked_source_identity_checks_passed','scope':'No VM, SDK, Rust tests, compiler, RPC or block/bundle reconstruction. Count is from retained actual component outputs; condition pricing applicability is manually inspected pinned consensus source, not an executed consensus-parser proof.','attempt4_receipt_sha256':sha(read(BASE/'execution-receipt.json')),'crate':{'name':pkg['name'],'version':pkg['version'],'archive_bytes':len(ar),'archive_sha256':sha(ar),'matches_cargo_lock':True,'installed_selected_sources_match_archive':True,'vcs_metadata':vcs,'git_metadata_is_not_source_deployment_authentication':True},'sources':source_rows,'counts':{'components':129,'registered_names':3836,'CREATE_COIN_total':total,'zero_mojo_CREATE_COIN':zero,'one_mojo_controller_CREATE_COIN':one,'unique_created_coin_ids':len(coin_ids),'per_component_create_count_histogram':dict(collections.Counter(x['create_coin_conditions']for x in rows))},'cost':{'CREATE_COIN_unit_cost':rate,'maximum_block_cost':ceiling,'all_observed_creation_condition_charge':total*rate,'zero_mojo_creation_condition_charge_only':zero*rate,'zero_only_excess_over_block_limit':zero*rate-ceiling,'consolidated_one_controller_plus_same_zero_creates':(zero+1)*rate,'ignored_nonnegative_costs':['primary CLVM execution','other condition charges','generator structure/serialized bytes','BLS','omitted companion spends'],'claim':'Under chia-consensus0.36.1 fixed condition pricing, preserving these11508 zero-mojo slot CREATE_COIN occurrences alone requires20714400000 cost, exceeding11000000000; no full-bundle fee/cost was measured.'},'pure_falsification_controls':{'count':len(controls),'rejected':controls,'not_consensus_executions':True},'rows':rows}
(OUT/'receipt.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items()if k not in ('rows','sources')},indent=2))
