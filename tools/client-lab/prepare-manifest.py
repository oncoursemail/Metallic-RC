"""Create a NEW v2 manifest from the previously reviewed exact-build manifest.
Does not overwrite evidence, replace game files, launch software, or request elevation.
"""
from pathlib import Path
import argparse,hashlib,json,os,sys
ROOT=Path(__file__).resolve().parent

def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--previous',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
    if a.output.exists():raise FileExistsError('Choose a new manifest path; original review evidence is retained')
    m=json.loads(a.previous.read_text(encoding='utf8'))
    if m['schemaVersion']!=1 or m['steamBuildId']!='23191908':raise ValueError('Expected the reviewed initial build profile')
    for row in m['files']:
        path=Path(row['path']);assert sha(path)==row['sha256'] and path.stat().st_size==row['bytes'],'Prior pinned input changed: '+path.name
    old_code={os.path.normcase(str(Path(m[key]).resolve())) for key in ('agent','policy')}
    records=[row for row in m['files'] if os.path.normcase(str(Path(row['path']).resolve())) not in old_code]
    m['schemaVersion']=2;m['agent']=str(ROOT/'runtime-agent.js');m['policy']=str(ROOT/'route-policy.js')
    for name in ('run-native-client.py','supervision.py','winjob.py','runtime-agent.js','route-policy.js','Launch-NativeClientTest.ps1'):
        path=ROOT/name;records.append({'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size})
    m['files']=records;m['runnerSha256']=sha(ROOT/'run-native-client.py');m['runnerPath']=str(ROOT/'run-native-client.py')
    m['budgets']={'totalWorkerSeconds':90,'startupSeconds':30,'attachSeconds':15,'outerSupervisorWaitSeconds':130}
    m['scope']='One explicitly approved exact-build client attempt. Separate supervisor/job contains only its new process tree. Phased cancellation, fixed deadlines, truthful evidence. Existing processes, installed binaries, system DNS/certificates/authentication checks are not altered.'
    with a.output.open('x',encoding='utf8',newline='\n') as f:f.write(json.dumps(m,indent=2)+'\n')
    print(json.dumps({'status':'v2_manifest_created','runnerSha256':m['runnerSha256'],'manifestSha256':sha(a.output),'pinnedFiles':len(records),'gameLaunched':False}))
if __name__=='__main__':main()
