namespace Metallic.RC.Backend;

public sealed record BackendOptions(int Port, string DataDirectory)
{
    // Embedding/test opt-in only; the CLI and native routes do not enable it.
    public bool EnableNativeIdentityMapping { get; init; }
    // Embedding-only opt-in, after the separate v2 identity migration.
    public bool EnableNativeSessions { get; init; }
    // Explicit owner-entered profile storage; no native route activation.
    public bool EnableNativeAccountProfiles { get; init; }
    public const int DefaultPort = 4187;
    public const int MaxRequestBytes = 65536;
    public static BackendOptions Parse(string[] args)
    {
        int port = DefaultPort;
        string data = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Metallic-RC", "backend", "data");
        var seen = new HashSet<string>(StringComparer.Ordinal);
        for (int i = 0; i < args.Length; i++)
        {
            string key = args[i];
            if (!seen.Add(key) || key is not ("--port" or "--data-dir") || ++i >= args.Length)
                throw new ArgumentException("Use --port <1024-65535> and --data-dir <absolute directory> once each.");
            if (key == "--port")
            {
                if (!int.TryParse(args[i], out port) || port is < 1024 or > 65535) throw new ArgumentException("Invalid port.");
            }
            else data = args[i];
        }
        if (!Path.IsPathFullyQualified(data)) throw new ArgumentException("An absolute data directory is required.");
        return new(port, Path.GetFullPath(data));
    }
}
public sealed class BackendFault(int status, string code) : Exception(code)
{
    public int Status { get; } = status;
    public string Code { get; } = code;
}
public sealed record RegisterRequest(string Username, string Password, string DisplayName);
public sealed record LoginRequest(string Username, string Password);
public sealed record ProfileWrite(string DisplayName);
public sealed record RoomWrite(string Name, int Capacity, bool IsPublic);
public sealed record AccountView(string Id, string Username, string DisplayName, long Revision);
public sealed record RoomView(string Id, string OwnerId, string Name, int Capacity, bool IsPublic, long Revision);
public sealed record SessionView(AccountView Account, string AccessToken, DateTimeOffset ExpiresAt);
public sealed record Principal(string AccountId, string TokenHash, DateTimeOffset ExpiresAt);
