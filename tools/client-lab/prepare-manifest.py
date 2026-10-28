"""Create a NEW candidate v3 profile from verified v1/v2/v3 pinned inputs.
No overwrite, game launch, elevation request, binary patch, or runtime security change.
"""
from pathlib import Path
import argparse,hashlib,json,os,re
ROOT=Path(__file__).resolve().parent
AUTHORED=('run-native-client.py','supervision.py','progress_store.py','winjob.py','runtime-agent.js','route-policy.js','Launch-NativeClientTest.ps1')
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def identity(path):
    if not isinstance(path,(str,Path)) or not str(path) or not Path(path).is_absolute():
        raise ValueError('Pinned paths must be absolute')
    return os.path.normcase(str(Path(path).resolve()))

def unique_object(pairs):
    result={}
    for key,value in pairs:
        if key in result:raise ValueError('Duplicate JSON property')
        result[key]=value
    return result

def refresh(previous,output):
    previous=Path(previous);output=Path(output)
    if identity(previous.resolve())==identity(output.resolve()) or output.exists():
        raise FileExistsError('Choose a new path; earlier manifests are retained')
    m=json.loads(previous.read_text(encoding='utf8'),object_pairs_hook=unique_object)
    if not isinstance(m,dict) or type(m.get('schemaVersion')) is not int or m['schemaVersion'] not in (1,2,3) or m.get('steamBuildId')!='23191908' or type(m.get('port')) is not int or m['port']!=4187:
        raise ValueError('Expected build 23191908 profile on the fixed loopback port')
    if not isinstance(m.get('files'),list) or not m['files']:
        raise ValueError('Pinned input records required')
    records={}
    for row in m['files']:
        if not isinstance(row,dict):raise ValueError('Invalid pinned input record')
        key=identity(row.get('path'))
        if key in records:raise ValueError('Duplicate pinned input path')
        if type(row.get('bytes')) is not int or row['bytes']<0 or not isinstance(row.get('sha256'),str) or not re.fullmatch('[0-9a-f]{64}',row['sha256']):
            raise ValueError('Invalid pinned input identity')
        path=Path(row['path'])
        if sha(path)!=row['sha256'] or path.stat().st_size!=row['bytes']:
            raise ValueError('Previous pinned input changed: '+path.name)
        records[key]=row
    previous_authored=set()
    roots=set()
    for role in ('agent','policy'):
        key=identity(m.get(role))
        if key not in records:raise ValueError('Required previous '+role+' pin missing')
        previous_authored.add(key)
        if role=='agent':roots.add(Path(records[key]['path']).parent)
    runner_path=m.get('runnerPath')
    runner_hash=m.get('runnerSha256')
    if m['schemaVersion']==3 and (runner_path is None or runner_hash is None):
        raise ValueError('Previous v3 runner identity required')
    if runner_path is not None or runner_hash is not None:
        # Older profiles may store only the runner hash beside the agent/policy paths.
        if runner_path is None:runner_path=Path(m['agent']).parent/'run-native-client.py'
        key=identity(runner_path)
        if key not in records:raise ValueError('Required previous runner pin missing')
        if not isinstance(runner_hash,str) or runner_hash!=records[key]['sha256']:
            raise ValueError('Previous runner identity does not match its verified pin')
        previous_authored.add(key);roots.add(Path(records[key]['path']).parent)
    # Only old authored siblings are replaced. A vendor file with the same basename is retained.
    for root in roots:
        previous_authored.update(identity(root/name) for name in AUTHORED if identity(root/name) in records)
    rows=[row for row in m['files'] if identity(row['path']) not in previous_authored]
    new_keys={identity(row['path']) for row in rows}
    new_pins={}
    for name in AUTHORED:
        path=ROOT/name;key=identity(path)
        if key in new_keys:raise ValueError('New authored path collides with a retained input')
        new_keys.add(key);new_pins[name]={'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size};rows.append(new_pins[name])
    m.update(schemaVersion=3,agent=str(ROOT/'runtime-agent.js'),policy=str(ROOT/'route-policy.js'),runnerPath=str(ROOT/'run-native-client.py'),runnerSha256=new_pins['run-native-client.py']['sha256'],files=rows,
             progressTransport='sqlite-wal-single-publisher',maxProgressSnapshotBytes=2_097_152,maxProgressGenerations=4096,
             budgets={'totalWorkerSeconds':90,'startupSeconds':30,'attachSeconds':15,'outerSupervisorWaitSeconds':130})
    m['scope']='One separately approved exact-build test. Transactional progress; final JSON written once after cleanup. Same reviewed temporary routing/network guards. No existing-process, game-file, DNS, certificate/authentication or security-policy changes.'
    with output.open('x',encoding='utf8',newline='\n')as f:f.write(json.dumps(m,indent=2)+'\n')
    return {'status':'v3_candidate_manifest_created','runnerSha256':m['runnerSha256'],'manifestSha256':sha(output),'pinnedFiles':len(rows),'gameLaunched':False,'approvedForExecution':False}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--previous',required=True,type=Path);parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args();print(json.dumps(refresh(args.previous,args.output)))
if __name__=='__main__':main()
