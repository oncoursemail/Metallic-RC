using System.Net;
using System.Text.Json;
using System.Threading.RateLimiting;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.AspNetCore.RateLimiting;
using Microsoft.Data.Sqlite;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Backend;

public static class BackendApplication
{
    public static WebApplication Build(BackendOptions options)
    {
        if(options.Port is <0 or >65535)throw new ArgumentException("Invalid port.");
        if(!Path.IsPathFullyQualified(options.DataDirectory))throw new ArgumentException("Absolute data directory required.");
        var builder=WebApplication.CreateBuilder(new WebApplicationOptions{Args=[],ContentRootPath=AppContext.BaseDirectory});
        builder.Logging.ClearProviders();
        // Ignore machine/environment endpoint configuration: development service is loopback-only.
        builder.Configuration.Sources.Clear();
        builder.WebHost.ConfigureKestrel(server=>
        {
            server.Listen(IPAddress.Loopback,options.Port);
            server.Limits.MaxRequestBodySize=BackendOptions.MaxRequestBytes+8192;
            server.Limits.MaxConcurrentConnections=300;
            server.Limits.MaxConcurrentUpgradedConnections=256;
            server.Limits.RequestHeadersTimeout=TimeSpan.FromSeconds(10);
            server.Limits.KeepAliveTimeout=TimeSpan.FromSeconds(30);
        });
        builder.Services.AddSingleton(_=>new BackendStore(options));
        builder.Services.AddSingleton<RoomHub>();
        builder.Services.AddSingleton<RequestCounters>();
        builder.Services.AddHostedService<BackupWorker>();
        builder.Services.AddRateLimiter(limits=>
        {
            limits.RejectionStatusCode=429;
            limits.AddFixedWindowLimiter("auth",limiter=>{limiter.PermitLimit=40;limiter.Window=TimeSpan.FromMinutes(1);limiter.QueueLimit=0;});
            limits.AddFixedWindowLimiter("api",limiter=>{limiter.PermitLimit=1200;limiter.Window=TimeSpan.FromMinutes(1);limiter.QueueLimit=0;});
        });
        var app=builder.Build();
        BackendStore store;
        try{store=app.Services.GetRequiredService<BackendStore>();}
        catch{app.DisposeAsync().AsTask().GetAwaiter().GetResult();throw;}
        var hub=app.Services.GetRequiredService<RoomHub>();
        var counters=app.Services.GetRequiredService<RequestCounters>();
        app.Lifetime.ApplicationStarted.Register(()=>
        {
            string address=app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
            store.WriteRuntime(new Uri(address).Port);
            Console.WriteLine(JsonSerializer.Serialize(new{status="listening",service="metallic-rc-backend",address,instanceId=store.InstanceId,nativeClientIntegrated=false}));
        });
        app.Use(async(context,next)=>
        {
            context.Response.Headers.CacheControl="no-store";
            context.Response.Headers["X-Content-Type-Options"]="nosniff";
            context.Response.Headers["Referrer-Policy"]="no-referrer";
            context.Response.Headers["Content-Security-Policy"]="default-src 'none'; frame-ancestors 'none'";
            try
            {
                ValidateLocal(context);
                if(context.Request.ContentLength>BackendOptions.MaxRequestBytes)throw new BackendFault(413,"request_too_large");
                await next(context);
            }
            catch(BackendFault fault){await Error(context,fault.Status,fault.Code);}
            catch(JsonException){await Error(context,400,"invalid_json");}
            catch(BadHttpRequestException fault){await Error(context,fault.StatusCode,"request_rejected");}
            catch(OperationCanceledException)when(context.RequestAborted.IsCancellationRequested){}
            catch(OperationCanceledException){await Error(context,408,"request_timeout");}
            catch(Exception ex)when(ex is SqliteException or IOException or UnauthorizedAccessException){await Error(context,503,"storage_unavailable");}
            catch(Exception){await Error(context,500,"internal_error");}
            finally{counters.Record(context.Request.Method,context.Request.Path.Value??"",context.Response.StatusCode);}
        });
        app.UseRouting();app.UseRateLimiter();
        app.UseWebSockets(new WebSocketOptions{KeepAliveInterval=TimeSpan.FromSeconds(20)});
        object Capabilities()=>new
        {
            product="Metallic-RC",service="metallic-rc-backend",protocol="metallic-v1",deployment="loopback-development",instanceId=store.InstanceId,
            compatibility=new{steamBuildId=DiscoveryContract.SteamBuildId,discoveryContract=true,configuredServiceAddresses=31,nativeLogin=false,nativeLoginErrorProfile=true,nativeRoomJoin=false,nativeGameplay=false,nativeVoice=false,actualGameClientVerified=false},
            implemented=new{ownedAccounts=true,hashedPasswords=true,revocableSessions=true,persistentProfiles=true,persistentRoomDirectory=true,roomPresenceAndPoseWebSocket=true,consistentBackups=true},
            limits=new{requestBytes=BackendOptions.MaxRequestBytes,roomCapacity=32,webSocketFrameBytes=4096,perPeerMessagesPerSecond=60,sessionHours=1},
            note="Metallic-v1 APIs are owned backend APIs. Their success is not RecNet protocol or original-game compatibility."
        };
        IResult Discovery(HttpContext context)
        {
            string build=context.Request.Query["buildId"].ToString();
            if(build.Length>0&&build!=DiscoveryContract.SteamBuildId)throw new BackendFault(409,"unsupported_build");
            var response=new DiscoveryResponse();response.RecNetStatus=null;
            string root="http://127.0.0.1:"+context.Connection.LocalPort.ToString(System.Globalization.CultureInfo.InvariantCulture);
            for(int i=1;i<32;i++)response.Values[i]=root+"/recnet/"+DiscoveryContract.CanonicalNames[i].ToLowerInvariant()+"/";
            context.Response.Headers["X-Metallic-Compatibility"]="discovery-only; build=23191908";
            return Results.Bytes(DiscoveryContract.Write(response),"application/json; charset=utf-8");
        }
        app.MapGet("/",Discovery).RequireRateLimiting("api");
        app.MapGet("/nameserver",Discovery).RequireRateLimiting("api");
        app.MapGet("/compat/23191908/nameserver",Discovery).RequireRateLimiting("api");
        app.MapGet("/healthz",()=>Json(new{status="ok",service="metallic-rc-backend",instanceId=store.InstanceId}));
        app.MapGet("/readyz",()=>Json(new{status="owned-backend-ready",nativeClientReady=false}));
        app.MapGet("/metallic/v1/capabilities",()=>Json(Capabilities())).RequireRateLimiting("api");
        app.MapPost("/metallic/v1/auth/register",async(HttpContext context)=>
        {Admin(context,store);return Json(store.Register(await BackendJson.Read<RegisterRequest>(context.Request)),201);}).RequireRateLimiting("auth");
        app.MapPost("/metallic/v1/auth/login",async(HttpContext context)=>
        {return Json(store.Login(await BackendJson.Read<LoginRequest>(context.Request)));}).RequireRateLimiting("auth");
        app.MapPost("/metallic/v1/auth/logout",(HttpContext context)=>
        {var user=User(context,store);store.Logout(user);hub.Revoke(user.TokenHash);return Results.NoContent();}).RequireRateLimiting("api");
        app.MapGet("/metallic/v1/me",(HttpContext context)=>
        {var profile=store.Profile(User(context,store).AccountId);context.Response.Headers.ETag="\"r"+profile.Revision+"\"";return Json(profile);}).RequireRateLimiting("api");
        app.MapPut("/metallic/v1/me",async(HttpContext context)=>
        {var user=User(context,store);long revision=BackendJson.Revision(context.Request);var profile=store.UpdateProfile(user,await BackendJson.Read<ProfileWrite>(context.Request),revision);context.Response.Headers.ETag="\"r"+profile.Revision+"\"";return Json(profile);}).RequireRateLimiting("api");
        app.MapGet("/metallic/v1/rooms",(HttpContext context)=>Json(new{rooms=store.Rooms(User(context,store))})).RequireRateLimiting("api");
        app.MapPost("/metallic/v1/rooms",async(HttpContext context)=>
        {var user=User(context,store);var room=store.CreateRoom(user,await BackendJson.Read<RoomWrite>(context.Request));context.Response.Headers.Location="/metallic/v1/rooms/"+room.Id;return Json(room,201);}).RequireRateLimiting("api");
        app.MapGet("/metallic/v1/rooms/{id}",(string id,HttpContext context)=>Json(store.Room(User(context,store),id))).RequireRateLimiting("api");
        app.MapGet("/metallic/v1/rooms/{id}/socket",async(string id,HttpContext context)=>
        {var user=User(context,store);var room=store.Room(user,id);await hub.Connect(context,user,room);}).RequireRateLimiting("api");
        app.MapGet("/metallic/v1/admin/diagnostics",(HttpContext context)=>
        {Admin(context,store);return Json(new{instanceId=store.InstanceId,connections=hub.Connections,requests=counters.Snapshot(),nativeClientVerified=false});}).RequireRateLimiting("api");
        app.MapPost("/metallic/v1/admin/backups",(HttpContext context)=>
        {Admin(context,store);return Json(store.Backup(),201);}).RequireRateLimiting("auth");
        app.MapPost("/metallic/v1/admin/shutdown",(HttpContext context)=>
        {
            Admin(context,store);
            context.Response.OnCompleted(()=>{app.Lifetime.StopApplication();return Task.CompletedTask;});
            return Json(new{status="stopping"});
        }).RequireRateLimiting("auth");
        // Verified native LoginResponse error envelope only. Do not process or store
        // production credentials, mint guessed native tokens, or fabricate account success.
        app.MapPost("/recnet/auth/connect/token", async (HttpContext context) =>
        {
            if(options.EnableNativeGateway)return await NativeGateway.Token(context,store);
            context.Response.Headers["X-Metallic-Compatibility"] = "login-error-profile-only; build=23191908";
            var reply = new LoginResponseProfile(Error: "native_login_not_implemented",
                ErrorDescription: "Metallic native token and account integration is not available yet.");
            return Results.Content(System.Text.Encoding.UTF8.GetString(LoginResponseContract.Write(reply)), "application/json; charset=utf-8", statusCode: 501);
        }).RequireRateLimiting("auth");
        if(options.EnableNativeGateway)
            app.MapGet(NativeGateway.AccountPath,(HttpContext context)=>NativeGateway.Account(context,store)).RequireRateLimiting("api");
        app.Map("/recnet/{**path}",()=>Json(new{error=new{code="native_contract_not_implemented"},nativeClientVerified=false},501));
        app.MapFallback(()=>Json(new{error=new{code="not_found"}},404));
        return app;
    }
    private static Principal User(HttpContext context,BackendStore store)=>store.Authenticate(context.Request.Headers.Authorization.ToString());
    private static void Admin(HttpContext context,BackendStore store)
    {if(!store.IsAdmin(context.Request.Headers["X-Metallic-Admin"].ToString()))throw new BackendFault(401,"local_admin_key_required");}
    private static IResult Json(object value,int status=200)=>Results.Json(value,BackendJson.Options,statusCode:status);
    private static async Task Error(HttpContext context,int status,string code)
    {
        if(context.Response.HasStarted){context.Abort();return;}
        context.Response.StatusCode=status;
        await context.Response.WriteAsJsonAsync(new{error=new{code}},BackendJson.Options,context.RequestAborted);
    }
    private static void ValidateLocal(HttpContext context)
    {
        var host=context.Request.Host;
        if(context.Connection.RemoteIpAddress is null||!IPAddress.IsLoopback(context.Connection.RemoteIpAddress)||host.Port!=context.Connection.LocalPort||host.Host is not("127.0.0.1" or "localhost"))throw new BackendFault(421,"loopback_host_required");
        if(context.Request.Headers.TryGetValue("Origin",out var origin)&&(origin.Count!=1||origin[0]!="http://"+host.Value))throw new BackendFault(403,"origin_rejected");
        if(context.Request.Headers.TryGetValue("Sec-Fetch-Site",out var site)&&site.ToString() is not("same-origin" or "none"))throw new BackendFault(403,"cross_site_rejected");
        if(context.Request.Method is not("GET" or "HEAD" or "POST" or "PUT" or "DELETE"))throw new BackendFault(405,"method_not_allowed");
    }
}
public sealed class RequestCounters
{
    private readonly object gate=new();private readonly Dictionary<string,long> routes=new();private long total;
    public void Record(string method,string path,int status)
    {
        string key=method+":"+BackendFiles.Hash(path)+":"+status.ToString(System.Globalization.CultureInfo.InvariantCulture);
        lock(gate){total++;if(routes.TryGetValue(key,out long count))routes[key]=count+1;else if(routes.Count<256)routes[key]=1;}
    }
    public object Snapshot(){lock(gate)return new{total,routeHashes=new Dictionary<string,long>(routes),credentialsAndBodiesRecorded=false};}
}
internal sealed class BackupWorker(BackendStore store):BackgroundService
{
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        using var timer=new PeriodicTimer(TimeSpan.FromHours(3));
        while(await timer.WaitForNextTickAsync(stoppingToken))
        {
            try{store.Backup();Console.WriteLine("{\"event\":\"scheduled_backup_verified\"}");}
            catch(Exception ex)when(ex is IOException or SqliteException or UnauthorizedAccessException){Console.WriteLine("{\"event\":\"scheduled_backup_failed\"}");}
        }
    }
}
