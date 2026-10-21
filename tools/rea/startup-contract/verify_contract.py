"""Differential check of generated formatter decisions, with explicit adapters.
Native constructor/Serialize/Deserialize instructions are original. Json token,
allocation, metadata and string encoding dependencies are modeled callbacks.
Thus this proves the tested formatter-level behavior, NOT the original parser,
all malformed UTF-8 handling, GC, threading, network or the full game.
"""
from pathlib import Path
import json,hashlib,random,collections,subprocess,time,datetime
from native_contract_probe import NativeFormatter,R,SHA
O=R/'contract-differential';O.mkdir(exist_ok=False)
f=NativeFormatter();manifest=json.loads((R/'StartupContract/contract-manifest.json').read_text());assert f.canonical==manifest['canonicalNames'] and f.aliases==manifest['aliases']
rng=random.Random(23191908);cases=[];tokens={}
def add_read(value):
 doc='null' if value is None else '{'+','.join(json.dumps(x['name'],ensure_ascii=False)+':'+json.dumps(x['value'],ensure_ascii=False,separators=(',',':'))for x in value)+'}'
 i=len(cases);cases.append({'id':i,'operation':'read','document':doc});tokens[i]=value

def add_write(values):cases.append({'id':len(cases),'operation':'write','fields':values})
add_read(None);add_read([])
for alias in f.aliases:
 for value in [None,'','https://fixture.example.invalid/path/','quotes " and \\','line\nend','caf\u00e9 \u2605']:
  add_read([{'name':alias['name'],'value':value}])
 for value in [12,True,{},[],{'nested':[None,{'a':1}]}]:add_read([{'name':alias['name'],'value':value}])
for name in f.canonical:
 for candidate in [name.upper(),name.swapcase(),'x'+name,name+'x']:
  if candidate not in f.alias_map:add_read([{'name':candidate,'value':{'ignored':[1,False,{'x':'y'}]}}])
for i,name in enumerate(f.canonical):
 aliases=[a['name']for a in f.aliases if a['ordinal']==i]
 add_read([{'name':aliases[0],'value':'first'},{'name':aliases[-1],'value':'last'}])
 add_read([{'name':aliases[-1],'value':'first'},{'name':aliases[0],'value':None}])
for i in range(1200):
 value=[]
 for j in range(rng.randrange(1,30)):
  key=rng.choice(f.aliases)['name'] if rng.random()<.8 else rng.choice(['unknown','AUTH','ROOMS','notAService'])
  v=rng.choice([None,'','v'+str(i)+'-'+str(j),'https://s.example.invalid/'+str(i),'Unicode \u03b1 \u00e9','quote" slash\\'])
  if rng.random()<.08:v=rng.choice([42,True,[1,{'x':[False]}],{'ignored':'nested'}])
  value.append({'name':key,'value':v})
 add_read(value)
add_write(None);add_write([None]*32)
for i in range(250):add_write([rng.choice([None,'',f'v{i}-{n}','https://s.example.invalid/'+str(i),'line\n\tend','\u03b1 \u00e9 \u2605','"quoted"\\'])for n in range(32)])
inputs=O/'cases.jsonl'
with inputs.open('x',encoding='utf-8')as out:
 for case in cases:out.write(json.dumps(case,ensure_ascii=True)+'\n')
start=time.monotonic();native=[]
with(O/'native-results.jsonl').open('x',encoding='utf-8')as out:
 for case in cases:
  result=f.deserialize(tokens[case['id']])if case['operation']=='read'else f.serialize(case['fields']);row={'id':case['id'],'operation':case['operation'],**result};native.append(row);out.write(json.dumps(row,ensure_ascii=True)+'\n')
  if (case['id']+1)%500==0:print(json.dumps({'nativeCases':case['id']+1,'total':len(cases)}),flush=True)
build=json.loads((R/'contract-build.json').read_text());comparisons={}
for mutant in [None,'empty-object','casefold','first-wins']:
 name='managed'+(''if mutant is None else '-'+mutant);p=O/(name+'.jsonl');command=[build['dotnet'],build['program'],str(inputs),str(p)]
 if mutant:command.append(mutant)
 completed=subprocess.run(command,capture_output=True,text=True,timeout=60);(O/(name+'.log')).write_text(completed.stdout+completed.stderr,encoding='utf-8');assert completed.returncode==0,completed.stdout+completed.stderr
 rows=[json.loads(x)for x in p.read_text(encoding='utf-8').splitlines()];assert len(rows)==len(native)
 bad=[]
 for n,m in zip(native,rows):
  mismatch=n['id']!=m['id']or n['status']!=m['status']
  if not mismatch and n['operation']=='read':mismatch=n['fields']!=m['fields']
  if not mismatch and n['operation']=='write':
   # Writer string-escaping implementation is a modeled boundary. Compare
   # exact ordered property/value pairs, not invented native escaping equality.
   mismatch=json.loads(n['json'],object_pairs_hook=lambda x:x)!=json.loads(m['json'],object_pairs_hook=lambda x:x)
  if mismatch:bad.append({'id':n['id'],'native':n,'managed':m})
 comparisons[name]={'cases':len(rows),'mismatches':len(bad),'examples':bad[:3],'outputSha256':hashlib.sha256(p.read_bytes()).hexdigest()}
 if mutant is None:assert not bad,f'Unexpected {len(bad)} mismatches: '+json.dumps(bad[:1])
 else:assert bad,'Negative control did not detect '+mutant
summary={'status':'formatter_control_flow_differential_passed','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'cases':len(cases),'readCases':sum(c['operation']=='read'for c in cases),'writeCases':sum(c['operation']=='write'for c in cases),'mismatches':0,'originalFormatterMethods':[126409,126410,126411],'exactCanonicalFields':len(f.canonical),'exactInputAliases':len(f.aliases),'outcomes':dict(collections.Counter(n['status']for n in native)),'guestInstructions':f.all_steps,'negativeControls':{k:{'cases':v['cases'],'mismatches':v['mismatches']}for k,v in comparisons.items()if k!='managed'},'sourceImageSha256':SHA,'inputSha256':hashlib.sha256(inputs.read_bytes()).hexdigest(),'nativeOutputSha256':hashlib.sha256((O/'native-results.jsonl').read_bytes()).hexdigest(),'managedOutputSha256':comparisons['managed']['outputSha256'],'emulatorVersion':'2.1.4','seconds':time.monotonic()-start,'originalGameExecuted':False,'originalFormatterInstructionBytesEmulated':True,'nativeParserAndAllocatorCallbacksModeled':True,'stringEncodingComparedAsOrderedJsonValuesNotNativeByteEquivalence':True,'limits':['Plain ASCII property names; arbitrary escaped keys and malformed UTF-8 are outside this differential profile.','Native token/parser, dictionary lookup, allocation, metadata, garbage collection and error helpers are declared modeled boundaries.','Tests establish generated-formatter control flow for these fixtures, not complete Utf8Json, Unity, HTTP, authentication or gameplay compatibility.']}
(O/'summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');(O/'negative-controls.json').write_text(json.dumps(comparisons,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(summary),flush=True)
