"""Cross-check the exact name-server route bindings using REA assembly,
REA pseudocode, recovered field offsets, and the reconstructed enum declaration.
No live requests, account data, configuration changes or server impersonation.
"""
from pathlib import Path
import json,re,hashlib,sqlite3
ROOT=Path(__file__).resolve().parent
OUT=ROOT/'findings';OUT.mkdir(exist_ok=True)
EVIDENCE=ROOT/'rea-dossiers/startup/m148260.evidence.json'
e=json.loads(EVIDENCE.read_text());nr=e['normalized_result'];assembly=nr['assembly'];pseudo=nr['pseudocode']
source=ROOT.parent/'csharp-reconstruction/CSharp.Source/RecNet.Interfaces.dll.partial.cs'
text=source.read_text(encoding='utf-8-sig');match=re.search(r'\benum Service\s*\{([^}]+)\}',text,re.S)
assert match is not None
block=re.sub(r'\[Token\([^\]]+\)\]','',match.group(1))
names=[x.strip() for x in block.split(',') if x.strip()]
assert len(names)==31 and all(re.fullmatch(r'[A-Za-z][A-Za-z0-9]*',x) for x in names)
db=sqlite3.connect((ROOT/'Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);db.row_factory=sqlite3.Row
fields={x['offset']:dict(x) for x in db.execute('SELECT id,name,type,offset FROM fields WHERE type_id=? AND is_static=0',(22164,))}
helper=db.execute('SELECT raw_json FROM methods WHERE id=?',(148123,)).fetchone();helper=json.loads(helper[0]);assert helper['parameters'][0]['type']=='RecNet.Service'
call_target=0x1894871b0
# Pseudocode and assembly are separate REA projections of the same source;
# neither is represented as an independent runtime observation.
pseudo_bindings={}
pattern=r'func_0x0001894871b0\((0x[0-9a-f]+|\d+),\*\(undefined8 \*\)\(lVar3 \+ (0x[0-9a-f]+|\d+)\),param_1 \+ 0xc,0\);'
for m in re.finditer(pattern,pseudo):pseudo_bindings[int(m.group(1),0)]=int(m.group(2),0)
assert len(pseudo_bindings)==31
rows=[]
for index,line in enumerate(assembly):
    if not line.endswith('CALL '+hex(call_target)):continue
    preceding=assembly[max(0,index-5):index]
    ids=[int(m.group(1),16) for ln in preceding for m in [re.search(r'MOV ECX, 0x([0-9a-f]+)$',ln)] if m]
    if any(ln.endswith('XOR ECX, ECX') for ln in preceding):ids.append(0)
    assert len(ids)==1
    service=ids[0]
    loads=[int(m.group(1),16) for ln in preceding for m in [re.search(r'MOV RDX, qword ptr \[RDI \+ 0x([0-9a-f]+)\]$',ln)] if m]
    if not loads:
        assert service==0 and any(x.endswith('MOV RDX, RSI') for x in preceding)
        candidates=[int(m.group(1),16) for ln in assembly[max(0,index-18):index] for m in [re.search(r'MOV RSI, qword ptr \[RDI \+ 0x([0-9a-f]+)\]$',ln)] if m]
        assert len(candidates)==1;loads=candidates
    assert len(loads)==1 and pseudo_bindings[service]==loads[0]
    f=fields[loads[0]];assert f['type']=='string'
    property_name=re.fullmatch(r'<(.+)>k__BackingField',f['name']).group(1)
    assert property_name==names[service]
    rows.append({'serviceId':service,'serviceName':names[service],'responseFieldOffset':loads[0],'responseFieldIndex':f['id'],'callAddress':line.split(':',1)[0],'nativeUpdaterAddress':hex(call_target),'nativeUpdaterMethodIndex':148123,'assemblyArgumentCheckPassed':True,'pseudocodeArgumentCheckPassed':True,'enumNameAndFieldNameAgree':True})
assert len(rows)==31 and {r['serviceId'] for r in rows}==set(range(31))
report={'status':'31_route_bindings_cross_checked','sourceMethodIndex':148260,'sourceEvidenceId':e['evidence_id'],'nativeHelperSignature':helper,'enumAssembly':'RecNet.Interfaces.dll','enumTypeIndex':46523,'enumSourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'responseTypeIndex':22164,'rows':sorted(rows,key=lambda r:r['serviceId']),'wireSchemaVerified':False,'endpointValuesCaptured':False,'networkRequestsMade':False,'limitations':['These are client-side service-ID to response-field bindings for the exact captured build.','They do not establish every service HTTP endpoint, schema, authentication requirement or server implementation.','The containing async procedure still has unresolved globals and excluded exception ranges; no full-function equivalence is claimed.']}
(OUT/'name-server-bindings.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n')
lines=['# Observed service-discovery bindings','','The native processing routine passes the following response fields to its typed `RecNet.Service` update helper. All 31 bindings agree between REA assembly, its pseudocode projection, recovered field layouts and enum declarations. This is not a tested wire/API schema.','','| Service ID | Recovered field | Object offset |','|---:|---|---:|']
lines += [f"| {r['serviceId']} | {r['serviceName']} | `0x{r['responseFieldOffset']:x}` |" for r in report['rows']]
lines += ['','Evidence: `'+e['evidence_id']+'`. No live URLs, account tokens or endpoint payloads were acquired.']
(OUT/'name-server-bindings.md').write_text('\n'.join(lines)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'status':report['status'],'evidenceId':e['evidence_id'],'services':[r['serviceName'] for r in report['rows']],'wireSchemaVerified':False}))
