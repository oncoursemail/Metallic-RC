"""Run full authored backend and pre-existing regression suites. No original game or external game APIs are used."""
from __future__ import annotations
import argparse,datetime,json,pathlib,subprocess,os,hashlib

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dotnet',required=True,type=pathlib.Path)
    parser.add_argument('--output',required=True,type=pathlib.Path)
    args=parser.parse_args()
    repository=pathlib.Path(__file__).resolve().parents[2]
    args.output.mkdir(parents=True,exist_ok=False)
    env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
    results=[]
    def run(name: str,command: list[str],timeout: int=180) -> None:
        result=subprocess.run(command,cwd=repository,env=env,capture_output=True,text=True,timeout=timeout)
        text=result.stdout+result.stderr;(args.output/(name+'.log')).write_text(text,encoding='utf-8')
        row={'name':name,'exitCode':result.returncode,'outputSha256':hashlib.sha256(text.encode()).hexdigest()};results.append(row);print(json.dumps(row),flush=True)
        if result.returncode: raise RuntimeError(name+' failed. Inspect its retained log; no passing result was fabricated.')
    dotnet=str(args.dotnet)
    run('locked-build',[dotnet,'build','Metallic.RC.slnx','-c','Release','-p:RestoreLockedMode=true','--nologo'])
    for project in ['Core','Platform']:
        run(project.lower()+'-regression',[dotnet,str(repository/f'tests/Metallic.RC.{project}.Tests/bin/Release/net10.0/Metallic.RC.{project}.Tests.dll')])
    run('compatibility-regression',[dotnet,str(repository/'tests/Metallic.RC.Compatibility.Tests/bin/Release/net10.0/Metallic.RC.Compatibility.Tests.dll'),'--self-test',str(args.output/'compatibility.json')])
    run('backend-real-transport',[dotnet,str(repository/'tests/Metallic.RC.Backend.Tests/bin/Release/net10.0/Metallic.RC.Backend.Tests.dll'),str(args.output/'backend.json')])
    report={'status':'full_local_backend_and_regressions_passed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'runs':results,
            'backend':json.loads((args.output/'backend.json').read_text()),'compatibility':json.loads((args.output/'compatibility.json').read_text()),'originalGameExecuted':False,'repositoryCIClaimed':False}
    (args.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':report['status'],'backendChecks':report['backend']['checks'],'actualRecRoomClientConnected':False}))

if __name__=='__main__':main()
