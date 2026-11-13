using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
namespace Metallic.Reconstruction;

// Authored deterministic tests of the compatibility component. This is not a
// live Rec Room session and does not evaluate original OS exception handling.
public static class ContractSelfTests
{
    public static async Task RunAsync(string report)
    {
        int checks=0,cases=0;
        void Check(bool ok,string why){checks++;if(!ok)throw new Exception(why);}
        void Expect<T>(Action action,string why)where T:Exception{bool thrown=false;try{action();}catch(T){thrown=true;}Check(thrown,why);}
        DiscoveryResponse Valid(int seed=0){var r=new DiscoveryResponse();foreach(int id in DiscoveryContract.ServiceOrder)r.Values[Array.IndexOf(DiscoveryContract.CanonicalNames,DiscoveryContract.ServiceNames[id])]=$"https://s{id}.example.invalid/build-{seed}";return r;}
        var dictionary=new Dictionary<int,Uri>();int saved=0,invalidated=0;DiscoveryResponse? cached=null;
        void Save(DiscoveryResponse x){saved++;cached=x;}
        void Invalidate(){invalidated++;}
        for(int seed=0;seed<128;seed++){
            var response=Valid(seed);dictionary.Clear();saved=0;invalidated=0;cached=null;
            response.RecNetStatus=seed%2==0?null:"";
            DiscoveryContract.ApplyResponse(response,dictionary,false,Save,Invalidate);
            Check(dictionary.Count==31&&saved==1&&invalidated==0,"Initial response writes all routes and saves once.");
            Check(cached is not null&&cached.RecNetStatus is null,"Reverse cached response leaves status null.");
            foreach(int id in DiscoveryContract.ServiceOrder)Check(dictionary[id].OriginalString.EndsWith('/'),"Address suffix preserved.");
            byte[] bytes=DiscoveryContract.Write(cached);var roundtrip=DiscoveryContract.Read(bytes);
            Check(roundtrip is not null&&roundtrip.Values.SequenceEqual(cached!.Values),"Cache wire roundtrip.");
            DiscoveryContract.ApplyResponse(response,dictionary,true,Save,Invalidate);
            Check(saved==1&&invalidated==0,"Unchanged loaded cache is not saved or invalidated.");
            int changed=seed%31;response.Values[Array.IndexOf(DiscoveryContract.CanonicalNames,DiscoveryContract.ServiceNames[changed])]+="/changed";
            Expect<DiscoveryChangedException>(()=>DiscoveryContract.ApplyResponse(response,dictionary,true,Save,Invalidate),"Changed cache is invalidated, then signaled.");
            Check(saved==1&&invalidated==1,"Changed cache must not be persisted before reconnect.");
            Check(dictionary[changed].OriginalString.EndsWith("/changed/"),"Observed update occurred before changed notification.");
            cases+=4;
        }
        foreach(string status in new[]{"OK","error"," ","503"}){
            var r=Valid();r.RecNetStatus=status;var routes=new Dictionary<int,Uri>();
            Expect<DiscoveryStatusException>(()=>DiscoveryContract.ApplyResponse(r,routes,false,_=>throw new Exception("Unexpected save"),()=>throw new Exception("Unexpected invalidate")),"Nonempty status is failure, even OK.");Check(routes.Count==0,"Status failure must not mutate routes.");cases++;
        }
        foreach(byte[] bytes in new[]{"null"u8.ToArray(),"{}"u8.ToArray()}){
            var r=DiscoveryContract.Read(bytes);Check(r is null,"Root null and empty object are missing responses.");
            Expect<DiscoveryResponseMissingException>(()=>DiscoveryContract.ApplyResponse(r,new(),false,_=>{},()=>{}),"Missing response rejection.");cases++;
        }
        for(int stop=0;stop<31;stop++){
            var r=Valid();int id=DiscoveryContract.ServiceOrder[stop];r.Values[Array.IndexOf(DiscoveryContract.CanonicalNames,DiscoveryContract.ServiceNames[id])]=null;var routes=new Dictionary<int,Uri>();bool threw=false;
            try{DiscoveryContract.ApplyResponse(r,routes,false,_=>throw new Exception("Unexpected save"),()=>{});}catch(Exception ex){threw=ex.GetType()==typeof(Exception);}
            Check(threw,"Missing service address fails.");Check(routes.Count==stop,"Earlier sequential updates remain; later ones do not happen.");
            Check(DiscoveryContract.ServiceOrder.Take(stop).All(routes.ContainsKey),"Failure follows actual native order.");cases++;
        }
        Check(DiscoveryContract.CacheKey(null)=="CachedNameServerResponse_","Null cache partition follows concatenation.");
        Check(DiscoveryContract.CacheKey("fixture")=="CachedNameServerResponse_fixture","Cache namespace spelling.");
        // Adapter-only guardrails. Native malformed-input parity is not asserted.
        foreach(string invalid in new[]{"", "[]", "42", "true", "{", "{}{}", "{\"Auth\":1}","{\"Auth\":true}","{\"Auth\":[]}","{\"Auth\":{}}","{\"Auth\":\"x\",}","{/*comment*/}"}){
            Expect<JsonException>(()=>DiscoveryContract.Read(Encoding.UTF8.GetBytes(invalid)),"Malformed/unsupported fixture rejected.");cases++;
        }
        Expect<JsonException>(()=>DiscoveryContract.Read(new byte[1_048_577]),"Size guardrail.");cases++;
        var unknown=DiscoveryContract.Read("{\"unknown\":{\"nested\":[1,true,null]},\"Auth\":\"x\"}"u8);Check(unknown!.Values[2]=="x","Nested unknown skipped without losing next property.");cases++;
        var duplicate=DiscoveryContract.Read("{\"Auth\":\"first\",\"auth\":null}"u8);Check(duplicate!.Values[2] is null,"Last exact alias wins, including null.");cases++;
        var uppercase=DiscoveryContract.Read("{\"AUTH\":\"ignored\"}"u8);Check(uppercase is not null&&uppercase.Values.All(v=>v is null),"No blanket case-insensitive matching.");cases++;
        // Explicit cleanup boundary: C# no-op List<Action>.Enumerator.Dispose and
        // unchanged exception propagation are tested separately from native EH.
        var events=new List<string>();var problem=new Exception("sentinel");
        try{using var enumerator=new List<Action>{()=>{}}.GetEnumerator();events.Add("entered");throw problem;}
        catch(Exception ex){Check(ReferenceEquals(ex,problem),"Managed cleanup does not replace original exception.");events.Add("caught");}
        Check(events.SequenceEqual(new[]{"entered","caught"}),"Managed cleanup control flow.");cases++;
        var http=await RunLoopbackFixtureAsync(Check);cases+=http.Requests;
        var result=new {status="startup_contract_and_loopback_tests_passed",cases,checks,httpRequests=http.Requests,listenerAddress="127.0.0.1",ephemeralPort=http.Port,externalRequests=0,listenerStopped=true,actualGameConnected=false,completeOriginalAsyncReconstruction=false,scope="Compiled authored compatibility component and its loopback HTTP fixture; not original client/backend acceptance."};
        File.WriteAllText(report,JsonSerializer.Serialize(result,new JsonSerializerOptions{WriteIndented=true})+"\n");Console.WriteLine(JsonSerializer.Serialize(result));
    }
    private static async Task<(int Requests,int Port)> RunLoopbackFixtureAsync(Action<bool,string> check)
    {
        var response=new DiscoveryResponse();foreach(int id in DiscoveryContract.ServiceOrder)response.Values[Array.IndexOf(DiscoveryContract.CanonicalNames,DiscoveryContract.ServiceNames[id])]=$"https://s{id}.example.invalid";
        byte[] good=DiscoveryContract.Write(response);int requests=0;
        var listener=new TcpListener(IPAddress.Loopback,0);listener.Start();int port=((IPEndPoint)listener.LocalEndpoint).Port;
        using var deadline=new CancellationTokenSource(TimeSpan.FromSeconds(20));
        var server=Task.Run(async()=>{
            for(int i=0;i<4;i++){
                using var socket=await listener.AcceptTcpClientAsync(deadline.Token);await using var stream=socket.GetStream();
                var request=new List<byte>();byte[] one=new byte[1];
                while(request.Count<8192){int n=await stream.ReadAsync(one,deadline.Token);if(n==0)break;request.Add(one[0]);if(request.Count>=4&&request.TakeLast(4).SequenceEqual(new byte[]{13,10,13,10}))break;}
                string first=Encoding.ASCII.GetString(request.ToArray()).Split('\r')[0];requests++;
                byte[] body=first.Contains("/empty ",StringComparison.Ordinal)?"{}"u8.ToArray():first.Contains("/malformed ",StringComparison.Ordinal)?"{"u8.ToArray():good;
                string status=first.Contains("/http-error ",StringComparison.Ordinal)?"503 Service Unavailable":"200 OK";
                byte[] headers=Encoding.ASCII.GetBytes($"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {body.Length}\r\nConnection: close\r\n\r\n");
                await stream.WriteAsync(headers,deadline.Token);await stream.WriteAsync(body,deadline.Token);
            }
        },deadline.Token);
        try{
            using var handler=new SocketsHttpHandler{UseProxy=false};using var client=new HttpClient(handler){Timeout=TimeSpan.FromSeconds(5)};
            async Task<DiscoveryResponse?> Get(string path,CancellationToken token){
                using var message=await client.GetAsync($"http://127.0.0.1:{port}/{path}",HttpCompletionOption.ResponseHeadersRead,token);message.EnsureSuccessStatusCode();
                if(message.Content.Headers.ContentLength>1_048_576)throw new JsonException("Fixture too large.");
                await using var input=await message.Content.ReadAsStreamAsync(token);using var data=new MemoryStream();byte[] chunk=new byte[4096];int n;
                while((n=await input.ReadAsync(chunk,token))!=0){if(data.Length+n>1_048_576)throw new JsonException("Fixture too large.");data.Write(chunk,0,n);}
                return DiscoveryContract.Read(data.ToArray());
            }
            var actual=await Get("valid",deadline.Token);check(actual is not null&&actual.Values.SequenceEqual(response.Values),"HTTP fixture preserves compiled contract fields.");
            check(await Get("empty",deadline.Token)is null,"Empty HTTP object remains missing response.");
            bool bad=false;try{await Get("malformed",deadline.Token);}catch(JsonException){bad=true;}check(bad,"Malformed HTTP fixture is not accepted.");
            bad=false;try{await Get("http-error",deadline.Token);}catch(HttpRequestException){bad=true;}check(bad,"HTTP failure is not a successful discovery response.");
            using var cancelled=new CancellationTokenSource();cancelled.Cancel();bad=false;try{await Get("must-not-connect",cancelled.Token);}catch(OperationCanceledException){bad=true;}check(bad,"Cancellation is not swallowed.");
            await server;check(requests==4,"Only four local requests were made.");
        }finally{listener.Stop();deadline.Cancel();}
        var probe=new TcpListener(IPAddress.Loopback,port);probe.Start();probe.Stop();check(true,"Loopback port released.");return(requests,port);
    }
}
