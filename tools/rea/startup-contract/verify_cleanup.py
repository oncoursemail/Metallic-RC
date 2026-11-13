"""Validate the omitted initialization block through the native propagation edge.
The original cleanup leaf is emulated; the native propagation routine is an
explicit stop boundary. This does not emulate Windows EH dispatch/unwinding.
"""
from pathlib import Path
import sys,json,hashlib,struct,random,subprocess,collections,datetime
R=Path(__file__).resolve().parent;W=R.parent;D=W/'runs/recovery-/20261020T212503.738773Z/decoded'
sys.path.insert(0,str(W/'verification-pass-20261021/vendor'))
from unicorn import Uc,UC_ARCH_X86,UC_MODE_64,UC_HOOK_CODE
from unicorn.x86_const import *
B=0x180000000;STACK=0x70000000;SP=STACK+0x8000;STOP=0x50000000;ENTRY=B+0x94837d4;LEAF=B+0xd0a4c0;FORWARD=B+0xc3c3b0;SLOT=0x18fcfca98
image=(D/'GameAssembly.code-restored.analysis.dll').read_bytes();sha=hashlib.sha256(image).hexdigest();assert sha=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
ss=json.loads((D.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections']
def raw(rva,n):
 for s in ss:
  if s['rva']<=rva and rva+n<=s['rva']+s['rawSize']:return image[s['rawOffset']+rva-s['rva']:s['rawOffset']+rva-s['rva']+n]
 raise ValueError('Source range absent')
uc=Uc(UC_ARCH_X86,UC_MODE_64)
for address in sorted({ENTRY&~4095,LEAF&~4095,FORWARD&~4095,SLOT&~4095}):uc.mem_map(address,4096);uc.mem_write(address,raw(address-B,4096))
uc.mem_map(STACK,0x10000);uc.mem_map(STOP,4096);uc.reg_write(UC_X86_REG_EFLAGS,2);base=uc.context_save();state={};steps=0
meta=struct.unpack('<Q',raw(SLOT-B,8))[0];assert meta==0xc02564af and raw(LEAF-B,3)==bytes.fromhex('c20000')
def hook(machine,address,size,unused):
 global steps
 steps+=1
 if address==LEAF:state['events'].append({'operation':'dispose','target':machine.reg_read(UC_X86_REG_RCX),'metadata':machine.reg_read(UC_X86_REG_RDX)})
 if address==FORWARD:
  state['events'].append({'operation':'propagate','target':machine.reg_read(UC_X86_REG_RCX),'metadata':0});state['status']='propagate';machine.emu_stop();return
 assert ENTRY<=address and address+size<=B+0x9483812 or LEAF<=address and address+size<=LEAF+3,hex(address)
uc.hook_add(UC_HOOK_CODE,hook)
rng=random.Random(23191908);cases=[];native=[]
for i in range(2048):
 enum=0 if i%17==0 else rng.randrange(0x60000000,0x61000000)&~7
 error=0 if i%2==0 else rng.randrange(0x62000000,0x63000000)&~7
 cases.append({'id':i,'enumerator':enum,'metadata':meta,'error':error})
 uc.context_restore(base);uc.mem_write(SP-64,b'\0'*512)
 for offset,value in [(0x58,error),(0x60,enum),(0xa0,0x2233445566778899),(0xa8,STOP),(0xb0,0x1122334455667788)]:uc.mem_write(SP+offset,struct.pack('<Q',value))
 uc.reg_write(UC_X86_REG_RSP,SP);state.clear();state.update(status='return',events=[])
 uc.emu_start(ENTRY,STOP,count=80)
 if not error:
  assert uc.reg_read(UC_X86_REG_RIP)==STOP and uc.reg_read(UC_X86_REG_RSP)==SP+0xb0
  assert uc.reg_read(UC_X86_REG_RDI)==0x2233445566778899 and uc.reg_read(UC_X86_REG_RBX)==0x1122334455667788
 assert state['events'][0]=={'operation':'dispose','target':enum,'metadata':meta}
 assert len(state['events'])==(2 if error else 1)
 native.append({'id':i,**state})
O=R/'cleanup-differential';O.mkdir(exist_ok=False)
for name,rows in [('cases.jsonl',cases),('native-results.jsonl',native)]:
 with(O/name).open('x',encoding='utf-8')as f:
  for row in rows:f.write(json.dumps(row)+'\n')
build=json.loads((R/'contract-build.json').read_text());negative={}
for mutant in [None,'skip-cleanup','drop-error']:
 out=O/('managed'+(''if mutant is None else '-'+mutant)+'.jsonl');cmd=[build['dotnet'],build['program'],'--cleanup-probe',str(O/'cases.jsonl'),str(out)]
 if mutant:cmd.append(mutant)
 p=subprocess.run(cmd,capture_output=True,text=True,timeout=60);assert p.returncode==0,p.stdout+p.stderr
 rows=[json.loads(x)for x in out.read_text().splitlines()];assert len(rows)==len(native)
 mismatch=sum(a!=b for a,b in zip(rows,native))
 if mutant:assert mismatch>0;negative[mutant]=mismatch
 else:assert mismatch==0
summary={'status':'omitted_cleanup_block_differential_passed','cases':len(cases),'mismatches':0,'returnCases':1024,'propagationBoundaryCases':1024,'guestInstructions':steps,'exactCleanupLeafBytes':'c20000','cleanupType':'System.Collections.Generic.List<System.Action>.Enumerator.Dispose','cleanupBlockRva':'0x94837d4','propagationStopRva':'0xc3c3b0','negativeControls':negative,'sourceImageSha256':sha,'sourceBlockBytes':0x9483812-0x94837d4,'sourceBlockSha256':hashlib.sha256(raw(ENTRY-B,0x9483812-0x94837d4)).hexdigest(),'nativeErrorDispatcherExecuted':False,'nativeUnwindSemanticsEstablished':False,'gameExecuted':False,'createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'limits':['Starts at the observed cleanup entry with synthetic saved stack state; does not prove OS dispatch into that entry.','The actual three-byte Dispose leaf and cleanup branch run in the CPU emulator; the propagation implementation is intentionally not executed.','Exact saved exception pointer, cleanup-before-forward ordering and normal epilogue are checked, not full original exception class or asynchronous state-machine equivalence.']}
(O/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(summary),flush=True)
