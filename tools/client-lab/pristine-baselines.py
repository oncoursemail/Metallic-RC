"""Pure v2 baseline plans. No client, section, guard or service executor."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath

_SPEC = importlib.util.spec_from_file_location('_pristine_v1', Path(__file__).with_name('pristine-control.py'))
_V1 = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_V1)
VARIANTS = ('no-authored-section', 'experimental-zero-section')
PINS = {**_V1.PINS, 'UnityPlayer.dll': (
    '30f4a99c49566b2c5045469b5a39572dd2298adc31f7fe2bc39d8bb8fc9d75bb', 32858656)}
_KEYS = {'schemaVersion', 'steamBuildId', 'gameRoot', 'labRoot', 'runDirectory', 'files', 'budgets'}


def text(value, reason):
    _V1.require(type(value) is str and '\0' not in value, reason)
    try:
        value.encode('utf-16-le', 'strict')
    except UnicodeError:
        raise ValueError(reason) from None


def plan(candidate, environment, variant=VARIANTS[0]):
    _V1.require(type(variant) is str and variant in VARIANTS, 'closed_variant_required')
    _V1.require(type(candidate) is dict and set(candidate) == _KEYS, 'closed_v2_candidate_required')
    _V1.require(type(candidate['schemaVersion']) is int and candidate['schemaVersion'] == 2, 'v2_schema_required')
    for key in ('gameRoot', 'labRoot', 'runDirectory'):
        text(candidate[key], 'path_unicode_required')
    selected = _V1.select_environment(environment)
    for value in selected.values():
        text(value, 'environment_unicode_required')
    rows = candidate['files']
    _V1.require(type(rows) is list and len(rows) == 4, 'four_original_pins_required')
    records = {}
    for row in rows:
        _V1.require(type(row) is dict and set(row) == {'path', 'sha256', 'bytes'}, 'closed_pin_record_required')
        text(row['path'], 'path_unicode_required')
        key = _V1.windows_path(row['path'])
        _V1.require(key not in records, 'duplicate_pin_path')
        records[key] = row
    for name, (digest, size) in PINS.items():
        row = records.get(_V1.windows_path(str(PureWindowsPath(_V1.GAME)/name)))
        _V1.require(row is not None and type(row['sha256']) is str and row['sha256'] == digest and
                    type(row['bytes']) is int and row['bytes'] == size, 'exact_original_pin_required')
    # Reuse v1 validation without mutating its constants or the caller's data.
    legacy = {**candidate, 'schemaVersion': 1,
              'files': [records[_V1.windows_path(str(PureWindowsPath(_V1.GAME)/name))]
                        for name in _V1.PINS]}
    result = _V1.plan(legacy, environment)
    result['schema'] = 'pristine-baseline-preparation-v2'
    result['variant'] = variant
    result['declaredOriginalPins'] = [
        {'path': str(PureWindowsPath(_V1.GAME)/name), 'sha256': digest, 'bytes': size}
        for name, (digest, size) in PINS.items()]
    result['blockingReasons'] = ['target_guard_profile_not_verified', 'guard_receipt_provenance_not_verified',
                                 'native_executor_not_implemented', 'original_bytes_not_verified']
    result.pop('sharedSectionPrerequisite')
    experimental = variant == VARIANTS[1]
    result['sectionPlan'] = {
        'authoredCreationPlanned': experimental, 'mappingViewPlanned': False,
        'headerInitializationPlanned': False, 'consumerPlanned': False,
        'vendorObjectPresence': 'unknown_not_observed', 'existingVendorObjectsAndServicesUntouched': True,
        'automaticFallback': False, 'experimental': experimental}
    if experimental:
        result['sectionPlan'].update(
            name=r'Global\KjMpQrStUvWxYzAb', bytes=66060456, futureCreateNewRequired=True,
            initialContents='windows_zero_initialized', existingSectionMustBeRefused=True,
            evidenceStatus='external_dumping_hypothesis_not_verified_startup_requirement')
    order = result['requiredOrder']
    order.remove('create_new_shared_section')
    if experimental:
        order.insert(order.index('install_and_verify_separately_approved_external_guards'),
                     'create_new_experimental_zero_section_if_absent')
    index = order.index('create_target_suspended_without_inherited_handles')
    order[index] = 'create_target_suspended_with_only_three_stdio_handles'
    result['stdioPlan'] = {
        'allowedInheritedHandles': ['stdin', 'stdout_write', 'stderr_write'],
        'unlistedHandleInheritance': False, 'parentReadEndsInheritable': False,
        'jobHandleInheritable': False, 'explicitHandleListRequired': True}
    result['limitations'].extend([
        'authored_fixture_success_does_not_verify_original_target_guard_or_grant_execution',
        'no_authored_section_does_not_assert_vendor_object_absence',
        'zero_section_is_an_experimental_variable_not_a_proven_crash_cause_or_remedy',
        'baseline_variants_never_automatically_fall_back_to_each_other'])
    return result


def _constant(_value):
    raise ValueError('nonfinite_json')


def read_candidate(path):
    with path.open('rb') as stream:
        raw = stream.read(65537)
    _V1.require(len(raw) <= 65536, 'candidate_input_limit')
    try:
        value = json.loads(raw.decode('utf8'), object_pairs_hook=_V1.unique_object, parse_constant=_constant)
        _V1.require(type(value) is dict, 'candidate_json_invalid')
        return value
    except (UnicodeError, json.JSONDecodeError, RecursionError):
        raise ValueError('candidate_json_invalid') from None
    except ValueError as error:
        if str(error) in ('duplicate_json_property', 'nonfinite_json'):
            raise
        raise ValueError('candidate_json_invalid') from None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--variant', choices=VARIANTS, default=VARIANTS[0])
    args = parser.parse_args(argv)
    print(json.dumps(plan(read_candidate(args.candidate), dict(os.environ), args.variant), indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
