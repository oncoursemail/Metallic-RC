using System.Net;
using System.Net.Http.Headers;
using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.AspNetCore.Builder;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Data.Sqlite;
using Metallic.RC.Backend;
using Metallic.RC.Compatibility;

string output=args.Length==1?Path.GetFullPath(args[0]):throw new ArgumentException("Pass a new result.json path.");
if(File.Exists(output))throw new IOException("Do not overwrite test evidence.");
string root=Path.Combine(Path.GetTempPath(),"mrc-backend-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(root);
string data=Path.Combine(root,"data");var passed=new List<string>();int checks=0,requests=0;
WebApplication? app=null;HttpClient? client=null;var sockets=new List<ClientWebSocket>();
const string password="Only_Test_Password_2026!";
void Check(bool condition,string name){checks++;if(!condition)throw new InvalidOperationException(name);passed.Add(name);}
async Task Start(string folder)
{
    app=BackendApplication.Build(new BackendOptions(0,folder));await app.StartAsync();
    string address=app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
    Check(address.StartsWith("http://127.0.0.1:",StringComparison.Ordinal),"loopback listener only");
    client=new HttpClient(new SocketsHttpHandler{AllowAutoRedirect=false,UseProxy=false}){BaseAddress=new Uri(address),Timeout=TimeSpan.FromSeconds(10)};
}
async Task Stop()
{
    foreach(var socket in sockets){socket.Abort();socket.Dispose();}sockets.Clear();
    client?.Dispose();client=null;
    if(app is not null){await app.StopAsync();await app.DisposeAsync();app=null;}
}
async Task<(int Status,JsonElement Body,string? ETag)> Request(string method,string path,object? body=null,string? token=null,string? admin=null,string? revision=null,string? raw=null,string? origin=null,string? host=null,string contentType="application/json")
{
    using var request=new HttpRequestMessage(new HttpMethod(method),path);
    if(token is not null)request.Headers.Authorization=new AuthenticationHeaderValue("Bearer",token);
    if(admin is not null)request.Headers.Add("X-Metallic-Admin",admin);
    if(revision is not null)request.Headers.TryAddWithoutValidation("If-Match",revision);
    if(origin is not null)request.Headers.Add("Origin",origin);
    if(host is not null)request.Headers.Host=host;
    if(raw is not null||body is not null)request.Content=new StringContent(raw??JsonSerializer.Serialize(body),Encoding.UTF8,contentType);
    using var response=await client!.SendAsync(request);requests++;
    byte[] bytes=await response.Content.ReadAsByteArrayAsync();JsonElement json=default;
    if(bytes.Length>0){using var document=JsonDocument.Parse(bytes);json=document.RootElement.Clone();}
    return((int)response.StatusCode,json,response.Headers.ETag?.ToString());
}
async Task<ClientWebSocket> Connect(string room,string token)
{
    var socket=new ClientWebSocket();socket.Options.SetRequestHeader("Authorization","Bearer "+token);
    var address=new UriBuilder(client!.BaseAddress!){Scheme="ws",Path="/metallic/v1/rooms/"+room+"/socket"}.Uri;
    using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(8));
    try{await socket.ConnectAsync(address,timeout.Token);sockets.Add(socket);return socket;}catch{socket.Dispose();throw;}
}
async Task<JsonElement> Event(ClientWebSocket socket,string type)
{
    using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(8));
    for(int i=0;i<32;i++)
    {
        byte[] buffer=new byte[32768];int length=0;WebSocketReceiveResult result;
        do{result=await socket.ReceiveAsync(new ArraySegment<byte>(buffer,length,buffer.Length-length),timeout.Token);length+=result.Count;if(result.MessageType==WebSocketMessageType.Close)throw new IOException("Closed before expected event.");}while(!result.EndOfMessage);
        using var json=JsonDocument.Parse(buffer.AsMemory(0,length));
        if(json.RootElement.GetProperty("type").GetString()==type)return json.RootElement.Clone();
    }
    throw new InvalidOperationException("Expected event not received.");
}
async Task Send(ClientWebSocket socket,string json)
{using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(5));await socket.SendAsync(Encoding.UTF8.GetBytes(json),WebSocketMessageType.Text,true,timeout.Token);}
try
{
    CredentialFormattingChecks.Run(Check);
    await Start(data);string admin=File.ReadAllText(Path.Combine(data,"admin.key")).Trim();
    var health=await Request("GET","/healthz");Check(health.Status==200&&health.Body.GetProperty("service").GetString()=="metallic-rc-backend","real HTTP health");
    var capabilities=await Request("GET","/metallic/v1/capabilities");Check(!capabilities.Body.GetProperty("compatibility").GetProperty("actualGameClientVerified").GetBoolean(),"native compatibility not invented");
    var discovery=await Request("GET","/nameserver?buildId=23191908");var response=DiscoveryContract.Read(Encoding.UTF8.GetBytes(discovery.Body.GetRawText()));
    var routes=new Dictionary<int,Uri>();DiscoveryContract.ApplyResponse(response,routes,false,_=>{},()=>{});
    Check(discovery.Status==200&&routes.Count==31&&response!.RecNetStatus is null,"real discovery consumed by recovered compatibility library");
    foreach(var route in routes.Values)Check(route.Host=="127.0.0.1"&&route.Port==client!.BaseAddress!.Port&&route.AbsolutePath.StartsWith("/recnet/",StringComparison.Ordinal),"discovery route stays owned");
    Check((await Request("GET","/nameserver?buildId=other")).Status==409,"unsupported build rejected");
    Check((await Request("GET","/recnet/auth/unmapped-login")).Status==501,"unknown native login does not fake success");
    var nativeFailure=await Request("POST","/recnet/auth/connect/token",raw:"grant_type=password&username=fixture&password=never-echo-this",contentType:"application/x-www-form-urlencoded");
    var nativeReply=LoginResponseContract.Read(Encoding.UTF8.GetBytes(nativeFailure.Body.GetRawText()));
    Check(nativeFailure.Status==501&&nativeReply is not null&&nativeReply.Error=="native_login_not_implemented","native auth failure uses the observed response envelope");
    Check(nativeReply!.AccessToken is null&&nativeReply.RefreshToken is null&&nativeReply.Key is null,"unimplemented native auth never fabricates credentials");
    Check(LoginResponseContract.Classify(nativeReply,nativeFailure.Status)==LoginResponseDisposition.Rejected,"native failure cannot be mistaken for completed login");
    Check(!nativeFailure.Body.GetRawText().Contains("never-echo-this",StringComparison.Ordinal),"native failure never echoes submitted credentials");
    Check((await Request("GET","/missing")).Status==404,"unknown route rejected");
    Check((await Request("GET","/healthz",host:"evil.invalid:"+client!.BaseAddress!.Port)).Status==421,"DNS rebinding host blocked");
    Check((await Request("GET","/healthz",origin:"https://evil.invalid")).Status==403,"cross-origin request blocked");
    Check((await Request("POST","/metallic/v1/auth/register",new{username="alpha",password,displayName="Alpha"})).Status==401,"account creation requires local admin key");
    Check((await Request("POST","/metallic/v1/auth/register",admin:admin,raw:"{\"username\":\"a\",\"username\":\"b\"}")).Status==400,"duplicate JSON keys rejected");
    Check((await Request("POST","/metallic/v1/auth/register",new{username="alpha",password,displayName="Alpha",admin=true},admin:admin)).Status==400,"unknown JSON fields rejected");
    Check((await Request("POST","/metallic/v1/auth/register",admin:admin,raw:"{}",contentType:"text/plain")).Status==415,"non-JSON request rejected");
    Check((await Request("POST","/metallic/v1/auth/register",admin:admin,raw:new string(' ',65537))).Status==413,"oversize request rejected");
    var alpha=await Request("POST","/metallic/v1/auth/register",new{username="alpha",password,displayName="Alpha"},admin:admin);Check(alpha.Status==201,"first account persisted");
    var beta=await Request("POST","/metallic/v1/auth/register",new{username="beta",password,displayName="Beta"},admin:admin);Check(beta.Status==201,"second account persisted");
    var gamma=await Request("POST","/metallic/v1/auth/register",new{username="gamma",password,displayName="Gamma"},admin:admin);Check(gamma.Status==201,"third account persisted");
    Check((await Request("POST","/metallic/v1/auth/register",new{username="ALPHA",password,displayName="Duplicate"},admin:admin)).Status==409,"username uniqueness casefolded");
    Check((await Request("POST","/metallic/v1/auth/login",new{username="alpha",password="wrong_password_123"})).Status==401,"bad password rejected");
    var a=await Request("POST","/metallic/v1/auth/login",new{username="alpha",password});var b=await Request("POST","/metallic/v1/auth/login",new{username="beta",password});var g=await Request("POST","/metallic/v1/auth/login",new{username="gamma",password});
    Check(a.Status==200&&b.Status==200&&g.Status==200,"independent accounts authenticate");
    string ta=a.Body.GetProperty("accessToken").GetString()!,tb=b.Body.GetProperty("accessToken").GetString()!,tg=g.Body.GetProperty("accessToken").GetString()!;
    string aid=alpha.Body.GetProperty("id").GetString()!,bid=beta.Body.GetProperty("id").GetString()!;
    Check(ta!=tb&&ta.Length==64,"random distinct sessions");
    Check((await Request("GET","/metallic/v1/me")).Status==401,"profile requires session");
    var me=await Request("GET","/metallic/v1/me",token:ta);Check(me.Status==200&&me.Body.GetProperty("id").GetString()==aid,"session bound to account");
    Check((await Request("PUT","/metallic/v1/me",new{displayName="Updated Alpha"},token:ta)).Status==428,"profile revision required");
    var edits=await Task.WhenAll(Request("PUT","/metallic/v1/me",new{displayName="Updated Alpha"},token:ta,revision:me.ETag),Request("PUT","/metallic/v1/me",new{displayName="Updated Alpha"},token:ta,revision:me.ETag));
    Check(edits.Count(x=>x.Status==200)==1&&edits.Count(x=>x.Status==412)==1,"concurrent profile writes have one winner");
    var room=await Request("POST","/metallic/v1/rooms",new{name="Shared test room",capacity=2,isPublic=true},token:ta);Check(room.Status==201,"persistent room created");string rid=room.Body.GetProperty("id").GetString()!;
    var privateRoom=await Request("POST","/metallic/v1/rooms",new{name="Private room",capacity=2,isPublic=false},token:ta);string privateId=privateRoom.Body.GetProperty("id").GetString()!;
    Check((await Request("GET","/metallic/v1/rooms/"+privateId,token:tb)).Status==404,"private room hidden from other accounts");
    var listing=await Request("GET","/metallic/v1/rooms",token:tb);Check(listing.Body.GetProperty("rooms").GetArrayLength()==1,"private room excluded from other directory");
    var sa=await Connect(rid,ta);var snapA=await Event(sa,"snapshot");Check(snapA.GetProperty("accountId").GetString()==aid,"first WebSocket has server-assigned identity");
    var sb=await Connect(rid,tb);var snapB=await Event(sb,"snapshot");Check(snapB.GetProperty("peers").GetArrayLength()==2,"second independent client sees shared room");
    await Send(sa,"{\"type\":\"pose\",\"sequence\":1,\"x\":1.25,\"y\":2,\"z\":-3}");
    var pose=await Event(sb,"pose");Check(pose.GetProperty("accountId").GetString()==aid&&pose.GetProperty("x").GetDouble()==1.25,"pose broadcast crosses real client connections");
    await Send(sa,"{\"type\":\"pose\",\"sequence\":1,\"x\":9,\"y\":9,\"z\":9}");Check((await Event(sa,"error")).GetProperty("code").GetString()=="stale_sequence","out-of-order pose rejected");
    await Send(sa,"{\"type\":\"pose\",\"sequence\":2,\"x\":1e100,\"y\":0,\"z\":0}");Check((await Event(sa,"error")).GetProperty("code").GetString()=="invalid_pose","out-of-bounds pose rejected");
    await Send(sa,"{\"type\":\"pose\",\"sequence\":2,\"x\":0,\"y\":0,\"z\":0,\"accountId\":\"spoofed\"}");Check((await Event(sa,"error")).GetProperty("code").GetString()=="invalid_json","client cannot spoof actor id");
    bool full=false;try{using var ignored=await Connect(rid,tg);}catch(WebSocketException){full=true;}Check(full,"room capacity enforced on handshake");
    bool denied=false;try{using var ignored=await Connect(privateId,tb);}catch(WebSocketException){denied=true;}Check(denied,"private room socket denied");
    Check((await Request("POST","/metallic/v1/auth/logout",token:tb)).Status==204,"logout revokes durable token");
    JsonElement left;do{left=await Event(sa,"left");}while(left.GetProperty("accountId").GetString()!=bid);
    Check(left.GetProperty("accountId").GetString()==bid,"logout disconnects room peer");
    Check((await Request("GET","/metallic/v1/me",token:tb)).Status==401,"revoked token cannot access profile");
    var newB=await Request("POST","/metallic/v1/auth/login",new{username="beta",password});string tb2=newB.Body.GetProperty("accessToken").GetString()!;
    var sb2=await Connect(rid,tb2);var reconnect=await Event(sb2,"snapshot");
    var savedPose=reconnect.GetProperty("peers").EnumerateArray().Single(p=>p.GetProperty("accountId").GetString()==aid);
    Check(savedPose.GetProperty("x").GetDouble()==1.25&&savedPose.GetProperty("sequence").GetInt64()==1,"reconnect snapshot contains accepted pose only");
    Check(reconnect.GetProperty("epoch").GetString()==snapA.GetProperty("epoch").GetString(),"live room epoch retained across reconnect");
    var backup=await Request("POST","/metallic/v1/admin/backups",admin:admin);Check(backup.Status==201&&backup.Body.GetProperty("integrity").GetString()=="ok","online SQLite backup integrity verified");
    string backupFile=Path.Combine(data,"backups",backup.Body.GetProperty("file").GetString()!);
    Check(Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(backupFile)))==backup.Body.GetProperty("sha256").GetString(),"backup file hash matches report");
    using(var db=new SqliteConnection(new SqliteConnectionStringBuilder{DataSource=Path.Combine(data,"backend.sqlite"),Pooling=false}.ToString()))
    {
        db.Open();using var command=db.CreateCommand();command.CommandText="SELECT password_hash FROM accounts WHERE username='alpha'";
        string hash=(string)command.ExecuteScalar()!;Check(hash!=password&&hash.Length>64,"password stored as salted password hash");
        command.CommandText="SELECT COUNT(*) FROM sessions WHERE token_hash=$token";command.Parameters.AddWithValue("$token",ta);Check((long)command.ExecuteScalar()! ==0,"raw bearer token not stored");
    }
    bool locked=false;try{using var second=new BackendStore(new BackendOptions(0,data));}catch(IOException){locked=true;}Check(locked,"second server cannot own same state directory");
    var diagnostics=await Request("GET","/metallic/v1/admin/diagnostics",admin:admin);
    string diagnosticText=diagnostics.Body.GetRawText();Check(!diagnosticText.Contains(ta)&&!diagnosticText.Contains(password)&&!diagnosticText.Contains(admin),"diagnostics do not expose secrets");
    await Stop();await Start(data);
    Check((await Request("GET","/metallic/v1/me",token:ta)).Body.GetProperty("displayName").GetString()=="Updated Alpha","profile and session survive restart");
    Check((await Request("GET","/metallic/v1/me",token:tb)).Status==401,"revocation survives restart");
    Check((await Request("GET","/metallic/v1/rooms/"+rid,token:ta)).Status==200,"room survives restart");
    var restarted=await Connect(rid,ta);var restartedSnapshot=await Event(restarted,"snapshot");Check(restartedSnapshot.GetProperty("epoch").GetString()!=snapA.GetProperty("epoch").GetString(),"restart establishes new transient room epoch");
    bool limited=false;for(int i=0;i<42;i++){var attempt=await Request("POST","/metallic/v1/auth/login",new{username="bad?",password});if(attempt.Status==429)limited=true;}Check(limited,"authentication request rate limited");
    await Stop();
    string restored=Path.Combine(root,"restored");await Start(restored);await Stop();
    File.Copy(backupFile,Path.Combine(restored,"backend.sqlite"),true);await Start(restored);
    Check((await Request("GET","/metallic/v1/me",token:ta)).Body.GetProperty("id").GetString()==aid,"backup restore recovers account and session");
    Check((await Request("GET","/metallic/v1/rooms/"+rid,token:ta)).Status==200,"backup restore recovers room");
    await Stop();
    var report=new{status="owned_backend_real_transport_tests_passed",createdUtc=DateTimeOffset.UtcNow,checks,httpRequests=requests,webSocketClients=4,externalServiceRequests=0,actualRecRoomClientConnected=false,tests=passed};
    await File.WriteAllTextAsync(output,JsonSerializer.Serialize(report,new JsonSerializerOptions{WriteIndented=true}));
    Console.WriteLine(JsonSerializer.Serialize(new{report.status,checks,requests,actualRecRoomClientConnected=false}));
}
finally{await Stop();if(Directory.Exists(root))Directory.Delete(root,true);}
