from pathlib import Path
import subprocess,os,json,datetime,hashlib,time
root=Path(__file__).resolve().parent;out=root/'runs/recovery-/20261020T212503.738773Z/decoded/rea'
provenance=json.loads((out/'code-page-provenance.json').read_text());target=Path(provenance['output'])
assert hashlib.sha256(target.read_bytes()).hexdigest()==provenance['outputSha256']
env=os.environ.copy();tools=Path(r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\rea-native')
env.update(GHIDRA_INSTALL_DIR=str(tools/'ghidra_12.1.4_PUBLIC'),JAVA_HOME=str(tools/'jdk-21.0.12.1+1'),REA_ANALYSIS_PROVIDER='ghidra')
node=r'C:\Program Files\nodejs\node.exe';rea=r'C:\Users\alexi\AppData\Local\npm-cache\_npx\ad9a3fdbcad992c6\node_modules\rea-agents\scripts\rea.mjs'
results=[]
for method in provenance['methods']:
 name=method['name'];start=time.monotonic();cmd=[node,rea,'function',str(target),method['address'],'--provider','ghidra','--format','json','--snapshot',str(out/'bounded-code.snapshot.json')]
 with (out/(name+'.bounded.evidence.json')).open('xb') as stdout,(out/(name+'.bounded.stderr.log')).open('xb') as stderr:
  try:
   p=subprocess.run(cmd,env=env,stdout=stdout,stderr=stderr,stdin=subprocess.DEVNULL,timeout=120)
   item={'method':name,'address':method['address'],'exitCode':p.returncode,'seconds':time.monotonic()-start,'status':'completed'}
  except subprocess.TimeoutExpired:item={'method':name,'status':'timeout','seconds':time.monotonic()-start}
 results.append(item);(out/'bounded-operations.json').write_text(json.dumps({'syntheticContainer':True,'sourceProvenance':'code-page-provenance.json','operations':results},indent=2)+'\n');print(json.dumps(item),flush=True)
