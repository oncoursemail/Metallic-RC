"""Create-only Windows local deployment. Never patches a game, opens a firewall, or replaces a task.
The optional logon task runs as the current interactive user at limited privilege.
"""
from __future__ import annotations
import argparse, datetime, hashlib, json, os, pathlib, shutil, socket, subprocess, sys, time, urllib.request

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dotnet', required=True, type=pathlib.Path)
    parser.add_argument('--root', type=pathlib.Path, default=pathlib.Path(os.environ.get('LOCALAPPDATA', '.'))/'Metallic-RC/backend')
    parser.add_argument('--port', type=int, default=4187)
    parser.add_argument('--autostart', action='store_true')
    args = parser.parse_args()
    if os.name != 'nt': raise RuntimeError('This installer is Windows-only; the backend executable is cross-platform.')
    if not args.dotnet.is_absolute() or not args.dotnet.is_file() or not args.root.is_absolute(): raise ValueError('Absolute .NET host and deployment paths are required.')
    if not 1024 <= args.port <= 65535: raise ValueError('Port must be 1024-65535.')
    if args.root.exists(): raise FileExistsError('Create-only deployment refused an existing root. Preserve it and use a reviewed upgrade procedure.')
    repository = pathlib.Path(__file__).resolve().parents[2]
    powershell = pathlib.Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'
    def ps(text: str, timeout: int=30) -> str:
        result=subprocess.run([str(powershell),'-NoProfile','-NonInteractive','-Command',"$ErrorActionPreference='Stop';"+text],capture_output=True,text=True,timeout=timeout)
        if result.returncode: raise RuntimeError('PowerShell operation failed: '+result.stderr[-4000:])
        return result.stdout.strip()
    task_name='Metallic-RC-Backend'
    if ps("if(Get-ScheduledTask -TaskName 'Metallic-RC-Backend' -ErrorAction SilentlyContinue){'exists'}else{'absent'}")!='absent':
        raise FileExistsError('Scheduled task already exists; not replacing it.')
    with socket.socket() as probe:
        if probe.connect_ex(('127.0.0.1',args.port))==0: raise OSError('Requested local port already has a listener.')
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    release=args.root/'releases'/stamp; release.mkdir(parents=True)
    logs=args.root/'logs'; logs.mkdir()
    publish=subprocess.run([str(args.dotnet),'publish',str(repository/'src/Metallic.RC.Backend/Metallic.RC.Backend.csproj'),'-c','Release','-r','win-x64','--self-contained','false','-p:RestoreLockedMode=true','-o',str(release),'--nologo'],cwd=repository,capture_output=True,text=True,timeout=180)
    (logs/'publish.log').write_text(publish.stdout+publish.stderr,encoding='utf-8')
    if publish.returncode: raise RuntimeError('Publish failed; no service/task was installed. See publish.log.')
    assembly=release/'Metallic.RC.Backend.dll'
    if not assembly.is_file(): raise FileNotFoundError('Publish did not produce the backend assembly.')
    files=[]
    for item in sorted(release.rglob('*')):
        if not item.is_file(): continue
        if item.parent!=release: raise RuntimeError('Unexpected nested publish files; stop for a manifest review.')
        files.append({'name':item.name,'sha256':hashlib.sha256(item.read_bytes()).hexdigest(),'bytes':item.stat().st_size})
    ops=args.root/'ops';ops.mkdir()
    for name in ['Start-LocalBackend.ps1','Stop-LocalBackend.ps1','Get-LocalBackendStatus.ps1','New-LocalAccount.ps1']:
        source=repository/'ops/backend'/name
        if source.exists(): shutil.copyfile(source,ops/name)
    source_files=sorted(list((repository/'src/Metallic.RC.Backend').glob('*.cs'))+list((repository/'src/Metallic.RC.Backend').glob('*.csproj'))+list((repository/'src/Metallic.RC.Compatibility').glob('*.cs')))
    source_hash=hashlib.sha256(b'\n'.join(str(p.relative_to(repository)).replace('\\','/').encode()+b'\0'+p.read_bytes() for p in source_files)).hexdigest()
    manifest={'service':'metallic-rc-backend','schemaVersion':1,'port':args.port,'dotnet':str(args.dotnet),'assembly':str(assembly),'assemblySha256':hashlib.sha256(assembly.read_bytes()).hexdigest(),
              'files':files,'dataDirectory':str(args.root/'data'),'logDirectory':str(logs),'taskName':task_name,'sourceBaseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repository,text=True).strip(),
              'sourceContentSha256':source_hash,'createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'nativeClientVerified':False}
    manifest_path=args.root/'deployment.json';manifest_path.write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    def quoted(value: object) -> str: return "'"+str(value).replace("'","''")+"'"
    runner=ops/'Start-LocalBackend.ps1'
    ps("$tokens=$null;$errors=$null;[System.Management.Automation.Language.Parser]::ParseFile("+quoted(runner)+",[ref]$tokens,[ref]$errors)|Out-Null;if($errors.Count){throw ($errors|Out-String)}")
    if args.autostart:
        arguments=f'-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File "{runner}" -ManifestPath "{manifest_path}"'
        script=("$user=[System.Security.Principal.WindowsIdentity]::GetCurrent().Name;"
                "$action=New-ScheduledTaskAction -Execute "+quoted(powershell)+" -Argument "+quoted(arguments)+" -WorkingDirectory "+quoted(release)+";"
                "$trigger=New-ScheduledTaskTrigger -AtLogOn -User $user;"
                "$principal=New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited;"
                "$settings=New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew;"
                "Register-ScheduledTask -TaskName 'Metallic-RC-Backend' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Description 'Metallic-RC owned loopback backend; local user only.'|Out-Null;"
                "Start-ScheduledTask -TaskName 'Metallic-RC-Backend';")
        ps(script)
    else:
        subprocess.Popen([str(powershell),'-NoProfile','-NonInteractive','-WindowStyle','Hidden','-ExecutionPolicy','RemoteSigned','-File',str(runner),'-ManifestPath',str(manifest_path)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));health=None
    for _ in range(45):
        try:
            with opener.open(f'http://127.0.0.1:{args.port}/healthz',timeout=1) as response: health=json.load(response)
            if health.get('service')=='metallic-rc-backend': break
        except (OSError,ValueError): pass
        time.sleep(1)
    if not health or health.get('service')!='metallic-rc-backend': raise RuntimeError('Deployment did not become healthy. Preserve logs; do not report it running.')
    report={'status':'deployed_and_listening','address':f'http://127.0.0.1:{args.port}','instanceId':health['instanceId'],'logonTaskCreated':args.autostart,'privilege':'limited-current-user',
            'restartPolicy':'runner: at most 5 retries per 10-minute failure window, 2-30 second backoff; zero exit stops intentionally','autoStartWithoutUserLogon':False,'nativeClientVerified':False,'sourceContentSha256':source_hash,'publishFileCount':len(files)}
    (args.root/'installation-result.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report))

if __name__=='__main__': main()
