using System.Net;
using System.Net.Http.Headers;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Reflection;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Http.Features;
using Microsoft.AspNetCore.Server.Kestrel.Core;
using Microsoft.AspNetCore.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Data.Sqlite;
using Metallic.RC.Backend;
using Metallic.RC.Compatibility;

if(args.Length!=2||args[0]!="--self-test")throw new ArgumentException("Use --self-test NEW-result.json.");
if(File.Exists(args[1]))throw new IOException("Evidence output already exists.");
var checks=new List<string>();void Check(bool v,string label){if(!v)throw new InvalidOperationException(label);checks.Add(label);}
string name="metallic-native-gateway-"+Guid.NewGuid().ToString("N"),root=Path.Combine(Path.GetTempPath(),name);Directory.CreateDirectory(root);
WebApplication? app=null;HttpClient? client=null;string address="";int requests=0;
string Folder(){string p=Path.Combine(root,Guid.NewGuid().ToString("N"));Directory.CreateDirectory(p);return p;}
string Db(string p)=>Path.Combine(p,"backend.sqlite");
SqliteConnection Open(string p)
{if(!Path.GetFullPath(p).StartsWith(Path.GetFullPath(root)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))throw new InvalidOperationException("Fixture escaped generated root.");var db=new SqliteConnection(new SqliteConnectionStringBuilder{DataSource=Db(p),Pooling=false}.ToString());db.Open();return db;}
object? Sql(string p,string sql){using var db=Open(p);using var c=db.CreateCommand();c.CommandText=sql;return c.ExecuteScalar();}
long N(string p,string sql)=>(long)(Sql(p,sql)??throw new InvalidOperationException("Expected fixture integer."));
BackendOptions Options(string p,bool gateway=false)=>new(0,p){EnableNativeIdentityMapping=true,EnableNativeSessions=true,EnableNativeAccountProfiles=true,EnableNativeGateway=gateway};
string Token()=>Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(48));
string Hash(string s)=>Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(s)));
NativeSelfAccountResponseModel Model(AccountView a,int id)=>new(null,null,null,0,null,null,0,id,a.Username,a.DisplayName,null,null,null,false,false,0,0,0,false,null);
string Clone(string p)
{
    using var owner=new FileStream(Path.Combine(p,"backend.lock"),FileMode.OpenOrCreate,FileAccess.ReadWrite,FileShare.None);
    using(var db=Open(p))using(var c=db.CreateCommand()){c.CommandText="PRAGMA wal_checkpoint(TRUNCATE);";using var r=c.ExecuteReader();if(!r.Read()||r.GetInt64(0)!=0)throw new IOException("Fixture checkpoint busy.");}
    if(File.Exists(Db(p)+"-wal")&&new FileInfo(Db(p)+"-wal").Length!=0)throw new IOException("Fixture WAL not drained.");
    string copy=Folder();foreach(string file in new[]{"backend.sqlite",".metallic-backend","admin.key"})File.Copy(Path.Combine(p,file),Path.Combine(copy,file));return copy;
}
async Task Stop()
{client?.Dispose();client=null;if(app is not null){await app.StopAsync();await app.DisposeAsync();app=null;}}
async Task Start(string p,bool on)
{
    app=BackendApplication.Build(on?Options(p,true):new BackendOptions(0,p));await app.StartAsync();
    address=app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
    Check(address.StartsWith("http://127.0.0.1:",StringComparison.Ordinal),"test server is ephemeral IPv4 loopback only");
    client=new HttpClient(new SocketsHttpHandler{UseProxy=false,AllowAutoRedirect=false,ConnectTimeout=TimeSpan.FromSeconds(3)}){BaseAddress=new Uri(address),Timeout=TimeSpan.FromSeconds(10)};
}
async Task<(int Status,byte[] Bytes)> Request(string method,string path,string? form=null,string? bearer=null,string? signature=null,string? contentType=null,bool duplicateSig=false)
{
    requests++;using var r=new HttpRequestMessage(new HttpMethod(method),path);
    if(form is not null){r.Content=new ByteArrayContent(Encoding.UTF8.GetBytes(form));r.Content.Headers.TryAddWithoutValidation("Content-Type",contentType??NativePasswordFormProfile.ContentType);}
    if(bearer is not null)r.Headers.TryAddWithoutValidation("Authorization","Bearer "+bearer);
    if(signature is not null)r.Headers.TryAddWithoutValidation("X-RNSIG",duplicateSig?new[]{signature,signature}:new[]{signature});
    using var response=await client!.SendAsync(r);byte[] bytes=await response.Content.ReadAsByteArrayAsync();
    Check(response.Headers.CacheControl?.NoStore==true,"credential/profile response is not cached");
    return ((int)response.StatusCode,bytes);
}
string Form(string username,string password,string extra="")=>"grant_type=password&username="+Uri.EscapeDataString(username)+"&password="+Uri.EscapeDataString(password)+extra;
string Sign(byte[] key,string target)=>Convert.ToBase64String(HMACSHA256.HashData(key,Encoding.ASCII.GetBytes(target)));
void NativeError((int Status,byte[] Bytes) r,int expected)
{Check(r.Status==expected,"expected native failure HTTP "+expected);var body=LoginResponseContract.Read(r.Bytes);Check(body is not null&&!string.IsNullOrEmpty(body.Error)&&body.AccessToken is null&&body.RefreshToken is null&&body.Key is null,"failure envelope contains no credentials or false success");}
async Task<int> Raw(string target,string token,string signature,string additional="")
{
    requests++;using var socket=new TcpClient();using var deadline=new CancellationTokenSource(TimeSpan.FromSeconds(10));
    await socket.ConnectAsync(IPAddress.Loopback,new Uri(address).Port,deadline.Token);using var stream=socket.GetStream();
    string text="GET "+target+" HTTP/1.1\r\nHost: 127.0.0.1:"+new Uri(address).Port+"\r\nConnection: close\r\nAuthorization: Bearer "+token+"\r\nX-RNSIG: "+signature+"\r\n"+additional+"\r\n";
    await stream.WriteAsync(Encoding.ASCII.GetBytes(text),deadline.Token);byte[] buffer=new byte[8192];using var output=new MemoryStream();int n;
    while((n=await stream.ReadAsync(buffer,deadline.Token))!=0){if(output.Length+n>131072)throw new IOException("Owned response budget exceeded.");output.Write(buffer,0,n);}
    string header=Encoding.ASCII.GetString(output.ToArray());return int.Parse(header.Split('\n',2)[0].Split(' ',3)[1],System.Globalization.CultureInfo.InvariantCulture);
}
async Task RawPostControls(string folder,string form)
{
    const string path="/recnet/auth/connect/token";
    async Task<int> Post(string target,string extra="")
    {
        requests++;using var bound=new CancellationTokenSource(TimeSpan.FromSeconds(5));using var socket=new TcpClient();
        int port=new Uri(address).Port;await socket.ConnectAsync(IPAddress.Loopback,port,bound.Token);using var stream=socket.GetStream();
        string text="POST "+target+" HTTP/1.1\r\nHost: 127.0.0.1:"+port+"\r\nConnection: close\r\nContent-Type: "+NativePasswordFormProfile.ContentType+"\r\nContent-Length: "+Encoding.ASCII.GetByteCount(form)+"\r\n"+extra+"\r\n"+form;
        await stream.WriteAsync(Encoding.ASCII.GetBytes(text),bound.Token);using var output=new MemoryStream();byte[] buffer=new byte[4096];int count;
        while((count=await stream.ReadAsync(buffer,bound.Token))!=0){if(output.Length+count>8192)throw new IOException("Owned response budget exceeded.");output.Write(buffer,0,count);}
        return int.Parse(Encoding.ASCII.GetString(output.ToArray()).Split('\n',2)[0].Split(' ',3)[1],System.Globalization.CultureInfo.InvariantCulture);
    }
    void Empty()=>Check(N(folder,"SELECT count(*) FROM native_sessions;")==0&&N(folder,"SELECT count(*) FROM native_gateway_request_keys;")==0,"rejected raw POST creates no session or request key");
    foreach(string target in new[]{path+"?x=1","/%72ecnet/auth/connect/token","http://127.0.0.1:"+new Uri(address).Port+path,path+"/",path+"#fragment",path+"?bad=%"})
    {Check(await Post(target)==501,"raw POST alias or query is outside the exact token slice");Empty();}
    foreach(var control in new[]{("Content-Type: "+NativePasswordFormProfile.ContentType+"\r\n",501),("Transfer-Encoding: chunked\r\n",400),("Content-Length: "+Encoding.ASCII.GetByteCount(form)+"\r\n",400)})
    {Check(await Post(path,control.Item1)==control.Item2,"ambiguous raw POST headers are rejected by handler or Kestrel");Empty();}
    Check(await Post(path)==200,"exact raw POST positive control succeeds");
    Check(N(folder,"SELECT count(*) FROM native_sessions;")==1&&N(folder,"SELECT count(*) FROM native_gateway_request_keys;")==1,"only exact raw POST commits one session and request key");
}
async Task SlowAndAbortedForms(string folder)
{
    long Completed()=>JsonSerializer.SerializeToElement(app!.Services.GetRequiredService<RequestCounters>().Snapshot()).GetProperty("total").GetInt64();
    long slowBefore=Completed();
    requests++;using(var socket=new TcpClient())using(var bound=new CancellationTokenSource(TimeSpan.FromSeconds(16)))using(var pumping=new CancellationTokenSource())
    {
        await socket.ConnectAsync(IPAddress.Loopback,new Uri(address).Port,bound.Token);using var stream=socket.GetStream();
        string header="POST /recnet/auth/connect/token HTTP/1.1\r\nHost: 127.0.0.1:"+new Uri(address).Port+"\r\nContent-Type: "+NativePasswordFormProfile.ContentType+"\r\nContent-Length: 60000\r\nConnection: close\r\n\r\n";
        await stream.WriteAsync(Encoding.ASCII.GetBytes(header),bound.Token);
        async Task Pump(){try{byte[] chunk=Enumerable.Repeat((byte)'x',512).ToArray();while(!pumping.IsCancellationRequested){await stream.WriteAsync(chunk,pumping.Token);await Task.Delay(400,pumping.Token);}}catch(Exception e)when(e is OperationCanceledException or IOException or ObjectDisposedException){}}
        Task pump=Pump();using var output=new MemoryStream();byte[] buffer=new byte[2048];
        try
        {
            while(output.Length<4096)
            {
                int n=await stream.ReadAsync(buffer,bound.Token);if(n==0)break;output.Write(buffer,0,n);
                string text=Encoding.ASCII.GetString(output.ToArray());if(text.Contains("native_form_timeout",StringComparison.Ordinal))break;
            }
            string received=Encoding.ASCII.GetString(output.ToArray());
            Check(received.StartsWith("HTTP/1.1 408",StringComparison.Ordinal)&&received.Contains("native_form_timeout",StringComparison.Ordinal),"actual slow HTTP body reaches bounded native timeout envelope");
        }
        finally{pumping.Cancel();await pump;}
    }
    using(var completed=new CancellationTokenSource(TimeSpan.FromSeconds(3)))
        while(Completed()<=slowBefore)await Task.Delay(10,completed.Token);
    Check(true,"slow request completes before the disconnected-body control starts");
    long before=Completed();
    requests++;using(var socket=new TcpClient())using(var admitted=new CancellationTokenSource(TimeSpan.FromSeconds(3)))
    {
        await socket.ConnectAsync(IPAddress.Loopback,new Uri(address).Port,admitted.Token);using var stream=socket.GetStream();
        string header="POST /recnet/auth/connect/token HTTP/1.1\r\nHost: 127.0.0.1:"+new Uri(address).Port+"\r\nContent-Type: "+NativePasswordFormProfile.ContentType+"\r\nContent-Length: 60000\r\nExpect: 100-continue\r\n\r\n";
        await stream.WriteAsync(Encoding.ASCII.GetBytes(header),admitted.Token);
        using var interim=new MemoryStream();byte[] buffer=new byte[1024];string received;
        do
        {
            int count=await stream.ReadAsync(buffer,admitted.Token);if(count==0||interim.Length+count>4096)throw new IOException("Owned interim response invalid.");
            interim.Write(buffer,0,count);received=Encoding.ASCII.GetString(interim.ToArray());
        }while(!received.Contains("\r\n\r\n",StringComparison.Ordinal));
        Check(received=="HTTP/1.1 100 Continue\r\n\r\n","actual body-read admission precedes the deliberate disconnect");
        await stream.WriteAsync("x"u8.ToArray(),admitted.Token);
    }
    using var done=new CancellationTokenSource(TimeSpan.FromSeconds(3));
    while(Completed()<=before)await Task.Delay(10,done.Token);
    Check(N(folder,"SELECT count(*) FROM native_sessions;")==0&&N(folder,"SELECT count(*) FROM native_gateway_request_keys;")==0,"disconnected partial body completes without credential state");
}
async Task CancelledHandler(BackendStore store,string form,bool atEof)
{
    using var cancel=new CancellationTokenSource();var context=new DefaultHttpContext();
    context.Request.Method="POST";context.Request.ContentType=NativePasswordFormProfile.ContentType;
    context.Features.Get<IHttpRequestFeature>()!.RawTarget="/recnet/auth/connect/token";context.RequestAborted=cancel.Token;
    byte[] bytes=Encoding.UTF8.GetBytes(form);
    using Stream body=atEof?new CancelAtEofStream(bytes,cancel):new MemoryStream(bytes);context.Request.Body=body;
    using var fault=atEof?null:new LateWriteFault(cancel);
    var method=typeof(BackendStore).Assembly.GetType("Metallic.RC.Backend.NativeGateway")!.GetMethod("Token",BindingFlags.Static|BindingFlags.NonPublic)!;
    bool aborted=false;try{await (Task<IResult>)method.Invoke(null,new object[]{context,store})!;}catch(OperationCanceledException){aborted=true;}
    Check(aborted&&cancel.IsCancellationRequested,atEof?"EOF cancellation cannot enter synchronous login":"cancellation after staged writes rolls back before commit");
    if(fault is not null)Check(!fault.CallbackFailed&&fault.SessionWrites==1&&fault.HistoryWrites==2&&fault.KeyWrites==1,"cancel control actually observed all staged session and key writes");
}
async Task Http2Handlers(string folder,string form,string access,byte[] key)
{
    // Test-only h2c endpoint executes the ACTUAL internal handlers. This does not
    // alter or prove HTTP/2 negotiation on BackendApplication's normal listener.
    using var store=new BackendStore(Options(folder,true));
    var builder=WebApplication.CreateBuilder(new WebApplicationOptions{Args=[],ContentRootPath=AppContext.BaseDirectory});
    builder.Configuration.Sources.Clear();builder.Logging.ClearProviders();
    builder.WebHost.ConfigureKestrel(k=>{k.Listen(IPAddress.Loopback,0,l=>l.Protocols=HttpProtocols.Http2);k.Limits.MaxRequestBodySize=73728;});
    await using var host=builder.Build();host.Use(async(c,next)=>{c.Response.Headers.CacheControl="no-store";await next(c);});
    var type=typeof(BackendStore).Assembly.GetType("Metallic.RC.Backend.NativeGateway")!;
    var token=type.GetMethod("Token",BindingFlags.Static|BindingFlags.NonPublic)!;var account=type.GetMethod("Account",BindingFlags.Static|BindingFlags.NonPublic)!;
    const string path="/recnet/accounts/account/me";
    host.MapPost("/recnet/auth/connect/token",async(HttpContext c)=>await (Task<IResult>)token.Invoke(null,new object[]{c,store})!);
    host.MapGet(path,(HttpContext c)=>(IResult)account.Invoke(null,new object[]{c,store})!);
    await host.StartAsync();
    try
    {
        string origin=host.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
        using var h=new HttpClient(new SocketsHttpHandler{UseProxy=false,AllowAutoRedirect=false}){BaseAddress=new Uri(origin),Timeout=TimeSpan.FromSeconds(10)};
        foreach(bool body in new[]{false,true})
        {
            requests++;using var r=new HttpRequestMessage(HttpMethod.Get,path){Version=HttpVersion.Version20,VersionPolicy=HttpVersionPolicy.RequestVersionExact};
            r.Headers.TryAddWithoutValidation("Authorization","Bearer "+access);r.Headers.TryAddWithoutValidation("X-RNSIG",Sign(key,path));
            if(body)r.Content=new ByteArrayContent("body"u8.ToArray());
            using var response=await h.SendAsync(r);Check(response.Version.Major==2,"actual h2c handler test does not downgrade to HTTP/1");
            Check((int)response.StatusCode==(body?501:200),"HTTP/2 actual body detection permits only bodyless signed GET");
        }
        requests++;using var post=new HttpRequestMessage(HttpMethod.Post,"/recnet/auth/connect/token"){Version=HttpVersion.Version20,VersionPolicy=HttpVersionPolicy.RequestVersionExact,Content=new ByteArrayContent(Encoding.ASCII.GetBytes(form))};
        post.Content.Headers.TryAddWithoutValidation("Content-Type",NativePasswordFormProfile.ContentType);
        using var answer=await h.SendAsync(post);Check(answer.Version.Major==2&&(int)answer.StatusCode==200,"actual HTTP/2 handler reads valid bounded native form");
    }
    finally{await host.StopAsync();}
}
try
{
    string data=Folder(),password=Token();AccountView a,b;int id,bid;SessionView owned;
    using(var s=new BackendStore(new(0,data)))
    {a=s.Register(new("user_"+Guid.NewGuid().ToString("N")[..12],password,"Synthetic A"));b=s.Register(new("user_"+Guid.NewGuid().ToString("N")[..12],password,"Synthetic B"));owned=s.Login(new(a.Username,password));}
    string v1=Clone(data),admin=File.ReadAllText(Path.Combine(data,"admin.key")).Trim();
    using(var s=new BackendStore(new(0,data){EnableNativeIdentityMapping=true})){id=s.GetOrCreateNativeAccountId(a.Id);bid=s.GetOrCreateNativeAccountId(b.Id);}
    string v2=Clone(data);using(var s=new BackendStore(new(0,data){EnableNativeIdentityMapping=true,EnableNativeSessions=true})){}
    string v3=Clone(data);using(var s=new BackendStore(Options(data))){s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(a,id));s.WriteNativeAccountProfile(admin,b.Id,bid,0,Model(b,bid));}
    string v4=Clone(data);
    string rawPost=Clone(v4);await Start(rawPost,true);await RawPostControls(rawPost,Form(a.Username,password));await Stop();
    foreach(string early in new[]{v1,v2,v3})
    {long before=N(early,"PRAGMA user_version;");bool fail=false;try{using var unused=new BackendStore(Options(early,true));}catch(IOException){fail=true;}Check(fail&&N(early,"PRAGMA user_version;")==before,"gateway refuses earlier versions before migrating them");}
    foreach(int missing in new[]{0,1,2})
    {
        var invalid=Options(Folder(),true)with{EnableNativeIdentityMapping=missing!=0,EnableNativeSessions=missing!=1,EnableNativeAccountProfiles=missing!=2};
        bool fail=false;try{using var unused=new BackendStore(invalid);}catch(ArgumentException){fail=true;}Check(fail,"all four explicit feature options are required");
    }
    await Start(data,false);
    NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password)),501);
    Check((await Request("GET","/recnet/accounts/account/me")).Status==501,"default-off native account route remains unimplemented");
    await Stop();Check(N(data,"PRAGMA user_version;")==4&&N(data,"SELECT count(*) FROM sqlite_schema WHERE name='native_gateway_master';")==0,"default-off HTTP requests do not migrate v4");
    await Start(data,true);Check(N(data,"PRAGMA user_version;")==5,"explicit gateway activation migrates v4 to v5");
    await CancelledHandler(app!.Services.GetRequiredService<BackendStore>(),Form(a.Username,password),true);
    Check(N(data,"SELECT count(*) FROM native_sessions;")==0&&N(data,"SELECT count(*) FROM native_gateway_request_keys;")==0,"cancelled EOF emits no credential body or session state");
    await CancelledHandler(app!.Services.GetRequiredService<BackendStore>(),Form(a.Username,password),false);
    Check(N(data,"SELECT count(*) FROM native_sessions;")==0&&N(data,"SELECT count(*) FROM native_gateway_request_keys;")==0,"precommit cancellation leaves no partial session history or request key");
    using(var fault=new LateWriteFault())
    {
        NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password)),503);
        Check(!fault.CallbackFailed&&fault.SessionWrites==1&&fault.HistoryWrites==2&&fault.Denied==1,"real SQLite authorizer failure occurs only after observed earlier staged writes");
        Check(N(data,"SELECT count(*) FROM native_sessions;")==0&&N(data,"SELECT count(*) FROM native_session_tokens;")==0&&N(data,"SELECT count(*) FROM native_gateway_request_keys;")==0,"late SQL failure rolls back session history and key with no credential export");
    }
    await SlowAndAbortedForms(data);
    long oldSessions=N(data,"SELECT count(*) FROM sessions;");
    foreach(string bad in new[]{Form(a.Username,Token()),Form("unknown_user",password)})NativeError(await Request("POST","/recnet/auth/connect/token",bad),401);
    foreach(string extra in new[]{"&require_password=false","&require_password=","&security_code=","&2fa_code=code","&login_context=value","&username=duplicate","&%75sername=duplicate","&usern%D0%B0me=unknown","&unknown=value"})
        NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password,extra)),501);
    NativeError(await Request("POST","/recnet/auth/connect/token","grant_type=refresh_token&refresh_token="+Token()),501);
    NativeError(await Request("POST","/recnet/auth/connect/token","grant_type=password"),400);
    NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password),contentType:"application/json"),501);
    NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password),contentType:NativePasswordFormProfile.ContentType+"; charset=utf-8"),501);
    NativeError(await Request("POST","/recnet/auth/connect/token?context=uncovered",Form(a.Username,password)),501);
    NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password).Replace("password=","password=%FF",StringComparison.Ordinal)),400);
    Check(N(data,"SELECT count(*) FROM native_sessions;")==0&&N(data,"SELECT count(*) FROM native_gateway_request_keys;")==0,"failed forms and credentials create no session or request key");
    var login=await Request("POST","/recnet/auth/connect/token",Form(a.Username,password,"&require_password=true&client_secret=synthetic-discarded&locale=en"));
    Check(login.Status==200,"observed literal true still requires and accepts actual owned credentials");
    var envelope=LoginResponseContract.Read(login.Bytes)!;string access=envelope.AccessToken!,refresh=envelope.RefreshToken!;byte[] key=Convert.FromBase64String(envelope.Key!);
    Check(envelope.Error is null&&key.Length==32,"committed login exports canonical token material and independent request key");
    Check(N(data,"SELECT count(*) FROM native_sessions;")==1&&N(data,"SELECT count(*) FROM native_gateway_request_keys;")==1&&N(data,"SELECT count(*) FROM sessions;")==oldSessions,"native session and request key commit together without an owned-session side effect");
    Check((string)Sql(data,"SELECT access_hash FROM native_sessions;")! ==Hash(access)&&(string)Sql(data,"SELECT refresh_hash FROM native_sessions;")! ==Hash(refresh),"only token hashes are stored");
    byte[] master=(byte[])Sql(data,"SELECT secret FROM native_gateway_master;")!,storedKey=(byte[])Sql(data,"SELECT secret FROM native_gateway_request_keys;")!;
    Check(storedKey.AsSpan().SequenceEqual(key)&&!master.AsSpan().SequenceEqual(key)&&!master.AsSpan().SequenceEqual(Convert.FromHexString(admin)),"master and request keys are separate from each other and administrator material");
    const string path="/recnet/accounts/account/me";
    var account=await Request("GET",path,bearer:access,signature:Sign(key,path));
    Check(account.Status==200&&account.Bytes.AsSpan().SequenceEqual(NativeSelfAccountResponseProfile.Write(Model(a,id))),"actual signed HTTP account request returns only its stored explicit profile");
    foreach(string target in new[]{path+"?x=%2f&x=%2F",path+"?literal=1&other=2"})Check(await Raw(target,access,Sign(key,target))==200,"signed raw query bytes are bound verbatim and semantically ignored");
    Check(await Raw(path+"?x=1",access,Sign(key,path+"?x=2"))==401,"different raw query spelling fails signature");
    foreach(string target in new[]{"/%72ecnet/accounts/account/me",path+"#fragment",path.Replace("accounts","Accounts",StringComparison.Ordinal),"http://127.0.0.1:"+new Uri(address).Port+path,path+"?bad=\\value",path+"?bad=%",path+"?bad=%GG"})
        Check(await Raw(target,access,Sign(key,target))!=200,"raw aliases absolute form fragments and backslashes cannot enter the account slice");
    foreach(string sig in new[]{"",Sign(RandomNumberGenerator.GetBytes(32),path),Sign(key,path).Insert(4," "),"bad"})NativeError(await Request("GET",path,bearer:access,signature:sig),401);
    NativeError(await Request("GET",path,bearer:access,signature:Sign(key,path),duplicateSig:true),401);
    NativeError(await Request("GET",path,bearer:refresh,signature:Sign(key,path)),401);
    NativeError(await Request("GET",path,bearer:owned.AccessToken,signature:Sign(key,path)),401);
    NativeError(await Request("GET",path,"body",access,Sign(key,path)),501);
    Check((await Request("POST",path,"body",access,Sign(key,path))).Status==501,"a GET signature cannot authorize another method");
    var unissued=NativeTokenResponse.Create(bid,master,DateTimeOffset.FromUnixTimeSeconds(DateTimeOffset.UtcNow.ToUnixTimeSeconds()),DateTimeOffset.FromUnixTimeSeconds(DateTimeOffset.UtcNow.ToUnixTimeSeconds()).AddMinutes(30),DateTimeOffset.FromUnixTimeSeconds(DateTimeOffset.UtcNow.ToUnixTimeSeconds()).AddDays(1));
    var unissuedBody=LoginResponseContract.Read(unissued.WriteTransportBody())!;
    NativeError(await Request("GET",path,bearer:unissuedBody.AccessToken,signature:Sign(Convert.FromBase64String(unissuedBody.Key!),path)),401);
    Check(true,"a valid locally signed JWT is not authority without its exact issued token record");
    CryptographicOperations.ZeroMemory(master);
    await Stop();
    await Http2Handlers(Clone(data),Form(a.Username,password),access,key);
    using(var s=new BackendStore(Options(data,true)))
    {
        Check(s.Authenticate("Bearer "+owned.AccessToken).AccountId==a.Id,"v5 preserves existing owned account API");
        s.UpdateProfile(s.Authenticate("Bearer "+owned.AccessToken),new("Explicit renamed A"),a.Revision);
    }
    await Start(data,true);NativeError(await Request("GET",path,bearer:access,signature:Sign(key,path)),409);
    long count=N(data,"SELECT count(*) FROM native_sessions;");NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password)),409);
    Check(N(data,"SELECT count(*) FROM native_sessions;")==count,"stale profile refuses login before creating any session");await Stop();
    using(var s=new BackendStore(Options(data,true))){a=s.Profile(a.Id);s.WriteNativeAccountProfile(admin,a.Id,id,1,Model(a,id));}
    await Start(data,true);Check((await Request("GET",path,bearer:access,signature:Sign(key,path))).Status==200,"restart preserves exact request key and explicitly repaired profile");await Stop();
    string snapshot=Clone(data),restore=Folder();
    using(var s=new BackendStore(Options(data,true))){using var metadata=JsonDocument.Parse(JsonSerializer.Serialize(s.Backup()));File.Copy(Path.Combine(data,"backups",metadata.RootElement.GetProperty("file").GetString()!),Db(restore));}
    File.Copy(Path.Combine(data,".metallic-backend"),Path.Combine(restore,".metallic-backend"));
    await Start(restore,true);Check((await Request("GET",path,bearer:access,signature:Sign(key,path))).Status==200,"consistent backup restores token state profile and request key together");await Stop();
    string rotatedAccess=Token(),rotatedRefresh=Token();
    using(var s=new BackendStore(Options(data,true)))s.RotateNativeSession(refresh,a.Id,id,new(rotatedAccess,rotatedRefresh,DateTimeOffset.UtcNow.AddMinutes(30),DateTimeOffset.UtcNow.AddDays(1)));
    await Start(data,true);NativeError(await Request("GET",path,bearer:rotatedAccess,signature:Sign(key,path)),501);NativeError(await Request("GET",path,bearer:access,signature:Sign(key,path)),401);await Stop();
    string expired=Clone(snapshot);_=Sql(expired,"UPDATE native_sessions SET created_at=created_at-100,refresh_expires_at=refresh_expires_at-100,access_expires_at="+(DateTimeOffset.UtcNow.ToUnixTimeSeconds()-1)+";");
    await Start(expired,true);NativeError(await Request("GET",path,bearer:access,signature:Sign(key,path)),401);await Stop();
    string revoked=Clone(snapshot);using(var s=new BackendStore(Options(revoked,true)))s.RevokeNativeSession(access,a.Id,id);
    await Start(revoked,true);NativeError(await Request("GET",path,bearer:access,signature:Sign(key,path)),401);await Stop();
    string failure=Clone(v4);_=Sql(failure,"CREATE INDEX native_gateway_request_keys_secret ON accounts(username);");bool migrationFailed=false;
    try{using var unused=new BackendStore(Options(failure,true));}catch(IOException){migrationFailed=true;}
    Check(migrationFailed&&N(failure,"PRAGMA user_version;")==4&&N(failure,"SELECT count(*) FROM sqlite_schema WHERE name='native_gateway_master';")==0,"v5 migration conflict rolls back both new tables and version");
    foreach(string mutation in new[]{"DELETE FROM native_gateway_master;","PRAGMA ignore_check_constraints=ON;UPDATE native_gateway_request_keys SET secret=zeroblob(33);","CREATE TRIGGER unexpected_key AFTER INSERT ON native_gateway_request_keys BEGIN SELECT 1;END;","UPDATE native_gateway_request_keys SET generation=32;"})
    {
        string malformed=Clone(snapshot);_=Sql(malformed,mutation);bool rejected=false;try{using var unused=new BackendStore(new(0,malformed));}catch(IOException){rejected=true;}Check(rejected,"malformed v5 keys refused even with gateway off");
    }
    string noProfile=Clone(v4);_=Sql(noProfile,"DELETE FROM native_account_profiles WHERE account_id='"+a.Id+"';");
    await Start(noProfile,true);NativeError(await Request("POST","/recnet/auth/connect/token",Form(a.Username,password)),404);Check(N(noProfile,"SELECT count(*) FROM native_sessions;")==0,"missing explicit profile creates no session or key");await Stop();
    await Start(snapshot,false);Check((await Request("GET",path,bearer:access,signature:Sign(key,path))).Status==501,"previously enabled v5 stays inactive when reopened off");await Stop();
    var report=new{status="native_gateway_self_tests_passed",checks=checks.Count,requests,tests=checks,ephemeralLoopbackOnly=true,installedDatabaseAccessed=false,originalClientExecuted=false,productionActivated=false,nativeClientVerified=false};
    using(var output=new FileStream(args[1],FileMode.CreateNew))JsonSerializer.Serialize(output,report,new JsonSerializerOptions{WriteIndented=true});Console.WriteLine(JsonSerializer.Serialize(new{report.status,report.checks,report.requests}));
}
finally
{
    await Stop();string full=Path.GetFullPath(root),temp=Path.GetFullPath(Path.GetTempPath());
    if(!string.Equals(full,Path.GetFullPath(Path.Combine(temp,name)),StringComparison.OrdinalIgnoreCase)||!name.StartsWith("metallic-native-gateway-",StringComparison.Ordinal)||!Guid.TryParseExact(name["metallic-native-gateway-".Length..],"N",out _)||
        !full.StartsWith(temp.TrimEnd(Path.DirectorySeparatorChar)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase)||(File.GetAttributes(full)&FileAttributes.ReparsePoint)!=0)throw new InvalidOperationException("Temporary cleanup target invalid.");
    Directory.Delete(full,true);
}
