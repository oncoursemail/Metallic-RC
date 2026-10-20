using System.Text.Json;

namespace Metallic.RC.Launcher;

public sealed record LocalCandidate(string Name, string InstallRoot, string? LauncherPath, string Evidence);

public static class Discovery
{
    // Read only a small set of known installation locations. No user logs, sessions,
    // browser storage, credentials, network lookups, or recursive disk-wide scan.
    public static IReadOnlyList<LocalCandidate> Find()
    {
        var items = new List<LocalCandidate>();
        if (!OperatingSystem.IsWindows()) return items;
        var profile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        var steam = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
            "Steam", "steamapps", "common", "RecRoom");
        Add("Rec Room · Steam installation", steam, null, "Installed Steam client");
        var campusSettings = Path.Combine(local, "CampusLauncher", "settings.json");
        try
        {
            if (File.Exists(campusSettings) && new FileInfo(campusSettings).Length < 64 * 1024)
            {
                using var doc = JsonDocument.Parse(File.ReadAllText(campusSettings));
                if (doc.RootElement.ValueKind == JsonValueKind.Object && doc.RootElement.TryGetProperty("gameDir", out var field) &&
                    field.ValueKind == JsonValueKind.String && field.GetString() is { Length: > 0 } path)
                {
                    var exe = Path.Combine(desktop, "Campus.exe");
                    Add("Campus · existing installation", path, File.Exists(exe) ? exe : null,
                        "Campus launcher's configured game directory");
                }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException) { }
        Add("NexaNet · existing installation", Path.Combine(profile, "Downloads", "NexaNet", "NexaNet"),
            null, "Existing modified client; its own supported launcher is required");
        Add("Meow.Net · existing installation", @"C:\Meow.Net", null,
            "Existing modified client; its own supported launcher is required");
        return items;

        void Add(string name, string root, string? exe, string evidence)
        {
            if (!Path.IsPathFullyQualified(root) || !Directory.Exists(root)) return;
            root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root));
            if (!(File.Exists(Path.Combine(root, "RecRoom.exe")) && Directory.Exists(Path.Combine(root, "RecRoom_Data"))) &&
                !(File.Exists(Path.Combine(root, "Recroom_Release.exe")) && Directory.Exists(Path.Combine(root, "Recroom_Release_Data")))) return;
            if (items.Any(x => string.Equals(x.InstallRoot, root, StringComparison.OrdinalIgnoreCase))) return;
            items.Add(new(name, root, exe, evidence));
        }
    }
}
