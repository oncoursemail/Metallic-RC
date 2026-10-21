"""Read-only admission tests against an explicitly supplied private v2 manifest.
No process, section, Frida import, backend request or game launch occurs here.
"""
from pathlib import Path
import argparse,copy,hashlib,importlib.util,json,sys,tempfile
ROOT=Path(__file__).resolve().parent

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--manifest',required=True,type=Path);ap.add_argument('--output',required=True,type=Path);a=ap.parse_args()
    if a.output.exists():raise FileExistsError('New report destination required')
    spec=importlib.util.spec_from_file_location('runner_under_test',ROOT/'run-native-client.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    original=a.manifest.read_bytes();profile=json.loads(original);results=[]
    before=hashlib.sha256(original).hexdigest();valid=module.validate(a.manifest);assert valid['schemaVersion']==2
    results.append({'case':'reviewed_v2_profile','passed':True})
    cases={}
    bad=copy.deepcopy(profile);bad['schemaVersion']=1;cases['old_schema']=bad
    bad=copy.deepcopy(profile);bad['port']=9999;cases['wrong_backend_port']=bad
    bad=copy.deepcopy(profile);bad['gameRoot']=str(a.manifest.parent);cases['wrong_game_root']=bad
    bad=copy.deepcopy(profile);bad['files']=[x for x in bad['files'] if Path(x['path']).name!='winjob.py'];cases['missing_supervisor_input']=bad
    bad=copy.deepcopy(profile);bad['files'].append(copy.deepcopy(bad['files'][0]));cases['duplicate_path']=bad
    bad=copy.deepcopy(profile);bad['files'][0]['sha256']='0'*64;cases['changed_original_hash']=bad
    bad=copy.deepcopy(profile);bad['runnerSha256']='0'*64;cases['changed_runner_identity']=bad
    bad=copy.deepcopy(profile);bad['agent']=str(a.manifest.parent/'not-an-admitted-agent.js');cases['unpinned_agent_path']=bad
    with tempfile.TemporaryDirectory() as temp:
        for name,value in cases.items():
            p=Path(temp)/(name+'.json');p.write_text(json.dumps(value),encoding='utf8')
            try:module.validate(p)
            except (ValueError,FileNotFoundError) as e:results.append({'case':name,'passed':True,'rejectionType':type(e).__name__})
            else:raise AssertionError('Invalid admission accepted: '+name)
    assert hashlib.sha256(a.manifest.read_bytes()).hexdigest()==before
    assert 'frida' not in sys.modules
    report={'status':'manifest_admission_checks_passed','checks':len(results),'results':results,'manifestUnchanged':True,'fridaImported':False,'gameLaunched':False}
    a.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8',newline='\n');print(json.dumps(report))
if __name__=='__main__':main()
