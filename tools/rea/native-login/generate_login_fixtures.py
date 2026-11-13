"""Create synthetic fixture observations from the captured login formatter.
No account credentials, production client constants, live HTTP, or original game execution.
"""
from pathlib import Path
import json,random,hashlib,datetime
from native_login_probe import NativeFormatter,SHA
R=Path(__file__).resolve().parent;O=R/'fixtures';O.mkdir(exist_ok=False)
f=NativeFormatter();rng=random.Random(23191908);cases=[];instruction_count=0
values=[None,'','test-value','a"b\\c','line\nfeed','\u2605\U0001f31f','\u0000']
def read(tokens):
 global instruction_count
 result=f.deserialize(tokens);instruction_count+=result['instructions']
 raw='null'if tokens is None else'{'+','.join(json.dumps(t['name'])+':'+json.dumps(t['value'],ensure_ascii=True)for t in tokens)+'}'
 cases.append({'id':len(cases),'operation':'read','inputJson':raw,'expected':{'status':result['status'],'fields':result['fields']}})
def write(fields):
 global instruction_count
 result=f.serialize(fields);instruction_count+=result['instructions'];assert result['status']=='ok'
 cases.append({'id':len(cases),'operation':'write','fields':fields,'expected':{'status':'ok','json':result['json']}})
read(None);read([]);read([{'name':'unknown','value':{'nested':[1,True,None]}}])
for alias in f.aliases:
 for value in values+[42,False,{'unverified':'object'}]:read([{'name':alias['name'],'value':value}])
for name in f.canonical:
 read([{'name':name.upper(),'value':'wrong-case-is-not-accepted'}])
for _ in range(1000):
 tokens=[]
 for j in range(rng.randrange(1,18)):
  alias=rng.choice(f.aliases);value=rng.choice(values)
  if alias['ordinal']==5:value=None
  tokens.append({'name':alias['name'],'value':value})
  if rng.random()<.15:tokens.append({'name':'unrecognized'+str(j),'value':{'nested':[True,9]}})
 read(tokens)
write(None)
for _ in range(256):
 row=[rng.choice(values)for i in range(8)];row[5]=None;write(row)
with (O/'cases.jsonl').open('x',encoding='utf8',newline='\n')as out:
 for case in cases:out.write(json.dumps(case,ensure_ascii=True)+'\n')
profile={'buildId':'23191908','canonicalNames':f.canonical,'aliases':f.aliases,'nativeMethods':f.ranges,'sourceImageSha256':SHA,'constructorInstructions':f.constructor_steps,
 'profile':'root null/object; exact raw ASCII aliases; seven string/null fields; null or absent code_hint only; populated nested hint stops outside profile',
 'modeledBoundaries':['metadata','allocation','GC barrier','JSON token reading','exact alias dictionary','string encoding','null-only nested formatter interface dispatch'],
 'originalGameExecuted':False,'productionCredentialConstantsIncluded':False}
(R/'response-profile.json').write_text(json.dumps(profile,indent=2)+'\n',encoding='utf8',newline='\n')
summary={'status':'native_login_formatter_fixtures_generated','cases':len(cases),'reads':sum(c['operation']=='read'for c in cases),'writes':sum(c['operation']=='write'for c in cases),'guestInstructions':instruction_count+f.constructor_steps,
 'fixtureSha256':hashlib.sha256((O/'cases.jsonl').read_bytes()).hexdigest(),'sourceImageSha256':SHA,'createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'currentProbeSha256':hashlib.sha256((R/'native_login_probe.py').read_bytes()).hexdigest(),'actualGameExecuted':False,'runtimeCallbacksModeled':True}
(O/'native-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print(json.dumps(summary),flush=True)
