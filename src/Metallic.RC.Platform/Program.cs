using Metallic.RC.Platform;

if (args.Length == 1 && args[0] is "--help" or "-h")
{
    Console.WriteLine("Metallic-RC local authoring service\n--port <1024–65535>  default 4186\n--data-dir <directory>  private draft storage\nGame protocol, multiplayer and voice are not implemented by this service.");
    return 0;
}
try
{
    var options = PlatformOptions.Parse(args);
    await using var app = PlatformApplication.Build(options);
    Console.WriteLine($"Metallic-RC local authoring: http://127.0.0.1:{options.Port}");
    await app.RunAsync();
    return 0;
}
catch (ArgumentException error)
{
    Console.Error.WriteLine(error.Message);
    return 2;
}
catch (Exception)
{
    Console.Error.WriteLine("The local platform could not start. Check the port, private data directory, and whether another process owns this store.");
    return 1;
}
