"""Create a non-distributable, offline-only PE analysis derivative.
Original disk data sections are preserved; only captured code and documented
header fields change. No file is loaded as code or executed.
"""
from pathlib import Path
import json,hashlib,struct,collections,math,datetime
ROOT=Path(__file__).resolve().parent
RUN=ROOT/'runs/recovery-/20261020T212503.738773Z'
def sha(b):return hashlib.sha256(b).hexdigest()
report=json.loads((RUN/'capture/capture-report.json').read_text())
item=next(x for x in report['artifacts'] if x['kind']=='mapped_binary_evidence')
assert item['coverage']['complete'] and item['coverage']['readBytes']==item['bytes']
diskpath=Path(report['steamRoot'])/'GameAssembly.dll';disk=diskpath.read_bytes()
assert sha(disk)==report['gameAssemblyDiskSha256']
mem=(RUN/'capture'/item['file']).read_bytes();assert sha(mem)==item['sha256']
layout=report['gameAssemblyDiskLayout'];base=layout['imageBase'];delta=item['module']['base']-base
pe=struct.unpack_from('<I',disk,0x3c)[0];opt=pe+24
assert disk[pe:pe+4]==b'PE\0\0' and struct.unpack_from('<H',disk,opt)[0]==0x20b
section_start=opt+struct.unpack_from('<H',disk,pe+20)[0]
sections=layout['sections'];code=[s for s in sections if s['name'] in ('.text','il2cpp')]
assert len(code)==2
result=bytearray(disk)
for s in code:
 n=s['virtualSize'];assert n<=s['rawSize']
 result[s['rawOffset']:s['rawOffset']+n]=mem[s['rva']:s['rva']+n]
def rawoff(rva,length=1):
 for s in sections:
  if s['rva']<=rva and rva+length<=s['rva']+s['rawSize']:return s['rawOffset']+rva-s['rva']
 raise ValueError('RVA outside disk-backed sections '+hex(rva))
rrva,rsize=struct.unpack_from('<II',disk,opt+112+5*8)
rpos=rawoff(rrva,rsize);pos=rpos;end=rpos+rsize;relocs=collections.Counter();records=[]
while pos<end:
 page,bs=struct.unpack_from('<II',disk,pos)
 if page==0 and bs==0:break
 if bs<8 or bs%2 or pos+bs>end:raise ValueError('Malformed relocation block')
 for (entry,) in struct.iter_unpack('<H',disk[pos+8:pos+bs]):
  typ,off=entry>>12,entry&0xfff;rva=page+off
  if typ==0:continue
  if any(s['rva']<=rva<s['rva']+s['virtualSize'] for s in code):
   if typ not in (10,3):raise ValueError('Unhandled code relocation '+str(typ))
   n=8 if typ==10 else 4;fmt='<Q' if n==8 else '<I';p=rawoff(rva,n)
   old=struct.unpack_from(fmt,result,p)[0];new=(old-delta)&((1<<(n*8))-1)
   struct.pack_into(fmt,result,p,new);relocs[typ]+=1
   records.append({'rva':hex(rva),'type':typ,'before':hex(old),'after':hex(new)})
 pos+=bs
changes=[]
for i,s in enumerate(sections):
 if s['name'] in ('.text','il2cpp'):
  off=section_start+i*40+36;old=struct.unpack_from('<I',result,off)[0]
  struct.pack_into('<I',result,off,0x60000020)
  changes.append({'offset':off,'field':s['name']+' characteristics','old':hex(old),'new':'0x60000020'})
for off,n,desc in [(opt+64,4,'checksum cleared'),(opt+112+4*8,8,'Authenticode directory cleared in derivative')]:
 changes.append({'offset':off,'field':desc,'beforeHex':bytes(result[off:off+n]).hex(),'afterHex':'00'*n});result[off:off+n]=b'\0'*n
retained=[]
for s in sections:
 if s not in code:
  p,n=s['rawOffset'],s['rawSize'];assert result[p:p+n]==disk[p:p+n]
  retained.append({'section':s['name'],'sha256':sha(disk[p:p+n]),'bytes':n,'identicalToDisk':True})
windows=[]
for s in code:
 for i in range(16):
  off=(s['virtualSize']-65536)*i//15;p=s['rawOffset']+off;b=result[p:p+65536]
  h=-sum((v/len(b))*math.log2(v/len(b)) for v in collections.Counter(b).values())
  windows.append({'section':s['name'],'sectionOffset':off,'entropy':h,'sha256':sha(b)})
assert all(1<x['entropy']<7.7 for x in windows)
out=RUN/'decoded';out.mkdir(exist_ok=True)
path=out/'GameAssembly.code-restored.analysis.dll'
with path.open('xb') as f:f.write(result)
summary={'sourceDiskPath':str(diskpath),'sourceDiskSha256':sha(disk),'runtimeImageSha256':sha(mem),'preferredBase':hex(base),'runtimeBase':hex(item['module']['base']),'relocationDelta':hex(delta),'restoredCodeBytes':sum(s['virtualSize'] for s in code),'codeRelocationsUndone':dict(relocs),'relocations':records,'headerChanges':changes,'diskDataSectionsPreserved':retained,'codeWindows':windows,'output':path.name,'outputBytes':len(result),'outputSha256':sha(result),'entryPointRva':hex(struct.unpack_from('<I',disk,opt+16)[0]),'targetExecuted':False,'purpose':'Offline native-code analysis only; not a repaired or runnable game','limitations':['Runtime BSS tail is retained only in original mapped capture, not grafted here.','Native metadata and code registrations remain unnormalized in this derivative.','Imported targets are still original protector imports; function semantics require REA validation.','This is not original source and not a distributable game executable.']}
(out/'native-reconstruction.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:summary[k] for k in ('output','outputSha256','outputBytes','restoredCodeBytes','codeRelocationsUndone','entryPointRva','limitations')}))
