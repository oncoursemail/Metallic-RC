using System.Security.Cryptography;
using System.Text.Json;
using Metallic.RC.Backend;

internal static class CredentialFormattingChecks
{
    internal static void Run(Action<bool,string> check)
    {
        string secret = Convert.ToHexString(RandomNumberGenerator.GetBytes(32));
        var register = new RegisterRequest("synthetic_user",secret,"Synthetic");
        var login = new LoginRequest("synthetic_user",secret);
        var account = new AccountView(Guid.NewGuid().ToString("N"),"synthetic_user","Synthetic",1);
        var session = new SessionView(account,secret,DateTimeOffset.UtcNow.AddMinutes(1));
        object[] records = [register,login,session,register with { },login with { },session with { }];
        foreach (object record in records)
        {
            string formatted = string.Join(" | ",record.ToString(),$"request={record}",string.Format("{0}",record));
            check(!formatted.Contains(secret,StringComparison.Ordinal) && formatted.Contains("redacted",StringComparison.Ordinal),
                record.GetType().Name+" and its record copy redact incidental formatting");
        }
        using var registration = JsonDocument.Parse(JsonSerializer.Serialize(register,BackendJson.Options));
        using var request = JsonDocument.Parse(JsonSerializer.Serialize(login,BackendJson.Options));
        using var response = JsonDocument.Parse(JsonSerializer.Serialize(session,BackendJson.Options));
        check(registration.RootElement.GetProperty("password").GetString()==secret &&
            registration.RootElement.GetProperty("username").GetString()==register.Username,
            "registration wire fields remain explicit and unchanged");
        check(request.RootElement.GetProperty("password").GetString()==secret &&
            request.RootElement.GetProperty("username").GetString()==login.Username,
            "login wire fields remain explicit and unchanged");
        check(response.RootElement.GetProperty("accessToken").GetString()==secret &&
            response.RootElement.GetProperty("account").GetProperty("id").GetString()==account.Id,
            "session wire fields remain explicit and unchanged");
    }
}
