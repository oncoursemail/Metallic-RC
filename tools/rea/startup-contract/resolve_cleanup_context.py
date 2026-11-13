"""Resolve the omitted initialization cleanup call without inventing EH semantics."""
from pathlib import Path
import struct,json,hashlib,sqlite3
R=Path(__file__).resolve().parent;W=R.parent;D=W/'runs/recovery-/20261020T212503.738773Z/decoded';B=0x180000000
reg=json.loads((W/'csharp-reconstruction/verified-native/native-registration-validation.json').read_text());b=(D/'GameAssembly.code-restored.analysis.dll').read_bytes();assert hashlib.sha256(b).hexdigest()==reg['sourceSha256'];ss=json.loads((D.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections'];regs=reg['originalMetadataRegistration']
def off(a,n):
 for s in ss:
  if B+s['rva']<=a and a+n<=B+s['rva']+s['rawSize']:return s['rawOffset']+a-B-s['rva']
 raise ValueError('Unbacked source')
def rd(a,fmt):return struct.unpack_from(fmt,b,off(a,struct.calcsize(fmt)))
def ptrs(k):d=regs[k];return rd(int(d['pointer'],16),'<'+str(d['count'])+'Q')
pt=ptrs('types');by_ptr={p:i for i,p in enumerate(pt)};inst=ptrs('genericInsts');types={json.loads(l)['nativeTypeIndex']:json.loads(l)['name']for l in (W/'csharp-reconstruction/typed-reference-v3/native-types.jsonl').open()}
def args(i):
 if i==-1:return[]
 count,argv=rd(inst[i],'<QQ');assert 0<count<129
 return[types[by_ptr[p]]for p in rd(argv,'<'+str(count)+'Q')]
db=sqlite3.connect((W/'deep-rea-20261021/Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);db.row_factory=sqlite3.Row
address=0x18fcfca98;v=rd(address,'<Q')[0];tag=v>>29;idx=(v&0x1ffffffe)>>1;assert tag==6
mid,ci,gi=rd(int(regs['methodSpecs']['pointer'],16)+idx*12,'<3i');method=json.loads(db.execute('select raw_json from methods where id=?',(mid,)).fetchone()[0])
rng=db.execute('select * from native_ranges where start_rva=?',(0x9483060,)).fetchone();u=B+rng['unwind_rva'];head=rd(u,'<4B');count=head[2];flags=head[0]>>3;version=head[0]&7;tail=u+4+((count+1)&~1)*2
unwind={'rva':hex(rng['unwind_rva']),'version':version,'flags':flags,'prologBytes':head[1],'codeSlots':count,'frameRegister':head[3]&15,'frameOffset':(head[3]>>4)*16,'codeBytes':b[off(u+4,count*2):off(u+4,count*2)+count*2].hex()}
if flags&4:unwind['chainedRuntimeFunction']=[hex(x)for x in rd(tail,'<3I')]
elif flags&3:
 unwind['handlerRva']=hex(rd(tail,'<I')[0]);unwind['languageSpecificFirst32Bytes']=b[off(tail+4,32):off(tail+4,32)+32].hex();unwind['handlerDataDecoded']=False
no_op=b[off(0x180d0a4c0,16):off(0x180d0a4c0,16)+16]
assert no_op.startswith(bytes.fromhex('c20000'))  # RET imm16=0, not the shorter C3 encoding
no_op=no_op[:3]
summary={'status':'cleanup_generic_context_and_leaf_effect_resolved','sourceImageSha256':reg['sourceSha256'],'slotAddress':hex(address),'taggedValue':hex(v),'methodSpecIndex':idx,'methodDefinition':method,'classArguments':args(ci),'methodArguments':args(gi),'observedCallTarget':'0x180d0a4c0','observedLeafInstruction':'RET 0','observedLeafBytes':no_op.hex(),'observedLeafSha256':hashlib.sha256(no_op).hexdigest(),'unwind':unwind,'nativePropagationTarget':'0x180c3c3b0','propagationImplementationRecovered':False,'interpretation':'The omitted block calls the typed enumerator cleanup context at a shared immediate-RET body, then conditionally forwards the saved exception pointer to a native propagation boundary. Full exception dispatch/unwind behavior is not inferred from this.'}
(R/'cleanup-context.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(summary),flush=True)
