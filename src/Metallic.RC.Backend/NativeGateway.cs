using System.Security.Cryptography;
using System.Text;
using Microsoft.Data.Sqlite;
using Microsoft.AspNetCore.Http.Features;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Backend;

// Closed, opt-in localhost slice. These handlers never authorize decoded JWT
// claims, contact a vendor service or implement a refresh/challenge grant.
internal static class NativeGateway
{
    internal const string AccountPath="/recnet/accounts/account/me";
    internal const string TokenPath="/recnet/auth/connect/token";
    internal static bool AccountTarget(string? target)
    {
        if(target is null||target.Length>NativeRequestSigningProfile.MaxPathChars||
            !(target==AccountPath||target.StartsWith(AccountPath+"?",StringComparison.Ordinal)))return false;
        for(int i=0;i<target.Length;i++)
        {
            char c=target[i];if(c<0x21||c>0x7e||c is '#' or '\\')return false;
            if(c=='%')
            {
                if(i+2>=target.Length||!char.IsAsciiHexDigit(target[i+1])||!char.IsAsciiHexDigit(target[i+2]))return false;
                i+=2;
            }
        }
        return true;
    }
    private static IResult Failure(int status,string code)=>Results.Content(Encoding.UTF8.GetString(LoginResponseContract.Write(new LoginResponseProfile(Error:code))),
        "application/json; charset=utf-8",statusCode:status);
    private static IResult Fault(BackendFault e)=>Failure(e.Status,e.Code);
    private static void Scope(HttpContext c)=>c.Response.Headers["X-Metallic-Compatibility"]="owned-native-development-slice; build=23191908; client-unverified";

    internal static async Task<IResult> Token(HttpContext c,BackendStore store)
    {
        Scope(c);byte[] buffer=new byte[8192];byte[]? bytes=null;using var stream=new MemoryStream();
        try
        {
            if(c.Request.Method!="POST"||c.Features.Get<IHttpRequestFeature>()?.RawTarget!=TokenPath)return Failure(501,"native_request_outside_profile");
            if(c.Request.Headers.ContentType.Count!=1||c.Request.ContentType!=NativePasswordFormProfile.ContentType)
                return Failure(501,"native_manual_password_outside_profile");
            using var deadline=new CancellationTokenSource(TimeSpan.FromSeconds(10));
            using var cancel=CancellationTokenSource.CreateLinkedTokenSource(deadline.Token,c.RequestAborted);
            int count;
            while((count=await c.Request.Body.ReadAsync(buffer,cancel.Token))!=0)
            {
                if(stream.Length+count>NativePasswordFormProfile.MaxBodyBytes)return Failure(413,"native_form_too_large");
                stream.Write(buffer,0,count);
            }
            cancel.Token.ThrowIfCancellationRequested();
            bytes=stream.ToArray();var form=NativePasswordFormProfile.Read(bytes,c.Request.ContentType);
            if(form.Status!="within_profile")
            {
                bool malformed=form.ReasonCode is "missing_body" or "required_field_missing" or "empty_credentials" or "malformed_field" or "malformed_percent_encoding" or "invalid_utf8" or "non_ascii_body";
                return Failure(malformed?400:501,malformed?"native_form_invalid":"native_manual_password_outside_profile");
            }
            // The retained normal branch emits absent or literal true only.
            if(form.TryGetManualField("require_password",out var required)&&required!="true")return Failure(501,"native_manual_password_outside_profile");
            foreach(string field in new[]{"security_code","2fa_code","login_context"})
                if(form.TryGetManualField(field,out _))return Failure(501,"native_manual_password_outside_profile");
            if(!form.TryGetManualField("username",out var username)||!form.TryGetManualField("password",out var password)||username is null||password is null)
                return Failure(400,"native_form_invalid");
            cancel.Token.ThrowIfCancellationRequested();
            return Results.Bytes(store.LoginNativeGateway(new(username,password),cancel.Token),"application/json; charset=utf-8");
        }
        catch(BackendFault e){return Fault(e);}
        catch(OperationCanceledException)when(c.RequestAborted.IsCancellationRequested){throw;}
        catch(OperationCanceledException){return Failure(408,"native_form_timeout");}
        catch(BadHttpRequestException e){return Failure(e.StatusCode,e.StatusCode==408?"native_form_timeout":"native_form_invalid");}
        catch(SqliteException){return Failure(503,"native_storage_unavailable");}
        catch(UnauthorizedAccessException){return Failure(503,"native_storage_unavailable");}
        catch(IOException){return Failure(503,"native_storage_unavailable");}
        finally
        {
            CryptographicOperations.ZeroMemory(buffer);
            if(bytes is not null)CryptographicOperations.ZeroMemory(bytes);
            if(stream.TryGetBuffer(out var backing))CryptographicOperations.ZeroMemory(backing.AsSpan());
        }
    }
    internal static IResult Account(HttpContext c,BackendStore store)
    {
        Scope(c);
        try
        {
            if(c.Request.Method!="GET")return Failure(501,"native_request_outside_profile");
            var detection=c.Features.Get<IHttpRequestBodyDetectionFeature>();
            string? raw=c.Features.Get<IHttpRequestFeature>()?.RawTarget;
            if(!AccountTarget(raw)||detection is null||detection.CanHaveBody||c.Request.Headers.TransferEncoding.Count!=0||c.Request.ContentLength is >0)
                return Failure(501,"native_request_outside_profile");
            if(c.Request.Headers.Authorization.Count!=1||c.Request.Headers["X-RNSIG"].Count!=1)
                return Failure(401,"native_signature_rejected");
            string authorization=c.Request.Headers.Authorization.ToString();
            if(!authorization.StartsWith("Bearer ",StringComparison.Ordinal))return Failure(401,"native_session_rejected");
            return Results.Bytes(store.ReadNativeGatewayAccount(authorization[7..],c.Request.Headers["X-RNSIG"].ToString(),raw!),"application/json; charset=utf-8");
        }
        catch(BackendFault e){return Fault(e);}
        catch(SqliteException){return Failure(503,"native_storage_unavailable");}
        catch(UnauthorizedAccessException){return Failure(503,"native_storage_unavailable");}
        catch(IOException){return Failure(503,"native_storage_unavailable");}
    }
}
