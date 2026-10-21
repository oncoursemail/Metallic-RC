METALLIC-RC 0.2 — COMPILED-CLIENT REWORK

Open Metallic.RC.Launcher.exe on Windows or Metallic.RC.Launcher on Linux.
The bundle includes its .NET runtime; no system-wide SDK installation is needed.

YOUR BUILDS
Find installations or import an existing Rec Room client folder. Import reads the
client files and records fingerprints in your local Metallic-RC state directory.
It does not copy or change the installed game. Use the optional existing launcher
field for installations that require their own supported launcher. An intentional
client update can be registered using the explicit replace-baseline checkbox.

Verify files checks registered launch files. Show launch plan displays the exact
executable, arguments and working directory without starting it. Known modified
clients require an explicitly registered existing launcher. Windows game binaries
do not run natively under the Linux launcher; no Proton adapter is included yet.

CREATOR TOOLS
Creator tools starts the bundled local service and opens http://127.0.0.1:5187.
Create/save circuit drafts, edit typed node connections or graph JSON, validate,
and run a bounded preview. Closing the launcher stops the service it started.
Drafts remain on disk and are available the next time you open the same workspace.

CURRENT CAPABILITY
This is a compiled original C# launcher and local creator preview. It is not a
rebuilt Rec Room executable or a compatible multiplayer/voice server. Starting an
existing game uses that game's present services. CV3 is not yet in the game client.
Windows/Linux tooling builds do not establish Android, Quest or Linux game ports.

COMMAND LINE
Metallic.RC.Launcher.exe discover
Metallic.RC.Launcher.exe inspect "C:\path\to\RecRoom"
Metallic.RC.Launcher.exe import "C:\path\to\RecRoom" --name "My local build"
Metallic.RC.Launcher.exe list
Metallic.RC.Launcher.exe verify PROFILE_ID
Metallic.RC.Launcher.exe plan PROFILE_ID --mode screen
Use --state-dir DIRECTORY to choose a separate local workspace.

Source and technical evidence: https://github.com/oncoursemail/Metallic-RC
The accompanying build-manifest.json records the package's own file hashes.
