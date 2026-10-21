using System.Text.Json;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Launcher;

internal static class LoginCheck
{
    public static async Task<int> RunAsync(string path,string buildId,string statusText)
    {
        if(buildId!=DiscoveryContract.SteamBuildId)return Report(false,"unsupported_build",null);
        if(!int.TryParse(statusText,System.Globalization.NumberStyles.None,System.Globalization.CultureInfo.InvariantCulture,out int status)||status is <100 or >599)
            return Report(false,"invalid_http_status",null);
        try
        {
            await using var source=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.Read,8192,FileOptions.Asynchronous|FileOptions.SequentialScan);
            if(source.Length>LoginResponseContract.MaxJsonBytes)return Report(false,"response_too_large",null);
            using var output=new MemoryStream();byte[] buffer=new byte[8192];int count;
            using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(5));
            while((count=await source.ReadAsync(buffer,timeout.Token))!=0)
            {
                if(output.Length+count>LoginResponseContract.MaxJsonBytes)return Report(false,"response_too_large",null);
                output.Write(buffer,0,count);
            }
            var response=LoginResponseContract.Read(output.ToArray());
            return Report(true,"profile_parsed",LoginResponseContract.Classify(response,status));
        }
        catch(LoginProfileException error){return Report(false,error.Code,null);}
        catch(JsonException){return Report(false,"invalid_json",null);}
        catch(Exception error)when(error is IOException or UnauthorizedAccessException or OperationCanceledException)
        {return Report(false,"response_file_unavailable",null);}
    }
    private static int Report(bool parsed,string reason,LoginResponseDisposition? disposition)
    {
        Program.Write(new{profileParsed=parsed,reason,disposition=disposition?.ToString(),
            nativeLoginVerified=false,credentialsPrinted=false,networkAttempted=false,cacheWritten=false,gameLaunched=false,
            scope="Build 23191908 response profile; populated code_hint objects are not yet admitted. Parsing is not authentication."});
        return parsed?0:2;
    }
}
