param(
    [ValidateSet('win-x64', 'linux-x64')][string]$Runtime = 'win-x64',
    [string]$Dotnet = 'dotnet',
    [string]$OutputDirectory = '',
    [switch]$SkipAcceptance
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (!$OutputDirectory) { $OutputDirectory = Join-Path $root "artifacts/desktop/$Runtime" }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path $OutputDirectory) { throw "Choose a new output directory; existing output is preserved: $OutputDirectory" }
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_GENERATE_ASPNET_CERTIFICATE = 'false'
Push-Location $root
try {
    & $Dotnet --version
    if ($LASTEXITCODE) { throw 'The .NET SDK is unavailable.' }
    if (!$SkipAcceptance) {
        foreach ($test in @('Core', 'Platform')) {
            & $Dotnet run --project "tests/Metallic.RC.$test.Tests/Metallic.RC.$test.Tests.csproj" -c Release --no-launch-profile
            if ($LASTEXITCODE) { throw "$test acceptance failed." }
        }
    }
    & $Dotnet publish src/Metallic.RC.Launcher/Metallic.RC.Launcher.csproj -c Release -r $Runtime --self-contained true -p:PublishSingleFile=false -o $OutputDirectory
    if ($LASTEXITCODE) { throw 'Launcher publish failed.' }
    & $Dotnet publish src/Metallic.RC.Platform/Metallic.RC.Platform.csproj -c Release -r $Runtime --self-contained true -p:PublishSingleFile=false -o (Join-Path $OutputDirectory 'platform')
    if ($LASTEXITCODE) { throw 'Creator service publish failed.' }
    Copy-Item (Join-Path $root 'docs/DESKTOP_README.txt') (Join-Path $OutputDirectory 'README.txt')
    $files = @(Get-ChildItem $OutputDirectory -Recurse -File | Sort-Object FullName | ForEach-Object {
        [ordered]@{ path = $_.FullName.Substring($OutputDirectory.Length + 1).Replace('\','/'); bytes = $_.Length; sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $source = $null
    $dirty = $null
    if ((Test-Path (Join-Path $root '.git')) -and (Get-Command git -ErrorAction SilentlyContinue)) {
        $source = (& git rev-parse HEAD)
        $dirty = [bool](& git status --porcelain)
    }
    [ordered]@{ schemaVersion = 1; product = 'Metallic-RC'; runtime = $Runtime; sdk = (& $Dotnet --version); builtAtUtc = [DateTime]::UtcNow.ToString('o'); sourceCommit = $source; sourceTreeDirty = $dirty; files = $files } |
        ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutputDirectory 'build-manifest.json') -Encoding UTF8
    Write-Output "Desktop bundle: $OutputDirectory"
} finally { Pop-Location }
