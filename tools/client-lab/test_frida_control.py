"""Real cancellation/attach/network-guard test on a NEW synthetic Python child.
Never launches or attaches to Rec Room. Vendor path must contain the reviewed wheel.
"""
from pathlib import Path
import argparse,hashlib,json,os,sys,time
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))
from supervision import cancelled_call,PhaseTimeout,supervised,write_json
from winjob import OwnedJob,current_job_membership,retain_process

FIXTURE='''from pathlib import Path
import sys,time,json,socket,urllib.request
root=Path(sys.argv[1]);(root/'fixture-ready').write_text('ready')
end=time.monotonic()+10
while not(root/'fixture-go').exists():
 if time.monotonic()>end:raise RuntimeError('No verified guard release')
 time.sleep(.02)
answer={};opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open('http://127.0.0.1:4187/healthz',timeout=3)as response:answer['localService']=json.load(response)['service']
for label,endpoint in [('external',('198.51.100.42',80)),('otherLocalPort',('127.0.0.1',4189))]:
 sock=socket.socket();sock.settimeout(1)
 try:sock.connect(endpoint);answer[label]='unexpected_success'
 except OSError as e:answer[label]=e.winerror if getattr(e,'winerror',None)is not None else e.errno
 finally:sock.close()
try:socket.getaddrinfo('must-not-resolve.invalid',443);answer['dns']='unexpected_success'
except OSError as e:answer['dns']=e.errno
(root/'fixture-result.json').write_text(json.dumps(answer))
'''

def worker(args):
    if not current_job_membership(args.job):raise RuntimeError('Synthetic test worker not supervised')
    sys.path.insert(0,str(args.vendor));import frida
    assert frida.__version__=='17.24.3'
    events=[];process=None;session=None;script=None
    report={'gameLaunched':False,'version':frida.__version__,'jobMembershipVerified':True}
    try:
        start=time.monotonic()
        try:cancelled_call(frida,'missing-device',.2,lambda:frida.get_device_manager().get_device_by_id('metallic-fixture-device-does-not-exist',10000))
        except PhaseTimeout:report['realNativeCancellationSeconds']=round(time.monotonic()-start,3)
        else:raise AssertionError('Native wait was not cancelled')
        assert report['realNativeCancellationSeconds']<2
        fixture=args.output/'fixture.py';fixture.write_text(FIXTURE,encoding='utf8')
        device=cancelled_call(frida,'device',5,frida.get_local_device)
        pid=cancelled_call(frida,'spawn',8,lambda:device.spawn([sys.executable,'-I','-B',str(fixture),str(args.output)],cwd=str(ROOT),stdio='pipe'))
        process=retain_process(pid);assert current_job_membership(args.job,process,assign=True)
        session=cancelled_call(frida,'attach',10,lambda:device.attach(pid))
        code='const CONFIG={port:4187,fixtureOnly:true};\n'+(ROOT/'route-policy.js').read_text()+'\n'+(ROOT/'runtime-agent.js').read_text()
        script=cancelled_call(frida,'create_script',3,lambda:session.create_script(code));script.on('message',lambda value,data:events.append(value))
        cancelled_call(frida,'load_script',3,script.load)
        cancelled_call(frida,'resume',3,lambda:device.resume(pid))
        end=time.monotonic()+5
        while time.monotonic()<end:
            status=cancelled_call(frida,'status',1,lambda:script.exports_sync.status())
            if status['networkGuards'] and (args.output/'fixture-ready').exists():break
            time.sleep(.02)
        else:raise AssertionError('Fixture network guards never ready')
        (args.output/'fixture-go').write_text('go')
        assert process.wait(8)==0
        data=json.loads((args.output/'fixture-result.json').read_text())
        assert data=={'localService':'metallic-rc-backend','external':10013,'otherLocalPort':10013,'dns':11001},data
        assert not any(x.get('type')=='error' or x.get('payload',{}).get('kind')=='agent_failure' for x in events)
        report.update(status='real_frida_fixture_passed',attachPassed=True,networkGuards=data,ownedPid=pid,agentSha256=hashlib.sha256((ROOT/'runtime-agent.js').read_bytes()).hexdigest())
    finally:
        if process:
            if process.poll() is None:process.terminate();process.wait(2)
            process.close()
        if script:
            try:cancelled_call(frida,'unload',1,script.unload)
            except Exception:pass
        if session:
            try:cancelled_call(frida,'detach',1,session.detach)
            except Exception:pass
        write_json(args.output/'frida-fixture.json',report)
    return 0

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--vendor',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--worker',action='store_true');p.add_argument('--job');args=p.parse_args()
    if args.worker:
        try:return worker(args)
        except BaseException as e:write_json(args.output/'fixture-error.json',{'error':type(e).__name__,'detail':str(e)[:800]});return 1
    args.output.mkdir(parents=True,exist_ok=False)
    with OwnedJob() as job:
        start=time.monotonic();child=job.spawn([sys.executable,'-I','-B',str(__file__),'--vendor',str(args.vendor),'--output',str(args.output),'--worker','--job',job.name],ROOT)
        try:r=supervised(job,child,args.output/'unused.json',start,start+40,start+40)
        finally:
            if job.accounting()['activeProcesses']:job.stop();job.wait_empty()
            child.close()
    write_json(args.output/'supervision.json',r)
    assert r['workerExitCode']==0 and r['jobEmpty'],r
    result=json.loads((args.output/'frida-fixture.json').read_text());assert result['status']=='real_frida_fixture_passed';print(json.dumps(result),flush=True);return 0
if __name__=='__main__':raise SystemExit(main())
