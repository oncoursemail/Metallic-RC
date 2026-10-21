// Authored, bounded reconstruction of build 23191908's generated LoginResponse formatter.
// This is a wire-format component, not a token issuer, original JSON runtime, or login-success proof.
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Metallic.RC.Compatibility;

public sealed record LoginResponseProfile(
    string? Error = null, string? ErrorDescription = null, string? AccessToken = null,
    string? RefreshToken = null, string? Key = null, string? LoginContext = null,
    string? Challenge = null)
{
    // Prevent incidental logging of credentials via the generated record ToString.
    public override string ToString() => "LoginResponseProfile { credential values redacted }";
}

public enum LoginResponseDisposition
{
    MissingResponse,
    ContinueSecurityCode,
    RetrySecurityCode,
    RemoteAuthorizationPending,
    AdditionalChallengeRequired,
    Rejected,
    RequiresTokenAndAccountValidation
}

public static class LoginResponseContract
{
    // Native constructor observations: these spellings/order are intentionally mixed case.
    private static readonly string[] Names = ["Error", "error_description", "access_token", "refresh_token", "Key", "code_hint", "login_context", "hcc"];
    public static IReadOnlyList<string> CanonicalNames { get; } = Array.AsReadOnly(Names);
    private static readonly IReadOnlyDictionary<string,int> Aliases = new Dictionary<string,int>(StringComparer.Ordinal)
    {
        ["Error"]=0,["error"]=0,["Error_description"]=1,["error_description"]=1,
        ["Access_token"]=2,["access_token"]=2,["Refresh_token"]=3,["refresh_token"]=3,
        ["Key"]=4,["key"]=4,["Code_hint"]=5,["code_hint"]=5,
        ["Login_context"]=6,["login_context"]=6,["Hcc"]=7,["hcc"]=7
    };
    public const int MaxJsonBytes = 1_048_576;

    // This initial verified profile allows a null or absent code_hint only.
    // A populated nested hint is rejected, not silently discarded or treated as a successful login.
    public static LoginResponseProfile? Read(ReadOnlySpan<byte> utf8)
    {
        if(utf8.Length>MaxJsonBytes) throw new LoginProfileException("response_too_large");
        var reader=new Utf8JsonReader(utf8,new JsonReaderOptions{MaxDepth=64,CommentHandling=JsonCommentHandling.Disallow,AllowTrailingCommas=false});
        if(!reader.Read())throw new LoginProfileException("missing_json_value");
        if(reader.TokenType==JsonTokenType.Null){if(reader.Read())throw new LoginProfileException("trailing_json");return null;}
        if(reader.TokenType!=JsonTokenType.StartObject)throw new LoginProfileException("object_or_null_required");
        if(!reader.Read())throw new LoginProfileException("unterminated_object");
        if(reader.TokenType==JsonTokenType.EndObject){if(reader.Read())throw new LoginProfileException("trailing_json");return null;}
        string?[] values=new string?[8];
        while(reader.TokenType!=JsonTokenType.EndObject)
        {
            if(reader.TokenType!=JsonTokenType.PropertyName)throw new LoginProfileException("property_required");
            // Only the observed raw ASCII property-name aliases are admitted.
            // No invented blanket case-folding or escaped-name normalization.
            string name=Encoding.UTF8.GetString(reader.ValueSpan);
            bool recognized=Aliases.TryGetValue(name,out int index);
            if(!reader.Read())throw new LoginProfileException("property_value_required");
            if(recognized)
            {
                if(index==5 && reader.TokenType!=JsonTokenType.Null)
                    throw new LoginProfileException("nested_hint_outside_verified_profile");
                if(reader.TokenType is not (JsonTokenType.String or JsonTokenType.Null))
                    throw new LoginProfileException("known_value_type_error");
                values[index]=reader.TokenType==JsonTokenType.Null?null:reader.GetString();
            }
            else reader.Skip();
            if(!reader.Read())throw new LoginProfileException("unterminated_object");
        }
        if(reader.Read())throw new LoginProfileException("trailing_json");
        return new(values[0],values[1],values[2],values[3],values[4],values[6],values[7]);
    }

    public static byte[] Write(LoginResponseProfile? response)
    {
        using var stream=new MemoryStream();
        using(var writer=new Utf8JsonWriter(stream,new JsonWriterOptions{Encoder=JavaScriptEncoder.UnsafeRelaxedJsonEscaping}))
        {
            if(response is null)writer.WriteNullValue();
            else
            {
                var values=Values(response);writer.WriteStartObject();
                for(int i=0;i<8;i++)writer.WriteString(Names[i],values[i]);
                writer.WriteEndObject();
            }
        }
        if(stream.Length>MaxJsonBytes)throw new LoginProfileException("response_too_large");
        return stream.ToArray();
    }

    public static string?[] Values(LoginResponseProfile response) =>
        [response.Error,response.ErrorDescription,response.AccessToken,response.RefreshToken,response.Key,null,response.LoginContext,response.Challenge];

    // Branch classification derived from the initial native response handler.
    // This deliberately stops BEFORE token processing, cryptographic/key handling,
    // cache changes, account fetch, async retries, and a completed login result.
    public static LoginResponseDisposition Classify(LoginResponseProfile? response,int httpStatus,bool remoteLogin=false,bool originalRequestHasChallenge=false)
    {
        if(httpStatus is <100 or >599)throw new ArgumentOutOfRangeException(nameof(httpStatus));
        if(response is null)return LoginResponseDisposition.MissingResponse;
        if(response.ErrorDescription=="security_code required" || response.Error=="continue_2fa")return LoginResponseDisposition.ContinueSecurityCode;
        if(response.ErrorDescription is "invalid security_code" or "invalid 2fa_code")return LoginResponseDisposition.RetrySecurityCode;
        if(response.ErrorDescription=="hcr required" && !originalRequestHasChallenge)return LoginResponseDisposition.AdditionalChallengeRequired;
        if(remoteLogin && httpStatus==400 && response.Error is "authorization_pending" or "slow_down")return LoginResponseDisposition.RemoteAuthorizationPending;
        if(httpStatus>=400 || !string.IsNullOrEmpty(response.Error))return LoginResponseDisposition.Rejected;
        return LoginResponseDisposition.RequiresTokenAndAccountValidation;
    }
}

public sealed class LoginProfileException(string code) : JsonException(code)
{
    public string Code { get; } = code;
}
