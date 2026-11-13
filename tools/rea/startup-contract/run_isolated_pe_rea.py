"""Correct the analysis-only carrier scope, not the game.
Windows REA 6.5 cannot admit raw x64. This documented PE wrapper has one selected
native procedure/export and no neighboring function bytes. Unselected padding is
0xCC scaffolding. Exact source instructions keep their original virtual addresses.
"""
from pathlib import Path
import json,sqlite3,hashlib,struct,time
from rea_session import ReaClient
R=Path(__file__).resolve().parent;D=R.parent/'runs/recovery-/20261020T212503.738773Z/decoded';B=0x180000000
b=(D/'GameAssembly.code-restored.analysis.dll').read_bytes();expected='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066';assert hashlib.sha256(b).hexdigest()==expected
ss=json.loads((D.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections']
db=sqlite3.connect((R.parent/'deep-rea-20261021/Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);db.row_factory=sqlite3.Row
OUT=R/'isolated-pe';OUT.mkdir(exist_ok=False)
def records(obj):
 result=[]
 def visit(x):
  if isinstance(x,dict):
   if 'evidence_id'in x and 'normalized_result'in x:result.append(x);return
   for k,v in x.items():
    if k=='text' and isinstance(v,str):
     try:visit(json.loads(v))
     except(ValueError,TypeError):pass
    else:visit(v)
  elif isinstance(x,list):
   for v in x:visit(v)
 visit(obj);return list({x['evidence_id']:x for x in result}.values())
results=[]
for mid in [126410,347046,347050,347051,347119,347121]:
 m=db.execute('select * from methods where id=?',(mid,)).fetchone();start=m['rva'];nxt=db.execute('select min(rva) from methods where rva>?',(start,)).fetchone()[0]
 first=db.execute('select * from native_ranges where start_rva=?',(start,)).fetchone();fragments=[]
 if first:
  end=first['end_rva'];fragments=[dict(first)]
  while end<nxt:
   f=db.execute('select * from native_ranges where start_rva=?',(end,)).fetchone()
   if f is None or f['end_rva']>nxt:break
   fragments.append(dict(f));end=f['end_rva']
 else:end=nxt
 assert 0<end-start<16000
 off=next(s['rawOffset']+start-s['rva'] for s in ss if s['rva']<=start and end<=s['rva']+s['rawSize']);code=b[off:off+end-start]
 page=start&~4095;size=(end-page+4095)&~4095;chunk=bytearray(b'\xcc'*size);chunk[start-page:end-page]=code
 er=page+size+4096;name=f'm{mid}'.encode()+b'\0';exp=bytearray(512);struct.pack_into('<IIHHIIIIIII',exp,0,0,0,0,0,er+50,1,1,1,er+40,er+44,er+48);struct.pack_into('<IIH',exp,40,start,er+80,0);exp[50:65]=b'MetallicScope\0\0';exp[80:80+len(name)]=name
 h=bytearray(512);h[:2]=b'MZ';struct.pack_into('<I',h,60,0x80);h[0x80:0x84]=b'PE\0\0';struct.pack_into('<HHIIIHH',h,0x84,0x8664,2,0,0,0,240,0x2022)
 opt=0x98;struct.pack_into('<HBBIII',h,opt,0x20b,14,0,size,512,0);struct.pack_into('<IIQII',h,opt+16,start,page,B,4096,512);struct.pack_into('<HHHHHH',h,opt+40,6,0,0,0,6,0);struct.pack_into('<III',h,opt+52,0,er+4096,512);struct.pack_into('<HH',h,opt+68,3,0x100);struct.pack_into('<QQQQII',h,opt+72,0x100000,0x1000,0x100000,0x1000,0,16);struct.pack_into('<II',h,opt+112,er,100)
 for i,(nm,rv,sz,rp,fl)in enumerate([(b'.code',page,size,512,0x60000020),(b'.edata',er,512,512+size,0x40000040)]):
  p=opt+240+i*40;h[p:p+8]=nm.ljust(8,b'\0');struct.pack_into('<IIIIIIHHI',h,p+8,sz,rv,sz,rp,0,0,0,0,fl)
 target=OUT/f'm{mid}.analysis.dll';target.write_bytes(bytes(h)+chunk+exp);directory=OUT/f'm{mid}';directory.mkdir()
 record={'methodIndex':mid,'typedSignature':json.loads(m['raw_json']),'rva':hex(start),'sourceOffset':off,'sourceBytes':len(code),'sourceRangeSha256':hashlib.sha256(code).hexdigest(),'sourceImageSha256':expected,'carrierSha256':hashlib.sha256(target.read_bytes()).hexdigest(),'compilerFragments':fragments,'syntheticPaddingAndHeaders':True,'instructionsModified':False,'rawProviderFailurePreserved':True}
 ts=time.monotonic()
 with ReaClient(directory)as c:
  opened=c.tool('open_binary',{'path':str(target),'provider_id':'ghidra'},'open.json',60)
  if opened.get('result',{}).get('isError'):raise RuntimeError('PE admission rejected')
  response=c.tool('analyze_function',{'procedure':hex(B+start)},'analysis.mcp.json',240);es=[e for e in records(response)if e.get('operation')=='analyze_function']
  if es:
   e=es[-1];n=e['normalized_result'];c.save('evidence.json',e);p=n.get('pseudocode')or'';(directory/'pseudocode.c').write_text(p,encoding='utf-8',newline='\n');(directory/'assembly.txt').write_text('\n'.join(n.get('assembly',[])),encoding='utf-8');record.update(status='retained',evidenceId=e['evidence_id'],body=n.get('procedure',{}).get('body'),warnings=[x for x in p.splitlines()if'WARNING'in x or'halt_baddata'in x])
  else:record['status']='no_dossier'
  c.tool('export_evidence_bundle',{'path':str(directory/'bundle.json'),'overwrite':False},'export.json',60)
 record['seconds']=time.monotonic()-ts;results.append(record);(R/'isolated-pe-summary.json').write_text(json.dumps({'results':results,'gameExecuted':False,'scope':'Only documented source bytes; external dependencies absent.'},indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps({k:record.get(k)for k in['methodIndex','sourceBytes','status','evidenceId','seconds']}),flush=True)
