"""Bounded original-client routing experiment, not a generic process injector.
Requires the exact approved build and locally generated SHA256 manifest. Actual
execution is explicit (--run), with separate Windows elevation. Existing game
processes are refused. Only the new owned process receives temporary hooks.
"""
from __future__ import annotations
import argparse,ctypes as C,datetime,hashlib,importlib.util,json,os,pathlib,struct,subprocess,sys,time,urllib.request
from ctypes import wintypes as W

def sha(path: pathlib.Path) -> str:
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def load_module(name: str,path: pathlib.Path):
    spec=importlib.util.spec_from_file_location(name,path)
    if not spec or not spec.loader:raise RuntimeError('Module unavailable')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
def utc()->str:return datetime.datetime.now(datetime.timezone.utc).isoformat()

def main()->int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--manifest',required=True,type=pathlib.Path)
    ap.add_argument('--run',action='store_true')
    ap.add_argument('--seconds',type=int,default=90)
    args=ap.parse_args()
    if os.name!='nt' or struct.calcsize('P')!=8 or not 10<=args.seconds<=180:raise ValueError('Windows x64 and 10-180 seconds required')
    manifest=json.loads(args.manifest.read_text(encoding='utf8'))
    if manifest['schemaVersion']!=1 or manifest['steamBuildId']!='23191908' or manifest['port']!=4187:raise ValueError('Unreviewed target or backend')
    root=pathlib.Path(manifest['labRoot']).resolve();game=pathlib.Path(manifest['gameRoot']).resolve()
    if root==game or game in root.parents:raise ValueError('Lab must be outside the installation')
    for record in manifest['files']:
        path=pathlib.Path(record['path']);actual=sha(path)
        if actual!=record['sha256']:raise ValueError('Pinned input changed: '+path.name)
    if sha(pathlib.Path(__file__))!=manifest['runnerSha256']:raise ValueError('Runner changed since review')
    sys.dont_write_bytecode=True
    recovery=load_module('verified_recovery_wrapper',pathlib.Path(manifest['recoveryWrapper']))
    capture=load_module('verified_capture_reader',pathlib.Path(manifest['captureHelper']))
    api=recovery.Win32();reader=capture.WindowsReader()
    if reader.processes():raise RuntimeError('An existing Rec Room process is running; it was not touched')
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open('http://127.0.0.1:4187/healthz',timeout=3)as response:health=json.load(response)
    if health.get('service')!='metallic-rc-backend':raise RuntimeError('Owned backend is unavailable')
    if not args.run:
        print(json.dumps({'status':'preflight_passed','gameWillNotRun':True,'elevated':api.is_elevated(),'backendInstance':health['instanceId'],'pinnedFiles':len(manifest['files'])}));return 0
    if not api.is_elevated():raise PermissionError('This exact-build controlled launch requires separately approved Windows elevation; no self-elevation is attempted')
    run=root/'runs'/datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ');run.mkdir(parents=True,exist_ok=False)
    # Protect private game logs and runtime receipts before any process starts.
    sid=subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip().split(',')[-1].strip('"')
    if not sid.startswith('S-1-5-'):raise RuntimeError('Could not resolve current user SID')
    acl=subprocess.run(['icacls',str(run),'/inheritance:r','/grant:r','*'+sid+':(OI)(CI)F'],capture_output=True,text=True)
    if acl.returncode:raise RuntimeError('Private run directory ACL could not be established')
    receipt={'startedUtc':utc(),'status':'preparing','steamBuildId':'23191908','backendInstance':health['instanceId'],'runDirectory':str(run),'observationSeconds':args.seconds,
             'gameFilesPatched':False,'machineDnsChanged':False,'certificateChecksBypassed':False,'authenticationChecksBypassed':False,'runtimeRoutingHookUsed':True,
             'existingProcessesTouched':False,'gameMayWriteOrdinaryLocalPreferencesAndCaches':True,'events':[],'cleanup':{},'errors':[]}
    def save():
        temp=run/'receipt.tmp';temp.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8');temp.replace(run/'receipt.json')
    save();mapping=None;device=None;pid=None;handle=None;session=None;script=None;result=1
    k=C.WinDLL('kernel32',use_last_error=True)
    k.OpenProcess.argtypes=[W.DWORD,W.BOOL,W.DWORD];k.OpenProcess.restype=C.c_void_p
    k.CloseHandle.argtypes=[C.c_void_p];k.CloseHandle.restype=W.BOOL
    k.WaitForSingleObject.argtypes=[C.c_void_p,W.DWORD];k.WaitForSingleObject.restype=W.DWORD
    k.TerminateProcess.argtypes=[C.c_void_p,W.UINT];k.TerminateProcess.restype=W.BOOL
    k.GetExitCodeProcess.argtypes=[C.c_void_p,C.POINTER(W.DWORD)];k.GetExitCodeProcess.restype=W.BOOL
    k.QueryFullProcessImageNameW.argtypes=[C.c_void_p,W.DWORD,W.LPWSTR,C.POINTER(W.DWORD)];k.QueryFullProcessImageNameW.restype=W.BOOL
    try:
        sys.path.insert(0,str(pathlib.Path(manifest['vendor'])));import frida
        if frida.__version__!='17.24.3':raise RuntimeError('Unexpected instrumentation version')
        mapping=api.create_new_section();receipt['sharedSectionCreated']=True;save()
        if reader.processes():raise RuntimeError('Game appeared during preflight; no client launched')
        device=frida.get_local_device()
        allowed=['SystemRoot','WINDIR','PATH','COMSPEC','TEMP','TMP','LOCALAPPDATA','APPDATA','USERPROFILE','HOMEDRIVE','HOMEPATH','ProgramData','ProgramFiles','ProgramFiles(x86)','PUBLIC','USERNAME','USERDOMAIN','NUMBER_OF_PROCESSORS','PROCESSOR_ARCHITECTURE']
        env={name:os.environ[name]for name in allowed if name in os.environ}
        command=[str(game/'RecRoom.exe'),'+forcemode:screen','-screen-fullscreen','0','-screen-width','1280','-screen-height','720','-logFile',str(run/'private-player.log')]
        pid=device.spawn(command,cwd=str(game),envp=env,stdio='pipe');receipt['ownedPid']=pid;receipt['command']=command
        handle=k.OpenProcess(0x00100000|0x1000|0x0001,False,pid)
        if not handle:raise C.WinError(C.get_last_error())
        size=W.DWORD(32768);path=C.create_unicode_buffer(size.value)
        if not k.QueryFullProcessImageNameW(handle,0,path,C.byref(size))or pathlib.Path(path.value).resolve()!=game/'RecRoom.exe':raise RuntimeError('Spawned image identity mismatch')
        receipt['retainedProcessHandleVerified']=True
        session=device.attach(pid)
        identity=json.loads(pathlib.Path(manifest['methodIdentities']).read_text())
        configuration={'port':manifest['port'],'buildId':manifest['steamBuildId'],'methods':identity['methods'],'fixtureOnly':False}
        code='const CONFIG='+json.dumps(configuration)+';\n'+pathlib.Path(manifest['policy']).read_text()+'\n'+pathlib.Path(manifest['agent']).read_text()
        def message(value,data):
            if len(receipt['events'])>=512:return
            if value.get('type')=='send':receipt['events'].append(value['payload'])
            elif value.get('type')=='error':receipt['errors'].append({'phase':'agent','description':str(value.get('description',''))[:200]})
        script=session.create_script(code);script.on('message',message);script.load()
        receipt['status']='observing_original_client';save();device.resume(pid)
        deadline=time.monotonic()+args.seconds;lastSave=0
        while time.monotonic()<deadline and k.WaitForSingleObject(handle,0)==258:
            if receipt['errors']or any(e.get('kind')=='agent_failure'for e in receipt['events']):break
            if time.monotonic()-lastSave>1:save();lastSave=time.monotonic()
            time.sleep(.1)
        try:receipt['agentStatus']=script.exports_sync.status()
        except Exception:receipt['agentStatusUnavailable']=True
        receipt['nativeDiscoveryParsed']=any(e.get('kind')=='native_discovery_deserialized'and e.get('localServiceFields')==31 and not e.get('statusNonempty')for e in receipt['events'])
        receipt['nativeRequestsObserved']=sum(e.get('kind')=='native_http_request'for e in receipt['events'])
        receipt['status']='native_discovery_verified'if receipt['nativeDiscoveryParsed']else'client_attempt_completed_no_discovery_proof'
        receipt['nativeLoginVerified']=False;receipt['nativeRoomJoinVerified']=False;result=0
    except Exception as error:
        receipt['status']='client_attempt_failed';receipt['errors'].append({'phase':'runner','type':type(error).__name__,'message':str(error)[:400]})
    finally:
        # The route/network guard stays installed until this owned process exits.
        if handle:
            try:
                if k.WaitForSingleObject(handle,0)==258:
                    receipt['cleanup']['windowCloseRequests']=api.request_window_close(pid)
                    if k.WaitForSingleObject(handle,3000)==258:
                        receipt['cleanup']['ownedProcessTerminated']=bool(k.TerminateProcess(handle,1));k.WaitForSingleObject(handle,5000)
                exitCode=W.DWORD();k.GetExitCodeProcess(handle,C.byref(exitCode));receipt['cleanup']['exitCode']=exitCode.value
            finally:k.CloseHandle(handle)
        elif pid is not None and device is not None:
            # Only the PID returned by this invocation's own spawn, before any unrelated PID lookup.
            try:device.kill(pid);receipt['cleanup']['spawnKilledBeforeHandle']=True
            except Exception:receipt['cleanup']['spawnCleanupUncertain']=True
        if script:
            try:script.unload()
            except Exception:pass
        if session:
            try:session.detach()
            except Exception:pass
        if mapping:receipt['cleanup']['sharedSectionHandleClosed']=bool(api.close(mapping))
        receipt['originalHashesAfter']=[{'file':record['path'],'matches':sha(pathlib.Path(record['path']))==record['sha256']}for record in manifest['files']if pathlib.Path(record['path']).parent==game]
        receipt['finishedUtc']=utc();save()
        print(json.dumps({'status':receipt['status'],'receipt':str(run/'receipt.json'),'nativeDiscoveryParsed':receipt.get('nativeDiscoveryParsed',False),'nativeRequestsObserved':receipt.get('nativeRequestsObserved',0),'nativeLoginVerified':False,'errors':receipt['errors']}),flush=True)
    return result
if __name__=='__main__':raise SystemExit(main())
