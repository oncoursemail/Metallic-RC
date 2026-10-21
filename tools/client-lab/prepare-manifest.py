"""Create a NEW hash-pinned v3 progress-store profile from a still-matching prior profile.
No overwrite, game launch, elevation request, binary patch, or runtime security change.
"""
from pathlib import Path
import argparse,hashlib,json,os
ROOT=Path(__file__).resolve().parent
AUTHORED=('run-native-client.py','supervision.py','progress_store.py','winjob.py','runtime-agent.js','route-policy.js','Launch-NativeClientTest.ps1')
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--previous',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
    if a.output.exists():raise FileExistsError('Choose a new path; earlier manifests are retained')
    m=json.loads(a.previous.read_text(encoding='utf8'))
    if m.get('schemaVersion') not in (1,2) or m.get('steamBuildId')!='23191908':raise ValueError('Expected reviewed build 23191908 profile')
    for row in m['files']:
        p=Path(row['path'])
        if sha(p)!=row['sha256'] or p.stat().st_size!=row['bytes']:raise ValueError('Previous pinned input changed: '+p.name)
    previous_authored={os.path.normcase(str(Path(m[key]).resolve())) for key in ('agent','policy')}
    previous_authored.update(os.path.normcase(str(Path(row['path']).resolve())) for row in m['files'] if Path(row['path']).name in AUTHORED)
    rows=[row for row in m['files'] if os.path.normcase(str(Path(row['path']).resolve())) not in previous_authored]
    for name in AUTHORED:
        p=ROOT/name;rows.append({'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size})
    m.update(schemaVersion=3,agent=str(ROOT/'runtime-agent.js'),policy=str(ROOT/'route-policy.js'),runnerPath=str(ROOT/'run-native-client.py'),runnerSha256=sha(ROOT/'run-native-client.py'),files=rows,
             progressTransport='sqlite-wal-single-publisher',maxProgressSnapshotBytes=2_097_152,maxProgressGenerations=4096,
             budgets={'totalWorkerSeconds':90,'startupSeconds':30,'attachSeconds':15,'outerSupervisorWaitSeconds':130})
    m['scope']='One separately approved exact-build test. Transactional progress; final JSON written once after cleanup. Same reviewed temporary routing/network guards. No existing-process, game-file, DNS, certificate/authentication or security-policy changes.'
    with a.output.open('x',encoding='utf8',newline='\n')as f:f.write(json.dumps(m,indent=2)+'\n')
    print(json.dumps({'status':'v3_manifest_created','runnerSha256':m['runnerSha256'],'manifestSha256':sha(a.output),'pinnedFiles':len(rows),'gameLaunched':False}))
if __name__=='__main__':main()
