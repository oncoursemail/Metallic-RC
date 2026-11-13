namespace Metallic.RC.Platform;

public sealed record PlatformOptions(int Port, string DataDirectory)
{
    public const int DefaultPort = 4186;
    public const int MaxRequestBytes = 262144;

    public static PlatformOptions Parse(string[] args)
    {
        var port = DefaultPort;
        var directory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Metallic-RC", "platform");
        for (var i = 0; i < args.Length; i++)
        {
            var argument = args[i];
            if (argument is not ("--port" or "--data-dir") || i + 1 >= args.Length)
                throw new ArgumentException("Use --port <1024–65535> and --data-dir <directory>.");
            var value = args[++i];
            if (argument == "--port")
            {
                if (!int.TryParse(value, out port) || port is < 1024 or > 65535)
                    throw new ArgumentException("Port must be an integer between 1024 and 65535.");
            }
            else
            {
                if (string.IsNullOrWhiteSpace(value))
                    throw new ArgumentException("Data directory cannot be empty.");
                directory = Path.GetFullPath(value);
            }
        }
        return new(port, directory);
    }
}

public sealed class PlatformFault(int status, string code, string message) : Exception(message)
{
    public int Status { get; } = status;
    public string Code { get; } = code;
}
