"""Rerun prior bounded codec/native and service-model checks without overwrites."""
from pathlib import Path
import json,hashlib,subprocess,os
ROOT=Path(__file__).resolve().parent
PRIOR=ROOT.parent/'verification-pass-20261021'
script=PRIOR/'differential_codec.py';text=script.read_text(encoding='utf-8')
assert text.count('ROOT=Path(__file__).resolve().parent')==1
text=text.replace('ROOT=Path(__file__).resolve().parent','ROOT=Path('+repr(str(PRIOR))+')')
assert text.count("OUT=ROOT/'differential'")==1
text=text.replace("OUT=ROOT/'differential'","OUT=Path("+repr(str(ROOT/'codec-recheck'))+")")
compile(text,'codec_recheck.generated.py','exec')
with (ROOT/'codec_recheck.generated.py').open('x',encoding='utf-8',newline='\n') as f:f.write(text)
provenance={'originalRunnerSha256':hashlib.sha256(script.read_bytes()).hexdigest(),'change':'Only ROOT binding and new output destination changed; preserves previous evidence rather than overwriting it.','currentRunnerSha256':hashlib.sha256((ROOT/'codec_recheck.generated.py').read_bytes()).hexdigest()}
(ROOT/'recheck-provenance.json').write_text(json.dumps(provenance,indent=2)+'\n',encoding='utf-8',newline='\n')
p=subprocess.run([r'C:\Python315\python.exe','-u',str(ROOT/'codec_recheck.generated.py')],timeout=90)
assert p.returncode==0
report=ROOT/'service-model-recheck.json';assert not report.exists()
dotnet=r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe';env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
p=subprocess.run([dotnet,'run','--project',str(PRIOR/'ObservedServices/ObservedServices.csproj'),'-c','Release','--no-restore','--',str(report)],capture_output=True,text=True,timeout=90,env=env)
(ROOT/'service-model-recheck.log').write_text(p.stdout+p.stderr,encoding='utf-8');assert p.returncode==0,p.stdout+p.stderr
print(p.stdout,flush=True)
