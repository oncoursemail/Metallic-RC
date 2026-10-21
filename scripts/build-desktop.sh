#!/usr/bin/env bash
# Reconstructed after workspace maintenance; mirrors the retained PowerShell package contract.
set -euo pipefail

if (( $# > 2 )); then
  echo 'Usage: build-desktop.sh [linux-x64|win-x64] [OUTPUT_DIRECTORY]' >&2
  exit 2
fi

task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
task_runtime="${1:-linux-x64}"
case "$task_runtime" in
  linux-x64|win-x64) ;;
  *) echo 'Runtime must be linux-x64 or win-x64.' >&2; exit 2 ;;
esac
task_output="${2:-$task_root/artifacts/desktop/$task_runtime}"
task_output="$(python3 -c 'import os, sys; print(os.path.abspath(sys.argv[1]))' "$task_output")"
if [[ -e "$task_output" || -L "$task_output" ]]; then
  echo "Choose a new output directory; existing output is preserved: $task_output" >&2
  exit 1
fi

task_dotnet="${DOTNET:-dotnet}"
export DOTNET_CLI_TELEMETRY_OPTOUT=1
export DOTNET_NOLOGO=1
export DOTNET_GENERATE_ASPNET_CERTIFICATE=false
cd -- "$task_root"
task_sdk="$("$task_dotnet" --version)"
printf 'Using .NET SDK %s\n' "$task_sdk"
for task_suite in Core Platform; do
  "$task_dotnet" run --project "tests/Metallic.RC.$task_suite.Tests/Metallic.RC.$task_suite.Tests.csproj" \
    -c Release --no-launch-profile
done

"$task_dotnet" publish src/Metallic.RC.Launcher/Metallic.RC.Launcher.csproj \
  -c Release -r "$task_runtime" --self-contained true -p:PublishSingleFile=false -o "$task_output"
"$task_dotnet" publish src/Metallic.RC.Platform/Metallic.RC.Platform.csproj \
  -c Release -r "$task_runtime" --self-contained true -p:PublishSingleFile=false -o "$task_output/platform"
cp -- "$task_root/docs/DESKTOP_README.txt" "$task_output/README.txt"
python3 "$task_root/scripts/build_manifest.py" "$task_output" \
  --runtime "$task_runtime" --sdk "$task_sdk" --source-root "$task_root"
printf 'Desktop bundle: %s\n' "$task_output"
