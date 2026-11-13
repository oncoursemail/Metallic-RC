# Steam recovery wrapper source review

Reviewed without running it on the user's PC or requesting elevation.

## Exact reviewed files

| File | SHA-256 |
| --- | --- |
| `run_steam_recovery.py` | `ce5f715474db5c0f5aa021ba3aaba9fa9384764bb5a86e7b6ab6e3a85c785856` |
| `capture_steam_recroom.py` | `37ace729f9ee7ce45b4189c6bc54e4f21a1b9f3063b6ee56a1570f4e4ddb5179` |

Both files pass Python syntax compilation. This source review is not a Windows
runtime acceptance result, an elevation grant, or proof of metadata recovery.

## Corrected issues

1. The first wrapper launched the capture helper in name-polling mode. An
   independently started client could have been read during a race. The reviewed
   version starts the helper with `--pid-file`, waits for its ready report, launches
   its own game process, then atomically writes that retained process's PID. The
   helper validates the output directory, selected build hash and executable path
   before binding once to that PID.
2. An exception while closing a log could previously skip closing the named
   section. Log closes now have individual exception handling, and the entire
   process/log cleanup is enclosed by the section's `finally` block. This also
   handles a Python interrupt during a process wait.
3. The receipt now distinguishes `scriptWritesToGameFiles: false` from the
   original game's normal side effects and explicitly names the two originals
   whose post-run hashes are checked.

## Reviewed boundaries

- `CreateFileMappingW` and `CloseHandle` use pointer-sized handles. The backing
  file handle is `INVALID_HANDLE_VALUE`; size is the fixed 66,060,456 bytes.
- `GetLastError` is captured immediately. `ERROR_ALREADY_EXISTS` closes the newly
  opened handle and aborts; an existing section is never reused or overwritten.
- The new pagefile mapping relies on Windows zero initialization. No mapping view,
  memory writes, process injection, privilege adjustment, service modification or
  game-file patch is implemented.
- The exact selected Steam `RecRoom.exe` and `GameAssembly.dll` must match pinned
  hashes. The helper's hash is also pinned before import and invocation.
- Actual execution requires an already elevated token. The wrapper does not
  elevate itself. `--check-only` performs preflight without creating the section
  or launching the game/capture processes.
- Existing Rec Room processes cause a conservative abort. The game command uses
  a fixed executable path and argument list with `shell=False`.
- The capture helper reads the selected process through query/read rights only.
  It preserves read coverage and marks metadata/code as candidates, not decoded.
- Cleanup retains the wrapper's original `Popen` process handles. It requests a
  normal window close, then terminates only those owned handles if necessary.
- Output is outside the installed game directory. Original-file post-run hash
  evidence covers `GameAssembly.dll` and `RecRoom.exe` explicitly.

## Remaining practical limits

The original game may write its ordinary logs or create children. The wrapper
does not terminate arbitrary children, other clients, services or Steam. Closing
its mapping handle does not prove that no game-held reference remains. The
receipt records this limit. Abrupt OS termination cannot guarantee a final
receipt or owned-process cleanup, although Windows closes the terminated
wrapper's kernel handles.

The PID binding belongs to this local task and is not an authentication mechanism
against another program deliberately modifying the same user's task files.
Windows preflight and the separately approved elevated run remain necessary.

No additional blocking source-review issue remains in these exact file versions.
