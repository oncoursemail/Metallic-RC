"""Authored manifest/worker controls. No Frida, game process, UAC or backend calls."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent


def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value


runner=load('mode_runner',ROOT/'run-native-client.py')
prepare=load('mode_preparer',ROOT/'prepare-manifest.py')


class ManifestTests(unittest.TestCase):
    def setUp(self):
        temporary=tempfile.TemporaryDirectory(prefix='rc-mode-authored-');self.addCleanup(temporary.cleanup)
        self.root=Path(temporary.name).resolve();self.game=self.root/'game';self.game.mkdir()
        self.files=[ROOT/name for name in prepare.AUTHORED]
        for relative in ['game/RecRoom.exe','game/GameAssembly.dll','identity.json','recovery.py','capture.py',
                         'vendor/frida/__init__.py','vendor/frida/aio.py','vendor/frida/_frida.pyd']:
            p=self.root/relative;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b'INERT AUTHORED '+relative.encode());self.files.append(p)
        self.profile={'schemaVersion':3,'steamBuildId':'23191908','port':4187,'gameRoot':str(self.game),
            'labRoot':str(self.root/'lab'),'agent':str(ROOT/'runtime-agent.js'),'policy':str(ROOT/'route-policy.js'),
            'methodIdentities':str(self.root/'identity.json'),'recoveryWrapper':str(self.root/'recovery.py'),
            'captureHelper':str(self.root/'capture.py'),'vendor':str(self.root/'vendor'),
            'runnerPath':str(ROOT/'run-native-client.py'),'runnerSha256':runner.sha(ROOT/'run-native-client.py'),
            'files':[{'path':str(p),'sha256':runner.sha(p),'bytes':p.stat().st_size} for p in self.files]}
        self.path=self.root/'synthetic.json';self.output=self.root/'synthetic-candidate.json'
        self.expected={p.name:runner.sha(p) for p in self.files if p.parent==self.game}

    def validate(self,profile):
        self.path.write_text(json.dumps(profile),encoding='utf8')
        with patch.object(runner,'GAME',self.game),patch.object(runner,'EXPECTED',self.expected),patch.object(sys,'dont_write_bytecode'),patch.object(sys,'pycache_prefix'):
            return runner.validate(self.path)

    def test_manifest_default_and_valid_explicit_modes(self):
        self.assertEqual(runner.client_adapter_mode(self.validate(self.profile)),'full')
        for mode in ['full','network-only']:
            value=copy.deepcopy(self.profile);value['clientAdapterMode']=mode
            self.assertEqual(runner.client_adapter_mode(self.validate(value)),mode)

    def test_invalid_manifest_modes_fail_before_pinned_files_are_read(self):
        for mode in [None,True,False,0,1,{},[],['full'],'','FULL','network_only']:
            value=copy.deepcopy(self.profile);value['clientAdapterMode']=mode
            with self.subTest(mode=mode),patch.object(runner,'sha',side_effect=AssertionError('No pin reads permitted')),self.assertRaises(ValueError):
                self.validate(value)

    def test_worker_revalidates_mode_before_loading_modules(self):
        for mode in [None,False,0,{},[],'FULL']:
            with patch.object(runner,'modules',side_effect=AssertionError('No module loads permitted')),self.assertRaises(ValueError):
                runner.worker(None,{'clientAdapterMode':mode})

    def test_agent_config_is_explicit_and_never_fixture_mode(self):
        for value,mode in [({},'full'),({'clientAdapterMode':'full'},'full'),({'clientAdapterMode':'network-only'},'network-only')]:
            config=runner.agent_config(value,{'methods':{'synthetic':{'rva':1}}})
            self.assertIs(config['fixtureOnly'],False);self.assertEqual(config['clientAdapterMode'],mode)
            self.assertEqual(config['port'],4187);self.assertEqual(config['buildId'],'23191908')

    def test_refresh_defaults_preserves_and_explicitly_selects_mode(self):
        for previous_mode,override,expected in [(None,None,'full'),('full',None,'full'),('network-only',None,'network-only'),(None,'network-only','network-only'),('network-only','full','full')]:
            value=copy.deepcopy(self.profile)
            if previous_mode is not None:value['clientAdapterMode']=previous_mode
            self.path.write_text(json.dumps(value),encoding='utf8');before=self.path.read_bytes()
            result=prepare.refresh(self.path,self.output,override)
            self.assertEqual(self.path.read_bytes(),before);self.assertFalse(result['gameLaunched']);self.assertFalse(result['approvedForExecution'])
            self.assertEqual(result['clientAdapterMode'],expected)
            self.assertEqual(json.loads(self.output.read_text())['clientAdapterMode'],expected)
            self.output.unlink()

    def test_refresh_rejects_invalid_old_or_override_mode_without_output(self):
        for mode in [None,True,False,0,1,{},[],'','FULL','network_only']:
            value=copy.deepcopy(self.profile);value['clientAdapterMode']=mode
            self.path.write_text(json.dumps(value),encoding='utf8')
            with self.subTest(mode=mode),self.assertRaises(ValueError):prepare.refresh(self.path,self.output,'full')
            self.assertFalse(self.output.exists())
        self.path.write_text(json.dumps(self.profile),encoding='utf8')
        for mode in [True,False,0,1,{},[],'','FULL','network_only']:
            with self.assertRaises(ValueError):prepare.refresh(self.path,self.output,mode)
            self.assertFalse(self.output.exists())


class WorkerTests(unittest.TestCase):
    def attempt(self,mode=None,*,discovery=False,guard=True,stopped=False,rpc_mode=None,exit_code=3221225477,late_exit=None,diagnostics_error=False):
        with tempfile.TemporaryDirectory(prefix='rc-mode-worker-authored-') as temp:
            lab=Path(temp).resolve();run=lab/'runs'/'synthetic';run.mkdir(parents=True)
            identity=lab/'identity.json';identity.write_text('{"methods":{}}')
            agent=lab/'inert-agent.js';agent.write_text('// Authored no-op; actual agent VM tested separately.')
            policy=lab/'inert-policy.js';policy.write_text('// Authored no-op.')
            manifest={'labRoot':str(lab),'vendor':str(lab),'methodIdentities':str(identity),'agent':str(agent),'policy':str(policy)}
            if mode is not None:manifest['clientAdapterMode']=mode
            effective='full' if mode is None else mode
            state={};observed={'resumed':False};callbacks=[]
            class PhaseTimeout(TimeoutError):pass
            class Ledger:
                def __init__(self,path,started,deadline,startup_deadline,base):
                    state.update(base);state.update(phase='preflight',errors=[],events=[],cleanup={},processResumed=False,agentLoaded=False,nativeLoginVerified=False,nativeRoomJoinVerified=False,runtimeRoutingHookUsed=False)
                    self.state=state
                def phase(self,name,*args,**kwargs):state.update(phase=name,phaseLimitSeconds=args[0])
                def completed(self):pass
                def save(self):pass
                def save_best_effort(self):return True
                def close(self):pass
                def call(self,frida,name,limit,operation,**kwargs):state['phase']=name;return operation()
                def event(self,event):
                    state['events'].append(event)
                    if event['kind']=='agent_loaded':state['agentLoaded']=True
                    if event['kind']=='client_route_adapter_ready':state['runtimeRoutingHookUsed']=True
            class Game:
                pid=7
                def image(self):return runner.GAME/'RecRoom.exe'
                def __init__(self):self.exit_code=exit_code;self.diagnostic_calls=0
                def diagnostics(self):
                    self.diagnostic_calls+=1
                    if self.diagnostic_calls==2:
                        if diagnostics_error:raise RuntimeError('Authored diagnostic query failure')
                        if late_exit is not None:self.exit_code=late_exit
                    return {'exitCodeBeforeCleanup':self.exit_code,'synthetic':True}
                def poll(self):return self.exit_code
                def wait(self,seconds):return self.exit_code
                def terminate(self):self.exit_code=124;observed['terminated']=True
                def close(self):observed['gameClosed']=True
            owned_game=Game()
            class Script:
                def __init__(self):self.exports_sync=SimpleNamespace(status=lambda:{'networkGuards':guard,'stopped':stopped,'clientAdapterMode':effective if rpc_mode is None else rpc_mode})
                def on(self,signal,callback):self.callback=callback
                def load(self):
                    self.callback({'type':'send','payload':{'kind':'agent_loaded','fixtureOnly':False,'clientAdapterMode':effective}},None)
                    if effective=='full':self.callback({'type':'send','payload':{'kind':'client_route_adapter_ready'}},None)
                    if discovery:self.callback({'type':'send','payload':{'kind':'native_discovery_deserialized','response':True,'localServiceFields':31,'statusNonempty':False}},None)
                def unload(self):observed['unloaded']=True
            script=Script()
            def create_script(code):
                observed['config']=json.loads(code.split('\n',1)[0][len('const CONFIG='):-1]);return script
            session=SimpleNamespace(create_script=create_script,detach=lambda:observed.update(detached=True))
            def resume(pid):observed['resumed']=True
            def spawn(command,**kwargs):
                observed['command']=command;observed['spawn']=kwargs
                self.assertEqual(command,[str(runner.GAME/'RecRoom.exe'),'+forcemode:screen','-screen-fullscreen','0','-screen-width','1280','-screen-height','720','-logFile',str(run/'private-player.log')])
                self.assertEqual(kwargs['cwd'],str(runner.GAME));self.assertEqual(kwargs['stdio'],'pipe')
                return 7
            device=SimpleNamespace(on=lambda signal,callback:callbacks.append(callback),off=lambda signal,callback:callbacks.remove(callback),
                spawn=spawn,attach=lambda pid:session,resume=resume)
            fake_frida=SimpleNamespace(__version__='17.24.3',get_local_device=lambda:device)
            fake_modules=(SimpleNamespace(current_job_membership=lambda *a,**k:True,retain_process=lambda pid:owned_game),
                SimpleNamespace(Ledger=Ledger,PhaseTimeout=PhaseTimeout,cancelled_call=lambda f,n,s,operation:operation()),
                SimpleNamespace(Win32=lambda:SimpleNamespace(is_elevated=lambda:True,create_new_section=lambda:1,close=lambda h:True,request_window_close=lambda pid:0)),
                SimpleNamespace(WindowsReader=lambda:SimpleNamespace(processes=lambda:[])))
            args=SimpleNamespace(job='synthetic',run_dir=str(run),started=time.monotonic(),deadline=time.monotonic()+5.02,startup_deadline=time.monotonic()+5)
            stdout,stderr,paths=sys.stdout,sys.stderr,list(sys.path)
            try:
                with patch.object(runner,'modules',return_value=fake_modules),patch.object(runner,'GAME',lab/'owned-game'),patch.object(runner,'backend_health',return_value={'instanceId':'synthetic'}),patch.dict(sys.modules,{'frida':fake_frida}):
                    result=runner.worker(args,manifest)
            finally:
                worker_stdout,worker_stderr=sys.stdout,sys.stderr
                sys.stdout,sys.stderr,sys.path=stdout,stderr,paths
                if worker_stdout is not stdout:worker_stdout.close()
                if worker_stderr is not stderr:worker_stderr.close()
            return result,state,observed

    def test_network_only_never_promotes_discovery_shaped_event_and_preserves_exit(self):
        result,state,observed=self.attempt('network-only',discovery=True)
        self.assertEqual(result,1);self.assertEqual(state['status'],'native_client_exited_nonzero');self.assertFalse(state['nativeDiscoveryParsed'])
        self.assertFalse(state['nativeLoginVerified']);self.assertFalse(state['nativeRoomJoinVerified']);self.assertFalse(state['runtimeRoutingRequested'])
        self.assertEqual(state['clientAdapterMode'],'network-only');self.assertTrue(observed['resumed']);self.assertIs(observed['config']['fixtureOnly'],False)
        self.assertEqual(observed['config']['clientAdapterMode'],'network-only')
        self.assertEqual(state['beforeProcessCleanup']['exitCodeBeforeCleanup'],3221225477);self.assertEqual(state['cleanup']['exitCode'],3221225477)
        self.assertTrue(observed['gameClosed']);self.assertTrue(observed['unloaded']);self.assertTrue(observed['detached'])

    def test_full_default_and_explicit_full_preserve_existing_discovery_semantics(self):
        for mode in (None,'full'):
            result,state,observed=self.attempt(mode,discovery=True,exit_code=0)
            self.assertEqual(result,0);self.assertTrue(state['nativeDiscoveryParsed']);self.assertEqual(state['clientAdapterMode'],'full')
            self.assertTrue(state['runtimeRoutingRequested']);self.assertEqual(observed['config']['clientAdapterMode'],'full')

    def test_clean_admission_without_discovery_is_still_result_two(self):
        for mode in (None,'full','network-only'):
            result,state,_observed=self.attempt(mode,exit_code=None)
            self.assertEqual(result,2);self.assertFalse(state['nativeDiscoveryParsed'])

    def test_discovery_event_does_not_turn_native_crash_into_success(self):
        result,state,observed=self.attempt('full',discovery=True)
        self.assertTrue(state['nativeDiscoveryParsed'])
        self.assertEqual(state['beforeProcessCleanup']['exitCodeBeforeCleanup'],3221225477)
        self.assertEqual(result,1)
        self.assertEqual(state['status'],'native_client_exited_nonzero')

    def test_cleanup_termination_does_not_erase_live_discovery_proof(self):
        result,state,observed=self.attempt('full',discovery=True,exit_code=None)
        self.assertEqual(result,0);self.assertTrue(state['nativeDiscoveryParsed'])
        self.assertIsNone(state['clientExitCodeBeforeCleanup']);self.assertIsNone(state['beforeProcessCleanup']['exitCodeBeforeCleanup'])
        self.assertEqual(state['cleanup']['exitCode'],124);self.assertTrue(observed['terminated'])

    def test_exit_after_observation_is_rechecked_before_cleanup(self):
        result,state,_observed=self.attempt('full',discovery=True,exit_code=0,late_exit=3221225477)
        self.assertEqual(result,1);self.assertTrue(state['nativeDiscoveryParsed'])
        self.assertEqual(state['clientExitCodeBeforeCleanup'],3221225477)

    def test_unavailable_precleanup_diagnostics_cannot_claim_success(self):
        result,state,_observed=self.attempt('full',discovery=True,exit_code=0,diagnostics_error=True)
        self.assertEqual(result,1);self.assertTrue(state['nativeDiscoveryParsed'])
        self.assertEqual(state['status'],'native_client_exit_unverified')

    def test_partial_guards_stopped_or_mismatched_agent_mode_never_resume(self):
        for options in [{'guard':False},{'stopped':True},{'rpc_mode':'full'}]:
            result,state,observed=self.attempt('network-only',**options)
            self.assertEqual(result,1);self.assertFalse(observed['resumed']);self.assertFalse(state['processResumed'])
            self.assertEqual(state['cleanup']['exitCode'],3221225477)


if __name__=='__main__':unittest.main(verbosity=2)
