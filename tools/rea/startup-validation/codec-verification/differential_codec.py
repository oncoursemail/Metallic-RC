"""Compare exact captured x64 codec instructions with the C# reconstruction.
The game is never loaded by Windows. Unicorn evaluates only four allowlisted
instruction spans in synthetic guest memory. Exception-helper entries stop
emulation; their implementation/exception class is not emulated or claimed.
"""
from pathlib import Path
import sys,json,hashlib,struct,random,collections,subprocess,datetime,time
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'vendor'))
from unicorn import Uc,UcError,UC_ARCH_X86,UC_MODE_64,UC_HOOK_CODE,UC_HOOK_MEM_WRITE,UC_PROT_READ,UC_PROT_WRITE,UC_PROT_EXEC
from unicorn.x86_const import *
SOURCE=ROOT.parent/'runs/recovery-/20261020T212503.738773Z/decoded/GameAssembly.code-restored.analysis.dll'
EXPECTED='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
BASE=0x180000000
METHODS={387023:(0x2b91ef0,109),387055:(0x2b92120,121),380446:(0x9af2590,23),380444:(0x1ccbc30,3)}
FAILURES={BASE+0xc40390:'bounds',BASE+0xc403a0:'null'}
SENTINEL=0x50000000;DATA=0x60000000;SPAN=DATA+0x1000;STACK=0x70000000;SP=STACK+0x8000
RBX_VALUE=0x123456789abcdef0;RDI_VALUE=0xfedcba9876543210

def sha(b):return hashlib.sha256(b).hexdigest()
binary=SOURCE.read_bytes();assert sha(binary)==EXPECTED
pe=struct.unpack_from('<I',binary,0x3c)[0];opt=pe+24;start=opt+struct.unpack_from('<H',binary,pe+20)[0]
sections=[]
for i in range(struct.unpack_from('<H',binary,pe+6)[0]):
 off=start+i*40;vs,rva,size,raw=struct.unpack_from('<IIII',binary,off+8);sections.append((rva,vs,size,raw))
def rawoff(rva,n):
 for start,vs,size,raw in sections:
  if start<=rva and rva+n<=start+size:return raw+rva-start
 raise ValueError('Unbacked source range')
manifest=[]
for mid,(rva,n) in METHODS.items():
 p=rawoff(rva,n);manifest.append({'methodIndex':mid,'rva':hex(rva),'bytes':n,'fileOffset':p,'sha256':sha(binary[p:p+n])})
assert binary[rawoff(0x1ccbc30,3):rawoff(0x1ccbc30,3)+3]==bytes.fromhex('8bc1c3')
# Both helpers must be copied as real captured code, not replaced with a Python oracle.
uc=Uc(UC_ARCH_X86,UC_MODE_64)
for page in sorted({rva&~4095 for rva,n in METHODS.values()}):
 address=BASE+page;uc.mem_map(address,4096,UC_PROT_READ|UC_PROT_WRITE|UC_PROT_EXEC)
 uc.mem_write(address,binary[rawoff(page,4096):rawoff(page,4096)+4096]);uc.mem_protect(address,4096,UC_PROT_READ|UC_PROT_EXEC)
uc.mem_map(BASE+0xc40000,4096,UC_PROT_READ|UC_PROT_EXEC)
uc.mem_map(DATA,0x2000,UC_PROT_READ|UC_PROT_WRITE);uc.mem_map(STACK,0x10000,UC_PROT_READ|UC_PROT_WRITE)
uc.mem_map(SENTINEL,4096,UC_PROT_READ|UC_PROT_EXEC)
state={};observed=collections.Counter();registers=[UC_X86_REG_RAX,UC_X86_REG_RBX,UC_X86_REG_RCX,UC_X86_REG_RDX,UC_X86_REG_RSI,UC_X86_REG_RDI,UC_X86_REG_RBP,UC_X86_REG_RSP,UC_X86_REG_R8,UC_X86_REG_R9,UC_X86_REG_R10,UC_X86_REG_R11,UC_X86_REG_R12,UC_X86_REG_R13,UC_X86_REG_R14,UC_X86_REG_R15]
for reg in registers:uc.reg_write(reg,0)
uc.reg_write(UC_X86_REG_EFLAGS,2);baseline=uc.context_save()
allowed=[(BASE+rva,BASE+rva+n,mid) for mid,(rva,n) in METHODS.items()]
def code_hook(machine,address,size,unused):
 state['instructions']+=1
 if address in FAILURES:state['status']=FAILURES[address];state['failureHelper']=hex(address);machine.emu_stop();return
 matching=[mid for lo,hi,mid in allowed if lo<=address and address+size<=hi]
 if not matching:state['status']='unexpected_code';state['unexpectedAddress']=hex(address);machine.emu_stop();return
 observed[matching[0]]+=1

def write_hook(machine,access,address,size,value,unused):
 if DATA<=address<DATA+0x2000:state['writes'].append({'offset':address-(DATA+0x20),'bytes':size,'value':value})
uc.hook_add(UC_HOOK_CODE,code_hook);uc.hook_add(UC_HOOK_MEM_WRITE,write_hook)

def emulate(case):
 raw=None if case['bytesHex'] is None else bytes.fromhex(case['bytesHex']);length=len(raw or b'')
 assert length<=512
 uc.context_restore(baseline);uc.mem_write(STACK+0x7e00,b'\0'*0x400)
 initial=bytearray(b'\xa5'*0x2000);initial[:32]=b'\0'*32;struct.pack_into('<Q',initial,0x18,length)
 initial[0x20:0x20+length]=raw or b''
 struct.pack_into('<QI',initial,0x1000,DATA+0x20 if raw is not None else 0,length)
 uc.mem_write(DATA,bytes(initial));uc.mem_write(SP,struct.pack('<Q',SENTINEL))
 uc.reg_write(UC_X86_REG_RSP,SP);uc.reg_write(UC_X86_REG_RBX,RBX_VALUE);uc.reg_write(UC_X86_REG_RDI,RDI_VALUE)
 uc.reg_write(UC_X86_REG_RCX,SPAN if case['operation']=='read' else DATA if raw is not None else 0)
 uc.reg_write(UC_X86_REG_RDX,case['index']&0xffffffff);uc.reg_write(UC_X86_REG_R8,case['value']&0xffffffff)
 state.clear();state.update(status='running',instructions=0,writes=[])
 entry=METHODS[387023 if case['operation']=='read' else 387055][0]
 try:uc.emu_start(BASE+entry,SENTINEL,count=500)
 except UcError as e:raise AssertionError(f'Unexpected emulator memory/instruction failure: {case}: {e}')
 if state['status']=='running':
  assert uc.reg_read(UC_X86_REG_RIP)==SENTINEL,'Instruction limit, not a verified return'
  assert uc.reg_read(UC_X86_REG_RSP)==SP+8
  assert uc.reg_read(UC_X86_REG_RBX)==RBX_VALUE and uc.reg_read(UC_X86_REG_RDI)==RDI_VALUE
  state['status']='ok'
 assert state['status'] in ('ok','bounds','null'),state
 result=None
 if case['operation']=='read' and state['status']=='ok':result=struct.unpack('<i',struct.pack('<I',uc.reg_read(UC_X86_REG_EAX)))[0]
 after=bytes(uc.mem_read(DATA,0x2000));assert after[:32]==initial[:32] and after[0x20+length:]==initial[0x20+length:],'Write escaped array payload'
 return {'id':case['id'],'status':state['status'],'value':result,'bytesHex':None if raw is None else after[0x20:0x20+length].hex(),'writes':list(state['writes']),'instructions':state['instructions'],'failureHelper':state.get('failureHelper')}

rng=random.Random(23191908);cases=[]
def add(op,raw,index,value=0):cases.append({'id':len(cases),'operation':op,'bytesHex':None if raw is None else raw.hex(),'index':index,'value':value})
values=[0,1,-1,-2147483648,2147483647,0x01020304,-123456789]
for length in range(21):
 raw=rng.randbytes(length)
 for index in sorted({-2147483648,-2,-1,0,1,length-4,length-3,length-2,length-1,length,length+1,2147483647}):
  add('read',raw,index)
  for value in values:add('write',raw,index,value)
for _ in range(2000):
 length=rng.randrange(0,513);raw=rng.randbytes(length)
 index=rng.randrange(0,length-3) if length>=4 and rng.random()<.7 else rng.choice([-2147483648,-1,length-3,length-2,length-1,length,2147483647])
 value=rng.randrange(-2147483648,2147483648);add('read',raw,index);add('write',raw,index,value)
for index in [-2147483648,-1,0,1,2147483647]:
 add('read',None,index)
 for value in values:add('write',None,index,value)
OUT=ROOT/'differential';OUT.mkdir(exist_ok=False)
inputs=OUT/'cases.jsonl'
with inputs.open('x',encoding='utf-8') as f:
 for row in cases:f.write(json.dumps(row)+'\n')
results=[];start=time.monotonic()
with (OUT/'native-emulated.jsonl').open('x',encoding='utf-8') as f:
 for i,row in enumerate(cases):
  result=emulate(row);results.append(result);f.write(json.dumps(result)+'\n')
  if (i+1)%1000==0:print(json.dumps({'emulated':i+1,'total':len(cases)}),flush=True)
manifest_build=json.loads((ROOT/'managed-probe-build.json').read_text())
for file,mutant in [('managed.jsonl',False),('deliberate-upfront-mutant.jsonl',True)]:
 cmd=[manifest_build['dotnet'],manifest_build['output'],str(inputs),str(OUT/file)]
 if mutant:cmd.append('upfront-mutant')
 p=subprocess.run(cmd,capture_output=True,text=True,timeout=60)
 (OUT/(file+'.log')).write_text(p.stdout+p.stderr,encoding='utf-8');assert p.returncode==0,p.stdout+p.stderr

def comparisons(path):
 rows=[json.loads(x) for x in path.read_text().splitlines()];assert len(rows)==len(results)
 mismatches=[]
 for expected,actual in zip(results,rows):
  if any(expected[k]!=actual[k] for k in ('id','status','value','bytesHex')):mismatches.append({'input':cases[expected['id']],'native':expected,'managed':actual})
 return mismatches
mismatches=comparisons(OUT/'managed.jsonl');mutant=comparisons(OUT/'deliberate-upfront-mutant.jsonl')
(OUT/'mismatches.json').write_text(json.dumps(mismatches,indent=2)+'\n',encoding='utf-8')
assert not mismatches,f'{len(mismatches)} differential mismatches'
assert len(mutant)>0,'Negative control was not detected'
partial=[r for r in results if r['status']=='bounds' and r['writes']]
assert len(partial)==len(mutant)
summary={'status':'exact_native_instruction_differential_passed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'cases':len(cases),'operations':dict(collections.Counter(c['operation'] for c in cases)),'outcomes':dict(collections.Counter(r['status'] for r in results)),'mismatches':0,'partialWriteFailures':len(partial),'wrongUpfrontCheckRewriteRejectedCases':len(mutant),'executedGuestInstructionCount':sum(r['instructions'] for r in results),'executedGuestMethodInstructions':dict(observed),'exactNativeSourceSha256':EXPECTED,'sourceMethodRanges':manifest,'managedCodecSha256':manifest_build['codecSourceSha256'],'inputsSha256':sha(inputs.read_bytes()),'emulatedOutputsSha256':sha((OUT/'native-emulated.jsonl').read_bytes()),'managedOutputsSha256':sha((OUT/'managed.jsonl').read_bytes()),'emulatorVersion':'2.1.4','seconds':time.monotonic()-start,'partialWriteExamples':mutant[:3],'originalGameLaunched':False,'originalNativeBytesExecutedDirectlyByHost':False,'originalInstructionBytesEvaluatedInsideCpuEmulator':True,'originalCodeModified':False,'limitations':['Synthetic single-threaded guest memory uses the observed x64 Windows IL2CPP array/span layout.','Range/null helper entry is observed and stops emulation; the original exception implementation, type and unwind behavior are NOT emulated.','No OS calls, game process, Unity engine, account, remote endpoint, or recovered game assembly is loaded.','Two codec methods and two helper spans only; no full circuit compiler or full-game equivalence claim.']}
(OUT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
assert sha(SOURCE.read_bytes())==EXPECTED
print(json.dumps({k:summary[k] for k in ('status','cases','outcomes','mismatches','partialWriteFailures','wrongUpfrontCheckRewriteRejectedCases','executedGuestInstructionCount','seconds')}),flush=True)
