using System.Text.Json;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Launcher;

internal static class BackendProbe
{
    public static async Task<int> RunAsync(string address, string buildId)
    {
        if(buildId!=DiscoveryContract.SteamBuildId)return Report(false,"unsupported_build",0);
        if(!Uri.TryCreate(address,UriKind.Absolute,out var origin)||origin.Scheme!="http"||origin.Host!="127.0.0.1"||origin.UserInfo.Length!=0||origin.Query.Length!=0||origin.Fragment.Length!=0||origin.AbsolutePath!="/"||origin.Port is <1024 or >65535)
            return Report(false,"explicit_loopback_origin_required",0);
        using var handler=new SocketsHttpHandler{AllowAutoRedirect=false,UseProxy=false,ConnectTimeout=TimeSpan.FromSeconds(3)};
        using var client=new HttpClient(handler){BaseAddress=origin,Timeout=TimeSpan.FromSeconds(5)};
        try
        {
            using var health=JsonDocument.Parse(await Get(client,"healthz"));
            if(health.RootElement.GetProperty("service").GetString()!="metallic-rc-backend")return Report(false,"unexpected_service",0);
            byte[] bytes=await Get(client,"nameserver?buildId="+buildId);
            var response=DiscoveryContract.Read(bytes);var routes=new Dictionary<int,Uri>();
            DiscoveryContract.ApplyResponse(response,routes,false,_=>{},()=>{});
            if(routes.Count!=31||routes.Values.Any(uri=>uri.Scheme!=origin.Scheme||uri.Host!=origin.Host||uri.Port!=origin.Port||uri.UserInfo.Length!=0||uri.Query.Length!=0||uri.Fragment.Length!=0||!uri.AbsolutePath.StartsWith("/recnet/",StringComparison.Ordinal)))
                return Report(false,"discovery_address_escaped_backend",routes.Count);
            using var capabilities=JsonDocument.Parse(await Get(client,"metallic/v1/capabilities"));
            if(capabilities.RootElement.GetProperty("product").GetString()!="Metallic-RC"||capabilities.RootElement.GetProperty("compatibility").GetProperty("steamBuildId").GetString()!=buildId)
                return Report(false,"backend_contract_mismatch",routes.Count);
            return Report(true,"owned_backend_reachable",routes.Count);
        }
        catch(Exception ex)when(ex is HttpRequestException or OperationCanceledException or IOException or JsonException or KeyNotFoundException or InvalidOperationException or UriFormatException or DiscoveryResponseMissingException or DiscoveryStatusException or DiscoveryChangedException)
        {return Report(false,"backend_probe_failed",0);}
        catch(Exception ex)when(ex.GetType()==typeof(Exception))
        {return Report(false,"invalid_service_address",0);}
    }
    private static async Task<byte[]> Get(HttpClient client,string path)
    {
        using var response=await client.GetAsync(path,HttpCompletionOption.ResponseHeadersRead);
        response.EnsureSuccessStatusCode();
        if(response.Content.Headers.ContentType?.MediaType!="application/json"||response.Content.Headers.ContentLength>1_048_576)throw new JsonException();
        using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(5));
        using var body=await response.Content.ReadAsStreamAsync(timeout.Token);using var memory=new MemoryStream();byte[] buffer=new byte[8192];int count;
        while((count=await body.ReadAsync(buffer,timeout.Token))!=0)
        {if(memory.Length+count>1_048_576)throw new JsonException();memory.Write(buffer,0,count);}
        return memory.ToArray();
    }
    private static int Report(bool reachable,string reason,int addresses)
    {
        Program.Write(new{reachable,reason,serviceAddresses=addresses,gameLaunched=false,gameFilesChanged=false,
            originalClientLoginVerified=false,originalClientMultiplayerVerified=false,cacheWritten=false,
            scope="Live loopback discovery/health handshake with the owned backend, not original-game acceptance."});
        return reachable?0:2;
    }
}
