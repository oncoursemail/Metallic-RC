"""Prepare an additive REA pass on startup cache/initialization functions."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parent
DEEP=ROOT.parent/'deep-rea-20261021'
source=DEEP/'build_scoped_targets.py';code=source.read_text(encoding='utf-8')
a=code.index('GROUPS=');b=code.index('\nif not ',a)
code=code[:a]+'GROUPS='+repr({'startup-cache':[148090,148091,148096,145417]})+code[b:]
code=code.replace('ROOT=Path(__file__).resolve().parent','ROOT=Path(__file__).resolve().parent\nDEEP=ROOT.parent/\'deep-rea-20261021\'')
code=code.replace("ROOT/'Metallic-RC.Analysis.sqlite'","DEEP/'Metallic-RC.Analysis.sqlite'")
compile(code,'build_targets.py','exec')
for name,data in [('build_targets.py',code.encode()),('rea_session.py',(DEEP/'rea_session.py').read_bytes()),('run_startup_rea.py',(DEEP/'run_deep_rea.py').read_bytes())]:
 with (ROOT/name).open('xb') as f:f.write(data)
receipt={'sourceBuilderSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'selectedMethods':[148090,148091,148096,145417],'toolSources':[{ 'file':name,'sha256':hashlib.sha256((ROOT/name).read_bytes()).hexdigest()} for name in ('build_targets.py','rea_session.py','run_startup_rea.py')],'purpose':'New exact-source native dossiers for initialization, cache persistence/invalidation and endpoint-change exception. No original game execution.'}
(ROOT/'preparation.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8',newline='\n')
p=subprocess.run([r'C:\Python315\python.exe','-u',str(ROOT/'build_targets.py')],timeout=45)
raise SystemExit(p.returncode)
