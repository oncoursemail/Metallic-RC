"""Explicitly exercise only an installed, identity-checked Metallic-RC backend.
The crash test requires --allow-crash-test and stops its verified dotnet child, not the task runner or any game.
"""
from __future__ import annotations
import argparse,datetime,hashlib,json,os,pathlib,subprocess,time,urllib.request

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,req,fp,code,msg,headers,newurl):return None

def main() -> None:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',required=True,type=pathlib.Path)
    p.add_argument('--launcher',required=True,type=pathlib.Path)
    p.add_argument('--output',required=True,type=pathlib.Path)
    p.add_argument('--allow-crash-test',action='store_true')
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    m=json.loads((a.root/'deployment.json').read_text());assert m['service']=='metallic-rc-backend' and m['taskName']=='Metallic-RC-Backend'
    origin='http://127.0.0.1:'+str(m['port']);opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    powershell=str(pathlib.Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe')
    def ps(command):
        p=subprocess.run([powershell,'-NoProfile','-NonInteractive','-Command',"$ErrorActionPreference='Stop';"+command],capture_output=True,text=True,timeout=20)
        if p.returncode:raise RuntimeError(p.stderr[-2000:])
        return p.stdout.strip()
    def request(path,method='GET',admin=False):
        headers={}
        if admin:headers['X-Metallic-Admin']=(pathlib.Path(m['dataDirectory'])/'admin.key').read_text().strip()
        with opener.open(urllib.request.Request(origin+path,method=method,headers=headers),timeout=4) as r:return json.load(r)
    def health():
        try:
            h=request('/healthz');assert h['service']=='metallic-rc-backend';return h
        except (OSError,ValueError,AssertionError):return None
    def wait_ready(previous=None,seconds=110):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            h=health()
            if h and h['instanceId']!=previous:return h
            time.sleep(.5)
        raise RuntimeError('Backend did not return healthy within the verification deadline.')
    start=wait_ready(seconds=10);runtime=json.loads((pathlib.Path(m['dataDirectory'])/'runtime.json').read_text())
    assert runtime['instanceId']==start['instanceId'] and runtime['port']==m['port']
    task=json.loads(ps("$t=Get-ScheduledTask -TaskName 'Metallic-RC-Backend';[pscustomobject]@{state=[string]$t.State;description=$t.Description;runLevel=[string]$t.Principal.RunLevel;restartCount=$t.Settings.RestartCount;restartInterval=$t.Settings.RestartInterval;atLogon=($t.Triggers.CimClass.CimClassName -contains 'MSFT_TaskLogonTrigger')}|ConvertTo-Json"))
    assert task['description']=='Metallic-RC owned loopback backend; local user only.' and task['state']=='Running' and task['restartCount']==0 and task['atLogon']
    results=[]
    for name,url,extra,expected in [('positive',origin+'/',[],0),('external-blocked','https://example.invalid/',[],2),('userinfo-blocked','http://x@127.0.0.1:'+str(m['port'])+'/',[],2),('path-blocked',origin+'/extra',[],2),('wrong-build',origin+'/', ['--build-id','other'],2)]:
        command=[m['dotnet'],str(a.launcher),'probe-backend',url,*extra]
        p=subprocess.run(command,capture_output=True,text=True,timeout=15);assert p.returncode==expected,(name,p.returncode)
        result=json.loads(p.stdout);assert result['reachable']==(expected==0)
        (a.output/(name+'.json')).write_text(json.dumps(result,indent=2)+'\n')
        results.append({'name':name,'exitCode':p.returncode,'reachable':result['reachable']})
    backup=request('/metallic/v1/admin/backups','POST',True)
    backup_path=pathlib.Path(m['dataDirectory'])/'backups'/backup['file']
    assert hashlib.sha256(backup_path.read_bytes()).hexdigest()==backup['sha256'] and backup['integrity']=='ok'
    request('/metallic/v1/admin/shutdown','POST',True)
    for _ in range(30):
        if health() is None and ps("[string](Get-ScheduledTask -TaskName 'Metallic-RC-Backend').State")=='Ready':break
        time.sleep(.5)
    else:raise RuntimeError('Authenticated graceful stop did not finish.')
    ps("Start-ScheduledTask -TaskName 'Metallic-RC-Backend'")
    restarted=wait_ready(start['instanceId'],30)
    crash=None
    if a.allow_crash_test:
        runtime=json.loads((pathlib.Path(m['dataDirectory'])/'runtime.json').read_text());assert runtime['instanceId']==restarted['instanceId']
        def quote(s):return "'"+str(s).replace("'","''")+"'"
        pid=int(runtime['processId']);assert pid>4
        command=(f"$p=Get-CimInstance Win32_Process -Filter 'ProcessId = {pid}';"
                 "if($null -eq $p -or $p.ExecutablePath -ne "+quote(m['dotnet'])+" -or -not $p.CommandLine.Contains("+quote(m['assembly'])+")){throw 'Backend child identity mismatch'};"
                 f"Stop-Process -Id {pid} -Force;")
        before=time.monotonic();ps(command)
        try:after=wait_ready(restarted['instanceId'],110)
        except Exception:
            # Do not leave this task-owned service down after a failed recovery check.
            ps("Start-ScheduledTask -TaskName 'Metallic-RC-Backend'")
            wait_ready(seconds=30)
            raise
        crash={'forcedExitOfVerifiedBackendChild':True,'runnerRecoveredWithoutManualStart':True,'seconds':round(time.monotonic()-before,3),'newInstance':after['instanceId']!=restarted['instanceId']}
    assert backup_path.exists() and hashlib.sha256(backup_path.read_bytes()).hexdigest()==backup['sha256']
    final=wait_ready(seconds=10)
    report={'status':'deployment_transport_and_recovery_passed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'launcherChecks':results,'task':task,
            'onlineBackupIntegrity':'ok','backupPreservedAcrossRestarts':True,'gracefulStopAndTaskRestartPassed':True,'crashRecovery':crash,'finalHealth':final,
            'publicListenerOpened':False,'originalGameExecuted':False,'originalClientLoginVerified':False}
    (a.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
    print(json.dumps(report))

if __name__=='__main__':main()
