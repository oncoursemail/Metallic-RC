using System.Diagnostics;
using System.Net.Http;
using System.Text.Json;
using System.Security.Cryptography;
using System.Text;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using Metallic.RC.Core;

namespace Metallic.RC.Launcher;

public sealed class MainWindow : Window
{
    public static int SmokeSeconds { get; set; }
    private readonly LauncherCatalog _catalog;
    private readonly string _state;
    private readonly StackPanel _rows = new() { Spacing = 8 };
    private readonly StackPanel _detail = new() { Spacing = 13 };
    private readonly TextBlock _status = T("Ready", 12, "#A9ACB5");
    private readonly TextBlock _selectionTitle = T("Select an installation", 24, "#F5F5F7");
    private readonly TextBlock _selectionMeta = T("", 13, "#A9ACB5");
    private readonly TextBox _output = new()
    {
        IsReadOnly = true, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap,
        MinHeight = 126, MaxHeight = 205, FontSize = 12,
        Background = Brush.Parse("#191A20"), BorderBrush = Brush.Parse("#34353E")
    };
    private readonly ComboBox _mode = new() { ItemsSource = new[] { "Screen", "VR" }, SelectedIndex = 0, Width = 142 };
    private readonly Button _launch;
    private readonly Button _verify;
    private readonly Button _plan;
    private RegisteredInstallation? _selected;
    private bool _busy;
    private int _installationCount;
    private Process? _creator;
    private readonly List<Button> _busyButtons = [];

    public MainWindow(string stateDirectory)
    {
        _state = stateDirectory; _catalog = new(stateDirectory);
        Title = "Metallic-RC"; Width = 1170; Height = 810;
        MinWidth = 940; MinHeight = 690;
        Background = Brush.Parse("#15161B"); FontFamily = "Inter";
        WindowStartupLocation = WindowStartupLocation.CenterScreen;
        _launch = B("Launch client", LaunchSelectedAsync, true);
        _verify = B("Verify files", VerifySelectedAsync);
        _plan = B("Show launch plan", PlanSelectedAsync);
        var root = new Grid { ColumnDefinitions = new("214,*") };
        Content = root;

        var side = new Grid { RowDefinitions = new("Auto,Auto,*,Auto"), Margin = new(26, 32, 22, 24) };
        var brand = new StackPanel { Spacing = 3, Children =
        {
            T("METALLIC", 22, "#F1F2F5", FontWeight.SemiBold),
            T("RC  /  CLIENT & CREATOR", 10, "#B5A0F0", FontWeight.Medium)
        }};
        side.Children.Add(brand);
        var navigation = new StackPanel { Spacing = 9, Margin = new(0, 38, 0, 0) };
        navigation.Children.Add(new Border { Background = Brush.Parse("#292433"), CornerRadius = new(5), Padding = new(14, 11),
            Child = T("Your builds", 14, "#E4D9FF", FontWeight.Medium) });
        navigation.Children.Add(B("Creator tools", OpenCreatorAsync));
        navigation.Children.Add(B("Find installations", ShowDiscoveryAsync));
        navigation.Children.Add(B("Import a folder", () => ImportDialogAsync(null)));
        Grid.SetRow(navigation, 1); side.Children.Add(navigation);
        var sideFoot = new StackPanel { Spacing = 7, Children =
        {
            T("LOCAL WORKSPACE", 10, "#7F828D", FontWeight.Medium),
            T("Your installed clients stay in their own folders.", 12, "#A9ACB5"),
            T("0.2 · Compiled-client rework", 11, "#7F828D")
        }};
        Grid.SetRow(sideFoot, 3); side.Children.Add(sideFoot);
        root.Children.Add(new Border { Background = Brush.Parse("#1B1C22"), BorderBrush = Brush.Parse("#30313A"), BorderThickness = new(0, 0, 1, 0), Child = side });

        var main = new Grid { RowDefinitions = new("Auto,*,Auto"), Margin = new(34, 28, 30, 22) };
        Grid.SetColumn(main, 1); root.Children.Add(main);
        var head = new Grid { ColumnDefinitions = new("*,Auto"), Margin = new(0, 0, 0, 28) };
        head.Children.Add(new StackPanel { Spacing = 7, Children =
        {
            T("Your builds", 30, "#F4F4F7", FontWeight.SemiBold),
            T("One place for the clients you already have.", 14, "#A9ACB5")
        }});
        var add = B("Import installation", () => ImportDialogAsync(null), true);
        add.VerticalAlignment = VerticalAlignment.Center; Grid.SetColumn(add, 1); head.Children.Add(add); main.Children.Add(head);

        var body = new Grid { RowDefinitions = new("Auto,Auto,*,Auto") };
        Grid.SetRow(body, 1); main.Children.Add(body);
        body.Children.Add(T("INSTALLED CLIENTS", 10, "#898D98", FontWeight.SemiBold));
        var list = new ScrollViewer { Content = _rows, MaxHeight = 230, Margin = new(0, 12, 0, 24) };
        Grid.SetRow(list, 1); body.Children.Add(list);
        _detail.Children.Add(_selectionTitle); _detail.Children.Add(_selectionMeta);
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(_mode); row.Children.Add(_launch); row.Children.Add(_verify); row.Children.Add(_plan);
        _detail.Children.Add(row);
        _detail.Children.Add(T("Metallic room connectivity is still being integrated. Starting an existing client uses that client's current services.", 12, "#A9ACB5"));
        _detail.Children.Add(_output);
        var detailScroll = new ScrollViewer { Content = _detail };
        Grid.SetRow(detailScroll, 2); body.Children.Add(detailScroll);
        var creatorNote = new Border { BorderBrush = Brush.Parse("#33343E"), BorderThickness = new(0, 1, 0, 0),
            Padding = new(0, 19, 0, 0), Margin = new(0, 20, 0, 0), Child = new StackPanel { Spacing = 6, Children =
        {
            T("Create with CV3", 17, "#F4F4F7", FontWeight.Medium),
            T("Save circuit drafts, inspect typed connections, and preview their behavior in the local creator tools.", 12, "#A9ACB5")
        }}};
        Grid.SetRow(creatorNote, 3); body.Children.Add(creatorNote);
        _status.Margin = new(0, 15, 0, 0); Grid.SetRow(_status, 2); main.Children.Add(_status);
        Opened += async (_, _) =>
        {
            await GuardAsync(RefreshAsync);
            if (SmokeSeconds > 0)
            {
                Program.Write(new { ui = "opened", installs = _installationCount, smokeSeconds = SmokeSeconds });
                var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(SmokeSeconds) };
                timer.Tick += (_, _) => { timer.Stop(); Close(); }; timer.Start();
            }
        };
        Closed += (_, _) => StopOwnedCreator();
        SetButtons();
    }

    private async Task RefreshAsync()
    {
        var installations = await _catalog.ListAsync();
        _installationCount = installations.Count;
        _rows.Children.Clear();
        foreach (var install in installations)
        {
            var content = new Grid { ColumnDefinitions = new("*,Auto") };
            content.Children.Add(new StackPanel { Spacing = 5, Children =
            {
                T(install.DisplayName, 15, "#ECECF1", FontWeight.Medium),
                T($"Unity {install.Inventory.UnityVersion ?? "unknown"}  ·  {Bytes(install.Inventory.TotalBytes)}  ·  {install.Inventory.ScriptingBackend}", 11, "#9D9FAA")
            }});
            var tag = T(install.ExternalLauncher is null ? "Client" : "External launcher", 11, "#B5A0F0");
            tag.VerticalAlignment = VerticalAlignment.Center; Grid.SetColumn(tag, 1); content.Children.Add(tag);
            var button = new Button { Content = content, HorizontalContentAlignment = HorizontalAlignment.Stretch,
                HorizontalAlignment = HorizontalAlignment.Stretch, Padding = new(17, 13),
                Background = Brush.Parse("#202127"), BorderBrush = Brush.Parse("#363741"), BorderThickness = new(1), CornerRadius = new(5) };
            button.Click += (_, _) => Select(install); _rows.Children.Add(button);
        }
        if (installations.Count == 0)
        {
            _rows.Children.Add(new Border { BorderBrush = Brush.Parse("#353640"), BorderThickness = new(1),
                Padding = new(22), CornerRadius = new(5), Child = new StackPanel { Spacing = 9, Children =
                { T("Bring your existing client", 18, "#ECECF1"),
                  T("Choose its folder to inspect the build and register it here.", 13, "#9D9FAA") } } });
            _output.Text = "No installation has been imported. Use Find installations or Import installation.";
        }
        else Select(installations.FirstOrDefault(x => x.Id == _selected?.Id) ?? installations[0]);
        _status.Text = $"{installations.Count} registered installation{(installations.Count == 1 ? "" : "s")} · local client files preserved";
    }

    private void Select(RegisteredInstallation install)
    {
        _selected = install; _selectionTitle.Text = install.DisplayName;
        _selectionMeta.Text = install.InstallRoot;
        _launch.Content = install.ExternalLauncher is null ? "Launch client" : "Open registered launcher";
        _mode.IsEnabled = install.ExternalLauncher is null;
        _output.Text = string.Join(Environment.NewLine, new[]
        {
            $"{install.Inventory.FileCount:N0} files inventoried · {install.Inventory.Fingerprints.Count:N0} critical files fingerprinted",
            $"Source project: {(install.Inventory.IsUnityProject ? "project markers found" : "no complete Unity project markers found")}",
            install.ExternalLauncher is null ? $"Executable: {install.Inventory.ClientExecutableRelativePath}" : $"Launcher: {install.ExternalLauncher.ExecutablePath}",
            install.Inventory.OpaqueMetadataFiles.Count > 0 ? "The build contains opaque metadata. Source reconstruction is not established." : "",
            string.Join(Environment.NewLine, install.Inventory.Issues.Select(x => $"{x.Code}: {x.Message}"))
        }.Where(x => x.Length > 0));
        SetButtons();
    }

    private async Task VerifySelectedAsync()
    {
        if (_selected is null) return;
        _status.Text = "Verifying the imported build…";
        var result = await _catalog.VerifyAsync(_selected.Id);
        _output.Text = result.IsValid ? "Verified: all fingerprinted launch files still match the imported build.\nThis checks local integrity; game-server compatibility is a separate test.\n" +
            string.Join(Environment.NewLine, result.Issues.Select(x => $"{x.Severity} · {x.Code}: {x.Message}"))
            : string.Join(Environment.NewLine, result.Issues.Select(x => $"{x.Code}: {x.Message}"));
        _status.Text = result.IsValid ? "Verification passed" : "Build changed or files are missing; launch remains blocked until resolved";
    }

    private async Task<LaunchPlan?> SelectedPlanAsync()
    {
        if (_selected is null) return null;
        return await _catalog.CreateLaunchPlanAsync(_selected.Id,
            _mode.SelectedIndex == 1 ? LaunchMode.Vr : LaunchMode.Screen,
            _selected.ExternalLauncher is null ? LaunchTarget.Client : LaunchTarget.ExternalLauncher,
            _selected.ExternalLauncher?.ExecutablePath);
    }
    private async Task PlanSelectedAsync()
    {
        _status.Text = "Checking the exact launch files…";
        var plan = await SelectedPlanAsync(); if (plan is null) return;
        _output.Text = JsonSerializer.Serialize(plan, Program.Json);
        _status.Text = plan.CanExecuteOnCurrentPlatform ? "Launch plan ready; no process started" : plan.BlockReason ?? "Unsupported platform";
    }
    private async Task LaunchSelectedAsync()
    {
        _status.Text = "Verifying before launch…";
        var plan = await SelectedPlanAsync(); if (plan is null) return;
        if (!plan.CanExecuteOnCurrentPlatform) throw new InvalidOperationException(plan.BlockReason);
        var result = await _catalog.LaunchAsync(plan);
        _status.Text = $"Started {Path.GetFileName(plan.ExecutablePath)} · process {result.ProcessId}";
        _output.Text = $"Executable: {plan.ExecutablePath}\nWorking directory: {plan.WorkingDirectory}\nThe selected client handles its own sign-in and connection flow.";
    }

    private async Task ShowDiscoveryAsync()
    {
        var candidates = Discovery.Find();
        var panel = new StackPanel { Spacing = 12, Margin = new(24) };
        var dialog = Dialog("Find installations", panel, 700, 470);
        LocalCandidate? chosen = null;
        panel.Children.Add(T("Existing clients on this PC", 22, "#F4F4F7", FontWeight.SemiBold));
        panel.Children.Add(T("Select a folder to inspect and register. No client binaries are copied or patched.", 13, "#A9ACB5"));
        foreach (var candidate in candidates)
        {
            var button = new Button { HorizontalAlignment = HorizontalAlignment.Stretch, Padding = new(15),
                HorizontalContentAlignment = HorizontalAlignment.Left, Content = new StackPanel { Spacing = 4, Children =
                { T(candidate.Name, 15, "#F4F4F7"), T(candidate.InstallRoot, 11, "#A9ACB5") } } };
            button.Click += (_, _) => { chosen = candidate; dialog.Close(); };
            panel.Children.Add(button);
        }
        if (candidates.Count == 0) panel.Children.Add(T("No client found in the known locations. Import its folder directly.", 14, "#A9ACB5"));
        await dialog.ShowDialog(this);
        if (chosen is not null) await ImportDialogAsync(chosen);
    }

    private async Task ImportDialogAsync(LocalCandidate? candidate)
    {
        string? root = candidate?.InstallRoot;
        if (root is null)
        {
            var folders = await StorageProvider.OpenFolderPickerAsync(new() { Title = "Select the existing Rec Room client folder", AllowMultiple = false });
            root = folders.FirstOrDefault()?.TryGetLocalPath();
            if (root is null) return;
        }
        var panel = new StackPanel { Spacing = 13, Margin = new(26) };
        var dialog = Dialog("Import client", panel, 720, 525);
        panel.Children.Add(T("Register this installation", 24, "#F4F4F7", FontWeight.SemiBold));
        panel.Children.Add(T(root, 12, "#A9ACB5"));
        panel.Children.Add(T("Display name", 12, "#A9ACB5"));
        var name = new TextBox { Text = candidate?.Name ?? "Rec Room · local build" }; panel.Children.Add(name);
        panel.Children.Add(T("Optional existing launcher (.exe)", 12, "#A9ACB5"));
        var launcher = new TextBox { Text = candidate?.LauncherPath, Watermark = "Use the client's supported launcher when required" };
        panel.Children.Add(launcher);
        var browse = new Button { Content = "Choose launcher…" };
        browse.Click += async (_, _) =>
        {
            var files = await StorageProvider.OpenFilePickerAsync(new() { Title = "Select the existing launcher", AllowMultiple = false,
                FileTypeFilter = new[] { new FilePickerFileType("Executable") { Patterns = new[] { "*.exe" } } } });
            if (files.FirstOrDefault()?.TryGetLocalPath() is { } path) launcher.Text = path;
        }; panel.Children.Add(browse);
        var replace = new CheckBox { Content = "Replace this folder's existing registration and file baseline", IsChecked = false };
        panel.Children.Add(replace);
        var note = T("Import records file fingerprints and a local profile. Your game files stay unchanged.", 12, "#A9ACB5");
        panel.Children.Add(note);
        var save = new Button { Content = "Inspect and import", Background = Brush.Parse("#8D72C9"), Foreground = Brushes.White, Padding = new(18, 10) };
        save.Click += async (_, _) =>
        {
            save.IsEnabled = false; note.Text = "Inspecting the installed build and hashing its launch files…";
            try
            {
                await _catalog.ImportAsync(new(root, name.Text ?? "Local client",
                    string.IsNullOrWhiteSpace(launcher.Text) ? null : launcher.Text, ReplaceExisting: replace.IsChecked == true));
                dialog.Close(); await RefreshAsync();
            }
            catch (Exception ex) { note.Text = ex.Message; save.IsEnabled = true; }
        };
        panel.Children.Add(save); await dialog.ShowDialog(this);
    }

    private async Task OpenCreatorAsync()
    {
        const int port = 5187;
        var origin = $"http://127.0.0.1:{port}";
        var dataDirectory = Path.Combine(_state, "creator");
        var canonical = Path.TrimEndingDirectorySeparator(Path.GetFullPath(dataDirectory));
        if (OperatingSystem.IsWindows()) canonical = canonical.ToUpperInvariant();
        var workspaceId = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(canonical)));
        using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(2) };
        bool running = false;
        try
        {
            using var response = await http.GetAsync(origin + "/api/v1/capabilities");
            if (response.IsSuccessStatusCode)
            {
                using var document = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
                var body = document.RootElement;
                running = body.TryGetProperty("product", out var product) && product.GetString() == "Metallic-RC" &&
                    body.TryGetProperty("mode", out var serviceMode) && serviceMode.GetString() == "local-authoring" &&
                    body.TryGetProperty("apiVersion", out var apiVersion) && apiVersion.GetString() == "1";
                if (!running) throw new InvalidOperationException("Port 5187 is occupied by another service.");
                if (!body.TryGetProperty("workspaceId", out var workspace) || workspace.GetString() != workspaceId)
                    throw new InvalidOperationException("Creator tools for another workspace are already running on port 5187. Close that service before opening this workspace.");
            }
        }
        catch (HttpRequestException) { }
        catch (TaskCanceledException) { }
        if (!running)
        {
            var filename = OperatingSystem.IsWindows() ? "Metallic.RC.Platform.exe" : "Metallic.RC.Platform";
            var executable = Path.Combine(AppContext.BaseDirectory, "platform", filename);
            if (!File.Exists(executable)) throw new InvalidOperationException("The creator service is not in this package. Build the complete desktop bundle to include it.");
            var info = new ProcessStartInfo(executable) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(executable)!, CreateNoWindow = true };
            info.ArgumentList.Add("--data-dir"); info.ArgumentList.Add(dataDirectory);
            info.ArgumentList.Add("--port"); info.ArgumentList.Add(port.ToString());
            try
            {
                _creator = Process.Start(info) ?? throw new InvalidOperationException("Could not start the creator service.");
                for (var i = 0; i < 40; i++)
                {
                    await Task.Delay(150);
                    if (_creator.HasExited) throw new InvalidOperationException("The creator service exited. Check the package and local data directory.");
                    try
                    {
                        using var response = await http.GetAsync(origin + "/api/v1/capabilities");
                        if (!response.IsSuccessStatusCode) continue;
                        using var document = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
                        var body = document.RootElement;
                        if (body.TryGetProperty("workspaceId", out var workspace) && workspace.GetString() == workspaceId &&
                            body.TryGetProperty("product", out var product) && product.GetString() == "Metallic-RC")
                        { running = true; break; }
                    }
                    catch (HttpRequestException) { }
                    catch (TaskCanceledException) { }
                }
                if (!running) throw new InvalidOperationException("The creator service did not become ready.");
            }
            catch { StopOwnedCreator(); throw; }
        }
        Process.Start(new ProcessStartInfo(origin) { UseShellExecute = true });
        _status.Text = "Creator tools opened · local drafts and CV3 preview";
    }

    private void StopOwnedCreator()
    {
        var creator = _creator;
        _creator = null;
        if (creator is null) return;
        try { if (!creator.HasExited) creator.Kill(entireProcessTree: true); }
        catch (InvalidOperationException) { }
        catch (System.ComponentModel.Win32Exception) { }
        finally { creator.Dispose(); }
    }

    private async Task GuardAsync(Func<Task> action)
    {
        if (_busy) return; _busy = true; SetButtons();
        try { await action(); }
        catch (Exception ex) { _status.Text = "Action could not complete"; _output.Text = ex.Message; }
        finally { _busy = false; SetButtons(); }
    }
    private void SetButtons()
    {
        _rows.IsEnabled = !_busy;
        _mode.IsEnabled = !_busy && _selected is not null && _selected.ExternalLauncher is null;
        foreach (var b in _busyButtons) b.IsEnabled = !_busy;
        _launch.IsEnabled = !_busy && _selected is not null && OperatingSystem.IsWindows() &&
            (!_selected.Inventory.ExternalModificationsDetected || _selected.ExternalLauncher is not null);
        _verify.IsEnabled = _plan.IsEnabled = !_busy && _selected is not null;
    }
    private Button B(string text, Func<Task> action, bool accent = false)
    {
        var button = new Button { Content = text, Padding = new(14, 10), CornerRadius = new(5), FontSize = 13,
            Background = Brush.Parse(accent ? "#8D72C9" : "#292A32"), Foreground = Brush.Parse("#F5F4F8"),
            BorderBrush = Brush.Parse(accent ? "#A389DF" : "#3B3C46"), BorderThickness = new(1) };
        button.Click += async (_, _) => await GuardAsync(action); _busyButtons.Add(button); return button;
    }
    private static Window Dialog(string title, Control content, double width, double height) => new()
    {
        Title = title, Width = width, Height = height, MinWidth = width - 50, MinHeight = height - 40,
        Background = Brush.Parse("#1B1C22"), Content = new ScrollViewer { Content = content },
        WindowStartupLocation = WindowStartupLocation.CenterOwner
    };
    private static TextBlock T(string text, double size, string color, FontWeight? weight = null) => new()
    { Text = text, FontSize = size, Foreground = Brush.Parse(color), FontWeight = weight ?? FontWeight.Normal, TextWrapping = TextWrapping.Wrap };
    private static string Bytes(long value) => $"{value / 1024d / 1024 / 1024:0.00} GiB";
}
