"""Build bounded REA carriers from exact source pages and an indexed call graph.
No instruction bytes are synthesized. Generated PE headers/export labels are
analysis scaffolding. Outside-page dependencies and indirect flows stay unknown.
"""
from pathlib import Path
import sqlite3,json,struct,hashlib,re,time,collections
ROOT=Path(__file__).resolve().parent
RUN=ROOT.parent/'runs/recovery-/20261020T212503.738773Z'
SOURCE=RUN/'decoded/GameAssembly.code-restored.analysis.dll'
EXPECTED='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
GROUPS={'startup':[12789,12817,12811,148260,408427],
 'room-join':[12993,100412,314484,12925],
 'paintball':[1316,1333,1341,1344,1322],
 'circuits':[387023,387055,375556],
 'voice-vr':[328387,328397,264748,327128]}
if not (ROOT/'Metallic-RC.Analysis.sqlite').exists():raise RuntimeError('Atlas is not complete')
DB=sqlite3.connect('file:'+str(ROOT/'Metallic-RC.Analysis.sqlite').replace('\\','/')+'?mode=ro',uri=True);DB.row_factory=sqlite3.Row
source=SOURCE.read_bytes();assert hashlib.sha256(source).hexdigest()==EXPECTED
cap=json.loads((RUN/'capture/capture-report.json').read_text());image=next(a for a in cap['artifacts'] if a['kind']=='mapped_binary_evidence')
mem=(RUN/'capture'/image['file']).read_bytes();assert hashlib.sha256(mem).hexdigest()==image['sha256']
SECTIONS=cap['gameAssemblyDiskLayout']['sections'];BASE=0x180000000
OUT=ROOT/'scoped-targets';OUT.mkdir(exist_ok=False)
def rawoffset(rva,n):
 for s in SECTIONS:
  if s['rva']<=rva and rva+n<=s['rva']+s['rawSize']:return s['rawOffset']+rva-s['rva']
 raise ValueError('Unbacked source page '+hex(rva))
def pages_for(a,b):return set(range(a&~4095,(b+4095)&~4095,4096))
def merge_pages(pages):
 result=[]
 for p in sorted(pages):
  if result and result[-1][1]==p:result[-1][1]+=4096
  else:result.append([p,p+4096])
 return result
summaries=[]
for group,ids in GROUPS.items():
 seeds=[];pages=set();included={};notes=[];edges=[]
 for mid in ids:
  row=DB.execute('SELECT * FROM methods WHERE id=?',(mid,)).fetchone();assert row is not None and row['rva'] is not None
  m=json.loads(row['raw_json']);rv=row['rva'];rng=DB.execute('SELECT * FROM native_ranges WHERE start_rva=?',(rv,)).fetchone()
  m['range']=dict(rng) if rng else None
  if rng and rng['invalid_instructions']:
   notes.append({'methodIndex':mid,'reason':'linear_audit_has_invalid_bytes','invalid':rng['invalid_instructions']})
  end=rng['end_rva'] if rng else rv+512
  assert end-rv<=65536
  pages|=pages_for(rv,end);included[rv]=end;seeds.append(m)
 # Include explicit direct tail targets and small direct callees one level deep.
 for seed in seeds:
  rv=int(seed['nativeRva'],16)
  for e in DB.execute('SELECT * FROM direct_flows WHERE source_rva=? ORDER BY instruction_rva',(rv,)):
   e=dict(e);names=DB.execute('SELECT id,assembly,type,name,returns FROM methods WHERE rva=? LIMIT 5',(e['target_rva'],)).fetchall();e['targetMethodCandidates']=[dict(n) for n in names];edges.append(e)
   target=e['target_rva'];rng=DB.execute('SELECT * FROM native_ranges WHERE start_rva=?',(target,)).fetchone()
   if rng and not rng['invalid_instructions'] and (e['kind']=='out_of_range_jump_candidate' or rng['end_rva']-target<=512):
    proposal=pages|pages_for(target,rng['end_rva'])
    if len(proposal)<=240 and len(merge_pages(proposal))<=70:pages=proposal;included[target]=rng['end_rva']
 # Name only independently mapped method entry points contained in source pages.
 exports={};seed_by_rva={int(x['nativeRva'],16):x for x in seeds}
 for start,end in merge_pages(pages):
  for row in DB.execute('SELECT id,rva,name FROM methods WHERE rva>=? AND rva<? ORDER BY id',(start,end)):
   if row['rva'] not in exports or row['rva'] in seed_by_rva:
    chosen=seed_by_rva.get(row['rva']);mid=chosen['methodIndex'] if chosen else row['id'];name=chosen['name'] if chosen else row['name']
    exports[row['rva']]=f'm{mid}_'+re.sub(r'[^A-Za-z0-9_]','_',name)[:64]
 for rv in included:exports.setdefault(rv,'native_'+format(rv,'x'))
 spans=merge_pages(pages);assert len(spans)+1<=96
 exp_rva=spans[-1][1]+4096;ordered=sorted(exports.items());n=len(ordered)
 exp=bytearray(40+10*n);cursor=len(exp)
 module=f'Metallic_{group}_SourcePages'.encode()+b'\0';module_at=cursor;exp.extend(module);cursor+=len(module)
 name_slots=sorted(enumerate(ordered),key=lambda t:t[1][1]);funcs=40;names=funcs+4*n;ords=names+4*n
 for i,(rv,name) in enumerate(ordered):struct.pack_into('<I',exp,funcs+4*i,rv)
 for slot,(ordinal,(rv,name)) in enumerate(name_slots):
  text=name.encode()+b'\0';struct.pack_into('<I',exp,names+4*slot,exp_rva+cursor);struct.pack_into('<H',exp,ords+2*slot,ordinal);exp.extend(text);cursor+=len(text)
 struct.pack_into('<IIHHIIIIIII',exp,0,0,0,0,0,exp_rva+module_at,1,n,n,exp_rva+funcs,exp_rva+names,exp_rva+ords)
 exp_len=len(exp);exp.extend(b'\0'*((-len(exp))%512));exp_vs=(len(exp)+4095)&~4095
 section_count=len(spans)+1;opt=0x98;header_size=(opt+240+section_count*40+511)&~511
 hdr=bytearray(header_size);hdr[:2]=b'MZ';struct.pack_into('<I',hdr,0x3c,0x80);hdr[0x80:0x84]=b'PE\0\0'
 struct.pack_into('<HHIIIHH',hdr,0x84,0x8664,section_count,0,0,0,240,0x2022)
 struct.pack_into('<HBBIII',hdr,opt,0x20b,14,0,len(pages)*4096,len(exp),0)
 struct.pack_into('<IIQII',hdr,opt+16,int(seeds[0]['nativeRva'],16),spans[0][0],BASE,4096,512)
 struct.pack_into('<HHHHHH',hdr,opt+40,6,0,0,0,6,0);struct.pack_into('<III',hdr,opt+52,0,exp_rva+exp_vs,header_size)
 struct.pack_into('<HH',hdr,opt+68,3,0x100);struct.pack_into('<QQQQII',hdr,opt+72,0x100000,0x1000,0x100000,0x1000,0,16)
 struct.pack_into('<II',hdr,opt+112,exp_rva,exp_len)
 chunks=[];provenance=[];pointer=header_size
 for i,(start,end) in enumerate(spans+[[exp_rva,exp_rva+len(exp)]]):
  s=opt+240+i*40;name=(f'.c{i}' if i<len(spans) else '.edata').encode();hdr[s:s+8]=name.ljust(8,b'\0')
  size=end-start;struct.pack_into('<IIIIIIHHI',hdr,s+8,size,start,size,pointer,0,0,0,0,0x60000020 if i<len(spans) else 0x40000040)
  if i<len(spans):
   off=rawoffset(start,size);b=source[off:off+size];assert len(b)==size and b==mem[start:end]
   provenance.append({'sourceRva':hex(start),'sourceFileOffset':off,'bytes':size,'carrierFileOffset':pointer,'sha256':hashlib.sha256(b).hexdigest(),'matchesRuntime':True})
  else:b=bytes(exp)
  chunks.append(b);pointer+=size
 target=OUT/(group+'.analysis.dll');payload=bytes(hdr)+b''.join(chunks);target.write_bytes(payload)
 for seed in seeds:
  seed['incomingOperands']=DB.execute('SELECT count(*) FROM direct_flows WHERE target_rva=?',(int(seed['nativeRva'],16),)).fetchone()[0]
  seed['indirectOperands']=[dict(e) for e in DB.execute('SELECT * FROM indirect_flows WHERE source_rva=?',(int(seed['nativeRva'],16),))]
 fields=[dict(f) for tid in sorted(set(m['declaringTypeIndex'] for m in seeds)) for f in DB.execute('SELECT id,assembly,name,type,offset,offset_status,is_static FROM fields WHERE type_id=?',(tid,))]
 record={'group':group,'syntheticCarrier':True,'target':str(target),'sha256':hashlib.sha256(payload).hexdigest(),'bytes':len(payload),'sourceSha256':EXPECTED,'sourcePageCount':len(pages),'sourceRanges':provenance,'selectedMethods':seeds,'directOperandObservations':edges,'fieldLayouts':fields,'initialNotes':notes,'exportLabelsAreGenerated':True,'limits':['Unselected dependencies/data pages are absent. Do not execute the carrier.','Native signatures remain inference; recovered typed metadata is supplied separately.','Static operand edges do not prove reachability or server compatibility.','Per-method evidence must be checked for truncated body ranges and decompiler warnings.']}
 (OUT/(group+'.provenance.json')).write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n')
 summaries.append({'group':group,'path':str(target),'provenance':group+'.provenance.json','methods':ids,'pages':len(pages),'bytes':len(payload),'sha256':record['sha256'],'invalidAuditNotes':notes})
 print(json.dumps(summaries[-1]),flush=True)
DB.close();(ROOT/'scoped-targets.json').write_text(json.dumps(summaries,indent=2)+'\n',encoding='utf-8',newline='\n')
