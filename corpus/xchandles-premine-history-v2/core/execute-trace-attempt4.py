"""Target-only bounded historical component replay; NEVER builds or submits."""
import collections,csv,datetime,hashlib,json,os,pathlib,selectors,signal,subprocess,sys,time,urllib.request
sys.dont_write_bytecode=True
ROOT=pathlib.Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'scripts'))
from registry_codegen_evidence import inspect_compiled_artifact
from source_walk import source_files
BASE=pathlib.Path(__file__).resolve().parent;OUT=BASE/'attempt4';OUT.mkdir(exist_ok=False)
for name in ['programs','steps','http']: (OUT/name).mkdir()
CLI=ROOT/'target/debug/chialisp-oxide'
MAX_TOTAL=128*1024*1024;MAX_COST=11_000_000_000;MAX_STEPS=256
REGISTER='150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8'
SINGLETON='7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f'
ACTION_LAYER='afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b'
SEED='6f314f69a9cf776311e1c7781a3013e4daae3696c71d8a04d049a5af1530f050'
UP=ROOT/'corpus/upstream/slot-machine/6edc94856306d0f77e065c6c99aacf1055e51e2f'
CSV=UP/'xchandles_premine_mainnet.csv'
ALLOCATION={r['handle']:r for r in csv.DictReader(CSV.open())};assert len(ALLOCATION)==3836
remaining=MAX_TOTAL-sum(p.stat().st_size for p in BASE.rglob('*') if p.is_file());assert remaining>0
requests=[];steps=[];seen=set();start_all=time.monotonic();sources_before=None;prefix_ok=False

def charged_write(path,b):
 global remaining
 assert len(b)<=remaining,'retained aggregate budget';remaining-=len(b)
 with path.open('xb') as f:f.write(b)
def write_json(path,v):
 b=json.dumps(v,indent=2).encode()+b'\n';charged_write(path,b)
def file_id(p,maximum):
 assert p.is_file() and not p.is_symlink() and p.stat().st_size<=maximum
 h=hashlib.sha256();n=0
 with p.open('rb') as f:
  while b:=f.read(65536):n+=len(b);assert n<=maximum;h.update(b)
 return {'path':str(p),'bytes':n,'sha256':h.hexdigest()}
def freeze():
 paths=source_files(ROOT,['crates','experiments/backend','vendor/rue-lir'],suffixes=('.rs','.toml'))
 paths+=['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo/config.toml','scripts/registry_codegen_evidence.py','scripts/source_walk.py']
 paths+=[str(pathlib.Path(__file__).resolve().relative_to(ROOT)),str(CSV.relative_to(ROOT))]
 assert len(paths)<4096
 files=[file_id(ROOT/p,16*1024*1024) for p in sorted(set(paths))];assert sum(p['bytes'] for p in files)<64*1024*1024
 return {'cli':file_id(CLI,512*1024*1024),'sources':files,'scope':'Selected local execution-source and executable byte consistency, not installed-byte authentication or complete environment provenance.'}
def check_freeze():assert freeze()==sources_before,'source/CLI changed during execution lane'
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args):return None
opener=urllib.request.build_opener(NoRedirect)
def rpc(endpoint,body,maximum=65536):
 global remaining
 assert endpoint in {'get_network_info','get_coin_record_by_name','get_puzzle_and_solution'} and len(requests)<2*MAX_STEPS+4
 assert maximum<=1024*1024 and remaining>=maximum
 request=json.dumps(body).encode();assert len(request)<=4096
 start=time.monotonic()
 with opener.open(urllib.request.Request('https://api.coinset.org/'+endpoint,data=request,headers={'Content-Type':'application/json','User-Agent':'chialisp-oxide-read-only-component-research'}),timeout=15) as r:
  chunks=[];size=0
  while chunk:=r.read(8192):
   assert size+len(chunk)<=maximum and time.monotonic()-start<=25,'HTTP resource bound';chunks.append(chunk);size+=len(chunk)
  status=r.status
 raw=b''.join(chunks);path=OUT/'http'/f'{len(requests):03}-{endpoint}.json';charged_write(path,raw)
 value=json.loads(raw);requests.append({'url':'https://api.coinset.org/'+endpoint,'request':body,'path':str(path.relative_to(BASE)),'http_status':status,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat()})
 assert status==200 and value.get('success') is True and not value.get('error'),value
 return value

def uh(s):return bytes.fromhex(s.removeprefix('0x'))
def coin_id(coin):
 amount=coin['amount'];assert type(amount)is int and 0<=amount<1<<64
 atom=b'' if not amount else amount.to_bytes((amount.bit_length()+8)//8,'big')
 return hashlib.sha256(uh(coin['parent_coin_info'])+uh(coin['puzzle_hash'])+atom).hexdigest()
def inspect(raw):
 value=inspect_compiled_artifact(raw,max_bytes=1024*1024,max_nodes=65536,max_depth=512)
 stats=value['stats'];assert stats['nodes']*192+len(raw)+stats['atom_payload_bytes']<=16*1024*1024,'decoded work profile'
 return value

def decode(raw):
 inspect(raw);cursor=0;stack=[]
 while True:
  head=raw[cursor];cursor+=1
  if head==255:stack.append(None);continue
  if head<128:value=bytes([head])
  elif head==128:value=b''
  else:
   width=0
   while head&(128>>width):width+=1
   length=head&((1<<(7-width))-1)
   for byte in raw[cursor:cursor+width-1]:length=length*256+byte
   cursor+=width-1;value=raw[cursor:cursor+length];cursor+=length
  while stack and stack[-1] is not None:value=(stack.pop(),value)
  if stack:stack[-1]=value
  else:return value

def take(node,n):
 out=[]
 for _ in range(n):assert type(node)is tuple;out.append(node[0]);node=node[1]
 return out,node

def fields(node,maximum=65536):
 out=[]
 while type(node)is tuple:out.append(node[0]);node=node[1];assert len(out)<=maximum
 assert node==b'';return out

def th(node):
 pending=[(node,False)];hashes=[]
 while pending:
  v,done=pending.pop()
  if type(v)is bytes:hashes.append(hashlib.sha256(b'\1'+v).digest())
  elif done:
   r=hashes.pop();l=hashes.pop();hashes.append(hashlib.sha256(b'\2'+l+r).digest())
  else:pending.extend([(v,True),(v[1],False),(v[0],False)])
 assert len(hashes)==1;return hashes[0].hex()
def uncurry(node):
 v=fields(node,3);assert len(v)==3 and v[0]==b'\2' and v[1][0]==b'\1';module=v[1][1];args=[];env=v[2]
 while env!=b'\1':
  v=fields(env,3);assert len(v)==3 and v[0]==b'\4' and v[1][0]==b'\1';args.append(v[1][1]);env=v[2];assert len(args)<=16
 return module,args

def assembly(raw):
 stats=inspect(raw)['stats'];bound=stats['pairs']*5+stats['atom_payload_bytes']*2+stats['atoms']*2
 assert bound<=240000,'assembly/argv bound before conversion'
 tree=decode(raw);parts=[];pending=[tree];size=0
 while pending:
  v=pending.pop()
  if type(v)is str:part=v
  elif type(v)is tuple:pending.extend([')',v[1],' . ',v[0],'(']);continue
  else:part='()' if v==b'' else '0x'+v.hex()
  size+=len(part);assert size<=bound;parts.append(part)
 return ''.join(parts)

def execute(index,program,solution):
 check_freeze()
 program_info=inspect(program);inspect(solution)
 program_path=OUT/'programs'/(program_info['identity']['module_tree_hash']+'.clvm.hex')
 if not program_path.exists():charged_write(program_path,program.hex().encode()+b'\n')
 prefix=OUT/'steps'/f'{index:03}'
 charged_write(prefix.with_suffix('.solution.hex'),solution.hex().encode()+b'\n')
 text=assembly(solution);charged_write(prefix.with_suffix('.solution.asm'),text.encode()+b'\n')
 argv=[str(CLI),'run',str(program_path),'--solution',text,'--max-cost',str(MAX_COST)]
 command={'argv':argv,'cwd':str(ROOT),'max_cost':MAX_COST,'input_environment_hex':solution.hex(),'program':program_info,'scope':'Actual primary component execution only; no SDK, BLS or full bundle/chain admission.'}
 write_json(prefix.with_suffix('.command.json'),command)
 # One bounded child group. No build/driver/compiler command can be selected.
 process=subprocess.Popen(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
 selector=selectors.DefaultSelector();selector.register(process.stdout,selectors.EVENT_READ,'stdout');selector.register(process.stderr,selectors.EVENT_READ,'stderr')
 captures={'stdout':bytearray(),'stderr':bytearray()};start=time.monotonic();error=None
 try:
  while selector.get_map():
   if time.monotonic()-start>60:raise RuntimeError('inconclusive child wall-time limit')
   for key,_ in selector.select(.1):
    chunk=os.read(key.fileobj.fileno(),8192)
    if not chunk:selector.unregister(key.fileobj);continue
    output=captures[key.data]
    if len(output)+len(chunk)>1024*1024:raise RuntimeError('inconclusive child output limit')
    output.extend(chunk)
  process.wait(timeout=1)
 except Exception as e:error=e
 finally:
  try:os.killpg(process.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  process.wait();selector.close()
  for name,b in captures.items():charged_write(prefix.with_suffix('.'+name+'.json' if name=='stdout' else '.stderr.log'),b)
 if error:raise error
 result=json.loads(captures['stdout']);command['exit_code']=process.returncode
 write_json(prefix.with_suffix('.result-meta.json'),{'exit_code':process.returncode,'stdout_sha256':hashlib.sha256(captures['stdout']).hexdigest(),'stderr_sha256':hashlib.sha256(captures['stderr']).hexdigest()})
 assert result['outcome']['status']=='accepted' and result.get('failure') is None and 0<result['cost']<=MAX_COST,{'finite_execution_failed':result,'resource_failures_are_inconclusive':True}
 assert process.returncode==0
 output=uh(result['outcome']['output_hex']);inspect(output)
 check_freeze()
 return result,decode(output)

def analyze(program,solution):
 module,args=uncurry(decode(program));assert th(module)==SINGLETON and len(args)==2
 struct=args[0];assert struct[0].hex()==SINGLETON and struct[1][0].hex()==SEED
 module,action_args=uncurry(args[1]);assert th(module)==ACTION_LAYER and len(action_args)==3
 sol=fields(decode(solution),3);assert len(sol)==3
 raw,finalizer=take(sol[2],3);puzzles,selectors,solutions=map(fields,raw);assert len(selectors)==len(solutions)
 assert 1<=len(selectors)<=128
 hashes=[]
 for puzzle in puzzles:
  m,_=uncurry(puzzle);hashes.append(th(m))
 assert hashes and all(h==REGISTER for h in hashes),'unknown action module; stop without reinterpretation'
 handles=[]
 for s in solutions:
  v,_=take(s,4);pricing,_=take(v[3][1],3);handle=pricing[2]
  assert type(handle)is bytes and hashlib.sha256(b'\1'+handle).digest()==v[0]
  handle=handle.decode('ascii');assert handle in ALLOCATION,'registered name outside pinned allocation CSV'
  handles.append(handle)
 assert len(set(handles))==len(handles),'duplicate names in one step'
 return {'handles':handles,'action_count':len(selectors),'state_tree_hash':th(action_args[2]),'allowed_action_merkle_root':action_args[1].hex(),'action_module_hashes':hashes}

status='incomplete';error=None
try:
 sources_before=freeze();write_json(OUT/'source-cli-before.json',sources_before)
 network=rpc('get_network_info',{});assert network['network_name']=='mainnet' and network['network_prefix']=='xch'
 prefix=json.load((BASE/'attempt2/trace-receipt.json').open());prefix_rows=prefix['chain'][2:8];assert len(prefix_rows)==6
 current=prefix_rows[0]['coin_id'];expected_coin=None
 for index in range(MAX_STEPS):
  if index<6:
   known=prefix_rows[index];assert current==known['coin_id'];record=known['record']
   r=next(r for r in prefix['requests'] if r['request'].get('coin_id')=='0x'+current)
   spend=json.load((BASE/'attempt2'/r['path']).open())['coin_solution'];http_source=str((BASE/'attempt2'/r['path']).relative_to(BASE))
  else:
   record=rpc('get_coin_record_by_name',{'name':'0x'+current})['coin_record'];assert record and record['spent'],'history ended before CSV coverage'
   spend=rpc('get_puzzle_and_solution',{'coin_id':'0x'+current,'height':record['spent_block_index']},1024*1024)['coin_solution'];http_source=requests[-1]['path']
  assert coin_id(record['coin'])==current and spend['coin']==record['coin']
  if expected_coin is not None:assert record['coin']==expected_coin,'derived successor differs from point record'
  program,solution=uh(spend['puzzle_reveal']),uh(spend['solution']);assert inspect(program)['identity']['module_tree_hash']==record['coin']['puzzle_hash'].removeprefix('0x')
  analysis=analyze(program,solution);assert not seen.intersection(analysis['handles']),'repeated premine registration'
  result,output=execute(index,program,solution)
  creates=[]
  for condition in fields(output):
   c=fields(condition,32)
   if c and c[0]==b'3':
    assert len(c)>=3 and type(c[1])is bytes and len(c[1])==32 and type(c[2])is bytes
    if c[2]==b'\1':creates.append(c[1])
  assert len(creates)==1,'execution did not produce one exact amount1 successor'
  expected_coin={'parent_coin_info':'0x'+current,'puzzle_hash':'0x'+creates[0].hex(),'amount':1}
  successor=coin_id(expected_coin)
  seen.update(analysis['handles']);steps.append({'index':index,'coin_id':current,'record':record,'source_spend_response':http_source,'program_identity':inspect(program)['identity'],'solution_identity':inspect(solution)['identity'],**analysis,'primary_cost':result['cost'],'next_coin':expected_coin,'next_coin_id':successor,'next_source':'derived from actual CREATE_COIN output, not parent-list completeness'})
  print('EXECUTED',index,'height',record['spent_block_index'],'actions',analysis['action_count'],'cost',result['cost'],'covered',len(seen),'of',len(ALLOCATION),flush=True)
  if index==5:
   prefix_ok=True;write_json(OUT/'six-prefix-states.json',{'status':'six_primary_components_executed','steps':steps,'scope':'Finite primary VM component execution and exact derived successor checks only; no bundle/SDK or authenticated chain acceptance.'})
  current=successor
  if seen==set(ALLOCATION):status='all_pinned_csv_names_seen_in_executed_components';break
 else:raise RuntimeError('bounded256-step history budget exhausted')
except Exception as exc:error=f'{type(exc).__name__}: {str(exc)[:8192]}';print('STOP',error,flush=True)
finally:
 after=freeze();write_json(OUT/'source-cli-after.json',after)
 unchanged=after==sources_before
 if not unchanged:status='inconclusive_source_or_cli_changed'
 write_json(OUT/'execution-receipt.json',{'protocol':'chialisp-oxide-xchandles15-primary-prefix/v1','status':status,'six_prefix_states_executed':prefix_ok,'error':error,'source_cli_unchanged':unchanged,'max_steps':MAX_STEPS,'retained_maximum':MAX_TOTAL,'remaining_retained_bytes':remaining,'max_primary_cost':MAX_COST,'http_requests':requests,'steps':steps,'covered_names':sorted(seen),'pinned_csv_names':len(ALLOCATION),'missing_names':sorted(set(ALLOCATION)-seen),'elapsed_seconds':time.monotonic()-start_all,'scope':'Actual retained primary clvmr0.21 MEMPOOL_MODE component runs only, exact coin-ID and CSV-name consistency. No full bundle/signature/SDK/block replay, per-step fee/consensus cost, chain authentication, source/deployment authentication, autonomous next-work commitment, liveness or compiler preservation.'})
 print('STATUS',status,'steps',len(steps),'covered',len(seen),'source_cli_unchanged',unchanged,flush=True)
