"""Real transactional snapshot tests; fresh local fixtures only, never the game.
Uses a single publisher and independently reading processes. Partial commits,
slow readers, held legacy JSON handles and killed writers are explicit cases.
"""
from pathlib import Path
import contextlib,hashlib,io,json,os,sqlite3,subprocess,sys,tempfile,threading,time,unittest
from unittest import mock
ROOT=Path(__file__).resolve().parent;sys.path.insert(0,str(ROOT))
from progress_store import ProgressWriter,ProgressReader,ProgressError,database_path,read_latest,export_once,MAX_SNAPSHOT_BYTES
from supervision import Ledger,final_result_code,supervised

class StoreTests(unittest.TestCase):
    def test_roundtrip_and_generation(self):
        with tempfile.TemporaryDirectory() as td:
            path=Path(td)/'state.sqlite'
            with ProgressWriter(path) as writer,ProgressReader(path) as reader:
                with self.assertRaises(LookupError):reader.latest()
                for n in range(1,5):
                    result=writer.publish({'phase':'fixture','n':n});self.assertEqual(result['generation'],n)
                    self.assertEqual(reader.latest(),({'phase':'fixture','n':n},n))
                self.assertEqual(reader.db.execute('SELECT COUNT(*) FROM history').fetchone()[0],4)
                self.assertEqual(reader.db.execute('PRAGMA quick_check').fetchone()[0],'ok')
    def test_existing_database_is_never_adopted_or_erased(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'state.sqlite';p.write_bytes(b'owned by someone else')
            with self.assertRaises(FileExistsError):ProgressWriter(p)
            self.assertEqual(p.read_bytes(),b'owned by someone else')
    def test_invalid_payload_keeps_previous_committed_state(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'ok':1})
            for value in [{'number':float('nan')},['wrong root'],{'oversize':'x'*(MAX_SNAPSHOT_BYTES+1)}]:
                with self.assertRaises(ValueError):w.publish(value)
            with ProgressReader(w.path) as r:self.assertEqual(r.latest(),({'ok':1},1))
    def test_generation_cap_is_fail_closed(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'ok':1});w.count=4096
            with self.assertRaisesRegex(ProgressError,'generation'):w.publish({'ok':2})
            with ProgressReader(w.path) as r:self.assertEqual(r.latest()[1],1)
    def test_initializing_database_is_pending_not_corruption(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'state.sqlite';p.touch()
            with self.assertRaises(LookupError):ProgressReader(p)
    def test_schema_transaction_is_pending_until_committed(self):
        from progress_store import APP_ID
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'state.sqlite'
            with contextlib.closing(sqlite3.connect(p,isolation_level=None)) as db:
                db.execute('PRAGMA journal_mode=WAL');db.execute('BEGIN IMMEDIATE')
                db.execute(f'PRAGMA application_id={APP_ID}');db.execute('PRAGMA user_version=1')
                db.execute('CREATE TABLE latest(id INTEGER PRIMARY KEY,generation INTEGER,body BLOB,digest TEXT)')
                with self.assertRaises(LookupError):ProgressReader(p)
                db.execute('COMMIT')
                with ProgressReader(p) as reader:
                    with self.assertRaises(LookupError):reader.latest()
    def test_wal_bound_does_not_erase_last_snapshot(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'n':1})
            with mock.patch('progress_store.MAX_WAL_BYTES',0):
                with self.assertRaisesRegex(ProgressError,'WAL'):w.publish({'n':2})
            with ProgressReader(w.path) as r:self.assertEqual(r.latest()[0],{'n':1})
    def test_reader_is_readonly(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'ok':1})
            with ProgressReader(w.path) as r:
                with self.assertRaises(sqlite3.OperationalError):r.db.execute('DELETE FROM latest')
    def test_corrupted_digest_is_rejected_not_reported_success(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'status':'ok'});w.db.execute("UPDATE latest SET digest='incorrect'")
            with ProgressReader(w.path) as r:
                with self.assertRaises(ProgressError):r.latest()
    def test_unrelated_schema_rejected(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'other.sqlite'
            with contextlib.closing(sqlite3.connect(p)) as db:db.execute('CREATE TABLE unrelated(value)')
            with self.assertRaises(ProgressError):ProgressReader(p)
    def test_incomplete_transaction_is_not_visible(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'n':1})
            with ProgressReader(w.path) as r:
                w.db.execute('BEGIN IMMEDIATE');w.db.execute('UPDATE latest SET body=?',(b'partial invalid json',))
                self.assertEqual(r.latest(),({'n':1},1));w.db.execute('ROLLBACK')
                self.assertEqual(r.latest(),({'n':1},1))
    def test_long_lived_reader_transaction_does_not_block_commits(self):
        with tempfile.TemporaryDirectory() as td,ProgressWriter(Path(td)/'state.sqlite') as w:
            w.publish({'n':0})
            with ProgressReader(w.path) as r:
                r.db.execute('BEGIN');self.assertEqual(r.latest()[0]['n'],0)
                start=time.monotonic()
                for n in range(1,251):w.publish({'n':n})
                self.assertLess(time.monotonic()-start,15)
                self.assertEqual(r.latest()[0]['n'],0) # old transaction remains a snapshot
                r.db.execute('COMMIT');self.assertEqual(r.latest()[0]['n'],250)
    def test_final_json_is_create_only_not_a_live_replacement(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'receipt.json';export_once(p,{'n':1})
            with p.open('rb') as held:
                with self.assertRaises(FileExistsError):export_once(p,{'n':2})
                self.assertEqual(json.loads(held.read()),{'n':1})
    def test_open_legacy_json_does_not_block_progress(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'receipt.json';export_once(p,{'legacy':True})
            with p.open('rb') as held,ProgressWriter(database_path(p)) as w:
                for n in range(250):w.publish({'n':n})
                self.assertEqual(json.loads(held.read()),{'legacy':True})
                with ProgressReader(w.path) as r:self.assertEqual(r.latest()[0]['n'],249)
    def test_cleanup_remains_executable_after_publication_failure(self):
        with tempfile.TemporaryDirectory() as td:
            now=time.monotonic();ledger=Ledger(Path(td)/'receipt.json',now,now+10,now+5)
            try:
                with mock.patch.object(ledger.publisher,'publish',side_effect=sqlite3.OperationalError('fixture disk full')),contextlib.redirect_stderr(io.StringIO()):
                    self.assertFalse(ledger.save_best_effort())
                    ledger.state['cleanup']['fixtureClosed']=True
                self.assertTrue(ledger.save_best_effort())
                stored=read_latest(ledger.path);self.assertTrue(stored['receiptPersistenceFailed']);self.assertTrue(stored['cleanup']['fixtureClosed'])
                self.assertEqual(final_result_code(0,stored,True,True),1)
            finally:ledger.close()
    def test_concurrent_reader_process_1000_updates(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)
            with ProgressWriter(p/'state.sqlite') as w:
                w.publish({'generation':0,'repeat':'0'})
                child=subprocess.Popen([sys.executable,'-I','-B',str(__file__),'--reader',td],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                try:
                    deadline=time.monotonic()+10
                    while not(p/'reader.ready').exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.005)
                    self.assertTrue((p/'reader.ready').exists(),'reader did not initialize')
                    for n in range(1,1001):w.publish({'generation':n,'repeat':str(n)})
                    export_once(p/'stop.json',{'stop':True});output,error=child.communicate(timeout=10)
                    self.assertEqual(child.returncode,0,error.decode(errors='replace'))
                    result=json.loads((p/'reader.result.json').read_text());self.assertGreater(result['reads'],0);self.assertEqual(result['invalidSnapshots'],0)
                    with ProgressReader(w.path) as r:self.assertEqual(r.latest()[0]['generation'],1000)
                finally:
                    if child.poll() is None:child.terminate();child.wait(timeout=3)
    def test_killed_writer_retains_last_committed_snapshot(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td);child=subprocess.Popen([sys.executable,'-I','-B',str(__file__),'--uncommitted',td],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                deadline=time.monotonic()+10
                while not(p/'writer.ready').exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.01)
                self.assertTrue((p/'writer.ready').exists())
                child.terminate();child.communicate(timeout=3)
                with ProgressReader(p/'state.sqlite') as r:
                    self.assertEqual(r.latest(),({'committed':True},1));self.assertEqual(r.db.execute('PRAGMA quick_check').fetchone()[0],'ok')
            finally:
                if child.poll() is None:child.kill();child.communicate(timeout=3)
    @unittest.skipUnless(os.name=='nt','Actual Windows supervisor job required')
    def test_real_supervisor_observes_1000_commits(self):
        from winjob import OwnedJob
        with tempfile.TemporaryDirectory() as td,OwnedJob() as job:
            p=Path(td);start=time.monotonic();child=job.spawn([sys.executable,'-I','-B',str(__file__),'--writer',td],ROOT)
            try:
                result=supervised(job,child,p/'receipt.json',start,start+40,start+20,lambda _:None)
                self.assertEqual(result['workerExitCode'],0,result);self.assertTrue(result['jobEmpty']);self.assertGreater(result['lastProgressGeneration'],0)
                self.assertEqual(read_latest(p/'receipt.json')['generation'],1000)
            finally:child.close()


def reader(folder):
    p=Path(folder);reads=0;last=0
    with ProgressReader(p/'state.sqlite') as r:
        export_once(p/'reader.ready',{'ready':True});end=time.monotonic()+30
        while not(p/'stop.json').exists() and time.monotonic()<end:
            value,n=r.latest();assert n>=last;last=n
            assert value['repeat']==str(value['generation']);reads+=1
            # Consumer work happens after the query releases its read transaction.
            time.sleep(.0005)
    export_once(p/'reader.result.json',{'reads':reads,'invalidSnapshots':0,'lastSequence':last})
def writer(folder):
    p=Path(folder)
    with ProgressWriter(database_path(p/'receipt.json')) as w:
        for n in range(1001):
            w.publish({'phase':'observation','processResumed':True,'generation':n})
            if n%25==0:time.sleep(.002)
def uncommitted(folder):
    p=Path(folder);w=ProgressWriter(p/'state.sqlite');w.publish({'committed':True});w.db.execute('BEGIN IMMEDIATE');w.db.execute('UPDATE latest SET body=?',(b'incomplete',));export_once(p/'writer.ready',{'ready':True});time.sleep(30)

if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--reader':reader(sys.argv[2])
    elif len(sys.argv)==3 and sys.argv[1]=='--writer':writer(sys.argv[2])
    elif len(sys.argv)==3 and sys.argv[1]=='--uncommitted':uncommitted(sys.argv[2])
    else:unittest.main(verbosity=2)
