"""Authored candidate data only; no original files, processes, sections or guards."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path, PureWindowsPath
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('baselines', Path(__file__).with_name('pristine-baselines.py'))
planner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(planner)
LOCAL = r'C:\AuthoredFixture\Local'
LAB = str(PureWindowsPath(LOCAL)/'Metallic-RC'/'native-client-lab-20261021')
ENV = {'LOCALAPPDATA': LOCAL, 'SystemDrive': 'C:'}


def candidate():
    return dict(schemaVersion=2, steamBuildId='23191908', gameRoot=planner._V1.GAME, labRoot=LAB,
                runDirectory=str(PureWindowsPath(LAB)/'runs'/'authored_baseline'),
                files=[dict(path=str(PureWindowsPath(planner._V1.GAME)/name), sha256=digest, bytes=size)
                       for name, (digest, size) in planner.PINS.items()], budgets=dict(planner._V1.BUDGETS))


class BaselinesTests(unittest.TestCase):
    def test_default_no_section_does_not_assert_absence_or_plan_object_operations(self):
        with patch.object(Path, 'open', side_effect=AssertionError('Pure plan must not open files')):
            result = planner.plan(candidate(), ENV)
        self.assertEqual(result['variant'], 'no-authored-section')
        section = result['sectionPlan']
        for key in ['authoredCreationPlanned', 'mappingViewPlanned', 'headerInitializationPlanned',
                    'consumerPlanned', 'automaticFallback', 'experimental']:
            self.assertFalse(section[key])
        self.assertEqual(section['vendorObjectPresence'], 'unknown_not_observed')
        self.assertTrue(section['existingVendorObjectsAndServicesUntouched'])
        self.assertNotIn('sharedSectionPrerequisite', result)
        self.assertNotIn('KjMpQrStUvWxYzAb', json.dumps(result))
        self.assertFalse(any('section' in step for step in result['requiredOrder']))

    def test_experimental_variant_is_explicit_create_new_without_initialized_queues(self):
        result = planner.plan(candidate(), ENV, 'experimental-zero-section')
        section = result['sectionPlan']
        self.assertTrue(section['experimental'])
        self.assertTrue(section['authoredCreationPlanned'])
        self.assertTrue(section['futureCreateNewRequired'])
        self.assertTrue(section['existingSectionMustBeRefused'])
        self.assertEqual((section['name'], section['bytes']), (r'Global\KjMpQrStUvWxYzAb', 66060456))
        self.assertEqual(section['initialContents'], 'windows_zero_initialized')
        self.assertFalse(section['headerInitializationPlanned'])
        self.assertFalse(section['consumerPlanned'])
        self.assertFalse(section['automaticFallback'])
        self.assertIn('hypothesis', section['evidenceStatus'])
        self.assertEqual(result['requiredOrder'].count('create_new_experimental_zero_section_if_absent'), 1)

    def test_both_variants_stay_blocked_and_preserve_source_inputs(self):
        value = candidate()
        before = copy.deepcopy(value)
        old_pins = copy.deepcopy(planner._V1.PINS)
        environment = {**ENV, 'PATH': 'secret-path-value', 'SECRET_TOKEN': 'private-value'}
        for variant in planner.VARIANTS:
            result = planner.plan(value, environment, variant)
            self.assertTrue(result['gameWillNotRun'])
            for flag in ['executionReady', 'newNetworkMethodApproved', 'originalBytesVerified']:
                self.assertFalse(result[flag])
            self.assertIn('target_guard_profile_not_verified', result['blockingReasons'])
            self.assertIn('guard_receipt_provenance_not_verified', result['blockingReasons'])
            self.assertIn('native_executor_not_implemented', result['blockingReasons'])
            self.assertIn('original_bytes_not_verified', result['blockingReasons'])
            self.assertNotIn('secret-path-value', json.dumps(result))
            self.assertNotIn('private-value', json.dumps(result))
        self.assertEqual(value, before)
        self.assertEqual(planner._V1.PINS, old_pins)
        self.assertEqual(len(old_pins), 3)
        self.assertEqual(environment['PATH'], 'secret-path-value')

    def test_four_pins_include_independently_recorded_unity_identity(self):
        result = planner.plan(candidate(), ENV)
        self.assertEqual(len(result['declaredOriginalPins']), 4)
        unity = next(row for row in result['declaredOriginalPins'] if row['path'].endswith('UnityPlayer.dll'))
        self.assertEqual(unity['sha256'], '30f4a99c49566b2c5045469b5a39572dd2298adc31f7fe2bc39d8bb8fc9d75bb')
        self.assertEqual(unity['bytes'], 32858656)
        for name in planner.PINS:
            for key, bad in [('sha256', '0'*64), ('bytes', True), ('bytes', float(planner.PINS[name][1])),
                             ('path', str(PureWindowsPath(r'C:\Elsewhere')/name))]:
                value = candidate()
                row = next(r for r in value['files'] if PureWindowsPath(r['path']).name == name)
                row[key] = bad
                with self.subTest(name=name, key=key), self.assertRaises(ValueError):
                    planner.plan(value, ENV)

    def test_missing_extra_duplicate_alias_and_unknown_pin_fields_refuse(self):
        rows = candidate()['files']
        duplicate = copy.deepcopy(rows)
        duplicate[-1] = {**rows[0], 'path': rows[0]['path'].swapcase()}
        for bad in [rows[:-1], rows+[rows[0]], duplicate, tuple(rows), None]:
            value = candidate()
            value['files'] = bad
            with self.assertRaises(ValueError):
                planner.plan(value, ENV)
        value = candidate()
        value['files'][0]['verified'] = True
        with self.assertRaises(ValueError):
            planner.plan(value, ENV)

    def test_exact_argv_cwd_budgets_and_three_stdio_allowlist(self):
        result = planner.plan(candidate(), ENV)
        self.assertEqual(result['argv'], [r'C:\Program Files (x86)\Steam\steamapps\common\RecRoom\RecRoom.exe',
                                        '+forcemode:screen', '-screen-fullscreen', '0', '-screen-width', '1280',
                                        '-screen-height', '720', '-logFile',
                                        str(PureWindowsPath(candidate()['runDirectory'])/'private-player.log')])
        self.assertEqual(result['cwd'], planner._V1.GAME)
        self.assertEqual(result['budgets'], {'totalWorkerSeconds': 90, 'startupSeconds': 30, 'outerSupervisorWaitSeconds': 130})
        handles = result['stdioPlan']
        self.assertEqual(handles['allowedInheritedHandles'], ['stdin', 'stdout_write', 'stderr_write'])
        self.assertTrue(handles['explicitHandleListRequired'])
        for key in ['unlistedHandleInheritance', 'parentReadEndsInheritable', 'jobHandleInheritable']:
            self.assertFalse(handles[key])
        order = result['requiredOrder']
        self.assertNotIn('create_target_suspended_without_inherited_handles', order)
        for first, second in [('install_and_verify_separately_approved_external_guards', 'create_target_suspended_with_only_three_stdio_handles'),
                              ('assign_and_verify_kill_on_close_no_breakaway_job', 'resume_only_after_all_admissions'),
                              ('close_or_terminate_only_owned_process_tree', 'verify_job_empty_before_releasing_external_guards')]:
            self.assertLess(order.index(first), order.index(second))

    def test_schema_and_budget_types_cannot_coerce(self):
        for key, bad in [('schemaVersion', True), ('schemaVersion', 1), ('schemaVersion', 2.0), ('steamBuildId', 23191908)]:
            value = candidate()
            value[key] = bad
            with self.assertRaises(ValueError):
                planner.plan(value, ENV)
        for key in planner._V1.BUDGETS:
            for bad in [True, 90.0, None, '90', 1300]:
                value = candidate()
                value['budgets'][key] = bad
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                    planner.plan(value, ENV)

    def test_candidates_and_bad_variants_cannot_grant_authority_or_fallback(self):
        for key in ['fixturePassed', 'guardReceipt', 'executionReady', 'run', 'variant', 'sectionName', 'argv']:
            value = candidate()
            value[key] = {'ok': True}
            with self.subTest(key=key), self.assertRaises(ValueError):
                planner.plan(value, ENV)
        for variant in [None, True, '', 'zero', 'NO-AUTHORED-SECTION']:
            with self.assertRaises(ValueError):
                planner.plan(candidate(), ENV, variant)
        invalid = candidate()
        invalid['files'].pop()
        for variant in planner.VARIANTS:
            with self.assertRaises(ValueError):
                planner.plan(invalid, ENV, variant)

    def test_paths_local_owner_and_unicode_remain_strict(self):
        for key, bad in [('gameRoot', r'C:\Other'), ('labRoot', r'C:\Other'),
                         ('runDirectory', r'\\host\share\run'), ('runDirectory', LAB+r'\runs\..\escape'),
                         ('runDirectory', LAB+r'\runs\CON'), ('runDirectory', LAB+r'\runs\item:stream'),
                         ('runDirectory', LAB+r'\runs\trailing.'), ('runDirectory', LAB+'\runs\bad\ud800')]:
            value = candidate()
            value[key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError):
                planner.plan(value, ENV)
        for environment in [{}, {'LOCALAPPDATA': r'C:\DifferentOwner'}, {'LOCALAPPDATA': r'\\host\share'},
                            {**ENV, 'PATH': 'bad\0value'}, {**ENV, 'PATH': '\ud800'},
                            {**ENV, 'PATH': 'a', 'Path': 'b'}]:
            with self.assertRaises(ValueError):
                planner.plan(candidate(), environment)

    def test_all_twenty_environment_names_without_values_or_secret_names(self):
        source = {key.lower(): 'sensitive-'+key for key in planner._V1.ENVIRONMENT}
        source.update(localappdata=LOCAL, SECRET_TOKEN='hidden')
        result = planner.plan(candidate(), source)
        self.assertEqual(result['environmentNames'], list(planner._V1.ENVIRONMENT))
        self.assertEqual(len(result['environmentNames']), 20)
        self.assertIn('SystemDrive', result['environmentNames'])
        self.assertNotIn('sensitive-', json.dumps(result))
        self.assertNotIn('SECRET_TOKEN', json.dumps(result))

    def test_run_unknown_abbreviated_or_invalid_choice_reject_before_file_open(self):
        for extra in [['--run'], ['--variant', 'unknown'], ['--var', 'no-authored-section'], ['--allow-network']]:
            with patch.object(Path, 'open', side_effect=AssertionError('No candidate read allowed')), \
                    contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as caught:
                planner.main(['--candidate', 'unused.json', *extra])
            self.assertEqual(caught.exception.code, 2)

    def test_cli_only_reads_authored_candidate_and_preserves_files(self):
        with tempfile.TemporaryDirectory(prefix='baseline-v2-authored-') as tmp:
            path = Path(tmp)/'candidate.json'
            path.write_text(json.dumps(candidate()), encoding='utf8')
            before, stamp = path.read_bytes(), path.stat().st_mtime_ns
            for extra, variant in [([], 'no-authored-section'), (['--variant', 'experimental-zero-section'], 'experimental-zero-section')]:
                output = io.StringIO()
                with patch.object(planner.os, 'environ', {**ENV, 'SECRET': 'private'}), contextlib.redirect_stdout(output):
                    self.assertEqual(planner.main(['--candidate', str(path), *extra]), 0)
                result = json.loads(output.getvalue())
                self.assertEqual(result['variant'], variant)
                self.assertFalse(result['executionReady'])
                self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), (before, stamp))
                self.assertEqual(list(Path(tmp).iterdir()), [path])

    def test_json_size_duplicates_nonfinite_and_invalid_unicode_refuse(self):
        with tempfile.TemporaryDirectory(prefix='baseline-v2-json-') as tmp:
            path = Path(tmp)/'candidate.json'
            for data, reason in [(b' '*65537, 'candidate_input_limit'),
                                 (b'{"budgets":{"totalWorkerSeconds":90,"totalWorkerSeconds":90}}', 'duplicate_json_property'),
                                 (b'{"schemaVersion":2,"schemaVersion":2}', 'duplicate_json_property'),
                                 (b'{"x":NaN}', 'nonfinite_json'), (b'{"x":Infinity}', 'nonfinite_json'),
                                 (b'\xff', 'candidate_json_invalid'), (b'['*2000+b']'*2000, 'candidate_json_invalid'),
                                 (b'{"x":'+b'9'*5000+b'}', 'candidate_json_invalid')]:
                path.write_bytes(data)
                with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                    planner.read_candidate(path)


if __name__ == '__main__':
    unittest.main(verbosity=2)
