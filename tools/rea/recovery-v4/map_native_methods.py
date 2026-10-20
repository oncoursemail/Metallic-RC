"""Map metadata tokens to captured code-generation modules by exact image name.
Only offline artifacts are read. No guessed positional codeGenModule ordering.
"""
from pathlib import Path
import json,struct,hashlib,time,collections
ROOT=Path(__file__).resolve().parent;RUN=ROOT/'runs/recovery-/20261020T212503.738773Z';OUT=RUN/'decoded'
c=json.loads((RUN/'capture/capture-report.json').read_text());item=next(a for a in c['artifacts'] if a['kind']=='mapped_binary_evidence')
mem=(RUN/'capture'/item['file']).read_bytes();assert hashlib.sha256(mem).hexdigest()==item['sha256']
base=item['module']['base'];layout=c['gameAssemblyDiskLayout'];preferred=layout['imageBase'];sections=layout['sections']
meta=(OUT/'global-metadata.v31.normalized.dat').read_bytes();norm=json.loads((OUT/'metadata-normalization.json').read_text());assert hashlib.sha256(meta).hexdigest()==norm['normalizedSha256']
roles=norm['headerMapping']
def rr(role,fmt):q=roles[role];return list(struct.iter_unpack(fmt,meta[q['offset']:q['offset']+q['size']]))
q=roles['string'];strings=meta[q['offset']:q['offset']+q['size']]
def text(i):return strings[i:strings.index(b'\0',i)].decode('utf8')
types=rr('typeDefinitions','<16i8H2I');methods=rr('methods','<IiiiiiI4H');images=rr('images','<IiiIiiiIiI')
image_names=[text(i[0]) for i in images];owners=[None]*len(types)
for i,x in enumerate(images):owners[x[2]:x[2]+x[3]]=[i]*x[3]
max_rids=[0]*len(images)
for m in methods:
 i=owners[m[1]];max_rids[i]=max(max_rids[i],m[6]&0xffffff)
name_ptrs={};search_regions=[s for s in sections if s['name'] in ('.rdata','.data')]
for image_index,name in enumerate(image_names):
 needle=name.encode()+b'\0'
 for s in search_regions:
  pos=s['rva'];end=pos+s['virtualSize']
  while True:
   found=mem.find(needle,pos,end)
   if found<0:break
   # Avoid matching a suffix of a longer .dll name.
   if found==s['rva'] or mem[found-1]==0:name_ptrs[base+found]=(image_index,name)
   pos=found+1
print(json.dumps({'phase':'name_locations','images':len(images),'literalPointers':len(name_ptrs)}),flush=True)
candidates=collections.defaultdict(list)
for s in search_regions:
 start=s['rva'];end=start+s['virtualSize'];end-=((end-start)%8)
 for i,(p,) in enumerate(struct.iter_unpack('<Q',memoryview(mem)[start:end])):
  if p not in name_ptrs:continue
  off=start+i*8;img,name=name_ptrs[p]
  if off+24>len(mem):continue
  count,ptr=struct.unpack_from('<QQ',mem,off+8)
  if not(max_rids[img]<=count<=max(1000000,max_rids[img]*4) and base<=ptr and ptr+count*8<=base+len(mem)):continue
  targets=struct.unpack_from('<'+str(count)+'Q',mem,ptr-base)
  executable=[x for x in targets if x]
  bad=sum(not any(base+z['rva']<=x<base+z['rva']+z['virtualSize'] for z in sections if z['name'] in ('.text','il2cpp')) for x in executable)
  if bad:continue
  candidates[img].append({'imageIndex':img,'name':name,'moduleRva':off,'methodPointerCount':count,'methodPointersRva':ptr-base,'nonNullPointers':len(executable),'allNonNullPointersInCapturedCode':True})
unique={i:rows[0] for i,rows in candidates.items() if len(rows)==1}
print(json.dumps({'phase':'module_candidates','uniquelyMatchedImages':len(unique),'ambiguousImages':{str(i):len(v) for i,v in candidates.items() if len(v)!=1}}),flush=True)
summary={'runtimeBase':hex(base),'preferredBase':hex(preferred),'runtimeSha256':item['sha256'],'metadataSha256':norm['normalizedSha256'],'imageCount':len(images),'uniquelyMatchedImages':len(unique),'modules':[unique[i] for i in sorted(unique)],'missingImages':[{'index':i,'name':n,'maxMethodTokenRid':max_rids[i]} for i,n in enumerate(image_names) if i not in unique],'limitations':['Native type arrays, generic instantiations and code-registration permutation are not normalized here.','Null method pointers can represent abstract or generic methods; these are not discarded or invented.','Names are recovered metadata names and may remain obfuscated.']}
counts=collections.Counter();selected=[]
with (OUT/'native-method-map.jsonl').open('x',encoding='utf-8') as f:
 for idx,m in enumerate(methods):
  ti=m[1];img=owners[ti];rid=m[6]&0xffffff;module=unique.get(img);rva=None;status='module_unresolved'
  if module:
   pointer=struct.unpack_from('<Q',mem,module['methodPointersRva']+(rid-1)*8)[0]
   status='null_method_pointer' if not pointer else 'mapped';rva=pointer-base if pointer else None
  counts[status]+=1
  row={'methodIndex':idx,'imageIndex':img,'assembly':image_names[img],'declaringTypeIndex':ti,'namespace':text(types[ti][1]),'type':text(types[ti][0]),'method':text(m[0]),'token':hex(m[6]),'rva':hex(rva) if rva is not None else None,'analysisAddress':hex(preferred+rva) if rva is not None else None,'status':status}
  f.write(json.dumps(row,ensure_ascii=True)+'\n')
  if status=='mapped' and (('NameServer' in row['type'] or 'NameServer' in row['namespace']) or (image_names[img]=='Assembly-CSharp.dll' and row['method'] in ('get_Version','get_IsInitialized','get_IsConnected'))):
   selected.append(row)
summary['methodMapCounts']=dict(counts);summary['selectedMethods']=selected[:80]
(OUT/'native-module-mapping.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'methodMapCounts':dict(counts),'selectedMethods':selected[:20],'missingImages':summary['missingImages'][:20]}))
