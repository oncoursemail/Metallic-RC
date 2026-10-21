"""Bounded MCP stdio client for installed REA. Offline static analysis only."""
from pathlib import Path
import os,subprocess,json,threading,queue,time
ROOT=Path(__file__).resolve().parent
NODE=r'C:\Program Files\nodejs\node.exe'
REA=r'C:\Users\alexi\AppData\Local\npm-cache\_npx\ad9a3fdbcad992c6\node_modules\rea-agents\scripts\rea.mjs'
TOOLS=Path(r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\rea-native')
class ReaClient:
    def __init__(self,logdir):
        self.directory=Path(logdir);self.directory.mkdir(parents=True,exist_ok=True)
        self.stderr=(self.directory/'mcp.stderr.log').open('wb')
        env=os.environ.copy();env.update(GHIDRA_INSTALL_DIR=str(TOOLS/'ghidra_12.1.4_PUBLIC'),JAVA_HOME=str(TOOLS/'jdk-21.0.12.1+1'),REA_ANALYSIS_PROVIDER='ghidra')
        self.proc=subprocess.Popen([NODE,REA,'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr,env=env,bufsize=0)
        self.queue=queue.Queue();self.counter=0;self.closed=False;self.schemas={}
        def receive():
            try:
                for line in self.proc.stdout:
                    try:self.queue.put(json.loads(line))
                    except Exception:self.queue.put({'unparsedLine':line.decode('utf-8','replace')[:1000]})
            finally:self.queue.put({'transportEOF':True})
        threading.Thread(target=receive,daemon=True).start()
        init=self.request('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'MetallicOfflineAnalysis','version':'1.0'}},40)
        self.save('initialize.json',init);self.notify('notifications/initialized',{})
        self.catalog=self.request('tools/list',{},40);self.save('tools.json',self.catalog)
        self.schemas={x['name']:x for x in self.catalog['result']['tools']}
    def save(self,name,obj):
        (self.directory/name).write_text(json.dumps(obj,indent=2)+'\n',encoding='utf-8',newline='\n')
    def notify(self,method,params):
        self.proc.stdin.write((json.dumps({'jsonrpc':'2.0','method':method,'params':params})+'\n').encode());self.proc.stdin.flush()
    def request(self,method,params,timeout=180):
        self.counter+=1;rid=self.counter
        self.proc.stdin.write((json.dumps({'jsonrpc':'2.0','id':rid,'method':method,'params':params})+'\n').encode());self.proc.stdin.flush()
        deadline=time.monotonic()+timeout
        while True:
            try:message=self.queue.get(timeout=max(.01,deadline-time.monotonic()))
            except queue.Empty:
                self.notify('notifications/cancelled',{'requestId':rid,'reason':'Bounded analysis deadline'})
                raise TimeoutError(f'{method} exceeded {timeout}s')
            if message.get('id')==rid:return message
            if message.get('transportEOF'):raise RuntimeError('REA transport closed')
            with (self.directory/'notifications.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(message)+'\n')
            if time.monotonic()>=deadline:raise TimeoutError(method)
    def tool(self,name,args,filename=None,timeout=180):
        if name not in self.schemas:raise ValueError('Unadvertised tool: '+name)
        response=self.request('tools/call',{'name':name,'arguments':args},timeout)
        if filename:self.save(filename,response)
        return response
    def close(self):
        if self.closed:return
        self.closed=True
        try:
            if self.proc.poll() is None and 'close_binary' in self.schemas:self.tool('close_binary',{},'close.json',25)
        finally:
            try:self.proc.stdin.close();self.proc.wait(timeout=10)
            except Exception:
                self.proc.terminate()
                try:self.proc.wait(timeout=10)
                except subprocess.TimeoutExpired:pass
            self.stderr.close()
    def __enter__(self):return self
    def __exit__(self,*exc):self.close()
if __name__=='__main__':
    with ReaClient(ROOT/'schema-discovery') as c:
        for n in ['open_binary','binary_session','analyze_function','procedure_pseudo_code','export_evidence_bundle','close_binary']:
            t=c.schemas.get(n);print(n,json.dumps({'inputSchema':t.get('inputSchema'),'description':t.get('description','')[:800]} if t else None),flush=True)
