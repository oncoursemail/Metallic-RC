"""Validate and print a data-only direct-executable control plan. No executor."""
import argparse
import json
import ntpath
import os
from pathlib import Path, PureWindowsPath
import re

GAME=r'C:\Program Files (x86)\Steam\steamapps\common\RecRoom'
BUILD='23191908'
PINS={
    'RecRoom.exe':('3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70',672800),
    'GameAssembly.dll':('a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84',287903264),
    'Referee.dll':('5868c90a7c67757f3b678aa356cbd933d646c9749e0acaa31b7dcc9eaee12ead',63516208),
}
BUDGETS={'totalWorkerSeconds':90,'startupSeconds':30,'outerSupervisorWaitSeconds':130}
ENVIRONMENT=('SystemRoot','SystemDrive','WINDIR','PATH','COMSPEC','TEMP','TMP','LOCALAPPDATA','APPDATA','USERPROFILE','HOMEDRIVE','HOMEPATH','ProgramData','ProgramFiles','ProgramFiles(x86)','PUBLIC','USERNAME','USERDOMAIN','NUMBER_OF_PROCESSORS','PROCESSOR_ARCHITECTURE')


def require(value,reason):
    if not value:raise ValueError(reason)


def windows_path(value):
    require(type(value) is str and value and not any(ord(c)<32 for c in value),'absolute_windows_path_required')
    p=PureWindowsPath(value)
    require(p.is_absolute() and re.fullmatch('[A-Za-z]:',p.drive) and '..' not in p.parts,'absolute_windows_path_required')
    require(not any(':' in part or part.endswith((' ','.')) for part in p.parts[1:]),'ambiguous_windows_path')
    return ntpath.normcase(str(p))


def unique_object(pairs):
    result={}
    for key,value in pairs:
        require(key not in result,'duplicate_json_property')
        result[key]=value
    return result


def select_environment(environment):
    require(isinstance(environment,dict),'environment_mapping_required')
    normalized={}
    for key,value in environment.items():
        require(type(key) is str and type(value) is str,'environment_text_required')
        folded=key.casefold()
        require(folded not in normalized,'duplicate_environment_name')
        normalized[folded]=value
    return {name:normalized[name.casefold()] for name in ENVIRONMENT if name.casefold() in normalized}


def plan(candidate,environment):
    require(type(candidate) is dict and set(candidate)=={'schemaVersion','steamBuildId','gameRoot','labRoot','runDirectory','files','budgets'},'closed_candidate_schema_required')
    require(type(candidate['schemaVersion']) is int and candidate['schemaVersion']==1,'candidate_schema_version')
    require(candidate['steamBuildId']==BUILD,'exact_build_required')
    selected=select_environment(environment)
    local=selected.get('LOCALAPPDATA')
    require(type(local) is str and bool(local),'localappdata_required')
    windows_path(local)
    lab=str(PureWindowsPath(local)/'Metallic-RC'/'native-client-lab-20261021')
    require(windows_path(candidate['gameRoot'])==windows_path(GAME),'exact_game_root_required')
    require(windows_path(candidate['labRoot'])==windows_path(lab),'exact_lab_root_required')
    windows_path(candidate['runDirectory'])
    run=PureWindowsPath(candidate['runDirectory'])
    require(windows_path(str(run.parent))==windows_path(str(PureWindowsPath(lab)/'runs')),'private_run_path_required')
    require(re.fullmatch('[A-Za-z0-9][A-Za-z0-9_-]{0,95}',run.name) is not None,'private_run_name_required')
    require(run.name.upper() not in {'CON','PRN','AUX','NUL'}|{'COM'+str(i) for i in range(1,10)}|{'LPT'+str(i) for i in range(1,10)},'private_run_name_required')
    budgets=candidate['budgets']
    require(type(budgets) is dict and set(budgets)==set(BUDGETS) and all(type(budgets[k]) is int and budgets[k]==v for k,v in BUDGETS.items()),'fixed_budgets_required')
    rows=candidate['files']
    require(type(rows) is list and len(rows)==len(PINS),'three_original_pins_required')
    records={}
    for row in rows:
        require(type(row) is dict and set(row)=={'path','sha256','bytes'},'closed_pin_record_required')
        key=windows_path(row['path'])
        require(key not in records,'duplicate_pin_path')
        records[key]=row
    for name,(digest,size) in PINS.items():
        row=records.get(windows_path(str(PureWindowsPath(GAME)/name)))
        require(row is not None and row['sha256']==digest and type(row['bytes']) is int and row['bytes']==size,'exact_original_pin_required')
    command=[str(PureWindowsPath(GAME)/'RecRoom.exe'),'+forcemode:screen','-screen-fullscreen','0','-screen-width','1280','-screen-height','720','-logFile',str(run/'private-player.log')]
    return {
        'schema':'pristine-control-preparation-v1','gameWillNotRun':True,'executionReady':False,
        'newNetworkMethodApproved':False,'originalBytesVerified':False,
        'blockingReasons':['fixture_not_yet_verified','native_executor_not_implemented','original_bytes_not_verified'],
        'steamBuildId':BUILD,'target':command[0],'argv':command,'cwd':GAME,
        'declaredOriginalPins':[{'path':str(PureWindowsPath(GAME)/name),'sha256':digest,'bytes':size} for name,(digest,size) in PINS.items()],
        'environmentNames':list(selected),'environmentValuesEmitted':False,
        'runDirectory':str(run),'budgets':dict(BUDGETS),'attachment':'not_applicable_no_frida',
        'sharedSectionPrerequisite':{'name':r'Global\KjMpQrStUvWxYzAb','bytes':66060456,'futureCreateNewRequired':True,'existingSectionMustBeRefused':True},
        'privateOutputs':{'createNewDirectoryAndOwnerAclRequired':True,'playerLog':'private-player.log','stdoutAndStderr':'bounded_65536_byte_prefix_fingerprints_only','transactionalProgressRequired':True,'finalReceiptAfterCleanupRequired':True},
        'requiredOrder':['verify_new_reviewed_source_and_original_bytes','refuse_existing_target_processes','create_new_private_run_directory','create_new_shared_section','install_and_verify_separately_approved_external_guards','create_target_suspended_without_inherited_handles','retain_and_verify_exact_process_image','assign_and_verify_kill_on_close_no_breakaway_job','resume_only_after_all_admissions','observe_within_total_deadline','capture_precleanup_exit_evidence','close_or_terminate_only_owned_process_tree','verify_job_empty_before_releasing_external_guards','verify_original_hashes_and_export_final_receipt'],
        'limitations':['direct_executable_harness_control_not_normal_steam_or_release_launch','no_service_start_or_installation','external_fixture_needs_exact_reviewed_source_and_receipt_provenance','app_id_filters_do_not_automatically_cover_other_executables_or_delegated_services','dynamic_filter_removal_and_forced_job_cleanup_are_not_claimed_atomic'],
    }


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__,allow_abbrev=False)
    parser.add_argument('--candidate',required=True,type=Path)
    args=parser.parse_args(argv)
    # Preparation consumes only an explicitly selected, bounded JSON input.
    with args.candidate.open('rb') as stream:raw=stream.read(65537)
    require(len(raw)<=65536,'candidate_input_limit')
    candidate=json.loads(raw.decode('utf8'),object_pairs_hook=unique_object)
    print(json.dumps(plan(candidate,dict(os.environ)),indent=2))
    return 0


if __name__=='__main__':raise SystemExit(main())
