"""Read-only navigator for the exact captured Metallic-RC analysis database.

Examples:
  python query_atlas.py search "NameServer" --limit 20
  python query_atlas.py method 148260
  python query_atlas.py calls 148260 --limit 50
  python query_atlas.py callers 148260 --limit 50
  python query_atlas.py fields 22192
  python query_atlas.py coverage

Names and inferred native edges are evidence, not proof of all runtime behavior.
"""
from pathlib import Path
import argparse,sqlite3,json,sys
ROOT=Path(__file__).resolve().parent

def connect(path):
    path=Path(path).resolve(strict=True)
    con=sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)
    con.row_factory=sqlite3.Row;con.execute('PRAGMA query_only=ON')
    return con

def output(value):print(json.dumps(value,indent=2,ensure_ascii=True))

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('command',choices=['search','method','calls','callers','fields','coverage'])
    ap.add_argument('value',nargs='?');ap.add_argument('--database',type=Path,default=ROOT/'Metallic-RC.Analysis.sqlite')
    ap.add_argument('--limit',type=int,default=30);ap.add_argument('--offset',type=int,default=0)
    a=ap.parse_args()
    if not 1<=a.limit<=500 or a.offset<0:ap.error('Use limit 1..500 and nonnegative offset')
    if a.command!='coverage' and a.value is None:ap.error('This command needs a search term or numeric identifier')
    with connect(a.database) as db:
        if a.command=='coverage':
            facts={r['key']:json.loads(r['value']) for r in db.execute('SELECT * FROM metadata')}
            facts.pop('inputManifest',None);output(facts);return 0
        if a.command=='search':
            # Quoted FTS prefix phrase finds NameServerResponse from NameServer.
            # User input remains literal text, not SQL or arbitrary FTS grammar.
            term='"'+a.value.replace('"','""')+'"*'
            rows=db.execute('SELECT m.id,m.assembly,m.type,m.name,m.returns,m.rva FROM method_search JOIN methods m ON method_search.rowid=m.id WHERE method_search MATCH ? ORDER BY rank LIMIT ? OFFSET ?',(term,a.limit+1,a.offset)).fetchall()
            output({'query':a.value,'offset':a.offset,'hasMore':len(rows)>a.limit,'results':[dict(x) for x in rows[:a.limit]]});return 0
        try:identifier=int(a.value,0)
        except ValueError:ap.error('Numeric IDs must be integers, optionally 0x-prefixed')
        if a.command=='fields':
            rows=db.execute('SELECT raw_json FROM fields WHERE type_id=? ORDER BY id LIMIT ? OFFSET ?',(identifier,a.limit+1,a.offset)).fetchall()
            output({'typeIndex':identifier,'offset':a.offset,'hasMore':len(rows)>a.limit,'fields':[json.loads(x[0]) for x in rows[:a.limit]]});return 0
        m=db.execute('SELECT * FROM methods WHERE id=?',(identifier,)).fetchone()
        if m is None:raise ValueError('Method index not present in this captured build')
        if a.command=='method':
            leaf=db.execute('SELECT raw_json FROM recovered_leafs WHERE method_id=?',(identifier,)).fetchone()
            rng=db.execute('SELECT * FROM native_ranges WHERE start_rva=?',(m['rva'],)).fetchone()
            output({'metadata':json.loads(m['raw_json']),'recoveredLeaf':json.loads(leaf[0]) if leaf else None,'nativeUnwindRange':dict(rng) if rng else None,'unknownBody':not bool(leaf),'sourceAssemblyFile':str(ROOT.parent/'csharp-reconstruction/CSharp.Source'/(m['assembly']+'.partial.cs'))});return 0
        column='source_rva' if a.command=='calls' else 'target_rva'
        rows=db.execute(f'SELECT * FROM direct_flows WHERE {column}=? ORDER BY instruction_rva LIMIT ? OFFSET ?',(m['rva'],a.limit+1,a.offset)).fetchall()
        results=[]
        for r in rows[:a.limit]:
            r=dict(r);target=r['target_rva'] if a.command=='calls' else r['source_rva']
            r['methodCandidates']=[dict(x) for x in db.execute('SELECT id,assembly,type,name,returns FROM methods WHERE rva=? ORDER BY id LIMIT 12',(target,))]
            r['candidateListMayBeSharedOrTruncated']=True;results.append(r)
        output({'methodIndex':identifier,'offset':a.offset,'hasMore':len(rows)>a.limit,'observations':results,'meaning':'Static operands, not a guarantee of execution. Shared bodies can map to multiple metadata methods.'})
    return 0
if __name__=='__main__':
    try:raise SystemExit(main())
    except (OSError,sqlite3.Error,ValueError) as e:print(json.dumps({'error':str(e)}),file=sys.stderr);raise SystemExit(1)
