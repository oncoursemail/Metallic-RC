using Metallic.RC.Backend;
var options = BackendOptions.Parse(args);
await using var app = BackendApplication.Build(options);
await app.RunAsync();
