"""Evaluate the exact native argument-dispatch sequence against authored C#.
Only a bounded instruction slice is emulated. The service updater is replaced
with a recorded callback (return or stop), not claimed as native-equivalent.
No game process, engine, OS target code, network service, or account is loaded.
"""
from pathlib import Path
import sys,struct,json,hashlib,random,collections,subprocess,time,datetime
ROOT=Path(__file__).resolve().parent;WORK=ROOT.parent
sys.path.insert(0,str(WORK/'verification-pass-20261021/vendor'))
import unicorn
from unicorn import Uc,UC_ARCH_X86,UC_MODE_64,UC_HOOK_CODE,UC_PROT_READ,UC_PROT_WRITE,UC_PROT_EXEC
from unicorn.x86_const import *
SOURCE=WORK/'runs/recovery-/20261020T212503.738773Z/decoded/GameAssembly.code-restored.analysis.dll'
EXPECTED='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
BASE=0x180000000;START=0x9495e9d;END=0x9496173;HELPER=BASE+0x94871b0
DATA=0x50000000;RESPONSE=DATA+0x1000;KLASS=DATA+0x2000;STATE=DATA+0x3000;STACK=0x60000000;SP=STACK+0x8000
GLOBAL=0x18fd64e18
binary=SOURCE.read_bytes();assert hashlib.sha256(binary).hexdigest()==EXPECTED
pe=struct.unpack_from('<I',binary,0x3c)[0];opt=pe+24;st=opt+struct.unpack_from('<H',binary,pe+20)[0]
sections=[struct.unpack_from('<IIII',binary,st+i*40+8) for i in range(struct.unpack_from('<H',binary,pe+6)[0])]
def off(rva,n):
 for vs,va,rawsize,rawoff in sections:
  if va<=rva and rva+n<=va+rawsize:return rawoff+rva-va
 raise ValueError('Unbacked source range')
bindings=json.loads((WORK/'deep-rea-20261021/findings/name-server-bindings.json').read_text())['rows']
assert len(bindings)==31
uc=Uc(UC_ARCH_X86,UC_MODE_64)
for page in range(START&~4095,(END+4095)&~4095,4096):
 uc.mem_map(BASE+page,4096,UC_PROT_READ|UC_PROT_WRITE|UC_PROT_EXEC);uc.mem_write(BASE+page,binary[off(page,4096):off(page,4096)+4096]);uc.mem_protect(BASE+page,4096,UC_PROT_READ|UC_PROT_EXEC)
uc.mem_map(HELPER&~4095,4096,UC_PROT_READ|UC_PROT_EXEC)
uc.mem_map(GLOBAL&~4095,4096,UC_PROT_READ|UC_PROT_WRITE)
uc.mem_map(DATA,0x5000,UC_PROT_READ|UC_PROT_WRITE);uc.mem_map(STACK,0x10000,UC_PROT_READ|UC_PROT_WRITE)
uc.mem_write(GLOBAL,struct.pack('<Q',KLASS));uc.mem_write(KLASS+0xec,struct.pack('<I',1))
uc.reg_write(UC_X86_REG_EFLAGS,2);saved=uc.context_save();state={};stats=collections.Counter()
volatile=[UC_X86_REG_RAX,UC_X86_REG_RCX,UC_X86_REG_RDX,UC_X86_REG_R8,UC_X86_REG_R9,UC_X86_REG_R10,UC_X86_REG_R11]
def hook(machine,address,size,unused):
 state['instructions']+=1
 if address==HELPER:
  service=machine.reg_read(UC_X86_REG_ECX);pointer=machine.reg_read(UC_X86_REG_RDX)
  assert machine.reg_read(UC_X86_REG_R8)==STATE+0x30
  assert pointer==0 or pointer in state['strings']
  state['trace'].append({'serviceId':service,'value':state['strings'].get(pointer)})
  if len(state['trace'])-1==state['failAt']:
   state['status']='callback_stopped';machine.emu_stop();return
  rsp=machine.reg_read(UC_X86_REG_RSP);ret=struct.unpack('<Q',bytes(machine.mem_read(rsp,8)))[0]
  assert BASE+START<=ret<=BASE+END
  for reg in volatile:machine.reg_write(reg,0xdeadbeef)
  machine.reg_write(UC_X86_REG_RSP,rsp+8);machine.reg_write(UC_X86_REG_RIP,ret);machine.reg_write(UC_X86_REG_EFLAGS,2)
  return
 assert BASE+START<=address and address+size<=BASE+END,hex(address)
uc.hook_add(UC_HOOK_CODE,hook)
def emulate(case):
 uc.context_restore(saved);uc.mem_write(STACK+0x7f00,b'\0'*0x200)
 uc.reg_write(UC_X86_REG_RSP,SP);uc.reg_write(UC_X86_REG_RDI,RESPONSE);uc.reg_write(UC_X86_REG_RBX,STATE)
 pointers={};raw=bytearray(512)
 for row in bindings:
  ident=row['serviceId'];value=case['values'][ident];p=0 if value is None else 0x70000000+0x100*ident
  if p:pointers[p]=value
  struct.pack_into('<Q',raw,row['responseFieldOffset'],p)
 uc.mem_write(RESPONSE,bytes(raw));state.clear();state.update(strings=pointers,trace=[],failAt=case['failAt'],status='running',instructions=0)
 uc.emu_start(BASE+START,BASE+END,count=1000)
 if state['status']=='running':
  assert uc.reg_read(UC_X86_REG_RIP)==BASE+END and uc.reg_read(UC_X86_REG_RSP)==SP
  state['status']='completed'
 assert bytes(uc.mem_read(RESPONSE,512))==raw,'Dispatcher modified response fields'
 assert state['status'] in ('completed','callback_stopped')
 stats['instructions']+=state['instructions'];stats['callbackInvocations']+=len(state['trace'])
 return {'id':case['id'],'status':state['status'],'trace':state['trace']}
rng=random.Random(23191908);cases=[]
for dataset in range(64):
 values=[None if rng.random()<0.2 else f'fixture-{dataset}-service-{i}-tag-{rng.randrange(1000000)}' for i in range(31)]
 if dataset==0:values=[None]*31
 if dataset==1:values=['same-value']*31
 for failAt in range(-1,31):cases.append({'id':len(cases),'values':values,'failAt':failAt})
OUT=ROOT/'binding-differential';OUT.mkdir(exist_ok=False)
with (OUT/'cases.jsonl').open('x',encoding='utf-8') as f:
 for c in cases:f.write(json.dumps(c)+'\n')
start=time.monotonic();results=[emulate(c) for c in cases]
with (OUT/'native-emulated.jsonl').open('x',encoding='utf-8') as f:
 for r in results:f.write(json.dumps(r)+'\n')
build=json.loads((ROOT/'binding-probe-build.json').read_text())
comparison={}
for filename,mutant in [('managed.jsonl',False),('sorted-mutant.jsonl',True)]:
 cmd=[build['dotnet'],build['program'],str(OUT/'cases.jsonl'),str(OUT/filename)]
 if mutant:cmd.append('sorted-mutant')
 p=subprocess.run(cmd,capture_output=True,text=True,timeout=60);(OUT/(filename+'.log')).write_text(p.stdout+p.stderr,encoding='utf-8');assert p.returncode==0,p.stderr
 got=[json.loads(s) for s in (OUT/filename).read_text().splitlines()];assert len(got)==len(results)
 comparison[filename]=[{'id':c['id'],'failAt':c['failAt'],'native':native,'managed':managed} for c,native,managed in zip(cases,results,got) if native!=managed]
assert not comparison['managed.jsonl'],comparison['managed.jsonl'][:1]
assert comparison['sorted-mutant.jsonl'],'Sorted-order negative control escaped detection'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
summary={'status':'native_dispatch_sequence_differential_passed','recordedUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'cases':len(cases),'mismatches':0,'completedCases':sum(x['status']=='completed' for x in results),'stoppedCallbackCases':sum(x['status']=='callback_stopped' for x in results),'sortedOrderMutantRejectedCases':len(comparison['sorted-mutant.jsonl']),'guestInstructions':stats['instructions'],'callbackInvocations':stats['callbackInvocations'],'sourceMethodIndex':148260,'helperMethodIndex':148123,'sourceImageSha256':EXPECTED,'sliceStartRva':hex(START),'sliceEndExclusiveRva':hex(END),'sourceInstructionBytes':END-START,'sliceSha256':hashlib.sha256(binary[off(START,END-START):off(START,END-START)+END-START]).hexdigest(),'reaEvidenceId':'ev_3240650d079e5e63f4ca35ac67282c5e6290b2f5167c7c69bbbb476c5ec36973','modelSha256':build['modelSha256'],'artifacts':[{'file':name,'sha256':sha(OUT/name)} for name in ('cases.jsonl','native-emulated.jsonl','managed.jsonl','sorted-mutant.jsonl')],'seconds':time.monotonic()-start,'originalGameLaunched':False,'exactSourceInstructionsEmulated':True,'fullMethodEquivalent':False,'limitations':['Starts at the observed dispatch slice after earlier async/status checks, with synthetic initialized runtime state.','The service-update helper is a recording callback that returns or stops; its mutation and exception internals are not emulated here.','No proof of original JSON wire schema, account authentication, complete async/error paths, or a functioning backend.','This verifies argument values and ordering under modeled callback outcomes, not game performance or platform portability.']}
(OUT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({k:summary[k] for k in ('status','cases','mismatches','completedCases','stoppedCallbackCases','sortedOrderMutantRejectedCases','guestInstructions','callbackInvocations','sourceInstructionBytes')}),flush=True)
