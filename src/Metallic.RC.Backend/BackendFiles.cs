using System.Security.AccessControl;
using System.Security.Principal;
using System.Security.Cryptography;
using System.Text;

namespace Metallic.RC.Backend;

internal static class BackendFiles
{
    public static void CheckLinks(string path)
    {
        for (var current = new DirectoryInfo(Path.GetFullPath(path)); current is not null; current = current.Parent)
            if (current.Exists && (current.Attributes & FileAttributes.ReparsePoint) != 0)
                throw new IOException("Backend storage must not traverse links.");
    }
    public static void Prepare(string path)
    {
        CheckLinks(path);
        if (Directory.Exists(path) && Directory.EnumerateFileSystemEntries(path).Any() && !File.Exists(Path.Combine(path, ".metallic-backend")))
            throw new IOException("Refusing to adopt a nonempty unrelated directory.");
        PrivateDirectory(path);
        string marker = Path.Combine(path, ".metallic-backend");
        if (!File.Exists(marker)) WriteAtomic(marker, Encoding.UTF8.GetBytes("metallic-rc-backend-v1\n"));
        foreach (string file in Directory.EnumerateFileSystemEntries(path))
            if ((File.GetAttributes(file) & FileAttributes.ReparsePoint) != 0) throw new IOException("Linked backend state rejected.");
    }
    public static void PrivateDirectory(string path)
    {
        CheckLinks(path);
        if (OperatingSystem.IsWindows())
        {
            Directory.CreateDirectory(path);
            var sid = WindowsIdentity.GetCurrent().User ?? throw new IOException("Missing user identity.");
            var access = new DirectorySecurity();
            access.SetAccessRuleProtection(true, false); access.SetOwner(sid);
            access.AddAccessRule(new FileSystemAccessRule(sid, FileSystemRights.FullControl,
                InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit, PropagationFlags.None, AccessControlType.Allow));
            new DirectoryInfo(path).SetAccessControl(access);
        }
        else
        {
            Directory.CreateDirectory(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
            File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
        }
    }
    public static void WriteAtomic(string path, byte[] bytes)
    {
        if (File.Exists(path) && (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) throw new IOException("Linked state rejected.");
        string temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096, FileOptions.WriteThrough))
            { stream.Write(bytes); stream.Flush(true); }
            if (!OperatingSystem.IsWindows()) File.SetUnixFileMode(temporary, UnixFileMode.UserRead | UnixFileMode.UserWrite);
            File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
    public static string Hash(string value) => Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(value)));
}
