"""Cross-check reverse service-field bindings and identify generic call candidates.
Static metadata evidence only. Generic shared code does not by itself identify
which instantiation a particular call actually supplies at runtime.
"""
from pathlib import Path
import struct,json,sqlite3,hashlib,re,collections
ROOT=Path(__file__).resolve().parent;WORK=ROOT.parent;DEEP=WORK/'deep-rea-20261021'
DB=sqlite3.connect((DEEP/'Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);DB.row_factory=sqlite3.Row
source=WORK/'runs/recovery-/20261020T212503.738773Z/decoded/GameAssembly.code-restored.analysis.dll'
b=source.read_bytes();expected='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066';assert hashlib.sha256(b).hexdigest()==expected
cap=json.loads((source.parent.parent/'capture/capture-report.json').read_text());sections=cap['gameAssemblyDiskLayout']['sections'];base=0x180000000
reg=json.loads((WORK/'csharp-reconstruction/verified-native/native-registration-validation.json').read_text())
assert reg['sourceSha256']==expected
registration=reg['originalMetadataRegistration']
def off(p,n):
 for s in sections:
  if base+s['rva']<=p and p+n<=base+s['rva']+s['rawSize']:return s['rawOffset']+p-base-s['rva']
 raise ValueError('Not file-backed: '+hex(p))
def rd(p,fmt):return struct.unpack_from(fmt,b,off(p,struct.calcsize(fmt)))
def pointers(role):
 v=registration[role];return rd(int(v['pointer'],16),'<'+str(v['count'])+'Q')
old=json.loads((DEEP/'findings/name-server-bindings.json').read_text());oldby={x['serviceId']:x for x in old['rows']}
ev=json.loads((ROOT/'rea-dossiers/startup-cache/m148091.evidence.json').read_text());asm=ev['normalized_result']['assembly'];inverse=[]
for i,line in enumerate(asm):
 if not line.endswith('CALL 0x185873790'):continue
 before=asm[max(0,i-5):i]
 ids=[int(m.group(1),16) for ln in before for m in [re.search(r'MOV EDX, 0x([0-9a-f]+)$',ln)] if m]
 if any(ln.endswith('XOR EDX, EDX') for ln in before):ids.append(0)
 assert len(ids)==1
 tail=asm[i+1:i+16];stores=[(ln,int(m.group(1),16)) for ln in tail for m in [re.search(r'MOV qword ptr \[RBX \+ 0x([0-9a-f]+)\], RAX$',ln)] if m]
 assert len(stores)==1 and any(ln.endswith('CALL 0x18ac4ff00') for ln in tail)
 service=ids[0];field=stores[0][1];assert oldby[service]['responseFieldOffset']==field
 inverse.append({'serviceId':service,'name':oldby[service]['serviceName'],'fieldOffset':field,'lookupCall':line.split(':')[0],'storeInstruction':stores[0][0],'originalStringCallObserved':True,'matchesDiscoveryReadOffset':True})
assert len(inverse)==31 and {x['serviceId'] for x in inverse}==set(range(31))
# Generic sharing resolution, retaining all candidates before applying a display filter.
targets={0x3b0e610:'serialize_candidate',0x5873790:'dictionary_get',0x5873a30:'dictionary_set'}
array=next(x for x in reg['codeArrays'] if x['role']=='genericMethods')
code=rd(int(array['pointer'],16),'<'+str(array['count'])+'Q')
indices={i:p-base for i,p in enumerate(code) if p-base in targets}
gt=registration['genericMethodTable'];gtbytes=gt['count']*16;gtstart=off(int(gt['pointer'],16),gtbytes)
spec=registration['methodSpecs'];specstart=off(int(spec['pointer'],16),spec['count']*12)
typeptrs=pointers('types');typeindex={p:i for i,p in enumerate(typeptrs)};inst=pointers('genericInsts')
names={}
with (WORK/'csharp-reconstruction/typed-reference-v3/native-types.jsonl').open(encoding='utf-8') as f:
 for line in f:
  item=json.loads(line);names[item['nativeTypeIndex']]=item['name']
def args(i):
 if i==-1:return []
 assert 0<=i<len(inst);n,p=rd(inst[i],'<QQ');assert 0<n<=128
 return [names[typeindex[x]] for x in rd(p,'<'+str(n)+'Q')]
allcandidates=[];cache={}
for specid,codeid,invoker,adjustor in struct.iter_unpack('<4i',memoryview(b)[gtstart:gtstart+gtbytes]):
 if codeid not in indices:continue
 assert 0<=specid<spec['count'];mid,ci,gi=struct.unpack_from('<3i',b,specstart+specid*12)
 if mid not in cache:cache[mid]=json.loads(DB.execute('SELECT raw_json FROM methods WHERE id=?',(mid,)).fetchone()[0])
 m=cache[mid];allcandidates.append({'targetRva':hex(indices[codeid]),'methodSpecIndex':specid,'methodDefinitionIndex':mid,'assembly':m['assembly'],'declaringType':m['declaringType'],'method':m['name'],'returnType':m['returnType'],'classArguments':args(ci),'methodArguments':args(gi),'nativeSharedBody':True})
selected=[x for x in allcandidates if any('NameServerResponse' in a or 'RecNet.Service' in a for a in x['classArguments']+x['methodArguments'])]
fields=[dict(x) for x in DB.execute('SELECT id,name,type,offset,is_static FROM fields WHERE type_id=? AND offset IN (56,208,296,297,304,312,320) AND is_static=1',(22195,))]
slots=[json.loads(x[0]) for x in DB.execute('SELECT raw_json FROM methods WHERE type=?',('RecRoom.Preferences.STAKPLUPOSH',))];slots=[x for x in slots if x['slot'] in (22,24)]
result={'status':'reverse_service_bindings_and_generic_candidates_checked','reverseBindingCount':len(inverse),'reverseBindingEvidenceId':ev['evidence_id'],'rows':inverse,'selectedGenericCandidates':selected,'allGenericCandidateCounts':dict(collections.Counter(x['targetRva'] for x in allcandidates)),'fieldEvidence':fields,'preferenceInterfaceSlots':slots,'sourceSha256':expected,'limitations':['Reverse assignments and OriginalString calls are observed; the final storage provider and serialization wire format have not been executed.','Generic shared body matches are candidates until the call-specific MethodInfo/context is validated.','Obfuscated interface slot 22 is a bool-returning one-string operation; delete/remove semantics are not established by signature alone.','Initialize flags and request queues are identified structurally; complete async dispatch and failure behavior remain unresolved.']}
(ROOT/'cache-findings.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8',newline='\n')
(ROOT/'generic-candidates.private.json').write_text(json.dumps(allcandidates,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'status':result['status'],'reverseBindingCount':len(inverse),'selectedGenericCandidates':selected,'allGenericCandidateCounts':result['allGenericCandidateCounts']}),flush=True)
