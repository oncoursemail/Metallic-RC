"""Tests target newly created fixtures only. No Rec Room process is launched."""
from pathlib import Path
import contextlib,ctypes,json,os,subprocess,sys,tempfile,threading,time,unittest,uuid
from supervision import Ledger,PhaseTimeout,cancelled_call,supervised
from progress_store import read_latest

ROOT=Path(__file__).resolve().parent
class FakeCancelled(Exception):pass
class FakeCancellation:
    current=None
    def __init__(self):self.event=threading.Event()
    def cancel(self):self.event.set()
    def __enter__(self):FakeCancellation.current=self;return self
    def __exit__(self,*args):FakeCancellation.current=None
class FakeFrida:Cancellable=FakeCancellation

class ControlTests(unittest.TestCase):
    def test_normal_result(self):self.assertEqual(cancelled_call(FakeFrida,'test',1,lambda:42),42)
    def test_original_error_preserved(self):
        with self.assertRaisesRegex(ValueError,'original'):
            cancelled_call(FakeFrida,'test',1,lambda:(_ for _ in ()).throw(ValueError('original')))
    def test_cancellation_stops_a_wait(self):
        def operation():FakeCancellation.current.event.wait(5);raise FakeCancelled()
        start=time.monotonic()
        with self.assertRaises(PhaseTimeout) as caught:cancelled_call(FakeFrida,'attach',.08,operation)
        self.assertEqual(caught.exception.phase,'attach');self.assertLess(time.monotonic()-start,1)
    def test_zero_budget_never_calls(self):
        called=[]
        with self.assertRaises(PhaseTimeout):cancelled_call(FakeFrida,'spawn',0,lambda:called.append(1))
        self.assertEqual(called,[])
    def test_facts_are_not_intent(self):
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            start=time.monotonic();l=Ledger(Path(td)/'receipt.json',start,start+10,start+3)
            self.addCleanup(l.close);stack.callback(l.close)
            self.assertTrue(l.state['runtimeRoutingRequested']);self.assertFalse(l.state['runtimeRoutingHookUsed']);self.assertFalse(l.state['attached']);self.assertFalse(l.state['processResumed'])
            l.event({'kind':'agent_loaded'});self.assertTrue(l.state['agentLoaded']);self.assertFalse(l.state['runtimeRoutingHookUsed'])
            l.event({'kind':'client_route_adapter_ready'});self.assertTrue(l.state['runtimeRoutingHookUsed'])
    def test_late_timer_is_not_reset_by_phase(self):
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            now=time.monotonic();l=Ledger(Path(td)/'r.json',now,now+2,now-.1)
            self.addCleanup(l.close);stack.callback(l.close)
            with self.assertRaises(PhaseTimeout):l.phase('attach',15)
    def test_phase_receipt_identifies_block(self):
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            now=time.monotonic();p=Path(td)/'r.json';l=Ledger(p,now,now+10,now+3)
            self.addCleanup(l.close);stack.callback(l.close)
            def fail():raise ValueError('fixture')
            with self.assertRaises(ValueError):l.call(FakeFrida,'attach',1,fail)
            d=read_latest(p);self.assertEqual(d['phase'],'attach');self.assertEqual(d['phases'][-1]['outcome'],'failed')
    def test_bounded_event_size_and_count(self):
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            now=time.monotonic();l=Ledger(Path(td)/'r.json',now,now+5,now+2)
            self.addCleanup(l.close);stack.callback(l.close)
            l.event({'kind':'test','oversize':'x'*5000});self.assertEqual(len(l.state['events']),0)
            for i in range(600):l.event({'kind':'test','number':i})
            self.assertEqual(len(l.state['events']),512)

class AcceptanceTests(unittest.TestCase):
    def test_zero_exit_without_proof_is_not_success(self):
        from supervision import final_result_code
        self.assertEqual(final_result_code(0,{},True,True),2)
    def test_complete_discovery_proof_and_clean_state(self):
        from supervision import final_result_code
        receipt={k:True for k in ('nativeDiscoveryParsed','attached','agentLoaded','processResumed','runtimeRoutingHookUsed','gameJobMembershipVerified')}
        receipt['status']='native_discovery_verified'
        self.assertEqual(final_result_code(0,receipt,True,True),0)
        for key in ('attached','processResumed','gameJobMembershipVerified'):
            bad={**receipt,key:False};self.assertEqual(final_result_code(0,bad,True,True),2)
        self.assertEqual(final_result_code(0,{**receipt,'agentFailed':True},True,True),2)
        self.assertEqual(final_result_code(0,receipt,False,True),1)
        self.assertEqual(final_result_code(0,receipt,True,False),1)
    def test_timeout_exit_code_survives(self):
        from supervision import final_result_code
        self.assertEqual(final_result_code(124,{},True,True),124)
    def test_failure_not_hidden_by_event_limit(self):
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            now=time.monotonic();l=Ledger(Path(td)/'r.json',now,now+5,now+2)
            self.addCleanup(l.close);stack.callback(l.close)
            for i in range(520):l.event({'kind':'test','number':i})
            l.event({'kind':'agent_failure','scope':'fixture','error':'fixture'})
            self.assertTrue(l.state['agentFailed']);self.assertEqual(len(l.state['events']),512)

@unittest.skipUnless(os.name=='nt','Real Job Object tests require Windows')
class WindowsJobTests(unittest.TestCase):
    def test_owned_process_diagnostics_are_readonly(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            child=job.spawn([sys.executable,'-I','-c','import time;time.sleep(5)'],ROOT)
            try:
                data=child.diagnostics();self.assertEqual(data['pid'],child.pid);self.assertTrue(data['readOnly']);self.assertFalse(data['securityPolicyChanged'])
                self.assertIsNone(data['exitCodeBeforeCleanup']);self.assertGreaterEqual(data['kernel100ns'],0)
                self.assertEqual(set(data['mitigationPolicies']),{'dynamicCode','controlFlowGuard','binarySignature'})
                self.assertIsNone(child.poll())
            finally:job.stop();job.wait_empty();child.close()
    def test_struct_layouts_match_x64_win32(self):
        import winjob
        self.assertEqual(ctypes.sizeof(winjob.STARTUP),104);self.assertEqual(ctypes.sizeof(winjob.EXTENDED),144);self.assertEqual(ctypes.sizeof(winjob.ACCOUNTING),48)
    def test_assign_before_run_and_successful_exit(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            p=Path(td);script=p/'worker.py'
            script.write_text('import sys,json;sys.path.insert(0,'+repr(str(ROOT))+');from winjob import current_job_membership;from pathlib import Path;Path(sys.argv[2]).write_text(str(current_job_membership(sys.argv[1])))')
            process=job.spawn([sys.executable,'-I',str(script),job.name,str(p/'member.txt')],p)
            try:self.assertEqual(process.wait(5),0);self.assertEqual((p/'member.txt').read_text(),'True');self.assertTrue(job.wait_empty())
            finally:process.close()
    def test_timeout_reaps_worker_and_grandchild(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            p=Path(td);script=p/'worker.py'
            script.write_text('import subprocess,sys,time,json;from pathlib import Path;c=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"]);Path(sys.argv[1]).write_text(str(c.pid));time.sleep(60)')
            before=time.monotonic();process=job.spawn([sys.executable,'-I',str(script),str(p/'child.txt')],p)
            try:
                until=time.monotonic()+2
                while not (p/'child.txt').exists() and time.monotonic()<until:time.sleep(.02)
                self.assertTrue((p/'child.txt').exists());child=winjob.retain_process(int((p/'child.txt').read_text()))
                try:
                    r=supervised(job,process,p/'absent.json',before,time.monotonic()+.2,time.monotonic()+5,lambda _:None)
                    self.assertEqual(r['resultCode'],124);self.assertTrue(r['jobEmpty']);self.assertGreaterEqual(r['jobBeforeCleanup']['totalProcesses'],2)
                    self.assertIsNotNone(child.wait(1));self.assertLess(time.monotonic()-before,5)
                finally:child.close()
            finally:process.close()
    def test_startup_deadline_precedes_total_budget(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            before=time.monotonic();process=job.spawn([sys.executable,'-I','-c','import time;time.sleep(60)'],td)
            try:
                r=supervised(job,process,Path(td)/'absent.json',before,before+10,before+.2,lambda _:None)
                self.assertEqual(r['deadlineReason'],'startup_deadline');self.assertTrue(r['jobEmpty']);self.assertLess(r['elapsedSeconds'],3)
            finally:process.close()
    def test_normal_worker_exit_does_not_leave_child(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            before=time.monotonic();child_record=Path(td)/'child.txt'
            code='import subprocess,sys;from pathlib import Path;c=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"]);Path(sys.argv[1]).write_text(str(c.pid))'
            process=job.spawn([sys.executable,'-I','-c',code,str(child_record)],ROOT)
            child=None
            try:
                end=time.monotonic()+3
                while not child_record.exists() and time.monotonic()<end:time.sleep(.02)
                self.assertTrue(child_record.exists());child=winjob.retain_process(int(child_record.read_text()))
                r=supervised(job,process,Path(td)/'absent.json',before,before+10,before+5,lambda _:None)
                self.assertEqual(r['workerExitCode'],0);self.assertTrue(r['jobEmpty']);self.assertGreaterEqual(r['jobBeforeCleanup']['totalProcesses'],2)
                self.assertIsNotNone(child.wait(2))
            finally:
                if child:child.close()
                process.close()
    def test_attach_phase_watchdog_handles_uncancellable_wait(self):
        import winjob
        with tempfile.TemporaryDirectory() as td,winjob.OwnedJob() as job:
            p=Path(td);script=p/'blocked.py';receipt=p/'receipt.json'
            script.write_text('import time,sys;sys.path.insert(0,'+repr(str(ROOT))+');from progress_store import ProgressWriter,database_path;w=ProgressWriter(database_path(sys.argv[1]));w.publish({"phase":"attach","phaseStartedMonotonic":time.monotonic(),"processResumed":False});time.sleep(60)')
            before=time.monotonic();process=job.spawn([sys.executable,'-I',str(script),str(receipt)],ROOT)
            try:
                r=supervised(job,process,receipt,before,before+40,before+35,lambda _:None)
                self.assertEqual(r['deadlineReason'],'phase_deadline:attach');self.assertTrue(r['jobEmpty'])
                self.assertGreaterEqual(r['elapsedSeconds'],15);self.assertLess(r['elapsedSeconds'],25)
            finally:process.close()
    def test_last_parent_handle_close_kills_owned_child(self):
        import winjob
        with tempfile.TemporaryDirectory() as td, contextlib.ExitStack() as stack:
            p=Path(td);script=p/'owner.py';script.write_text('import sys,os;from pathlib import Path;sys.path.insert(0,'+repr(str(ROOT))+');from winjob import OwnedJob;j=OwnedJob();c=j.spawn([sys.executable,"-c","import time;time.sleep(60)"],sys.argv[1]);Path(sys.argv[1],"pid.txt").write_text(str(c.pid));import time;time.sleep(1);os._exit(0)')
            owner=subprocess.Popen([sys.executable,'-I',str(script),td]);child=None
            try:
                end=time.monotonic()+5
                while not (p/'pid.txt').exists() and time.monotonic()<end:time.sleep(.02)
                self.assertTrue((p/'pid.txt').exists());child=winjob.retain_process(int((p/'pid.txt').read_text()))
                self.assertEqual(owner.wait(timeout=5),0);self.assertIsNotNone(child.wait(3))
            finally:
                if owner.poll() is None:owner.terminate();owner.wait(timeout=3)
                if child:child.close()

if __name__=='__main__':unittest.main(verbosity=2)
