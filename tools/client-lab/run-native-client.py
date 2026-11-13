"""Exact-build native-client experiment with separate process supervision.
--seconds is the TOTAL worker budget (startup + observation), not an observation
window that starts after an unbounded attach. UAC waiting is outside this budget.
No security/authentication bypass, automatic retry, persistent hooks, or game-file edits.
"""
from __future__ import annotations
import argparse,datetime,hashlib,importlib.util,json,os,pathlib,struct,subprocess,sys,time,types,urllib.request

ROOT=pathlib.Path(__file__).resolve().parent
GAME=pathlib.Path(r'C:\Program Files (x86)\Steam\steamapps\common\RecRoom')
EXPECTED={'RecRoom.exe':'3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70','GameAssembly.dll':'a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84'}

def sha(path):
    with pathlib.Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def source_module(name,path):
    module=types.ModuleType(name);module.__file__=str(path);sys.modules[name]=module
    exec(compile(pathlib.Path(path).read_bytes(),str(path),'exec'),module.__dict__);return module

def validate(path):
    manifest=json.loads(path.read_text(encoding='utf8'))
    if manifest.get('schemaVersion')!=3 or manifest.get('steamBuildId')!='23191908' or manifest.get('port')!=4187:raise ValueError('Manifest requires reviewed v3 profile for build 23191908')
    if pathlib.Path(manifest['gameRoot']).resolve()!=GAME:raise ValueError('Unreviewed game installation')
    root=pathlib.Path(manifest['labRoot']).resolve()
    if root==GAME or GAME in root.parents:raise ValueError('Lab must be outside game installation')
    records={}
    for record in manifest['files']:
        p=pathlib.Path(record['path']).resolve();key=os.path.normcase(str(p))
        if key in records:raise ValueError('Duplicate manifest path')
        if sha(p)!=record['sha256'] or p.stat().st_size!=record['bytes']:raise ValueError('Pinned input changed: '+p.name)
        records[key]=record
    required=[ROOT/'run-native-client.py',ROOT/'supervision.py',ROOT/'progress_store.py',ROOT/'winjob.py',pathlib.Path(manifest['agent']),pathlib.Path(manifest['policy']),pathlib.Path(manifest['methodIdentities']),pathlib.Path(manifest['recoveryWrapper']),pathlib.Path(manifest['captureHelper'])]
    required += [GAME/name for name in EXPECTED]
    required += [pathlib.Path(manifest['vendor'])/'frida'/name for name in ['__init__.py','aio.py','_frida.pyd']]
    if any(os.path.normcase(str(p.resolve())) not in records for p in required):raise ValueError('Required executable input missing from manifest')
    if sha(__file__)!=manifest['runnerSha256']:raise ValueError('Runner changed since review')
    for name,expected in EXPECTED.items():
        if records[os.path.normcase(str(GAME/name))]['sha256']!=expected:raise ValueError('Original-build identity changed')
    sys.dont_write_bytecode=True;sys.pycache_prefix=str(root/'disabled-bytecode-cache')
    return manifest

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):return None

def backend_health():
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    with opener.open('http://127.0.0.1:4187/healthz',timeout=3) as response:
        if response.status!=200:raise RuntimeError('Backend unavailable')
        raw=response.read(4097)
        if len(raw)>4096:raise ValueError('Unexpected health response')
        health=json.loads(raw)
    if health.get('service')!='metallic-rc-backend':raise RuntimeError('Owned backend identity mismatch')
    return health

def private_folder(path):
    path.mkdir(parents=True,exist_ok=False)
    system=pathlib.Path(os.environ['SystemRoot'])/'System32'
    sid=subprocess.check_output([str(system/'whoami.exe'),'/user','/fo','csv','/nh'],text=True,timeout=3).strip().split(',')[-1].strip('"')
    if not sid.startswith('S-1-5-'):raise RuntimeError('User identity unavailable')
    result=subprocess.run([str(system/'icacls.exe'),str(path),'/inheritance:r','/grant:r','*'+sid+':(OI)(CI)F'],capture_output=True,text=True,timeout=3)
    if result.returncode:raise RuntimeError('Private evidence directory ACL failed')

def modules(manifest):
    source_module('progress_store',ROOT/'progress_store.py')
    win=source_module('winjob',ROOT/'winjob.py');control=source_module('supervision',ROOT/'supervision.py')
    recovery=source_module('verified_recovery_wrapper',pathlib.Path(manifest['recoveryWrapper']))
    capture=source_module('verified_capture_reader',pathlib.Path(manifest['captureHelper']))
    return win,control,recovery,capture

def worker(args,manifest):
    win,control,recovery,capture=modules(manifest)
    if not win.current_job_membership(args.job):raise RuntimeError('Worker is not in the supervising job; no game launch')
    run=pathlib.Path(args.run_dir).resolve();lab=pathlib.Path(manifest['labRoot']).resolve()
    if run.parent!=lab/'runs' or not run.is_dir():raise ValueError('Unexpected supervisor run directory')
    sys.stdout=(run/'worker.stdout.log').open('w',encoding='utf8',buffering=1)
    sys.stderr=(run/'worker.stderr.log').open('w',encoding='utf8',buffering=1)
    ledger=control.Ledger(run/'receipt.json',args.started,args.deadline,args.startup_deadline,{
        'steamBuildId':'23191908','runDirectory':str(run),'gameFilesPatched':False,'machineDnsChanged':False,'certificateChecksBypassed':False,'authenticationChecksBypassed':False,'existingProcessesTouched':False,
        'jobMembershipVerified':True,'gameMayWriteOrdinaryLocalPreferencesAndCaches':True})
    state=ledger.state;api=recovery.Win32();reader=capture.WindowsReader();mapping=None;game=None;session=None;script=None;frida=None;result=1
    try:
        if not api.is_elevated():raise PermissionError('Separate Windows approval required')
        if reader.processes():raise RuntimeError('Existing Rec Room process; no launch')
        state['backendInstance']=backend_health()['instanceId']
        ledger.phase('instrumentation_import',8)
        sys.path.insert(0,str(pathlib.Path(manifest['vendor'])));import frida
        if frida.__version__!='17.24.3':raise RuntimeError('Unexpected instrumentation version')
        ledger.completed()
        mapping=api.create_new_section();state['sharedSectionCreated']=True;ledger.save()
        if reader.processes():raise RuntimeError('Existing game appeared before controlled launch')
        device=ledger.call(frida,'device',8,frida.get_local_device)
        command=[str(GAME/'RecRoom.exe'),'+forcemode:screen','-screen-fullscreen','0','-screen-width','1280','-screen-height','720','-logFile',str(run/'private-player.log')]
        allowed=['SystemRoot','WINDIR','PATH','COMSPEC','TEMP','TMP','LOCALAPPDATA','APPDATA','USERPROFILE','HOMEDRIVE','HOMEPATH','ProgramData','ProgramFiles','ProgramFiles(x86)','PUBLIC','USERNAME','USERDOMAIN','NUMBER_OF_PROCESSORS','PROCESSOR_ARCHITECTURE']
        env={name:os.environ[name] for name in allowed if name in os.environ}
        pid=ledger.call(frida,'spawn',12,lambda:device.spawn(command,cwd=str(GAME),envp=env,stdio='pipe'))
        state['ownedPid']=pid;state['command']=command;ledger.save()
        game=win.retain_process(pid)
        if pathlib.Path(game.image()).resolve()!=GAME/'RecRoom.exe':raise RuntimeError('Spawned process image mismatch')
        if not win.current_job_membership(args.job,game,assign=True):raise RuntimeError('Game process containment failed')
        state['retainedProcessHandleVerified']=True;state['gameJobMembershipVerified']=True
        try:state['beforeAttach']=game.diagnostics()
        except Exception as error:state['beforeAttach']={'diagnosticQueryError':type(error).__name__}
        state['attachmentAttempted']=True;ledger.save()
        session=ledger.call(frida,'attach',15,lambda:device.attach(pid))
        state['attached']=True;ledger.save()
        identity=json.loads(pathlib.Path(manifest['methodIdentities']).read_text())
        config={'port':4187,'buildId':'23191908','methods':identity['methods'],'fixtureOnly':False}
        code='const CONFIG='+json.dumps(config)+';\n'+pathlib.Path(manifest['policy']).read_text()+'\n'+pathlib.Path(manifest['agent']).read_text()
        def message(value,data):
            if value.get('type')=='send':ledger.event(value.get('payload'))
            elif value.get('type')=='error':ledger.event({'kind':'agent_failure','scope':'script','error':str(value.get('description',''))[:200]})
        script=ledger.call(frida,'create_script',5,lambda:session.create_script(code));script.on('message',message)
        ledger.call(frida,'load_script',5,script.load)
        ready=ledger.call(frida,'status_rpc',3,lambda:script.exports_sync.status())
        if ready.get('stopped') or not ready.get('networkGuards') or not state['agentLoaded']:
            raise RuntimeError('Network guard or agent readiness not established; game will NOT resume')
        ledger.call(frida,'resume',5,lambda:device.resume(pid))
        state['processResumed']=True;state['status']='observing_original_client'
        ledger.phase('observation',max(.01,args.deadline-time.monotonic()-5),startup=False)
        end=min(args.deadline-5,time.monotonic()+state['phaseLimitSeconds'])
        while time.monotonic()<end and game.poll() is None:
            if state.get('agentFailed'):raise RuntimeError('Agent error; ending controlled test')
            ledger.save();time.sleep(.1)
        ledger.completed()
        if game.poll() is None:
            state['agentStatus']=ledger.call(frida,'status_rpc',3,lambda:script.exports_sync.status(),startup=False)
        if state.get('agentFailed') or state.get('agentStatus',{}).get('stopped'):raise RuntimeError('Agent failure prevents a successful result')
        state['nativeDiscoveryParsed']=any(e.get('kind')=='native_discovery_deserialized' and e.get('response') and e.get('localServiceFields')==31 and not e.get('statusNonempty') for e in state['events'])
        state['nativeRequestsObserved']=sum(e.get('kind')=='native_http_request' for e in state['events'])
        state['status']='native_discovery_verified' if state['nativeDiscoveryParsed'] else 'no_native_discovery_proof'
        result=0 if state['nativeDiscoveryParsed'] else 2
    except control.PhaseTimeout as error:
        state['status']='phase_timed_out';state['errors'].append({'phase':error.phase,'type':type(error).__name__,'message':str(error)});result=124
    except Exception as error:
        state['status']='client_attempt_failed';state['errors'].append({'phase':state['phase'],'type':type(error).__name__,'message':str(error)[:400]})
    finally:
        state['failedPhase']=state['errors'][-1]['phase'] if state['errors'] else None
        state['phase']='cleanup';state['phaseStartedMonotonic']=time.monotonic();ledger.save_best_effort()
        try:
            if game:
                try:state['beforeProcessCleanup']=game.diagnostics()
                except Exception as error:state['beforeProcessCleanup']={'diagnosticQueryError':type(error).__name__}
                if game.poll() is None:
                    if state['processResumed']:
                        state['cleanup']['windowCloseRequests']=api.request_window_close(game.pid);game.wait(1)
                    if game.poll() is None:game.terminate();game.wait(2);state['cleanup']['ownedProcessTerminated']=True
                state['cleanup']['exitCode']=game.poll();game.close();game=None
            if script:
                try:control.cancelled_call(frida,'unload',1,script.unload)
                except Exception:state['cleanup']['scriptUnloadIncomplete']=True
            if session:
                try:control.cancelled_call(frida,'detach',1,session.detach)
                except Exception:state['cleanup']['detachIncomplete']=True
        finally:
            if mapping:state['cleanup']['sharedSectionHandleClosed']=bool(api.close(mapping))
            if state.get('receiptPersistenceFailed'):result=1
            state['finishedUtc']=utc();state['runnerResultCode']=result
            if not ledger.save_best_effort():result=1
            try:ledger.close()
            except Exception:result=1
            # Independent one-shot failure snapshot after cleanup, never polled live.
            if state.get('receiptPersistenceFailed'):
                try:control.write_json(run/'receipt.failure.json',state)
                except Exception:pass
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--manifest',required=True,type=pathlib.Path)
    parser.add_argument('--run',action='store_true');parser.add_argument('--seconds',type=int,default=90)
    parser.add_argument('--worker',action='store_true',help=argparse.SUPPRESS)
    for arg in ['job','run-dir']:parser.add_argument('--'+arg,help=argparse.SUPPRESS)
    for arg in ['started','deadline','startup-deadline']:parser.add_argument('--'+arg,type=float,help=argparse.SUPPRESS)
    args=parser.parse_args()
    if os.name!='nt' or struct.calcsize('P')!=8 or not 10<=args.seconds<=180:raise ValueError('Windows x64 and a total budget of 10-180 seconds required')
    manifest=validate(args.manifest)
    if args.worker:
        if not all([args.run,args.job,args.run_dir,args.started,args.deadline,args.startup_deadline]):raise ValueError('Worker arguments are incomplete')
        return worker(args,manifest)
    win,control,recovery,capture=modules(manifest);api=recovery.Win32()
    if capture.WindowsReader().processes():raise RuntimeError('Existing Rec Room process; it is not touched')
    health=backend_health()
    if not args.run:
        print(json.dumps({'status':'preflight_passed','gameWillNotRun':True,'elevated':api.is_elevated(),'schemaVersion':3,'backendInstance':health['instanceId'],'pinnedFiles':len(manifest['files']),'totalBudgetSeconds':args.seconds,'attachLimitSeconds':15,'startupLimitSeconds':30}));return 0
    if not api.is_elevated():raise PermissionError('Approve Windows elevation separately; no self-elevation')
    run=pathlib.Path(manifest['labRoot'])/'runs'/datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ');private_folder(run)
    started=time.monotonic();deadline=started+args.seconds;startup_deadline=min(deadline,started+30)
    record={'startedUtc':utc(),'status':'starting_worker','totalBudgetSeconds':args.seconds,'startupLimitSeconds':min(30,args.seconds),'attachLimitSeconds':15,'manifestSha256':sha(args.manifest),'runnerSha256':sha(__file__),'existingProcessesTouched':False}
    control.write_json(run/'supervisor.start.json',record)
    with win.OwnedJob() as job:
        process=None
        try:
            command=[sys.executable,'-I','-B',str(ROOT/'run-native-client.py'),'--manifest',str(args.manifest),'--run','--worker','--job',job.name,'--run-dir',str(run),'--started',str(started),'--deadline',str(deadline),'--startup-deadline',str(startup_deadline),'--seconds',str(args.seconds)]
            process=job.spawn(command,ROOT)
            record['workerPid']=process.pid;record['jobKillOnClose']=True
            record.update(control.supervised(job,process,run/'receipt.json',started,deadline,startup_deadline,emit=lambda x:print(x,flush=True)))
        finally:
            if job.accounting()['activeProcesses']:job.stop()
            record['jobEmpty']=job.wait_empty(3);record['jobAfterCleanup']=job.accounting()
            if process:process.close()
    # Verification follows child cleanup; it does not lengthen the permitted child lifetime.
    record['originalHashesAfter']=[{'name':name,'matches':sha(GAME/name)==expected} for name,expected in EXPECTED.items()]
    record['finishedUtc']=utc();record['elapsedSeconds']=round(time.monotonic()-started,3)
    if not record['jobEmpty'] or not all(x['matches'] for x in record['originalHashesAfter']):record['resultCode']=1
    try:receipt=control.read_latest(run/'receipt.json')
    except Exception as error:
        receipt={'status':'progress_unavailable','receiptPersistenceFailed':True,'errors':[{'type':type(error).__name__}]}
    failure=run/'receipt.failure.json'
    if failure.exists():
        try:receipt=json.loads(failure.read_text())
        except Exception:receipt['receiptPersistenceFailed']=True
    try:control.write_json(run/'receipt.json',receipt)
    except Exception as error:
        receipt['receiptPersistenceFailed']=True;record['receiptExportError']=type(error).__name__
    record['resultCode']=control.final_result_code(record.get('resultCode'),receipt,record['jobEmpty'],all(x['matches'] for x in record['originalHashesAfter']))
    control.write_json(run/'supervisor.json',record)
    print(json.dumps({'status':receipt.get('status',record['status']),'supervisorStatus':record['status'],'receipt':str(run/'receipt.json'),'supervisor':str(run/'supervisor.json'),'failedPhase':receipt.get('failedPhase') or record.get('workerPhase'),'resultCode':record.get('resultCode',1),'jobEmpty':record['jobEmpty'],'nativeDiscoveryParsed':receipt.get('nativeDiscoveryParsed',False),'nativeLoginVerified':False}),flush=True)
    return record.get('resultCode',1)

if __name__=='__main__':
    try:raise SystemExit(main())
    except Exception as error:
        print(json.dumps({'status':'preflight_or_supervisor_failed','type':type(error).__name__,'message':str(error)[:400],'nativeLoginVerified':False}),flush=True);raise SystemExit(1)
