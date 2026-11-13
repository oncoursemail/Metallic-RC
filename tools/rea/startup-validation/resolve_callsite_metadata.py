"""Resolve source-observed tagged metadata slots to concrete IL2CPP contexts.
Uses the version>=27 tag decoder from the pinned upstream Metadata.cs, and
checks the resulting method specifications against code registration pointers.
"""
from pathlib import Path
import struct,json,hashlib,sqlite3
ROOT=Path(__file__).resolve().parent;WORK=ROOT.parent
RUN=WORK/'runs/recovery-/20261020T212503.738773Z';SOURCE=RUN/'decoded'
reg=json.loads((WORK/'csharp-reconstruction/verified-native/native-registration-validation.json').read_text());b=(SOURCE/'GameAssembly.code-restored.analysis.dll').read_bytes();assert hashlib.sha256(b).hexdigest()==reg['sourceSha256']
c=json.loads((RUN/'capture/capture-report.json').read_text());record=next(a for a in c['artifacts'] if a['kind']=='mapped_binary_evidence');memory=(RUN/'capture'/record['file']).read_bytes();assert hashlib.sha256(memory).hexdigest()==record['sha256']
BASE=0x180000000;sections=c['gameAssemblyDiskLayout']['sections'];regs=reg['originalMetadataRegistration']
def off(p,n):
 for s in sections:
  if BASE+s['rva']<=p and p+n<=BASE+s['rva']+s['rawSize']:return s['rawOffset']+p-BASE-s['rva']
 raise ValueError('Unbacked source')
def rd(p,fmt):return struct.unpack_from(fmt,b,off(p,struct.calcsize(fmt)))
def ptrs(role):d=regs[role];return rd(int(d['pointer'],16),'<'+str(d['count'])+'Q')
ptypes=ptrs('types');by_ptr={p:i for i,p in enumerate(ptypes)};insts=ptrs('genericInsts');types={}
with (WORK/'csharp-reconstruction/typed-reference-v3/native-types.jsonl').open() as f:
 for line in f:
  item=json.loads(line);types[item['nativeTypeIndex']]=item['name']
def inst_args(index):
 if index==-1:return []
 assert 0<=index<len(insts);count,argv=rd(insts[index],'<QQ');assert 0<count<=128
 return [types[by_ptr[p]] for p in rd(argv,'<'+str(count)+'Q')]
slots=[('serialized_response',0x18fdb4040,0x3b0e610),('get_service_uri',0x18fc6d910,0x5873790),('set_service_uri',0x18fc6dc60,0x5873a30),('response_allocation_type',0x18fe32920,None),('preference_interface',0x18fd21c90,None)]
db=sqlite3.connect((WORK/'deep-rea-20261021/Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True)
results=[];wanted={}
for role,address,target in slots:
 value=struct.unpack_from('<Q',memory,address-BASE)[0];disk=rd(address,'<Q')[0]
 assert value==disk and value<2**32 and value&1
 tag=(value&0xe0000000)>>29;index=(value&0x1ffffffe)>>1
 row={'role':role,'slotAddress':hex(address),'capturedTaggedValue':hex(value),'matchesStaticValue':True,'tag':tag,'decodedIndex':index,'expectedNativeTargetRva':hex(target) if target else None}
 if tag==6:
  assert index<regs['methodSpecs']['count'];mid,ci,gi=rd(int(regs['methodSpecs']['pointer'],16)+index*12,'<3i')
  row.update(methodDefinition=json.loads(db.execute('SELECT raw_json FROM methods WHERE id=?',(mid,)).fetchone()[0]),classArguments=inst_args(ci),methodArguments=inst_args(gi));wanted[index]=(row,target)
 elif tag==1:row['nativeTypeName']=types[index]
 else:raise ValueError('Unexpected metadata usage tag')
 results.append(row)
code_array=next(x for x in reg['codeArrays'] if x['role']=='genericMethods');code=rd(int(code_array['pointer'],16),'<'+str(code_array['count'])+'Q')
d=regs['genericMethodTable'];n=d['count']*16;p=off(int(d['pointer'],16),n)
for si,pi,inv,adjust in struct.iter_unpack('<4i',memoryview(b)[p:p+n]):
 if si in wanted:
  row,target=wanted[si];row.setdefault('registeredNativeTargets',[]).append(hex(code[pi]-BASE));assert code[pi]-BASE==target
canonical=json.loads((ROOT/'generic-candidates.private.json').read_text())
for row,target in wanted.values():
 candidates=[x for x in canonical if x['targetRva']==hex(target) and x['methodDefinitionIndex']==row['methodDefinition']['methodIndex']]
 assert candidates,row
 row['registeredNativeTargets']=row.get('registeredNativeTargets',[])
 row['concreteInstantiationDirectlyListed']=bool(row['registeredNativeTargets'])
 row['sharedMethodDefinitionAgrees']=True
 row['canonicalSharedCandidates']=candidates
 row['interpretation']='Concrete tagged arguments resolved; code target agrees at generic definition level. A missing direct specialization row is not fabricated.'
assert 'NameServerResponse' in results[0]['methodArguments'][0]
assert results[1]['classArguments']==results[2]['classArguments']
assert any('Service' in s for s in results[1]['classArguments']) and any('Uri' in s for s in results[1]['classArguments'])
summary={'status':'tagged_callsite_contexts_resolved_with_shared_body_limit','sourceImageSha256':reg['sourceSha256'],'runtimeCaptureSha256':record['sha256'],'decoderProvenance':'Pinned Il2CppDumper commit 4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a Metadata.GetEncodedIndexType and GetDecodedMethodIndex for Version>=27','results':results,'limitations':['This resolves static captured tags and generic definition agreement. Missing direct concrete-instantiation registration rows remain explicit; no live runtime equivalence is claimed.','Generic serialization method identity and type arguments do not yet prove wire property names, HTTP format or service compatibility.','Interface identity and slot signatures do not reveal which runtime storage implementation was selected.']}
(ROOT/'callsite-metadata.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(summary),flush=True)
