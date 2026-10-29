"""Pure preparation controls: no original files, network, WFP, jobs or processes."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path, PureWindowsPath
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('pristine_planner',ROOT/'pristine-control.py')
planner=importlib.util.module_from_spec(spec);spec.loader.exec_module(planner)
LOCAL=r'C:\OwnedFixture\Local'
LAB=str(PureWindowsPath(LOCAL)/'Metallic-RC'/'native-client-lab-20261021')
ENV={'LOCALAPPDATA':LOCAL}


def candidate():
    return {'schemaVersion':1,'steamBuildId':'23191908','gameRoot':planner.GAME,'labRoot':LAB,
            'runDirectory':str(PureWindowsPath(LAB)/'runs'/'authored_fixture'),
            'files':[{'path':str(PureWindowsPath(planner.GAME)/name),'sha256':digest,'bytes':size} for name,(digest,size) in planner.PINS.items()],
            'budgets':{'totalWorkerSeconds':90,'startupSeconds':30,'outerSupervisorWaitSeconds':130}}


class PreparationTests(unittest.TestCase):
    def test_plan_is_nonexecuting_and_keeps_exact_launch_arguments(self):
        value=candidate();before=copy.deepcopy(value)
        with patch.object(Path,'open',side_effect=AssertionError('Pure planner must not read or write files')):
            result=planner.plan(value,{**ENV,'SystemDrive':'C:','PATH':'synthetic-private-path','SECRET_TOKEN':'not-for-output'})
        self.assertEqual(value,before)
        self.assertEqual(result['argv'],[r'C:\Program Files (x86)\Steam\steamapps\common\RecRoom\RecRoom.exe','+forcemode:screen','-screen-fullscreen','0','-screen-width','1280','-screen-height','720','-logFile',str(PureWindowsPath(value['runDirectory'])/'private-player.log')])
        self.assertEqual(result['cwd'],planner.GAME)
        self.assertTrue(result['gameWillNotRun']);self.assertFalse(result['executionReady'])
        self.assertFalse(result['newNetworkMethodApproved']);self.assertFalse(result['originalBytesVerified'])
        self.assertIn('fixture_not_yet_verified',result['blockingReasons'])
        self.assertIn('native_executor_not_implemented',result['blockingReasons'])
        self.assertEqual(result['attachment'],'not_applicable_no_frida')
        self.assertNotIn('synthetic-private-path',json.dumps(result));self.assertNotIn('not-for-output',json.dumps(result))

    def test_section_private_outputs_budgets_and_admission_order_are_explicit(self):
        result=planner.plan(candidate(),dict(ENV))
        self.assertEqual(result['budgets'],{'totalWorkerSeconds':90,'startupSeconds':30,'outerSupervisorWaitSeconds':130})
        self.assertEqual(result['sharedSectionPrerequisite'],{'name':r'Global\KjMpQrStUvWxYzAb','bytes':66060456,'futureCreateNewRequired':True,'existingSectionMustBeRefused':True})
        self.assertTrue(result['privateOutputs']['createNewDirectoryAndOwnerAclRequired'])
        order=result['requiredOrder']
        for first,second in [('install_and_verify_separately_approved_external_guards','create_target_suspended_without_inherited_handles'),('create_target_suspended_without_inherited_handles','retain_and_verify_exact_process_image'),('retain_and_verify_exact_process_image','assign_and_verify_kill_on_close_no_breakaway_job'),('assign_and_verify_kill_on_close_no_breakaway_job','resume_only_after_all_admissions'),('capture_precleanup_exit_evidence','close_or_terminate_only_owned_process_tree'),('close_or_terminate_only_owned_process_tree','verify_job_empty_before_releasing_external_guards')]:
            self.assertLess(order.index(first),order.index(second))

    def test_environment_preserves_existing_twenty_name_allowlist_without_full_inheritance(self):
        expected=('SystemRoot','SystemDrive','WINDIR','PATH','COMSPEC','TEMP','TMP','LOCALAPPDATA','APPDATA','USERPROFILE','HOMEDRIVE','HOMEPATH','ProgramData','ProgramFiles','ProgramFiles(x86)','PUBLIC','USERNAME','USERDOMAIN','NUMBER_OF_PROCESSORS','PROCESSOR_ARCHITECTURE')
        source={key.lower():'value-'+key for key in reversed(expected)}
        source.update(SECRET_TOKEN='private',UNRELATED='ignore',localappdata=LOCAL)
        selected=planner.select_environment(source)
        self.assertEqual(tuple(selected),expected)
        self.assertEqual(selected['SystemDrive'],'value-SystemDrive')
        self.assertNotIn('SECRET_TOKEN',selected);self.assertNotIn('UNRELATED',selected)
        self.assertEqual(planner.plan(candidate(),source)['environmentNames'],list(expected))
        self.assertEqual(planner.select_environment({}),{})
        with self.assertRaises(ValueError):planner.select_environment({'PATH':'a','Path':'b'})

    def test_localappdata_must_be_supplied_and_valid_without_fallback(self):
        for source in ({},{'APPDATA':LOCAL},{'LOCALAPPDATA':''},{'LOCALAPPDATA':'relative'},{'LOCALAPPDATA':r'\\server\share'},{'LOCALAPPDATA':r'C:\OwnedFixture\..\Local'}):
            with self.subTest(environment=source),self.assertRaises(ValueError):planner.plan(candidate(),source)
        self.assertTrue(planner.plan(candidate(),{'localappdata':LOCAL})['gameWillNotRun'])
        with self.assertRaisesRegex(ValueError,'exact_lab_root_required'):
            planner.plan(candidate(),{'LOCALAPPDATA':r'C:\DifferentOwner\Local'})

    def test_candidate_cannot_grant_network_or_execution_authority(self):
        for key in ('newNetworkMethodApproved','executionReady','fixturePassed','run','target','argv','environment'):
            value=candidate();value[key]=True
            with self.subTest(key=key),self.assertRaises(ValueError):planner.plan(value,dict(ENV))

    def test_exact_game_lab_and_owned_run_paths(self):
        values=[('gameRoot',r'C:\other'),('labRoot',r'C:\other'),('runDirectory',r'C:\outside\run'),('runDirectory',LAB+r'\runs\..\escape'),('runDirectory',LAB+r'\runs\nested\child'),('runDirectory',LAB+r'\runs\CON'),('runDirectory',LAB+r'\runs\item:stream'),('runDirectory',r'\\server\share\run'),('runDirectory',LAB+r'\runs\trailing.'),('runDirectory','C:relative'),('runDirectory',123)]
        for key,value in values:
            record=candidate();record[key]=value
            with self.subTest(key=key,value=value),self.assertRaises(ValueError):planner.plan(record,dict(ENV))

    def test_rejects_target_substitution_changed_missing_duplicate_or_extra_pins(self):
        for name in ('RecRoom.exe','GameAssembly.dll','Referee.dll'):
            for key,bad in [('sha256','0'*64),('bytes',0),('bytes',True),('path',str(PureWindowsPath(r'C:\other')/name))]:
                value=candidate();row=next(x for x in value['files'] if PureWindowsPath(x['path']).name==name);row[key]=bad
                with self.subTest(name=name,key=key),self.assertRaises(ValueError):planner.plan(value,dict(ENV))
        for rows in (candidate()['files'][:-1],candidate()['files']+[candidate()['files'][0]],[candidate()['files'][0]]*3):
            value=candidate();value['files']=rows
            with self.assertRaises(ValueError):planner.plan(value,dict(ENV))
        value=candidate();value['files'][0]['extra']='not accepted'
        with self.assertRaises(ValueError):planner.plan(value,dict(ENV))

    def test_schema_build_and_all_budget_values_are_strict(self):
        for key,bad in [('schemaVersion',True),('schemaVersion',3),('steamBuildId','different')]:
            value=candidate();value[key]=bad
            with self.assertRaises(ValueError):planner.plan(value,dict(ENV))
        for key in ('totalWorkerSeconds','startupSeconds','outerSupervisorWaitSeconds'):
            for bad in (True,0,1.0,180):
                value=candidate();value['budgets'][key]=bad
                with self.subTest(key=key,value=bad),self.assertRaises(ValueError):planner.plan(value,dict(ENV))

    def test_unknown_or_run_arguments_reject_before_any_file_access(self):
        for args in (['--candidate','unused.json','--run'],['--candidate','unused.json','--allow-network'],['--cand','unused.json']):
            with self.subTest(args=args),patch.object(Path,'open',side_effect=AssertionError('Input must not open')),contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit) as caught:
                planner.main(args)
            self.assertEqual(caught.exception.code,2)

    def test_cli_reads_only_bounded_candidate_and_prints_nonexecution_plan(self):
        with tempfile.TemporaryDirectory(prefix='pristine-plan-authored-') as tmp:
            path=Path(tmp)/'candidate.json';path.write_text(json.dumps(candidate()),encoding='utf8');before=path.read_bytes()
            output=io.StringIO()
            with patch.object(planner.os,'environ',{**ENV,'SystemDrive':'C:','SECRET':'private'}),contextlib.redirect_stdout(output):
                self.assertEqual(planner.main(['--candidate',str(path)]),0)
            result=json.loads(output.getvalue());self.assertFalse(result['executionReady']);self.assertTrue(result['gameWillNotRun'])
            self.assertEqual(path.read_bytes(),before);self.assertEqual(list(Path(tmp).iterdir()),[path])
            path.write_bytes(b' '*65537)
            with self.assertRaisesRegex(ValueError,'candidate_input_limit'):planner.main(['--candidate',str(path)])
            path.write_text('{"schemaVersion":1,"schemaVersion":1}',encoding='utf8')
            with self.assertRaisesRegex(ValueError,'duplicate_json_property'):planner.main(['--candidate',str(path)])


if __name__=='__main__':unittest.main(verbosity=2)
