from pathlib import Path
import sqlite3,json,hashlib,struct,re,subprocess
ROOT=Path(__file__).resolve().parent;W=ROOT.parent/'rework-20261020/rea-steam-decoding';O=W/'native-login-rea-20261021'
D=W/'runs/recovery-/20261020T212503.738773Z/decoded';DB=sqlite3.connect((W/'deep-rea-20261021/Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);DB.row_factory=sqlite3.Row
ids=[149600,149616,149614,149610,126718,126719,126720,149472]
rows=[]
for mid in ids:
    m=DB.execute('SELECT id,rva FROM methods WHERE id=?',(mid,)).fetchone();n=DB.execute('SELECT * FROM native_ranges WHERE start_rva=?',(m['rva'],)).fetchone();assert n
    rows.append({'methodIndex':mid,'rva':m['rva'],'length':n['end_rva']-n['start_rva'],'reaRanges':[],'rangeBasis':'Exact metadata entry first compiler-unwind range. Additional async/exception ranges not included in this initial instruction survey.'})
p=O/'initial-instruction-selections.json';p.write_text(json.dumps({'methods':rows},indent=2)+'\n')
checker=W/'deep-rea-20261021/SelectedInstructionAudit/bin/Release/net10.0/SelectedInstructionAudit.dll'
result=subprocess.run([str(ROOT.parent/'tooling/dotnet-10.0.402/dotnet.exe'),str(checker),str(D/'GameAssembly.code-restored.analysis.dll'),str(p),str(O/'initial-instructions')],capture_output=True,text=True,timeout=60)
(O/'initial-instruction-audit.log').write_text(result.stdout+result.stderr);assert result.returncode==0,result.stdout+result.stderr
b=(D/'GameAssembly.code-restored.analysis.dll').read_bytes();assert hashlib.sha256(b).hexdigest()=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
meta=(D/'global-metadata.v31.normalized.dat').read_bytes();roles=json.loads((D/'metadata-normalization.json').read_text())['headerMapping'];sections=json.loads((D.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections']
def literal(address):
    rva=address-0x180000000
    for s in sections:
        if s['rva']<=rva and rva+8<=s['rva']+s['rawSize']:
            tag=struct.unpack_from('<Q',b,s['rawOffset']+rva-s['rva'])[0]
            if tag>>29!=5 or not tag&1:return None
            index=(tag&0x1ffffffe)>>1;q=roles['stringLiteral'];n,o=struct.unpack_from('<II',meta,q['offset']+index*8);q=roles['stringLiteralData'];assert o+n<=q['size']
            return {'literalIndex':index,'value':meta[q['offset']+o:q['offset']+o+n].decode('utf8')}
    return None
summary=[]
for mid in ids:
    instructions=json.loads((O/f'initial-instructions/m{mid}.instructions.json').read_text());annotated=[];refs=[]
    for row in instructions:
        line=row['address']+': '+row['assembly'];notes=[]
        for match in re.finditer(r'(?:0x)?(1[89][0-9A-Fa-f]{7,9})h?',row['assembly']):
            address=int(match.group(1),16);value=literal(address)
            if value:
                notes.append(json.dumps(value['value'],ensure_ascii=True));refs.append({'instruction':row['address'],'slot':hex(address),**value})
        annotated.append(line+(' ; LITERAL '+', '.join(notes)if notes else''))
    (O/f'initial-instructions/m{mid}.annotated.txt').write_text('\n'.join(annotated)+'\n',encoding='utf8',newline='\n')
    summary.append({'methodIndex':mid,'literalReferences':refs})
(O/'login-literal-references.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print(result.stdout,flush=True);print(json.dumps(summary),flush=True)
