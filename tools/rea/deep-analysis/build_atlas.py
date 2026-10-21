"""Build a private, queryable evidence atlas. No original game execution.
Decoded native operands are indexed as observations, not proven reachable calls.
"""
from pathlib import Path
import sqlite3,json,csv,hashlib,time,datetime,collections
ROOT=Path(__file__).resolve().parent
REC=ROOT.parent/'csharp-reconstruction'
OLD=ROOT.parent/'runs/recovery-/20261020T212503.738773Z/decoded'
FINAL=ROOT/'Metallic-RC.Analysis.sqlite';TEMP=ROOT/'Metallic-RC.Analysis.building.sqlite'
if FINAL.exists() or TEMP.exists():raise RuntimeError('Choose a new atlas destination')
inputs=[REC/'typed-reference-v3/method-signatures.jsonl',REC/'typed-reference-v3/field-layouts.jsonl',OLD/'upstream-parser/types.jsonl',REC/'native-flow/unwind-function-ranges.csv',REC/'native-flow/direct-call-and-tail-edges.csv',REC/'native-flow/indirect-flows.csv',REC/'partial-csharp/emitted-leaf-methods.jsonl']
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
manifest=[{'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)} for p in inputs]
DB=sqlite3.connect(TEMP);DB.execute('PRAGMA journal_mode=WAL');DB.execute('PRAGMA synchronous=NORMAL');DB.execute('PRAGMA cache_size=-65536')
DB.executescript('''
CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE methods(id INTEGER PRIMARY KEY,assembly TEXT,type_id INTEGER,type TEXT,name TEXT,returns TEXT,parameters TEXT,token INTEGER,flags INTEGER,rva INTEGER,body_status TEXT,raw_json TEXT);
CREATE TABLE fields(id INTEGER PRIMARY KEY,assembly TEXT,type_id INTEGER,name TEXT,type TEXT,offset INTEGER,offset_status TEXT,is_static INTEGER,raw_json TEXT);
CREATE TABLE types(id INTEGER PRIMARY KEY,assembly TEXT,namespace TEXT,name TEXT,raw_json TEXT);
CREATE TABLE native_ranges(start_rva INTEGER PRIMARY KEY,end_rva INTEGER,unwind_rva INTEGER,instructions INTEGER,invalid_instructions INTEGER,direct_calls INTEGER,indirect_calls INTEGER,branches INTEGER,sha256 TEXT);
CREATE TABLE direct_flows(source_rva INTEGER,instruction_rva INTEGER,target_rva INTEGER,kind TEXT,in_captured_code INTEGER);
CREATE TABLE indirect_flows(source_rva INTEGER,instruction_rva INTEGER,flow TEXT,operand_kind TEXT,memory_base TEXT,memory_index TEXT,slot_address INTEGER);
CREATE TABLE recovered_leafs(method_id INTEGER PRIMARY KEY,operation TEXT,rva INTEGER,byte_sha256 TEXT,raw_json TEXT);
''')
def emit(name,sql,iterator):
 n=0;batch=[];start=time.monotonic()
 for row in iterator:
  batch.append(row)
  if len(batch)>=20000:
   DB.executemany(sql,batch);DB.commit();n+=len(batch);batch=[]
   if n%200000==0:print(json.dumps({'phase':name,'rows':n}),flush=True)
 if batch:DB.executemany(sql,batch);n+=len(batch);DB.commit()
 print(json.dumps({'phase':name,'rows':n,'seconds':round(time.monotonic()-start,2)}),flush=True)
 return n
def jsonl(p):
 with p.open(encoding='utf-8-sig') as f:
  for line in f:yield json.loads(line),line.rstrip()
def hexint(s):return int(s,16) if s else None
counts={}
counts['methods']=emit('methods','INSERT INTO methods VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',((m['methodIndex'],m['assembly'],m['declaringTypeIndex'],m['declaringType'],m['name'],m['returnType'],json.dumps(m['parameters']),m['token'],m['flags'],hexint(m['nativeRva']),m['bodyStatus'],line) for m,line in jsonl(inputs[0])))
counts['fields']=emit('fields','INSERT INTO fields VALUES(?,?,?,?,?,?,?,?,?)',((m['fieldIndex'],m['assembly'],m['declaringTypeIndex'],m['name'],m['type'],m['offset'],m['offsetStatus'],int(m['isStatic']),line) for m,line in jsonl(inputs[1])))
counts['types']=emit('types','INSERT INTO types VALUES(?,?,?,?,?)',((m['typeIndex'],m['assembly'],m['namespace'],m['name'],line) for m,line in jsonl(inputs[2])))
with inputs[3].open(newline='') as f:
 counts['ranges']=emit('ranges','INSERT INTO native_ranges VALUES(?,?,?,?,?,?,?,?,?)',((int(x['start_rva'],16),int(x['end_rva'],16),int(x['unwind_rva'],16),int(x['instructions']),int(x['invalid_instructions']),int(x['direct_calls']),int(x['indirect_calls']),int(x['conditional_branches']),x['sha256']) for x in csv.DictReader(f)))
with inputs[4].open(newline='') as f:
 counts['directFlows']=emit('direct_flows','INSERT INTO direct_flows VALUES(?,?,?,?,?)',((int(x['function_rva'],16),int(x['instruction_rva'],16),int(x['target_address'],16)-0x180000000,x['kind'],x['target_in_captured_code']=='True') for x in csv.DictReader(f)))
with inputs[5].open(newline='') as f:
 counts['indirectFlows']=emit('indirect_flows','INSERT INTO indirect_flows VALUES(?,?,?,?,?,?,?)',((int(x['function_rva'],16),int(x['instruction_rva'],16),x['flow'],x['operand_kind'],x['memory_base'],x['memory_index'],hexint(x['ip_relative_slot_address'])) for x in csv.DictReader(f)))
counts['leafs']=emit('leafs','INSERT INTO recovered_leafs VALUES(?,?,?,?,?)',((m['methodIndex'],m['operation'],int(m['rva'],16),m['nativeBytesSha256'],line) for m,line in jsonl(inputs[6])))
print('INDEXING',flush=True)
DB.executescript('''
CREATE INDEX methods_rva ON methods(rva);
CREATE INDEX methods_type ON methods(type_id);
CREATE INDEX methods_assembly_name ON methods(assembly,name);
CREATE INDEX fields_type ON fields(type_id);
CREATE INDEX types_name ON types(name);
CREATE INDEX flows_source ON direct_flows(source_rva);
CREATE INDEX flows_target ON direct_flows(target_rva);
CREATE INDEX indirect_source ON indirect_flows(source_rva);
CREATE VIRTUAL TABLE method_search USING fts5(assembly,type,name,returns,content='methods',content_rowid='id');
INSERT INTO method_search(method_search) VALUES('rebuild');
''')
for k,v in {'buildId':'23191908','inputManifest':manifest,'counts':counts,'scope':'Recovered artifacts only; indexed static operands are not proof of reachability or complete source understanding.'}.items():DB.execute('INSERT INTO metadata VALUES(?,?)',(k,json.dumps(v)))
DB.commit();integrity=DB.execute('PRAGMA integrity_check').fetchall();assert integrity==[('ok',)],integrity
assert counts['methods']==409503 and counts['types']==63648 and counts['fields']==252928
assert DB.execute('SELECT count(*) FROM methods WHERE rva IS NOT NULL').fetchone()[0]==367865
assert DB.execute('SELECT count(*) FROM recovered_leafs l LEFT JOIN methods m ON l.method_id=m.id WHERE m.id IS NULL').fetchone()[0]==0
counts['namedNativeRanges']=DB.execute('SELECT count(DISTINCT n.start_rva) FROM native_ranges n JOIN methods m ON m.rva=n.start_rva').fetchone()[0]
DB.execute('PRAGMA wal_checkpoint(TRUNCATE)');DB.close();TEMP.replace(FINAL)
summary={'status':'atlas_built_and_integrity_checked','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'file':str(FINAL),'bytes':FINAL.stat().st_size,'sha256':sha(FINAL),'counts':counts,'sqliteIntegrity':'ok','inputs':manifest,'targetExecuted':False,'allSemanticsUnderstood':False,'limits':['Native ranges are compiler unwind records, not a one-to-one C# method list.','Static direct operands can be unreachable code or misclassified data; invalid-range counts are preserved.','Indirect target resolution and dynamic dispatch require separate proof.','Recovered leaf entries are prior translation results, not new REA-proven bodies.']}
(ROOT/'atlas-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({k:summary[k] for k in ('status','file','bytes','counts','sqliteIntegrity')}),flush=True)
