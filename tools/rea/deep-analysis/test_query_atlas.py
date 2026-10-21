from pathlib import Path
import json,sqlite3,subprocess,unittest,hashlib,time
from query_atlas import connect
ROOT=Path(__file__).resolve().parent
PYTHON=r'C:\Python315\python.exe'
class AtlasTests(unittest.TestCase):
    def invoke(self,*args,success=True):
        p=subprocess.run([PYTHON,str(ROOT/'query_atlas.py'),*args],capture_output=True,text=True,timeout=20)
        self.assertEqual(p.returncode==0,success,p.stderr)
        return json.loads(p.stdout) if success else p
    def test_exact_counts(self):
        counts=self.invoke('coverage')['counts'];self.assertEqual(counts['methods'],409503);self.assertEqual(counts['types'],63648)
    def test_typed_signature(self):
        x=self.invoke('method','387055');self.assertEqual(x['metadata']['returnType'],'void');self.assertEqual([p['type'] for p in x['metadata']['parameters']],['byte[]','int','int'])
    def test_name_server_search(self):
        x=self.invoke('search','NameServer','--limit','5');self.assertGreater(len(x['results']),0);self.assertLessEqual(len(x['results']),5)
    def test_specific_leaf(self):
        x=self.invoke('method','148135');self.assertEqual(x['metadata']['name'],'get_Auth');self.assertIsNotNone(x['recoveredLeaf'])
    def test_calls(self):
        x=self.invoke('calls','148260','--limit','3');self.assertGreater(len(x['observations']),0);self.assertLessEqual(len(x['observations']),3)
    def test_callers(self):
        x=self.invoke('callers','1316','--limit','2');self.assertIn('observations',x)
    def test_field_offsets(self):
        x=self.invoke('fields','22164','--limit','100');self.assertGreater(len(x['fields']),0);self.assertTrue(all('offset' in f for f in x['fields']))
    def test_invalid_method_rejected(self):self.invoke('method','999999999',success=False)
    def test_limit_rejected(self):self.invoke('search','x','--limit','0',success=False)
    def test_quote_is_data(self):
        x=self.invoke('search','"; DROP TABLE methods; --');self.assertEqual(x['results'],[])
        self.assertEqual(self.invoke('coverage')['counts']['methods'],409503)
    def test_readonly(self):
        with connect(ROOT/'Metallic-RC.Analysis.sqlite') as db:
            with self.assertRaises(sqlite3.OperationalError):db.execute('CREATE TABLE test_should_never_exist(x)')
    def test_paging(self):
        first=self.invoke('search','RecRoom','--limit','2');second=self.invoke('search','RecRoom','--limit','2','--offset','2')
        self.assertTrue(first['hasMore']);self.assertFalse(set(x['id'] for x in first['results']) & set(x['id'] for x in second['results']))
if __name__=='__main__':
    start=time.monotonic();suite=unittest.defaultTestLoader.loadTestsFromTestCase(AtlasTests)
    with (ROOT/'atlas-tests.log').open('w',encoding='utf-8') as log:r=unittest.TextTestRunner(stream=log,verbosity=2).run(suite)
    report={'tests':r.testsRun,'failures':len(r.failures),'errors':len(r.errors),'passed':r.wasSuccessful(),'seconds':round(time.monotonic()-start,3),'databaseOpenedReadOnly':True,'originalGameExecuted':False}
    (ROOT/'atlas-tests.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(report));raise SystemExit(0 if r.wasSuccessful() else 1)
