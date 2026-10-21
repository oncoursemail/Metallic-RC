from pathlib import Path
import json,subprocess,hashlib,datetime,collections
R=Path(__file__).resolve().parent;ROOT=R.parents[2];REPO=ROOT/'client-work-20261021'
DOTNET=ROOT/'tooling/dotnet-10.0.402/dotnet.exe';DLL=REPO/'tests/Metallic.RC.LoginContract.Tests/bin/Release/net10.0/Metallic.RC.LoginContract.Tests.dll'
F=R/'fixtures';cases=[json.loads(line)for line in(F/'cases.jsonl').read_text().splitlines()];summary=json.loads((F/'native-summary.json').read_text())
assert hashlib.sha256((F/'cases.jsonl').read_bytes()).hexdigest()==summary['fixtureSha256']
results=[]
for mutation in [None,'casefold','first-wins','empty-object']:
 name=mutation or'actual';path=F/(name+'.managed.jsonl');assert not path.exists()
 command=[str(DOTNET),str(DLL),str(F/'cases.jsonl'),str(path)]
 if mutation:command.append(mutation)
 process=subprocess.run(command,capture_output=True,text=True,timeout=60);(F/(name+'.log')).write_text(process.stdout+process.stderr)
 assert process.returncode==0,process.stdout+process.stderr
 rows=[json.loads(line)for line in path.read_text().splitlines()];assert len(rows)==len(cases)
 mismatches=[]
 for case,row in zip(cases,rows):
  expected=case['expected'];assert row['id']==case['id'];equal=row['status']==expected['status']
  if equal and case['operation']=='read':equal=row.get('fields')==expected.get('fields')
  elif equal and case['operation']=='write':equal=json.loads(row['json'],object_pairs_hook=list)==json.loads(expected['json'],object_pairs_hook=list)
  if not equal:mismatches.append({'id':case['id'],'operation':case['operation'],'expectedStatus':expected['status'],'actualStatus':row['status']})
 if mutation:assert mismatches,'Negative control not detected: '+name
 else:assert not mismatches,mismatches[:20]
 results.append({'variant':name,'cases':len(cases),'mismatches':len(mismatches),'firstDifferingFixtureIds':[x['id']for x in mismatches[:12]],'outputSha256':hashlib.sha256(path.read_bytes()).hexdigest()})
report={**summary,'status':'compiled_login_response_profile_matches_native_fixture_observations','comparisonResults':results,'admittedOrTypeErrorCases':sum(c['expected']['status']!='nested_hint_outside_profile'for c in cases),'deliberatelyUnsupportedNestedCases':sum(c['expected']['status']=='nested_hint_outside_profile'for c in cases),
 'csharpContractSha256':hashlib.sha256((REPO/'src/Metallic.RC.Compatibility/LoginResponseContract.cs').read_bytes()).hexdigest(),'comparedUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'wireEscapingBitwiseEquivalentClaimed':False,'nativeClassifierEquivalenceClaimed':False,'liveOriginalClientAcceptance':False,
 'limits':['Generated constructor/read/write instructions are evaluated in an isolated x64 guest with declared runtime and token-reader callbacks.','Only null/absent code_hint is admitted. Non-null nested values stop at a modeled boundary; their original parser behavior is not claimed.','Writer checks compare ordered JSON values, not every byte-escaping rule of the original Utf8Json fork.','This component does not mint or validate real native access tokens, fetch an account, or complete login.']}
(F/'comparison-summary.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf8',newline='\n');print(json.dumps({'status':report['status'],'cases':len(cases),'comparisons':results,'unsupportedNested':report['deliberatelyUnsupportedNestedCases']}),flush=True)
