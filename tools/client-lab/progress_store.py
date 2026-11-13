"""Transactional, bounded progress snapshots for one local test run.
No live JSON replacement or reader/writer mutex. A single publisher commits to
SQLite WAL; readers fetch a complete committed snapshot and release the query
before parsing or displaying it. JSON is exported once after the worker exits.
"""
from __future__ import annotations
import hashlib,json,os,pathlib,sqlite3,time

APP_ID=0x4d524350
MAX_SNAPSHOT_BYTES=2_097_152
MAX_GENERATIONS=4096
MAX_WAL_BYTES=67_108_864

class ProgressError(RuntimeError):pass

def database_path(receipt):
    p=pathlib.Path(receipt)
    return p.with_name(p.name+'.progress.sqlite')

def encode(value):
    if not isinstance(value,dict):raise ValueError('A progress snapshot must be an object')
    blob=json.dumps(value,ensure_ascii=True,allow_nan=False,separators=(',',':')).encode('utf8')
    if len(blob)>MAX_SNAPSHOT_BYTES:raise ValueError('Progress snapshot exceeds byte limit')
    return blob

def _validate(db):
    # Validate the schema/header from one read transaction, not across an init commit.
    db.execute('BEGIN')
    try:
        tables=db.execute("SELECT COUNT(*) FROM sqlite_master WHERE type='table'").fetchone()[0]
        app_id=db.execute('PRAGMA application_id').fetchone()[0]
        version=db.execute('PRAGMA user_version').fetchone()[0]
        if app_id==0 and version==0 and tables==0:
            raise LookupError('Progress initialization has not committed yet')
        if app_id!=APP_ID or version!=1:
            raise ProgressError('Not a supported Metallic progress database')
    finally:db.execute('ROLLBACK')

class ProgressWriter:
    def __init__(self,path):
        self.path=pathlib.Path(path).absolute();self.db=None;self.closed=False;self.count=0
        # Each run gets a new database; old or user-supplied state is never adopted.
        fd=os.open(self.path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600);os.close(fd)
        try:
            self.db=sqlite3.connect(self.path,timeout=.2,isolation_level=None)
            self.db.execute('PRAGMA busy_timeout=200')
            if self.db.execute('PRAGMA journal_mode=WAL').fetchone()[0].lower()!='wal':raise ProgressError('WAL unavailable')
            self.db.execute('PRAGMA synchronous=FULL')
            self.db.execute('PRAGMA wal_autocheckpoint=256')
            self.db.execute('PRAGMA journal_size_limit=4194304')
            self.db.execute('PRAGMA max_page_count=16384')
            self.db.executescript(f'''
                BEGIN IMMEDIATE;
                PRAGMA application_id={APP_ID};
                PRAGMA user_version=1;
                CREATE TABLE latest(id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER NOT NULL,body BLOB NOT NULL,digest TEXT NOT NULL);
                CREATE TABLE history(generation INTEGER PRIMARY KEY,phase TEXT NOT NULL,status TEXT NOT NULL,bytes INTEGER NOT NULL,digest TEXT NOT NULL);
                COMMIT;
            ''')
        except BaseException:self.close();raise
    def publish(self,value):
        if self.closed:raise ProgressError('Progress writer is closed')
        body=encode(value)
        if self.count>=MAX_GENERATIONS:raise ProgressError('Progress generation limit exceeded')
        wal=pathlib.Path(str(self.path)+'-wal')
        if wal.exists() and wal.stat().st_size>MAX_WAL_BYTES:raise ProgressError('Progress WAL size limit exceeded')
        digest=hashlib.sha256(body).hexdigest();generation=self.count+1
        try:
            self.db.execute('BEGIN IMMEDIATE')
            self.db.execute('INSERT INTO latest VALUES(1,?,?,?) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation,body=excluded.body,digest=excluded.digest',(generation,body,digest))
            self.db.execute('INSERT INTO history VALUES(?,?,?,?,?)',(generation,str(value.get('phase',''))[:80],str(value.get('status',''))[:120],len(body),digest))
            self.db.execute('COMMIT')
        except BaseException:
            if self.db.in_transaction:self.db.execute('ROLLBACK')
            raise
        self.count=generation
        return {'published':True,'generation':generation,'sha256':digest}
    def close(self):
        if self.db is not None:self.db.close();self.db=None
        self.closed=True
    def __enter__(self):return self
    def __exit__(self,*args):self.close()

class ProgressReader:
    def __init__(self,path):
        self.path=pathlib.Path(path).absolute();self.db=None
        try:
            self.db=sqlite3.connect(self.path.as_uri()+'?mode=ro',uri=True,timeout=.2,isolation_level=None)
            self.db.execute('PRAGMA query_only=ON');self.db.execute('PRAGMA busy_timeout=200');_validate(self.db)
        except BaseException:self.close();raise
    def latest(self):
        cursor=self.db.execute('SELECT generation,body,digest FROM latest WHERE id=1')
        try:row=cursor.fetchone()
        finally:cursor.close()
        if row is None:raise LookupError('No committed progress snapshot')
        generation,body,digest=row
        if not isinstance(body,bytes) or len(body)>MAX_SNAPSHOT_BYTES or not isinstance(generation,int) or not 1<=generation<=MAX_GENERATIONS:
            raise ProgressError('Invalid progress record bounds')
        if hashlib.sha256(body).hexdigest()!=digest:raise ProgressError('Progress digest mismatch')
        value=json.loads(body)
        if not isinstance(value,dict):raise ProgressError('Invalid progress object')
        return value,generation
    def close(self):
        if self.db is not None:self.db.close();self.db=None
    def __enter__(self):return self
    def __exit__(self,*args):self.close()

def read_latest(receipt):
    with ProgressReader(database_path(receipt)) as reader:return reader.latest()[0]

def export_once(path,value):
    """Create a final JSON once. Refuse overwrite, including an open legacy receipt.
    Readers of live progress never open this file; final failure does not stop cleanup.
    """
    data=(json.dumps(value,indent=2,allow_nan=False)+'\n').encode('utf8')
    if len(data)>MAX_SNAPSHOT_BYTES:raise ValueError('Final receipt exceeds byte limit')
    with pathlib.Path(path).open('xb') as stream:stream.write(data);stream.flush();os.fsync(stream.fileno())

def inspect(path):
    with ProgressReader(path) as reader:
        state,generation=reader.latest()
        return {'generation':generation,'snapshot':state,'integrity':reader.db.execute('PRAGMA quick_check').fetchone()[0]}
