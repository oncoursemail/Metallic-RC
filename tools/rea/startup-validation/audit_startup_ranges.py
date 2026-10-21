from pathlib import Path
import json,subprocess
ROOT=Path(__file__).resolve().parent;WORK=ROOT.parent
j=json.loads((ROOT/'rea-analysis-summary.json').read_text());rows=[]
for entry in j['results']:
 assert entry['status']=='dossier_retained'
 m=entry['typedMetadataSignature'];start=int(m['nativeRva'],16);unwind=m['range'];assert start==unwind['start_rva']
 end=max([unwind['end_rva']]+[int(x['end'],16)+1-0x180000000 for x in entry['analyzedBody']['ranges']])
 assert end-start<100000
 rows.append({'methodIndex':m['methodIndex'],'rva':start,'length':end-start,'reaRanges':entry['analyzedBody']['ranges'],'rangeBasis':'Union extent of the metadata entry unwind record and REA reported inclusive body; no extra metadata functions claimed.'})
p=ROOT/'selected-startup-ranges.json';p.write_text(json.dumps({'methods':rows},indent=2)+'\n',encoding='utf-8',newline='\n')
checker=WORK/'deep-rea-20261021/SelectedInstructionAudit/bin/Release/net10.0/SelectedInstructionAudit.dll';assert checker.is_file()
source=WORK/'runs/recovery-/20261020T212503.738773Z/decoded/GameAssembly.code-restored.analysis.dll'
command=[r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe',str(checker),str(source),str(p),str(ROOT/'full-startup-disassembly')]
r=subprocess.run(command,capture_output=True,text=True,timeout=90);(ROOT/'full-startup-audit.log').write_text(r.stdout+r.stderr,encoding='utf-8');print(r.stdout,flush=True);assert r.returncode==0,r.stderr
