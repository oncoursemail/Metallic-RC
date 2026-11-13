from pathlib import Path
import subprocess,os,json,datetime,hashlib,time
root=Path(__file__).resolve().parent
run=root/'runs/recovery-/20261020T212503.738773Z'
out=run/'decoded/rea';out.mkdir(exist_ok=False)
target=run/'decoded/GameAssembly.code-restored.analysis.dll'
assert hashlib.sha256(target.read_bytes()).hexdigest()=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
node=r'C:\Program Files\nodejs\node.exe'
rea=r'C:\Users\alexi\AppData\Local\npm-cache\_npx\ad9a3fdbcad992c6\node_modules\rea-agents\scripts\rea.mjs'
env=os.environ.copy();tools=Path(r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\rea-native')
env['GHIDRA_INSTALL_DIR']=str(tools/'ghidra_12.1.4_PUBLIC');env['JAVA_HOME']=str(tools/'jdk-21.0.12.1+1');env['REA_ANALYSIS_PROVIDER']='ghidra'
command=[node,rea,'function',str(target),'0x181b28e30','--provider','ghidra','--format','json','--snapshot',str(out/'restored-game.snapshot.json')]
receipt={'startedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'status':'running','target':str(target),'targetSha256':'a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066','selectedMethod':'PaintballManager.get_IsInitialized','methodIndex':1296,'address':'0x181b28e30','command':command,'targetExecuted':False}
(out/'operation.json').write_text(json.dumps(receipt,indent=2)+'\n')
start=time.monotonic()
with (out/'PaintballManager.get_IsInitialized.evidence.json').open('wb') as stdout,(out/'PaintballManager.get_IsInitialized.stderr.log').open('wb') as stderr:
 try:
  p=subprocess.run(command,env=env,stdout=stdout,stderr=stderr,stdin=subprocess.DEVNULL,timeout=390)
  receipt.update(status='completed',exitCode=p.returncode)
 except subprocess.TimeoutExpired:
  receipt.update(status='timeout',timeoutSeconds=390)
receipt.update(finishedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),elapsedSeconds=time.monotonic()-start)
(out/'operation.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
