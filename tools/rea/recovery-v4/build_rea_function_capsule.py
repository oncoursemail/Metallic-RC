"""Package three exact captured code pages for bounded static REA analysis.
This synthetic PE is a source-range carrier, not a game executable or a replacement
for whole-program analysis. No instruction bytes within these pages are changed.
"""
from pathlib import Path
import json,struct,hashlib,datetime
R=Path(__file__).resolve().parent/'runs/recovery-/20261020T212503.738773Z'
O=R/'decoded';d=(O/'GameAssembly.code-restored.analysis.dll').read_bytes()
assert hashlib.sha256(d).hexdigest()=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
c=json.loads((R/'capture/capture-report.json').read_text());a=next(x for x in c['artifacts'] if x['kind']=='mapped_binary_evidence')
m=(R/'capture'/a['file']).read_bytes();assert hashlib.sha256(m).hexdigest()==a['sha256']
methods=[('NameServerResponse.get_Auth',0xd0ccb0,5,148135),('NameServerResponse.get_API',0xd11e20,5,148137),('PaintballManager.get_IsInitialized',0x1b28e30,0x74,1296)]
sections=c['gameAssemblyDiskLayout']['sections']
pages=sorted(set(rva & ~4095 for _,rva,_,_ in methods))
def sourceoff(rva):
 for s in sections:
  if s['rva']<=rva and rva+4096<=s['rva']+s['rawSize']:return s['rawOffset']+rva-s['rva']
 raise ValueError('Page not file backed')
exp_rva=pages[-1]+8192;exp=bytearray(4096)
n=len(methods);funcs=40;names=funcs+4*n;ords=names+4*n;cursor=ords+2*n
module=b'Metallic.SourceRangeAnalysis.dll\0';module_at=cursor;exp[cursor:cursor+len(module)]=module;cursor+=len(module)
order=sorted(enumerate(methods),key=lambda t:t[1][0])
for i,(_,rva,_,_) in enumerate(methods):struct.pack_into('<I',exp,funcs+i*4,rva)
for slot,(ordinal,(name,_,_,_)) in enumerate(order):
 b=name.encode()+b'\0';struct.pack_into('<I',exp,names+slot*4,exp_rva+cursor);struct.pack_into('<H',exp,ords+slot*2,ordinal);exp[cursor:cursor+len(b)]=b;cursor+=len(b)
struct.pack_into('<IIHHIIIIIII',exp,0,0,0,0,0,exp_rva+module_at,1,n,n,exp_rva+funcs,exp_rva+names,exp_rva+ords)
headers=bytearray(1024);headers[:2]=b'MZ';struct.pack_into('<I',headers,0x3c,0x80)
headers[0x80:0x84]=b'PE\0\0';struct.pack_into('<HHIIIHH',headers,0x84,0x8664,4,0,0,0,240,0x2022)
opt=0x98;struct.pack_into('<HBBIII',headers,opt,0x20b,14,0,3*4096,4096,0)
struct.pack_into('<IIQII',headers,opt+16,methods[-1][1],pages[0],0x180000000,4096,512)
struct.pack_into('<HHHHHH',headers,opt+40,6,0,0,0,6,0)
struct.pack_into('<III',headers,opt+52,0,exp_rva+4096,1024)
struct.pack_into('<HH',headers,opt+68,3,0x100)
struct.pack_into('<QQQQII',headers,opt+72,0x100000,0x1000,0x100000,0x1000,0,16)
struct.pack_into('<II',headers,opt+112,exp_rva,cursor)
chunks=[];ranges=[]
for i,rva in enumerate(pages+[exp_rva]):
 off=opt+240+40*i;name=(f'.code{i}' if i<3 else '.edata').encode();headers[off:off+8]=name.ljust(8,b'\0')
 struct.pack_into('<IIIIIIHHI',headers,off+8,4096,rva,4096,1024+4096*i,0,0,0,0,0x60000020 if i<3 else 0x40000040)
 if i<3:
  p=sourceoff(rva);b=d[p:p+4096];assert b==m[rva:rva+4096]
  chunks.append(b);ranges.append({'rva':hex(rva),'sourceFileOffset':p,'capsuleFileOffset':1024+4096*i,'bytes':4096,'sha256':hashlib.sha256(b).hexdigest(),'matchesRuntimeAndRestoredImage':True})
 else:chunks.append(bytes(exp))
output=bytes(headers)+b''.join(chunks);path=O/'rea/ExactCodePages.analysis.dll'
with path.open('xb') as f:f.write(output)
record={'syntheticContainer':True,'purpose':'Bounded REA decompilation of exact source code pages','originalRestoredSha256':hashlib.sha256(d).hexdigest(),'runtimeSha256':a['sha256'],'output':str(path),'outputSha256':hashlib.sha256(output).hexdigest(),'ranges':ranges,'methods':[{'name':name,'rva':hex(rva),'address':hex(0x180000000+rva),'selectedLength':ln,'methodIndex':mi,'firstBytes':m[rva:rva+min(ln,16)].hex()} for name,rva,ln,mi in methods],'limitations':['Whole-DLL REA import hit provider_timeout after approximately 335 seconds; this carrier limits the import to three exact code pages.','Callees and globals outside these pages are intentionally absent; their behavior remains unresolved.','Header/export names are generated from the validated method map, not original PE exports.','Never execute this file; it is not a runnable game or repaired DLL.'],'targetExecuted':False}
(O/'rea/code-page-provenance.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
