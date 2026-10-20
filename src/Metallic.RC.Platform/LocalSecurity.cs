using System.Buffers;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Text.Json;

namespace Metallic.RC.Platform;

public sealed class LocalSecurity
{
    private readonly byte[] tokenBytes = RandomNumberGenerator.GetBytes(32);
    private readonly string token;
    private readonly string sessionFile;
    public LocalSecurity(string dataDirectory)
    {
        token = Convert.ToHexStringLower(tokenBytes);
        sessionFile = Path.Combine(dataDirectory, "local-session.json");
    }

    internal void WriteSession(int port)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(new
        {
            token, port, processId = Environment.ProcessId, createdAt = DateTimeOffset.UtcNow,
            scope = "local-authoring"
        }, PlatformJson.Options);
        using var stream = PrivateFiles.Create(sessionFile);
        stream.Write(bytes);
        stream.Flush(true);
    }

    public string SessionToken(HttpContext context)
    {
        if (context.Request.Headers["X-Metallic-Client"] != "desktop")
            throw new PlatformFault(403, "local_client_required", "Request the local session from the Metallic launcher.");
        return token;
    }

    public bool IsAuthorized(HttpRequest request)
    {
        var candidate = request.Headers["X-Metallic-Token"].ToString();
        if (candidate.Length != 64) return false;
        Span<byte> decoded = stackalloc byte[32];
        return Convert.FromHexString(candidate, decoded, out var consumed, out var written) == OperationStatus.Done
            && consumed == candidate.Length && written == tokenBytes.Length
            && CryptographicOperations.FixedTimeEquals(decoded, tokenBytes);
    }

    public static void ValidateRequest(HttpContext context)
    {
        var host = context.Request.Host;
        if (host.Port != context.Connection.LocalPort
            || !(host.Host.Equals("127.0.0.1", StringComparison.OrdinalIgnoreCase)
                 || host.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase)))
            throw new PlatformFault(421, "local_host_required", "Use the local Metallic address.");
        if (context.Request.Headers.TryGetValue("Origin", out var origin))
        {
            var expected = "http://" + host.Value;
            if (origin.Count != 1 || !origin[0]!.Equals(expected, StringComparison.OrdinalIgnoreCase))
                throw new PlatformFault(403, "origin_rejected", "This request did not originate from the local Metallic app.");
        }
        if (context.Request.Headers.TryGetValue("Sec-Fetch-Site", out var site)
            && site.ToString() is not ("same-origin" or "none"))
            throw new PlatformFault(403, "cross_site_rejected", "Cross-site access is not supported.");
        if (context.Request.ContentLength > PlatformOptions.MaxRequestBytes)
            throw new PlatformFault(413, "request_size", "This request is larger than the local API limit.");
    }
}

internal static class PrivateFiles
{
    public static void CreateDirectory(string path)
    {
        if (OperatingSystem.IsWindows())
        {
            Directory.CreateDirectory(path);
            var sid = WindowsIdentity.GetCurrent().User ?? throw new IOException("Local identity unavailable.");
            var access = new DirectorySecurity();
            access.SetAccessRuleProtection(true, false);
            access.SetOwner(sid);
            access.AddAccessRule(new FileSystemAccessRule(sid, FileSystemRights.FullControl,
                InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit,
                PropagationFlags.None, AccessControlType.Allow));
            new DirectoryInfo(path).SetAccessControl(access);
        }
        else
        {
            Directory.CreateDirectory(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
            File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
        }
    }

    public static FileStream Create(string path)
    {
        if (OperatingSystem.IsWindows())
        {
            var sid = WindowsIdentity.GetCurrent().User ?? throw new IOException("Local identity unavailable.");
            var access = new FileSecurity();
            access.SetAccessRuleProtection(true, false);
            access.SetOwner(sid);
            access.AddAccessRule(new FileSystemAccessRule(sid, FileSystemRights.FullControl, AccessControlType.Allow));
            // ACL-aware creation applies protection before writing and requests
            // WRITE_DAC/WRITE_OWNER so existing-file permissions can be tightened.
            var protectedStream = new FileInfo(path).Create(FileMode.Create, FileSystemRights.FullControl,
                FileShare.None, 4096, FileOptions.WriteThrough, access);
            try { protectedStream.SetAccessControl(access); return protectedStream; }
            catch { protectedStream.Dispose(); throw; }
        }
        var options = new FileStreamOptions
        {
            Mode = FileMode.Create, Access = FileAccess.Write, Share = FileShare.None,
            Options = FileOptions.WriteThrough,
            UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite
        };
        var stream = new FileStream(path, options);
        try
        {
            File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite);
            return stream;
        }
        catch { stream.Dispose(); throw; }
    }
}
