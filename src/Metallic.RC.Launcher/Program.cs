using System.Text.Json;
using System.Text.Json.Serialization;
using System.Runtime.InteropServices;
using Avalonia;
using Metallic.RC.Core;

namespace Metallic.RC.Launcher;

public static class Program
{
    public static string StateDirectory { get; private set; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Metallic-RC", "launcher");
    public static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.CamelCase) }
    };

    [STAThread]
    public static int Main(string[] args)
    {
        AttachParentConsoleForCli(args);
        try
        {
            var list = args.ToList();
            var state = TakeOption(list, "--state-dir");
            if (state is not null) StateDirectory = Path.GetFullPath(state);
            if (list.Count == 0 || list[0] == "ui")
            {
                var seconds = TakeOption(list, "--smoke-seconds");
                if (seconds is not null && int.TryParse(seconds, out var s) && s is >= 1 and <= 120)
                    MainWindow.SmokeSeconds = s;
                return BuildAvaloniaApp().StartWithClassicDesktopLifetime([]);
            }
            return RunCliAsync(list).GetAwaiter().GetResult();
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine(JsonSerializer.Serialize(new
            {
                error = ex is CatalogException catalog ? catalog.Code : "operation_failed",
                message = ex.Message
            }, Json));
            return 1;
        }
    }

    public static AppBuilder BuildAvaloniaApp() => AppBuilder.Configure<App>()
        .UsePlatformDetect().WithInterFont().LogToTrace();

    private static void AttachParentConsoleForCli(string[] args)
    {
        if (!OperatingSystem.IsWindows()) return;
        string? command = null;
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] == "--state-dir")
            {
                if (i + 1 < args.Length && !args[i + 1].StartsWith("--", StringComparison.Ordinal)) i++;
                continue;
            }
            command = args[i];
            break;
        }
        if (command is null or "ui") return;

        // AttachConsole can replace standard handles. Preserve every inherited pipe,
        // file or console handle; attach only when this CLI process has none.
        if (HasValidStandardHandle(-10) || HasValidStandardHandle(-11) || HasValidStandardHandle(-12)) return;
        _ = AttachConsole(uint.MaxValue); // ATTACH_PARENT_PROCESS; never allocate a new console.
    }

    private static bool HasValidStandardHandle(int kind)
    {
        var handle = GetStdHandle(kind);
        if (handle == 0 || handle == new nint(-1)) return false;
        return GetFileType(handle) != 0 || Marshal.GetLastPInvokeError() == 0;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern nint GetStdHandle(int kind);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint GetFileType(nint handle);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool AttachConsole(uint processId);

    private static async Task<int> RunCliAsync(List<string> args)
    {
        var command = args[0]; args.RemoveAt(0);
        if (command == "probe-backend")
        {
            var buildId = TakeOption(args, "--build-id") ?? Metallic.RC.Compatibility.DiscoveryContract.SteamBuildId;
            return await BackendProbe.RunAsync(One(args), buildId);
        }
        if (command == "check-discovery")
        {
            var buildId = TakeOption(args, "--build-id") ?? Metallic.RC.Compatibility.DiscoveryContract.SteamBuildId;
            return await DiscoveryCheck.RunAsync(One(args), buildId);
        }
        var catalog = new LauncherCatalog(StateDirectory);
        switch (command)
        {
            case "help":
            case "--help":
                Console.WriteLine("""
                    Metallic-RC — installed-client launcher

                    ui [--state-dir DIRECTORY]
                    discover
                    inspect DIRECTORY
                    check-discovery JSON_FILE [--build-id 23191908]
                    probe-backend http://127.0.0.1:4187/ [--build-id 23191908]
                    import DIRECTORY --name NAME [--launcher EXE] [--replace]
                    list
                    verify ID
                    plan ID [--mode screen|vr] [--target client|external] [--approved-launcher EXE]
                    launch ID [--mode screen|vr] [--target client|external] [--approved-launcher EXE]

                    --state-dir DIRECTORY can be used with every command.
                    Inspection and registration leave the client files untouched.
                    Windows clients need Windows; Linux compatibility is a separate test target.
                    External clients retain their own supported launchers and service requirements.
                    """);
                return 0;
            case "discover":
                RequireNone(args); Write(Discovery.Find()); return 0;
            case "inspect":
            {
                var inventory = await new ClientInspector().InspectAsync(One(args));
                Write(inventory); return inventory.CanImport ? 0 : 2;
            }
            case "import":
            {
                var name = TakeOption(args, "--name") ?? throw new ArgumentException("--name is required.");
                var launcher = TakeOption(args, "--launcher");
                var replace = TakeFlag(args, "--replace");
                Write(await catalog.ImportAsync(new ImportRequest(One(args), name, launcher, replace)));
                return 0;
            }
            case "list":
                RequireNone(args); Write(await catalog.ListAsync()); return 0;
            case "verify":
            {
                var result = await catalog.VerifyAsync(One(args)); Write(result);
                return result.IsValid ? 0 : 2;
            }
            case "plan":
            case "launch":
            {
                var modeText = TakeOption(args, "--mode") ?? "screen";
                var mode = modeText switch { "screen" => LaunchMode.Screen, "vr" => LaunchMode.Vr,
                    _ => throw new ArgumentException("Mode must be screen or vr.") };
                var targetText = TakeOption(args, "--target") ?? "client";
                var target = targetText switch { "client" => LaunchTarget.Client, "external" => LaunchTarget.ExternalLauncher,
                    _ => throw new ArgumentException("Target must be client or external.") };
                var approved = TakeOption(args, "--approved-launcher");
                var plan = await catalog.CreateLaunchPlanAsync(One(args), mode, target, approved);
                if (command == "plan") { Write(plan); return plan.CanExecuteOnCurrentPlatform ? 0 : 3; }
                Write(await catalog.LaunchAsync(plan)); return 0;
            }
            default: throw new ArgumentException("Unknown command. Run 'help' for available commands.");
        }
    }

    public static void Write<T>(T value) => Console.WriteLine(JsonSerializer.Serialize(value, Json));
    private static string One(List<string> args) => args.Count == 1 && !args[0].StartsWith("--", StringComparison.Ordinal)
        ? args[0] : throw new ArgumentException("Exactly one path or installation ID is required.");
    private static void RequireNone(List<string> args)
    { if (args.Count != 0) throw new ArgumentException("Unexpected arguments."); }
    private static bool TakeFlag(List<string> args, string name)
    {
        var i = args.IndexOf(name); if (i < 0) return false;
        args.RemoveAt(i); return true;
    }
    private static string? TakeOption(List<string> args, string name)
    {
        var i = args.IndexOf(name); if (i < 0) return null;
        if (i + 1 >= args.Count || args[i + 1].StartsWith("--", StringComparison.Ordinal))
            throw new ArgumentException($"{name} needs a value.");
        var value = args[i + 1]; args.RemoveRange(i, 2); return value;
    }
}
