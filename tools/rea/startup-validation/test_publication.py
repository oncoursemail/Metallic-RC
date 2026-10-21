"""Build the exact publication copies and compare them to retained native results."""
from pathlib import Path
import subprocess,json,hashlib,os,datetime
ROOT=Path(__file__).resolve().parent
REPO=ROOT.parent/'deep-rea-20261021/publication-source';TOOLS=REPO/'tools/rea/startup-validation'
OUT=ROOT/'publication-tests';OUT.mkdir(exist_ok=False)
DOT=r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe'
env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
def run(args,name):
 p=subprocess.run([DOT,*args],capture_output=True,text=True,timeout=120,env=env)
 (OUT/(name+'.log')).write_text(p.stdout+p.stderr,encoding='utf-8');assert p.returncode==0,p.stdout+p.stderr
 return p.stdout
checks=[]
for folder,project,fixtures,expected,keys in [('BindingProbe','BindingProbe',ROOT/'binding-differential/cases.jsonl',ROOT/'binding-differential/native-emulated.jsonl',('id','status','trace')),('ManagedProbe','ManagedProbe',ROOT/'codec-recheck/cases.jsonl',ROOT/'codec-recheck/native-emulated.jsonl',('id','status','value','bytesHex'))]:
 run(['build',str(TOOLS/folder/(project+'.csproj')),'-c','Release','--nologo'],folder+'-build')
 program=TOOLS/folder/'bin/Release/net10.0'/(project+'.dll');assert program.is_file()
 result=OUT/(folder+'.jsonl');run([str(program),str(fixtures),str(result)],folder+'-run')
 source=[json.loads(x) for x in expected.read_text().splitlines()];actual=[json.loads(x) for x in result.read_text().splitlines()]
 assert len(source)==len(actual)
 bad=[i for i,(a,b) in enumerate(zip(source,actual)) if any(a[k]!=b[k] for k in keys)];assert not bad,bad[:5]
 checks.append({'project':folder,'cases':len(actual),'mismatches':0,'sourceComparison':'Retained bounded native-emulation output, with the documented modeled boundaries.','publishedSourceHashes':[{ 'file':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in (TOOLS/folder).iterdir() if p.is_file()]})
result=OUT/'service-model.json'
run(['run','--project',str(TOOLS/'ObservedServices/ObservedServices.csproj'),'-c','Release','--',str(result)],'ObservedServices-build-and-run')
service=json.loads(result.read_text());assert service['checks']==6045 and service['cases']==2037
summary={'status':'publication_sources_built_and_verified','recordedUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeComparisonChecks':checks,'serviceModel':service,'gameAssembliesLoaded':False,'fullGameCompilationPerformed':False}
(OUT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
(REPO/'docs/evidence/rea-startup-validation-20261021/publication-validation.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'status':summary['status'],'nativeComparisonCases':sum(x['cases'] for x in checks),'mismatches':0,'serviceModelAssertions':service['checks'],'publicationProjects':3}),flush=True)
