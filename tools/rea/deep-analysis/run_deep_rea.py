"""Use actual REA MCP to produce per-function evidence from hash-bound pages."""
from pathlib import Path
import json,hashlib,time,datetime,traceback
from rea_session import ReaClient
ROOT=Path(__file__).resolve().parent
OUT=ROOT/'rea-dossiers';OUT.mkdir(exist_ok=False)
SCOPES=json.loads((ROOT/'scoped-targets.json').read_text())
def evidence_records(obj):
    found=[]
    def visit(x):
        if isinstance(x,dict):
            if 'evidence_id' in x and 'normalized_result' in x:found.append(x);return
            for k,v in x.items():
                if k=='text' and isinstance(v,str):
                    try:visit(json.loads(v))
                    except (ValueError,TypeError):pass
                else:visit(v)
        elif isinstance(x,list):
            for y in x:visit(y)
    visit(obj)
    return list({e['evidence_id']:e for e in found}.values())
all_results=[]
for scope in SCOPES:
    name=scope['group'];directory=OUT/name;directory.mkdir()
    target=Path(scope['path']);assert hashlib.sha256(target.read_bytes()).hexdigest()==scope['sha256']
    provenance=json.loads((ROOT/'scoped-targets'/scope['provenance']).read_text())
    print(json.dumps({'scope':name,'phase':'opening'}),flush=True)
    with ReaClient(directory) as c:
        opened=c.tool('open_binary',{'path':str(target),'provider_id':'ghidra'},'open.json',60)
        if 'error' in opened or opened.get('result',{}).get('isError'):raise RuntimeError('REA rejected '+name)
        for method in provenance['selectedMethods']:
            mid=method['methodIndex'];addr=hex(0x180000000+int(method['nativeRva'],16));started=time.monotonic()
            row={'scope':name,'methodIndex':mid,'method':method['declaringType']+'::'+method['name'],'typedMetadataSignature':method,'address':addr,'syntheticSourcePageCarrier':str(target),'gameExecuted':False}
            try:
                response=c.tool('analyze_function',{'procedure':addr},f'm{mid}.mcp.json',240)
                records=evidence_records(response)
                dossiers=[e for e in records if e.get('operation')=='analyze_function']
                if not dossiers:
                    row.update(status='no_inline_dossier',responseFile=f'm{mid}.mcp.json')
                else:
                    ev=dossiers[-1];nr=ev['normalized_result'];pseudo=nr.get('pseudocode') or ''
                    c.save(f'm{mid}.evidence.json',ev)
                    (directory/f'm{mid}.pseudocode.c').write_text('// REA/Ghidra observation, NOT original C# source. Scope restrictions are in the adjacent JSON.\n'+pseudo,encoding='utf-8',newline='\n')
                    (directory/f'm{mid}.assembly.txt').write_text('\n'.join(nr.get('assembly',[]))+'\n',encoding='utf-8')
                    warnings=[line.strip() for line in pseudo.splitlines() if 'WARNING' in line or 'halt_baddata' in line]
                    body=nr.get('procedure',{}).get('body',{})
                    expected=method.get('range');source_bytes=(expected['end_rva']-expected['start_rva']) if expected else None
                    actual=body.get('total_bytes');matches=actual==source_bytes if source_bytes else None
                    row.update(status='dossier_retained',evidenceId=ev['evidence_id'],provider=ev.get('provider'),pseudocodeCharacters=len(pseudo),assemblyInstructions=len(nr.get('assembly',[])),analyzedBody=body,sourceUnwindBodyBytes=source_bytes,bodyByteCountMatchesUnwind=matches,warnings=warnings,callers=len(nr.get('callers',[])),callees=len(nr.get('callees',[])),unresolvedCalls=len(nr.get('unresolved_calls',[])),blocks=len(nr.get('basic_blocks',[])),inferredSignature=nr.get('procedure',{}).get('signature'),evidenceFile=f'{name}/m{mid}.evidence.json')
            except Exception as e:
                row.update(status='failed',error=str(e));c.save(f'm{mid}.failure.json',{'error':str(e),'trace':traceback.format_exc()})
            row['seconds']=round(time.monotonic()-started,3);all_results.append(row)
            (ROOT/'rea-analysis-summary.json').write_text(json.dumps({'status':'in_progress','results':all_results,'allOriginalSourceRecovered':False},indent=2)+'\n',encoding='utf-8',newline='\n')
            print(json.dumps({k:row.get(k) for k in ('scope','methodIndex','status','assemblyInstructions','bodyByteCountMatchesUnwind','seconds','error')}),flush=True)
        c.tool('export_evidence_bundle',{'path':str(directory/'canonical-bundle.json'),'overwrite':False},'export.json',60)
    print(json.dumps({'scope':name,'phase':'closed'}),flush=True)
summary={'status':'completed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'selectedFunctions':len(all_results),'retainedDossiers':sum(x['status']=='dossier_retained' for x in all_results),'scopes':len(SCOPES),'results':all_results,'allOriginalSourceRecovered':False,'limits':['Generated headers/export labels provide analysis structure; source instruction pages are SHA256-bound to recovered native code.','Omitted dependencies and globals can affect decompiler signatures and control-flow conclusions.','Body byte-count equality is a consistency check, not proof of semantic equivalence.','Failures, warnings and truncated ranges remain explicit.']}
(ROOT/'rea-analysis-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({k:summary[k] for k in ('status','selectedFunctions','retainedDossiers','scopes')}),flush=True)
