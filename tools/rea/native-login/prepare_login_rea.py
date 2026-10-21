from pathlib import Path
import sqlite3,json,re,shutil,subprocess
R=Path(__file__).resolve().parent;W=R.parent/'rework-20261020/rea-steam-decoding'
DB=sqlite3.connect((W/'deep-rea-20261021/Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True);DB.row_factory=sqlite3.Row
formatters=[dict(r)for r in DB.execute('SELECT id,type_id,type,name,returns,parameters,rva FROM methods WHERE (returns=? OR parameters LIKE ?) AND type LIKE ? ORDER BY id',('RecNet.Login.LoginResponse','%RecNet.Login.LoginResponse%','Utf8Json.Formatters.%'))]
print('LOGIN_FORMATTERS',json.dumps(formatters),flush=True)
request_methods=[149600,149616,149614,149610]
additional=[]
for row in formatters:
    additional.extend(r[0] for r in DB.execute('SELECT id FROM methods WHERE type_id=? AND name IN (?,?,?) AND rva IS NOT NULL',(row['type_id'],'.ctor','Serialize','Deserialize')))
additional=sorted(set(additional))
assert len(additional)==3,additional
out=W/'native-login-rea-20261021';out.mkdir(exist_ok=False)
previous=W/'startup-contract-fix-20261021'
source=(previous/'build_targets.py').read_text()
source=re.sub(r'^GROUPS=.*$', 'GROUPS='+repr({'login-request':[149600,149616],'login-result':[149614,149610],'login-formatter':additional}),source,flags=re.M)
(out/'build_targets.py').write_text(source,encoding='utf8',newline='\n')
for name in ['run_contract_rea.py','rea_session.py']:shutil.copyfile(previous/name,out/name)
(out/'selected-methods.json').write_text(json.dumps({'requestMethods':request_methods,'formatters':formatters,'formatterMethods':additional},indent=2)+'\n')
subprocess.run([r'C:\Python315\python.exe',str(out/'build_targets.py')],check=True)
subprocess.run([r'C:\Python315\python.exe',str(out/'run_contract_rea.py')],check=True)
