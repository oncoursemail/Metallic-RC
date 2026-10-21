using System.Text.Json;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Launcher;

/// <summary>Offline validation only. Never contacts an endpoint or starts a client.</summary>
internal static class DiscoveryCheck
{
    public static async Task<int> RunAsync(string file,string buildId)
    {
        const int maximumBytes=1_048_576;
        if(buildId!=DiscoveryContract.SteamBuildId)return Report(false,"unsupported_build",buildId,0);
        try
        {
            await using var input=new FileStream(Path.GetFullPath(file),FileMode.Open,FileAccess.Read,FileShare.Read,4096,true);
            if(input.Length>maximumBytes)return Report(false,"size_limit",buildId,0);
            using var buffer=new MemoryStream();byte[] chunk=new byte[4096];int count;
            while((count=await input.ReadAsync(chunk))>0)
            {
                if(buffer.Length+count>maximumBytes)return Report(false,"size_limit",buildId,0);
                buffer.Write(chunk,0,count);
            }
            var response=DiscoveryContract.Read(buffer.ToArray());
            var routes=new Dictionary<int,Uri>();
            DiscoveryContract.ApplyResponse(response,routes,false,_=>{},()=>{});
            return Report(true,"supported_contract_profile",buildId,routes.Count);
        }
        catch(DiscoveryResponseMissingException){return Report(false,"missing_response",buildId,0);}
        catch(DiscoveryStatusException){return Report(false,"nonempty_status",buildId,0);}
        catch(JsonException){return Report(false,"invalid_json_or_value_type",buildId,0);}
        catch(UriFormatException){return Report(false,"invalid_service_uri",buildId,0);}
        catch(Exception ex) when(ex.GetType()==typeof(Exception))
        {return Report(false,"invalid_service_address",buildId,0);}
    }
    private static int Report(bool valid,string reason,string buildId,int serviceCount)
    {
        // Do not print user-supplied addresses, status messages, or credentials.
        Program.Write(new{valid,reason,buildId,serviceCount,networkAttempted=false,gameLaunched=false,
            cacheWritten=false,liveClientCompatibilityVerified=false,
            scope="Offline build-specific response validation; not login or server acceptance."});
        return valid?0:2;
    }
}
