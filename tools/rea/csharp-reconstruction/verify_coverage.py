"""Record every input byte range without claiming every byte's meaning is known."""
from pathlib import Path
import json,hashlib,struct,collections,re,datetime
ROOT=Path(__file__).resolve().parent
SOURCE=ROOT.parent/'runs/recovery-/20261020T212503.738773Z/decoded'
OUT=ROOT/'verification';OUT.mkdir(exist_ok=False)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def merge(ranges):
 result=[]
 for a,z in sorted(set(ranges)):
  if result and a<=result[-1][1]:result[-1][1]=max(z,result[-1][1])
  else:result.append([a,z])
 return result
native=SOURCE/'GameAssembly.code-restored.analysis.dll';metadata=SOURCE/'global-metadata.v31.normalized.dat'
assert sha(native)=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
assert sha(metadata)=='d675ea92bfb0086c0238e56702e91413fcc663c81fce364253749edbf6fcec9f'
cap=json.loads((SOURCE.parent/'capture/capture-report.json').read_text());sections=cap['gameAssemblyDiskLayout']['sections']
regions=[{'name':'PE_headers','start':0,'end':cap['gameAssemblyDiskLayout']['headerSize'],'interpretation':'PE header layout inspected; analysis derivative, not runnable game'}]
for s in sections:
 regions.append({'name':s['name'],'start':s['rawOffset'],'end':s['rawOffset']+s['rawSize'],'interpretation':'captured code with only manifest-listed leaf behavior translated' if s['name'] in ('.text','il2cpp') else 'original disk section, typed/native arrays inspected where identified; not fully interpreted'})
regions.sort(key=lambda x:x['start']);complete=[];cursor=0
for r in regions:
 assert r['start']>=cursor and r['end']<=native.stat().st_size
 if r['start']>cursor:complete.append({'name':'alignment_or_unclassified_bytes','start':cursor,'end':r['start'],'interpretation':'hashed and retained, not semantically classified'})
 complete.append(r);cursor=r['end']
if cursor<native.stat().st_size:complete.append({'name':'trailing_file_bytes','start':cursor,'end':native.stat().st_size,'interpretation':'hashed and retained, not executed or claimed as code'})
n=json.loads((SOURCE/'metadata-normalization.json').read_text());mr=[{'name':'normalized_header','start':0,'end':256,'interpretation':'canonical v31 header; exact changes retained separately'}]
for name,r in n['headerMapping'].items():
 if r['size']:mr.append({'name':name,'start':r['offset'],'end':r['offset']+r['size'],'interpretation':'metadata table shape/ranges cross-checked; blob semantics only within documented parser scope'})
mr.sort(key=lambda x:x['start']);assert all(a['end']==b['start'] for a,b in zip(mr,mr[1:]));assert mr[-1]['end']==metadata.stat().st_size
ledgers=[]
for p,rr in ((native,complete),(metadata,mr)):
 assert sum(x['end']-x['start'] for x in rr)==p.stat().st_size
 chunks=[];whole=hashlib.sha256()
 with p.open('rb') as f:
  offset=0
  while data:=f.read(1024*1024):chunks.append({'offset':offset,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()});whole.update(data);offset+=len(data)
 assert offset==p.stat().st_size
 ledgers.append({'file':p.name,'bytes':offset,'sha256':whole.hexdigest(),'everyByteReadAndHashed':True,'rangeCoverageComplete':True,'allSemanticsKnown':False,'regions':rr,'chunks':chunks})
(OUT/'byte-range-ledger.json').write_text(json.dumps(ledgers,indent=2)+'\n',encoding='utf-8',newline='\n')
proofs={}
for line in (ROOT/'leaf-bodies/leaf-reconstruction.jsonl').open():
 p=json.loads(line);proofs[(p['assembly'],p['token'])]=p
leafs=[];ops=collections.Counter();assemblies=collections.Counter()
for line in (ROOT/'partial-csharp/emitted-leaf-methods.jsonl').open():
 e=json.loads(line);p=proofs[(e['assembly'],e['token'])];assert e['nativeBytesSha256']==p['nativeBytesSha256'];leafs.append((p['sourceOffset'],p['sourceOffset']+len(bytes.fromhex(p['nativeBytes']))));ops[e['operation']]+=1;assemblies[e['assembly']]+=1
union=merge(leafs)
assert len(leafs)==27302
file=ROOT/'typed-reference-v3/CSharp.Declarations.reference.cs';ids=[]
with file.open(encoding='utf-8') as f:
 for line in f:
  hit=re.search(r'// TypeDefIndex: (\d+)',line)
  if hit:ids.append(int(hit[1]))
assert ids==list(range(63648))
# Match every typed signature and native address to the previous independently
# built method map. Both inventories must terminate together.
from itertools import zip_longest
count=params=nonnull=0
with (ROOT/'typed-reference-v3/method-signatures.jsonl').open() as f,(SOURCE/'native-method-map.jsonl').open() as g:
 for a,b in zip_longest(f,g):
  assert a is not None and b is not None
  a=json.loads(a);b=json.loads(b)
  assert a['methodIndex']==b['methodIndex'] and a['assembly']==b['assembly'] and a['token']==int(b['token'],16) and a['nativeRva']==b['rva']
  count+=1;params+=len(a['parameters']);nonnull+=a['nativeRva'] is not None
assert (count,params,nonnull)==(409503,373383,367865)
corrections=json.loads((ROOT/'typed-reference-v3/runtime-pointer-corrections.json').read_text());assert corrections['correctionCount']==313
summary={'status':'coverage_and_crosschecks_passed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'completeByteHashCoverage':{'nativeBytes':native.stat().st_size,'metadataBytes':metadata.stat().st_size,'totalBytes':sum(x['bytes'] for x in ledgers),'gaps':0,'allSemanticsUnderstood':False},'typedSignatureRowsCrossChecked':count,'parameterRows':params,'nonnullNativeAddresses':nonnull,'referenceTypesInOrder':len(ids),'runtimeOnlyPointerCorrections':313,'emittedLeafBodies':len(leafs),'emittedOperations':dict(ops),'assembliesWithRecoveredLeafs':len(assemblies),'topAssemblyLeafCounts':assemblies.most_common(12),'distinctLeafByteSpans':len(set(leafs)),'unionLeafNativeInstructionBytes':sum(z-a for a,z in union),'nativeInstructionCoverageMeaning':'Only these exact source spans have typed leaf translations; not a percentage of game completion. Shared native bodies are not counted repeatedly.','nativeByteLedgerSha256':sha(OUT/'byte-range-ledger.json'),'unknownConcreteBodies':354441,'abstractOrExternalBodylessMethods':27760,'fullGameRebuild':False}
(OUT/'coverage-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(summary))
