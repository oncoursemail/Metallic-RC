"""Verify the exact solution, short-path identical regression binaries and CLI.
No acceptance test is skipped or weakened for Windows path length.
"""
from pathlib import Path
import subprocess,json,hashlib,os,datetime,shutil,uuid
R=Path(__file__).resolve().parent;REPO=R.parent/'deep-rea-20261021/publication-source';DOTNET=r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe'
O=R/'final-validation';O.mkdir(exist_ok=False);env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
results=[]
def sha(p):
 with p.open('rb')as f:return hashlib.file_digest(f,'sha256').hexdigest()
def run(name,args,expected=0,timeout=180):
 p=subprocess.run(args,cwd=REPO,env=env,capture_output=True,text=True,timeout=timeout);(O/(name+'.log')).write_text(p.stdout+p.stderr,encoding='utf-8',newline='\n');row={'name':name,'exitCode':p.returncode,'expectedExitCode':expected,'lastLines':p.stdout.splitlines()[-5:]};results.append(row);assert p.returncode==expected,p.stdout+p.stderr;print(json.dumps(row),flush=True);return p
# One ordinary restore admits only the new project reference; then locked mode.
run('restore-added-project',[DOTNET,'restore',str(REPO/'Metallic.RC.slnx'),'--nologo'])
run('solution-locked-build',[DOTNET,'build',str(REPO/'Metallic.RC.slnx'),'-c','Release','-p:RestoreLockedMode=true','--nologo'])
short=Path(r'C:\Users\alexi\AppData\Local\Metallic-RC')/('qa-sc-'+uuid.uuid4().hex[:10]);short.mkdir(exist_ok=False);copies=[]
for name in ['Core','Platform']:
 source=REPO/f'tests/Metallic.RC.{name}.Tests/bin/Release/net10.0';dest=short/name
 shutil.copytree(source,dest,ignore=shutil.ignore_patterns('acceptance-*'))
 for p in source.rglob('*'):
  if p.is_file()and not any(x.startswith('acceptance-')for x in p.relative_to(source).parts):
   d=dest/p.relative_to(source);assert sha(p)==sha(d);copies.append({'suite':name,'file':p.relative_to(source).as_posix(),'sha256':sha(p)})
 run(name.lower()+'-regression',[DOTNET,str(dest/f'Metallic.RC.{name}.Tests.dll')])
program=REPO/'tests/Metallic.RC.Compatibility.Tests/bin/Release/net10.0/Metallic.RC.Compatibility.Tests.dll'
run('compatibility-selftests',[DOTNET,str(program),'--self-test',str(O/'selftests.json')])
run('formatter-comparison',[DOTNET,str(program),str(R/'contract-differential/cases.jsonl'),str(O/'managed-results.jsonl')])
native=[json.loads(x)for x in(R/'contract-differential/native-results.jsonl').read_text().splitlines()];managed=[json.loads(x)for x in(O/'managed-results.jsonl').read_text().splitlines()];assert len(native)==len(managed)==2474
for n,m in zip(native,managed):
 assert n['id']==m['id']and n['status']==m['status']
 if n['operation']=='read':assert n['fields']==m['fields']
 else:assert json.loads(n['json'],object_pairs_hook=lambda v:v)==json.loads(m['json'],object_pairs_hook=lambda v:v)
cli=REPO/'src/Metallic.RC.Launcher/bin/Release/net10.0/Metallic.RC.Launcher.dll';names=json.loads((R/'StartupContract/contract-manifest.json').read_text())['canonicalNames'];valid={name:(None if name=='RecNetStatus'else'https://'+name.lower()+'.example.invalid')for name in names}
fixtures=O/'cli-fixtures';fixtures.mkdir();cli_checks=[]
cases=[('valid',valid,0,'supported_contract_profile'),('empty',{},2,'missing_response'),('null',None,2,'missing_response'),('nonempty-status',{**valid,'RecNetStatus':'OK'},2,'nonempty_status'),('missing-auth',{**valid,'Auth':None},2,'invalid_service_address'),('bad-uri',{**valid,'Auth':'not-an-absolute-uri'},2,'invalid_service_uri'),('wrong-type',{**valid,'Auth':42},2,'invalid_json_or_value_type'),('privacy',{**valid,'Auth':'https://private-user:private-pass@auth.example.invalid'},0,'supported_contract_profile')]
for name,data,exit_code,reason in cases:
 path=fixtures/(name+'.json');path.write_text(json.dumps(data),encoding='utf-8',newline='\n');state=fixtures/('must-not-create-'+name)
 p=run('cli-'+name,[DOTNET,str(cli),'check-discovery',str(path),'--state-dir',str(state)],exit_code)
 output=json.loads(p.stdout);assert output['reason']==reason and not output['networkAttempted']and not output['cacheWritten']and not output['gameLaunched'];assert not state.exists();assert '.example.invalid'not in p.stdout and 'private-pass'not in p.stdout
 cli_checks.append({'case':name,'valid':output['valid'],'reason':reason,'exitCode':exit_code,'stateDirectoryAbsent':True,'addressesNotPrinted':True})
p=run('cli-unsupported-build',[DOTNET,str(cli),'check-discovery',str(fixtures/'valid.json'),'--build-id','unknown'],2);assert json.loads(p.stdout)['reason']=='unsupported_build';cli_checks.append({'case':'unsupported-build','exitCode':2})
p=run('cli-help',[DOTNET,str(cli),'help']);assert 'check-discovery' in p.stdout;cli_checks.append({'case':'help','exitCode':0})
hashes=[]
for kind,base in [('installed',Path(r'C:\Program Files (x86)\Steam\steamapps\common\RecRoom')),('fresh',Path(r'C:\Program Files (x86)\Steam\steamapps\content\app_471710\depot_471711'))]:
 for name,expected in [('RecRoom.exe','3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70'),('GameAssembly.dll','a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84')]:
  actual=sha(base/name);assert actual==expected;hashes.append({'copy':kind,'file':name,'sha256':actual,'unchanged':True})
files=[]
for folder in [REPO/'src/Metallic.RC.Compatibility',REPO/'tests/Metallic.RC.Compatibility.Tests']:
 for p in sorted(folder.iterdir()):
  if p.is_file():files.append({'path':p.relative_to(REPO).as_posix(),'sha256':sha(p),'bytes':p.stat().st_size})
for name in ['Program.cs','DiscoveryCheck.cs','Metallic.RC.Launcher.csproj','packages.lock.json']:
 p=REPO/'src/Metallic.RC.Launcher'/name;files.append({'path':p.relative_to(REPO).as_posix(),'sha256':sha(p),'bytes':p.stat().st_size})
summary={'status':'integrated_startup_contract_verified','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'formatterComparisonCases':len(native),'formatterMismatches':0,'selfTests':json.loads((O/'selftests.json').read_text()),'cliChecks':cli_checks,'checks':results,'sourceFiles':files,'originalHashes':hashes,'regressionRelocation':{'reason':'Existing Core fixture creates junctions beneath AppContext.BaseDirectory. Deep worktree exceeded Windows mklink path length; identical compiled binaries were copied to this short task-owned folder, without skipping tests.','path':str(short),'copiedFiles':len(copies),'allHashesMatch':True},'liveClientTested':False,'gameRedirected':False,'fullGameBuild':False}
(O/'short-path-binary-manifest.json').write_text(json.dumps(copies,indent=2)+'\n',encoding='utf-8',newline='\n');(O/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps({'status':summary['status'],'formatterComparisonCases':len(native),'formatterMismatches':0,'selfTestAssertions':summary['selfTests']['checks'],'cliChecks':len(cli_checks),'originalHashes':len(hashes)}),flush=True)
