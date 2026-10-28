"""Offline runner import, synthetic callbacks and fake worker ownership; no native Frida/process access."""
from __future__ import annotations

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

RUNNER_PATH=Path(__file__).with_name('run-native-client.py')
spec=importlib.util.spec_from_file_location('startup_output_runner',RUNNER_PATH)
runner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Device:
    def __init__(self):
        self.callbacks=[];self.off_calls=[];self.off_hook=None;self.on_error=False

    def on(self,signal,callback):
        self.callbacks.append(callback)
        callback(7,1,b'pre-bind payload')
        if self.on_error:raise RuntimeError('synthetic subscription failure')

    def off(self,signal,callback):
        self.off_calls.append((signal,callback))
        if self.off_hook:self.off_hook()
        self.callbacks.remove(callback)


class OutputChecks(unittest.TestCase):
    def test_import_is_inert(self):
        self.assertEqual(runner.StartupOutput.LIMIT_BYTES,65536)
        self.assertNotIn('frida',sys.modules)

    def test_before_bind_retains_only_event_count(self):
        output=runner.StartupOutput()
        output.callback(object(),object(),object())
        output.callback(8,2,b'unknown private payload')
        row=output.snapshot()
        self.assertEqual(row['droppedEvents'],{'beforeBind':2,'unowned':0,'malformed':0})
        self.assertIsNone(row['ownedPid'])
        self.assertTrue(all(stream['byteCount']==0 and stream['prefixBytes']==0 for stream in row['streams'].values()))
        self.assertNotIn('payload',json.dumps(row))

    def test_identity_and_stream_types_are_exact(self):
        output=runner.StartupOutput();output.bind(7)
        invalid=[(True,1,b'a'),(7,True,b'a'),(7,0,b'a'),(7,3,b'a'),(7,1,'text'),
                 (7,1,None),(7,1,bytearray(b'a')),(7,1,memoryview(b'a')),(7.0,1,b'a')]
        for event in invalid:output.callback(*event)
        output.callback(8,1,b'foreign process')
        row=output.snapshot()
        self.assertEqual(row['droppedEvents']['malformed'],len(invalid))
        self.assertEqual(row['droppedEvents']['unowned'],1)
        self.assertEqual(row['streams']['1']['byteCount'],0)
        for pid in (None,True,0,-1,'7'):
            with self.assertRaises(ValueError):runner.StartupOutput().bind(pid)
        with self.assertRaises(ValueError):output.bind(7)
        with self.assertRaises(ValueError):output.bind(8)

    def test_prefix_hashes_and_counts_are_per_fd_and_bounded(self):
        output=runner.StartupOutput();output.bind(7)
        block=bytes(range(256))*256
        output.callback(7,1,block[:-3]);output.callback(7,1,b'abcEXCESS')
        output.callback(7,2,b'error'*20000)
        row=output.snapshot()
        for fd,data in [('1',block[:-3]+b'abcEXCESS'),('2',b'error'*20000)]:
            stream=row['streams'][fd]
            self.assertEqual(stream['byteCount'],len(data))
            self.assertEqual(stream['prefixBytes'],65536)
            self.assertEqual(stream['prefixSha256'],hashlib.sha256(data[:65536]).hexdigest())
            self.assertTrue(stream['truncated'])
        output.callback(7,1,b'z'*1000000)
        self.assertEqual(output.snapshot()['streams']['1']['prefixSha256'],row['streams']['1']['prefixSha256'])
        self.assertFalse(any(isinstance(value,(bytes,bytearray,memoryview)) for value in output._streams[1].values()))

    def test_exact_cap_is_not_truncation_and_eof_is_terminal(self):
        output=runner.StartupOutput();output.bind(7)
        output.callback(7,1,b'x'*65536);output.callback(7,1,b'')
        before=output.snapshot()['streams']['1']
        self.assertEqual(before['events'],2);self.assertTrue(before['eof']);self.assertFalse(before['truncated'])
        output.callback(7,1,b'after EOF');output.callback(7,1,b'')
        after=output.snapshot()['streams']['1']
        self.assertEqual(after['byteCount'],before['byteCount'])
        self.assertEqual(after['prefixSha256'],before['prefixSha256'])
        self.assertEqual(after['afterEofEvents'],2)
        self.assertFalse(output.snapshot()['streams']['2']['eof'])

    def test_snapshot_is_deeply_detached_and_json_safe(self):
        output=runner.StartupOutput();output.bind(7);output.callback(7,2,b'private synthetic bytes')
        before=output.snapshot();before['streams']['2']['byteCount']=999;before['droppedEvents']['unowned']=999
        after=output.snapshot();self.assertEqual(after['streams']['2']['byteCount'],23)
        self.assertEqual(after['droppedEvents']['unowned'],0)
        self.assertNotIn('private synthetic',json.dumps(after))

    def test_concurrent_callbacks_and_snapshots_are_consistent(self):
        output=runner.StartupOutput();output.bind(7)
        barrier=threading.Barrier(5);errors=[]
        def emit(fd):
            try:
                barrier.wait()
                for _ in range(200):output.callback(7,fd,b'x'*1024)
            except BaseException as error:errors.append(error)
        workers=[threading.Thread(target=emit,args=(1+i%2,)) for i in range(4)]
        for worker in workers:worker.start()
        barrier.wait()
        for _ in range(300):
            for row in output.snapshot()['streams'].values():
                self.assertEqual(row['byteCount'],row['events']*1024)
                self.assertEqual(row['prefixBytes'],min(row['byteCount'],65536))
        for worker in workers:worker.join(3);self.assertFalse(worker.is_alive())
        self.assertFalse(errors)
        for row in output.snapshot()['streams'].values():
            self.assertEqual(row['byteCount'],400*1024)
            self.assertEqual(row['prefixSha256'],hashlib.sha256(b'x'*65536).hexdigest())

    def test_reentrant_unsubscribe_is_nonblocking_and_preserves_foreign_handlers(self):
        output=runner.StartupOutput();device=Device();foreign=lambda *args:None
        device.callbacks.append(foreign);output.subscribe(device);output.bind(7);output.callback(7,1,b'admitted')
        before=output.snapshot()['streams'];nested=[]
        def reentrant():
            output.callback(7,1,b'must not enter summary')
            nested.append(output.close());output.snapshot()
        device.off_hook=reentrant
        self.assertTrue(output.close());self.assertEqual(nested,[False]);self.assertTrue(output.close())
        self.assertEqual(device.callbacks,[foreign]);self.assertEqual(len(device.off_calls),1)
        self.assertIs(device.off_calls[0][1],output.callback)
        self.assertEqual(output.snapshot()['streams'],before)
        self.assertEqual(output.snapshot()['unsubscribe'],'complete')

    def test_concurrent_cleanup_reports_pending_until_owned_off_finishes(self):
        output=runner.StartupOutput();device=Device();output.subscribe(device);output.bind(7)
        entered=threading.Event();release=threading.Event();results=[]
        def wait():entered.set();release.wait(3)
        device.off_hook=wait
        thread=threading.Thread(target=lambda:results.append(output.close()));thread.start()
        try:
            self.assertTrue(entered.wait(2));self.assertFalse(output.close())
            self.assertEqual(output.snapshot()['unsubscribe'],'in_progress')
        finally:release.set();thread.join(3)
        self.assertEqual(results,[True]);self.assertTrue(output.close());self.assertEqual(len(device.off_calls),1)

    def test_failed_or_partial_subscription_cleanup_is_explicit_and_terminal(self):
        output=runner.StartupOutput();device=Device();device.on_error=True
        with self.assertRaises(RuntimeError):output.subscribe(device)
        self.assertEqual(output.snapshot()['subscription'],'failed');self.assertTrue(output.close())
        self.assertFalse(device.callbacks)
        output=runner.StartupOutput();device=Device();output.subscribe(device);output.bind(7)
        def fail():raise RuntimeError('synthetic off failure')
        device.off_hook=fail
        with self.assertRaises(RuntimeError):output.close()
        self.assertFalse(output.close());self.assertEqual(output.snapshot()['unsubscribe'],'failed')
        before=output.snapshot();output.callback(7,1,b'after failed off');self.assertEqual(output.snapshot(),before)
        self.assertTrue(runner.StartupOutput().close())


class WorkerChecks(unittest.TestCase):
    def attempt(self,*,image_ok=True,job_ok=True,off_timeout=False,version='17.24.3'):
        with tempfile.TemporaryDirectory(prefix='startup-output-offline-') as temp:
            lab=Path(temp);run=lab/'runs'/'synthetic';run.mkdir(parents=True)
            state={'phase':'preflight','errors':[],'events':[],'cleanup':{},'processResumed':False}
            observed=[];device=Device();fake_frida=SimpleNamespace(__version__=version,get_local_device=lambda:device)
            class PhaseTimeout(TimeoutError):pass
            class Ledger:
                def __init__(self,*args):self.state=state
                def phase(self,name,*args,**kwargs):state['phase']=name
                def completed(self):pass
                def save(self):pass
                def save_best_effort(self):return True
                def close(self):pass
                def call(self,frida,name,limit,operation,**kwargs):state['phase']=name;return operation()
            def spawn(*args,**kwargs):
                self.assertTrue(device.callbacks);self.assertEqual(kwargs['stdio'],'pipe')
                device.callbacks[0](7,1,b'before admitted');return 7
            def attach(pid):
                self.assertEqual(pid,7);observed.append(dict(state['startupOutputBeforeAttach']))
                raise RuntimeError('synthetic attachment failure')
            device.spawn=spawn;device.attach=attach
            class Game:
                pid=7
                def image(self):return runner.GAME/('RecRoom.exe' if image_ok else 'Unowned.exe')
                def diagnostics(self):
                    device.callbacks[0](7,1,b'admitted')
                    return {'synthetic':True}
                def poll(self):return 0
                def close(self):pass
            game=Game()
            def membership(*args,**kwargs):return job_ok if kwargs.get('assign') else True
            def cancelled(frida,phase,seconds,operation):
                self.assertEqual(phase,'output_unsubscribe');self.assertGreater(seconds,0);self.assertLessEqual(seconds,1)
                if off_timeout:raise PhaseTimeout('synthetic deadline')
                return operation()
            fake_modules=(SimpleNamespace(current_job_membership=membership,retain_process=lambda pid:game),
                          SimpleNamespace(Ledger=Ledger,PhaseTimeout=PhaseTimeout,cancelled_call=cancelled),
                          SimpleNamespace(Win32=lambda:SimpleNamespace(is_elevated=lambda:True,create_new_section=lambda:1,close=lambda value:True)),
                          SimpleNamespace(WindowsReader=lambda:SimpleNamespace(processes=lambda:[])))
            args=SimpleNamespace(job='synthetic',run_dir=str(run),started=time.monotonic(),deadline=time.monotonic()+20,startup_deadline=time.monotonic()+10)
            original_stdout,original_stderr=sys.stdout,sys.stderr
            try:
                with (patch.object(runner,'modules',return_value=fake_modules),
                      patch.object(runner,'backend_health',return_value={'instanceId':'synthetic'}),
                      patch.object(Path,'open',side_effect=lambda *args,**kwargs:io.StringIO()),
                      patch.dict(sys.modules,{'frida':fake_frida})):
                    result=runner.worker(args,{'labRoot':str(lab),'vendor':str(lab)})
            finally:
                sys.stdout,sys.stderr=original_stdout,original_stderr
                if sys.path and sys.path[0]==str(lab):sys.path.pop(0)
            return result,state,observed,device

    def test_actual_worker_binds_only_after_admission_and_receipts_bracket_attach_cleanup(self):
        result,state,observed,device=self.attempt()
        self.assertEqual(result,1);self.assertEqual(state['status'],'client_attempt_failed')
        self.assertEqual(observed[0]['ownedPid'],7)
        self.assertEqual(observed[0]['streams']['1']['byteCount'],8)
        self.assertEqual(state['startupOutput']['streams']['1']['byteCount'],16)
        self.assertEqual(state['startupOutput']['droppedEvents']['beforeBind'],2)
        self.assertTrue(state['startupOutput']['closed']);self.assertTrue(state['cleanup']['startupOutputUnsubscribed'])
        self.assertEqual(state['startupOutput']['unsubscribe'],'complete');self.assertFalse(device.callbacks)
        self.assertEqual(state['errors'][0]['message'],'synthetic attachment failure')

    def test_image_and_job_refusal_never_bind_or_attach(self):
        for values in ({'image_ok':False},{'job_ok':False}):
            with self.subTest(values=values):
                result,state,observed,_device=self.attempt(**values)
                self.assertEqual(result,1);self.assertFalse(observed)
                self.assertIsNone(state['startupOutput']['ownedPid'])
                self.assertEqual(state['startupOutput']['streams']['1']['byteCount'],0)

    def test_unsubscribe_timeout_does_not_mask_original_failure_or_enable_capture(self):
        result,state,_,device=self.attempt(off_timeout=True)
        self.assertEqual(result,1);self.assertEqual(state['errors'][0]['message'],'synthetic attachment failure')
        self.assertTrue(state['cleanup']['startupOutputUnsubscribeIncomplete'])
        self.assertEqual(state['cleanup']['startupOutputUnsubscribeErrorType'],'PhaseTimeout')
        self.assertTrue(state['startupOutput']['closed']);self.assertEqual(state['startupOutput']['unsubscribe'],'not_started')
        self.assertFalse(device.off_calls)

    def test_rejected_library_version_does_not_enter_new_native_cleanup(self):
        result,state,observed,device=self.attempt(version='unreviewed')
        self.assertEqual(result,1);self.assertFalse(observed);self.assertFalse(device.callbacks)
        self.assertFalse(device.off_calls);self.assertEqual(state['startupOutput']['unsubscribe'],'not_subscribed')
        self.assertNotIn('startupOutputUnsubscribeIncomplete',state['cleanup'])
        self.assertEqual(state['errors'][0]['message'],'Unexpected instrumentation version')


if __name__=='__main__':unittest.main(verbosity=2)
