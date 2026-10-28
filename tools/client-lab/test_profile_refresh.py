"""Synthetic profile-data refresh checks; no real profiles, game files or native code are used."""
from __future__ import annotations
import contextlib,copy,hashlib,importlib.util,io,json,sys
from pathlib import Path
import tempfile,unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('profile_refresher',Path(__file__).with_name('prepare-manifest.py'))
module=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(module)

class RefreshChecks(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='mrc-profile-refresh-')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.old=self.root/'old';self.new=self.root/'new'
        self.old.mkdir();self.new.mkdir()
        self.records=[]
        for name in module.AUTHORED:
            for folder in (self.old,self.new):
                (folder/name).write_bytes(b"raise AssertionError('profile input must never execute')\n"+folder.name.encode()+name.encode())
            self.records.append(self.pin(self.old/name))
        for relative in ('original/RecRoom.exe','original/GameAssembly.dll','vendor/frida/_frida.pyd',
                         'vendor/supervision.py','extra/runtime-agent.js','extra/extra-note.bin'):
            path=self.root/relative;path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(b'INERT SYNTHETIC INPUT '+relative.encode())
            self.records.append(self.pin(path)|{'preservedMetadata':{'label':relative}})
        self.profile={'schemaVersion':3,'steamBuildId':'23191908','port':4187,
            'agent':str(self.old/'runtime-agent.js'),'policy':str(self.old/'route-policy.js'),
            'runnerPath':str(self.old/'run-native-client.py'),'runnerSha256':module.sha(self.old/'run-native-client.py'),
            'gameRoot':str(self.root/'original'),'vendor':str(self.root/'vendor'),
            'scope':'untrusted stronger permissions must not carry forward',
            'budgets':{'totalWorkerSeconds':99999},'extraField':{'retained':'data only'},'files':self.records}
        self.previous=self.root/'previous.json';self.output=self.root/'candidate.json'

    @staticmethod
    def pin(path):
        return {'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'bytes':path.stat().st_size}

    def write_previous(self,profile=None):
        self.previous.write_text(json.dumps(self.profile if profile is None else profile),encoding='utf8')

    def refresh(self,profile=None):
        self.write_previous(profile)
        before=self.previous.read_bytes()
        with patch.object(module,'ROOT',self.new):result=module.refresh(self.previous,self.output)
        self.assertEqual(self.previous.read_bytes(),before)
        return result,json.loads(self.output.read_text(encoding='utf8'))

    def rejected(self,profile=None,error=ValueError):
        self.write_previous(profile)
        before=self.previous.read_bytes()
        with patch.object(module,'ROOT',self.new),self.assertRaises(error):module.refresh(self.previous,self.output)
        self.assertFalse(self.output.exists());self.assertEqual(self.previous.read_bytes(),before)

    def test_v3_refresh_verifies_pins_preserves_non_authored_records_and_creates_candidate_only(self):
        result,profile=self.refresh()
        self.assertEqual(result['status'],'v3_candidate_manifest_created')
        self.assertFalse(result['gameLaunched']);self.assertFalse(result['approvedForExecution'])
        self.assertEqual(profile['schemaVersion'],3);self.assertEqual(profile['steamBuildId'],'23191908')
        self.assertEqual(profile['port'],4187);self.assertEqual(profile['gameRoot'],self.profile['gameRoot'])
        self.assertEqual(profile['vendor'],self.profile['vendor']);self.assertEqual(profile['extraField'],self.profile['extraField'])
        retained=[row for row in self.records if Path(row['path']).parent!=self.old]
        self.assertEqual(profile['files'][:len(retained)],retained)
        self.assertEqual(profile['files'][len(retained):],[self.pin(self.new/name) for name in module.AUTHORED])
        self.assertEqual(profile['runnerSha256'],module.sha(self.new/'run-native-client.py'))
        self.assertEqual(profile['runnerPath'],str(self.new/'run-native-client.py'))
        self.assertEqual(profile['agent'],str(self.new/'runtime-agent.js'))
        self.assertEqual(profile['policy'],str(self.new/'route-policy.js'))
        self.assertEqual(profile['budgets'],{'totalWorkerSeconds':90,'startupSeconds':30,'attachSeconds':15,'outerSupervisorWaitSeconds':130})
        self.assertEqual(profile['maxProgressSnapshotBytes'],2_097_152);self.assertEqual(profile['maxProgressGenerations'],4096)
        self.assertEqual(profile['progressTransport'],'sqlite-wal-single-publisher')
        self.assertNotIn('stronger permissions',profile['scope']);self.assertIn('separately approved',profile['scope'])

    def test_v1_v2_without_runner_metadata_remain_supported(self):
        for schema in (1,2):
            with self.subTest(schema=schema):
                profile=copy.deepcopy(self.profile);profile['schemaVersion']=schema
                del profile['runnerPath'];del profile['runnerSha256']
                self.refresh(profile);self.output.unlink()

    def test_legacy_hash_only_requires_the_exact_pinned_agent_sibling_runner(self):
        for schema in (1,2):
            profile=copy.deepcopy(self.profile);profile['schemaVersion']=schema;del profile['runnerPath']
            self.refresh(profile);self.output.unlink()
            profile['files']=[row for row in profile['files'] if Path(row['path'])!=self.old/'run-native-client.py']
            self.rejected(profile)

    def test_v3_missing_or_unpinned_runner_metadata_is_rejected(self):
        for field in ('runnerPath','runnerSha256'):
            with self.subTest(field=field):
                profile=copy.deepcopy(self.profile);del profile[field];self.rejected(profile)
        profile=copy.deepcopy(self.profile)
        profile['files']=[row for row in profile['files'] if Path(row['path'])!=self.old/'run-native-client.py']
        self.rejected(profile)
        profile=copy.deepcopy(self.profile);profile['runnerPath']=str(self.root/'unknown.py');self.rejected(profile)

    def test_mismatched_old_runner_hash_is_rejected_in_all_versions(self):
        for schema in (1,2,3):
            profile=copy.deepcopy(self.profile);profile['schemaVersion']=schema;profile['runnerSha256']='0'*64
            self.rejected(profile)

    def test_any_old_changed_or_missing_pin_prevents_output(self):
        path=self.root/'extra/extra-note.bin';original=path.read_bytes()
        path.write_bytes(b'changed');self.rejected()
        path.write_bytes(original);path.unlink();self.rejected(error=FileNotFoundError)

    def test_changed_old_runner_bytes_cannot_be_accepted_via_metadata_alone(self):
        path=self.old/'run-native-client.py';path.write_bytes(b'changed runner')
        profile=copy.deepcopy(self.profile);profile['runnerSha256']=module.sha(path)
        self.rejected(profile)

    def test_duplicate_pin_paths_and_normalized_aliases_are_rejected(self):
        for alias in (str(self.old/'run-native-client.py'),str(self.old/'alias'/'..'/'run-native-client.py')):
            profile=copy.deepcopy(self.profile);row=copy.deepcopy(profile['files'][0]);row['path']=alias
            profile['files'].append(row);self.rejected(profile)

    def test_missing_old_agent_or_policy_pin_is_rejected(self):
        for field in ('agent','policy'):
            profile=copy.deepcopy(self.profile)
            profile['files']=[row for row in profile['files'] if row['path']!=profile[field]]
            self.rejected(profile)

    def test_bad_schema_build_port_paths_and_pin_shapes_are_rejected(self):
        for field,value in [('schemaVersion',True),('schemaVersion',4),('steamBuildId','other'),('port',4188),('port',4187.0),('files',[])]:
            profile=copy.deepcopy(self.profile);profile[field]=value;self.rejected(profile)
        for field,value in [('path','relative.py'),('bytes',True),('bytes',-1),('sha256','invalid')]:
            profile=copy.deepcopy(self.profile);profile['files'][0][field]=value;self.rejected(profile)

    def test_duplicate_json_keys_are_rejected_without_output(self):
        self.previous.write_text('{"schemaVersion":3,"schemaVersion":2}',encoding='utf8')
        with patch.object(module,'ROOT',self.new),self.assertRaises(ValueError):module.refresh(self.previous,self.output)
        self.assertFalse(self.output.exists())

    def test_existing_output_same_path_and_alias_are_never_overwritten(self):
        self.write_previous();before=self.previous.read_bytes()
        self.output.write_bytes(b'preserved existing output')
        with patch.object(module,'ROOT',self.new),self.assertRaises(FileExistsError):module.refresh(self.previous,self.output)
        self.assertEqual(self.output.read_bytes(),b'preserved existing output');self.output.unlink()
        for output in (self.previous,self.root/'alias'/'..'/'previous.json'):
            with patch.object(module,'ROOT',self.new),self.assertRaises(FileExistsError):module.refresh(self.previous,output)
        self.assertEqual(self.previous.read_bytes(),before)

    def test_missing_new_source_is_detected_before_output_is_created(self):
        (self.new/module.AUTHORED[-1]).unlink();self.rejected(error=FileNotFoundError)

    def test_exclusive_creation_preserves_an_output_created_after_initial_checks(self):
        self.write_previous();original_open=Path.open
        def raced_open(path,mode='r',*args,**kwargs):
            if path==self.output and mode=='x':
                with original_open(path,'wb') as stream:stream.write(b'racing output must survive')
            return original_open(path,mode,*args,**kwargs)
        with patch.object(module,'ROOT',self.new),patch.object(Path,'open',raced_open),self.assertRaises(FileExistsError):
            module.refresh(self.previous,self.output)
        self.assertEqual(self.output.read_bytes(),b'racing output must survive')

    def test_cli_reports_candidate_only_without_running_any_pinned_source(self):
        self.write_previous();stdout=io.StringIO()
        with patch.object(module,'ROOT',self.new),patch.object(sys,'argv',
                ['prepare-manifest.py','--previous',str(self.previous),'--output',str(self.output)]),contextlib.redirect_stdout(stdout):
            module.main()
        receipt=json.loads(stdout.getvalue())
        self.assertFalse(receipt['gameLaunched']);self.assertFalse(receipt['approvedForExecution'])
        self.assertEqual(receipt['manifestSha256'],module.sha(self.output))

    def test_retained_vendor_basename_is_not_mistaken_for_authored_source(self):
        _result,profile=self.refresh()
        for path in (self.root/'vendor/supervision.py',self.root/'extra/runtime-agent.js'):
            self.assertIn(next(row for row in self.records if row['path']==str(path)),profile['files'])

    def test_runner_metadata_uses_the_same_captured_pin_if_source_later_changes(self):
        original_sha=module.sha;runner=self.new/'run-native-client.py'
        captured=original_sha(runner);runner_hash_calls=[]
        def mutate_later_source(path):
            if path==runner:runner_hash_calls.append(path)
            if path==self.new/module.AUTHORED[-1]:runner.write_bytes(b'changed synthetic runner after its pin was captured')
            return original_sha(path)
        with patch.object(module,'sha',mutate_later_source):_result,profile=self.refresh()
        runner_pin=next(row for row in profile['files'] if row['path']==str(runner))
        self.assertEqual(runner_hash_calls,[runner])
        self.assertEqual(profile['runnerSha256'],captured)
        self.assertEqual(profile['runnerSha256'],runner_pin['sha256'])
        self.assertNotEqual(original_sha(runner),runner_pin['sha256'])

    def test_new_authored_location_cannot_replace_an_unrelated_retained_pin(self):
        self.profile['files'].append(self.pin(self.new/'supervision.py'))
        self.rejected()

if __name__=='__main__':unittest.main(verbosity=2)
