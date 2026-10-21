"""Deadlines/progress are independent of the instrumented application.
Cooperative cancellation keeps normal cleanup orderly; the separate parent and
Windows Job Object enforce the hard limit even if a native call ignores it.
"""
from __future__ import annotations
import datetime,json,pathlib,threading,time

class PhaseTimeout(TimeoutError):
    def __init__(self,phase):self.phase=phase;super().__init__('Phase deadline exceeded: '+phase)

def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def write_json(path,value):
    path=pathlib.Path(path);temporary=path.with_name(path.name+'.tmp')
    temporary.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8',newline='\n');temporary.replace(path)

def cancelled_call(frida,phase,seconds,operation):
    """Use this pinned Frida API's thread-local Cancellable context, not a fake kwarg."""
    if seconds<=0:raise PhaseTimeout(phase)
    cancellation=frida.Cancellable();expired=threading.Event()
    def cancel():expired.set();cancellation.cancel()
    timer=threading.Timer(seconds,cancel);timer.daemon=True;timer.start()
    try:
        try:
            with cancellation:result=operation()
        except BaseException:
            if expired.is_set():raise PhaseTimeout(phase) from None
            raise
        if expired.is_set():raise PhaseTimeout(phase)
        return result
    finally:timer.cancel();timer.join(timeout=.2)

class Ledger:
    def __init__(self,path,started,deadline,startup_deadline,base=None):
        self.path=pathlib.Path(path);self.started=started;self.deadline=deadline;self.startup_deadline=startup_deadline
        self.lock=threading.RLock();self.state={**(base or {}),'schemaVersion':2,'startedUtc':utc(),'status':'preparing','phase':'preflight','phaseStartedMonotonic':time.monotonic(),'phaseLimitSeconds':15,
            'totalBudgetSeconds':max(0,deadline-started),'runtimeRoutingRequested':True,'runtimeRoutingHookUsed':False,'attached':False,'agentLoaded':False,'processResumed':False,
            'nativeDiscoveryParsed':False,'nativeLoginVerified':False,'nativeRoomJoinVerified':False,'events':[],'phases':[],'errors':[],'cleanup':{}}
        self.save()
    def save(self):
        with self.lock:
            self.state['elapsedSeconds']=round(time.monotonic()-self.started,3)
            write_json(self.path,self.state)
    def phase(self,name,limit,startup=True):
        now=time.monotonic();end=min(self.deadline,now+limit,self.startup_deadline if startup else self.deadline)
        if end<=now:raise PhaseTimeout(name)
        with self.lock:
            self.state.update(phase=name,phaseStartedMonotonic=now,phaseLimitSeconds=end-now)
            self.state['phases'].append({'phase':name,'startedUtc':utc(),'startedMonotonic':now,'limitSeconds':round(end-now,3),'outcome':'running'})
        self.save();return end-now
    def completed(self):
        with self.lock:
            row=self.state['phases'][-1];row.update(outcome='completed',elapsedSeconds=round(time.monotonic()-row['startedMonotonic'],3))
        self.save()
    def call(self,frida,name,limit,operation,startup=True):
        seconds=self.phase(name,limit,startup)
        try:result=cancelled_call(frida,name,seconds,operation)
        except BaseException as error:
            with self.lock:
                self.state['phases'][-1].update(outcome='failed',elapsedSeconds=round(time.monotonic()-self.state['phaseStartedMonotonic'],3),errorType=type(error).__name__)
            self.save();raise
        self.completed();return result
    def event(self,payload):
        if not isinstance(payload,dict) or not isinstance(payload.get('kind'),str):return
        if len(json.dumps(payload,ensure_ascii=True))>4096:return
        with self.lock:
            # Failure truth must survive the bounded diagnostic-event buffer.
            if payload['kind']=='agent_failure':
                self.state['agentFailed']=True;self.state['lastAgentFailure']=payload
            if payload['kind']=='agent_loaded':self.state['agentLoaded']=True
            if payload['kind']=='client_route_adapter_ready':self.state['runtimeRoutingHookUsed']=True
            if len(self.state['events'])>=512:return
            self.state['events'].append(payload)
    def finish(self,status,error=None):
        with self.lock:
            self.state['status']=status;self.state['finishedUtc']=utc()
            if error:self.state['errors'].append({'phase':self.state['phase'],'type':type(error).__name__,'message':str(error)[:400]})
        self.save()

HARD_PHASE_CAPS={'preflight':15,'instrumentation_import':8,'device':8,'spawn':12,'attach':15,'create_script':5,'load_script':5,'resume':5,'status_rpc':3,'unload':2,'detach':2,'cleanup':8}

def supervised(job,process,receipt_path,started,deadline,startup_deadline,emit=print):
    """Watch a retained worker handle; never trust its own timer to cancel itself."""
    reason=None;latest={};last_phase=None
    while process.poll() is None:
        now=time.monotonic()
        try:latest=json.loads(pathlib.Path(receipt_path).read_text(encoding='utf8'))
        except (OSError,ValueError):pass
        phase=latest.get('phase','worker_start')
        if phase!=last_phase:
            emit(json.dumps({'phase':phase,'elapsedSeconds':round(now-started,2),'maximumTotalSeconds':round(deadline-started,2)}));last_phase=phase
        if now>=deadline:reason='total_deadline';break
        if not latest.get('processResumed') and phase!='cleanup' and now>=startup_deadline:reason='startup_deadline';break
        begin=latest.get('phaseStartedMonotonic')
        if phase in HARD_PHASE_CAPS and isinstance(begin,(int,float)) and begin>=started and now-begin>HARD_PHASE_CAPS[phase]+1:
            reason='phase_deadline:'+phase;break
        time.sleep(.05)
    if reason:job.stop(124)
    process.wait(2)
    code=process.poll()
    # On ANY return, do not leave a suspended child or crash helper behind.
    accounting_before=job.accounting()
    if accounting_before['activeProcesses']:job.stop(124 if reason else 1)
    empty=job.wait_empty(3)
    return {'status':'deadline_terminated' if reason else 'worker_returned','deadlineReason':reason,'workerExitCode':code,'workerPhase':latest.get('phase'),'jobBeforeCleanup':accounting_before,
            'jobAfterCleanup':job.accounting(),'jobEmpty':empty,'elapsedSeconds':round(time.monotonic()-started,3),'resultCode':124 if reason else (code if code is not None else 1)}

def final_result_code(worker_code,receipt,job_empty,hashes_match):
    """A clean process exit is not native-client proof."""
    if not job_empty or not hashes_match:return 1
    if worker_code!=0:return worker_code if worker_code is not None else 1
    required=('nativeDiscoveryParsed','attached','agentLoaded','processResumed','runtimeRoutingHookUsed','gameJobMembershipVerified')
    if receipt.get('agentFailed') or receipt.get('errors') or receipt.get('status')!='native_discovery_verified':return 2
    if not all(receipt.get(key) is True for key in required):return 2
    return 0
