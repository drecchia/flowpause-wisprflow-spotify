# flowpause

Pauses Spotify while you dictate with [Wispr Flow](https://wisprflow.ai) and resumes the music **on the same device** when dictation ends. It works whether Spotify is playing on the PC, on an Echo Dot, or on any other Spotify Connect device. Runs on Windows 11.

## Why this project exists

Wispr Flow can lower or pause your music while you dictate. That feature only acts on the PC: it sends a local media command, like the Play/Pause key on your keyboard, to whatever app is playing sound on Windows.

That is enough when the music comes out of the PC itself. It is not enough when the Spotify app on the PC is only a remote control for another device:

1. You open Spotify on the PC and choose to play on the Echo Dot (Spotify Connect).
2. The sound comes out of the Echo, not the PC. Windows has no Spotify audio session.
3. Flow's media command has nothing to pause on the PC and never reaches the Echo.
4. The music keeps playing while you talk, competing with your voice on the microphone.

The same happens with any Connect device: smart speaker, TV, phone, another computer.

The Spotify Web API reaches every device on the account. flowpause uses it to pause the music wherever it is playing.

## How it works

1. **Detects dictation.** It watches Windows audio capture sessions (WASAPI). When the Wispr Flow process starts capturing the microphone, dictation has started; when it stops, dictation has ended. It reacts to Windows events, with no polling.
2. **Pauses.** It asks Spotify whether music is playing and on which device. It pauses only if music is playing, and remembers the device.
3. **Resumes.** About 2 s after dictation ends, it presses play on the remembered device.

Safety rules:

- Never starts music that was not playing.
- Resumes only on the device where it paused, never on "the active device" or a default.
- Does not resume if you already pressed play, switched devices during the pause, or the pause lasted more than 5 min.
- Several dictations in a row, less than 2 s apart, give one pause and one resume.
- If the device dropped out of Spotify's device list (404 on play), it tries one playback transfer to that device; if that fails, it gives up.
- A network failure never crashes the program.

## Limitations

- **Windows 11 and Spotify only.** Other players and other operating systems are out of scope.
- **Requires Spotify Premium** and your own app in the Spotify developer dashboard (free, in Development Mode). See [`docs/CLIENT-ID.md`](docs/CLIENT-ID.md).
- **Latency.** In tests, each Web API call took 0.6 to 1.2 s on a new connection. Pausing takes two calls (a state check, then the pause), so the start of your speech may catch a bit of music. With the connection already open it should be faster; this has not been measured yet.
- **Depends on the Spotify Web API.** If Spotify restricts player commands for apps in Development Mode, flowpause stops working.
- **Depends on how Wispr Flow uses the microphone.** Detection looks for `wispr` in the executable path. If Flow changes its process or starts keeping the microphone open all the time, the detector needs an update.
- **Turn off Wispr Flow's own media pause.** With both enabled, they can fight each other when music plays on the PC.
- **No tray icon yet.** It runs in a console (`flowpause run`) or in the background, starting with Windows (`flowpause autostart on`).

The first version of the detector read microphone usage from the Windows registry. After a Windows update on 2026-11-24, the registry stopped being updated, and the detector moved to WASAPI. Details in section 10 of [`docs/spec.md`](docs/spec.md).

**Current status: phases 1 and 2.** The final behavior runs in a console (`flowpause run`) or in the background, starting with Windows (`flowpause autostart on`). There is no tray icon or configuration file yet (phase 3). Test results: [`docs/FASE0-RESULTADOS.md`](docs/FASE0-RESULTADOS.md) and [`docs/FASE1-2-RESULTADOS.md`](docs/FASE1-2-RESULTADOS.md).

## Requirements

- Windows 11, Rust toolchain `x86_64-pc-windows-msvc`.
- Spotify Premium account.

## 1. Create the app in the Spotify dashboard

Follow [`docs/CLIENT-ID.md`](docs/CLIENT-ID.md): how to create the app, where to copy the Client ID, and what to do if login fails. In short: create an app at <https://developer.spotify.com/dashboard>, add the redirect URI `http://127.0.0.1:8888/callback`, select **Web API**, and copy the **Client ID** from **Settings**. The client secret is not used.

## 2. Install with the script (recommended)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

The script first shows an antivirus warning and waits for Enter. Kaspersky may quarantine the executables produced by the build, and the build then fails with "Access denied". Add exclusions for the project's `target\` folder and for `%USERPROFILE%\.cargo\bin\flowpause.exe` and `flowpause-bg.exe`, or pause protection. The script then asks for the Client ID, saves `FLOWPAUSE_CLIENT_ID` in the user environment variables, runs `cargo build --release` and `cargo install --path . --locked`, and checks that the installed `flowpause.exe` runs.

To install by hand, follow sections 2a and 3.

## 2a. Set `FLOWPAUSE_CLIENT_ID`

Current PowerShell session only:

```powershell
$env:FLOWPAUSE_CLIENT_ID = "<your client id>"
```

Permanent for the user (applies to terminals opened afterwards):

```powershell
setx FLOWPAUSE_CLIENT_ID "<your client id>"
```

## 3. Build and install

```powershell
cargo build --release
cargo install --path .
```

`cargo install` copies `flowpause.exe` and `flowpause-bg.exe` to `%USERPROFILE%\.cargo\bin`, which is already on the `PATH`. Without installing, use `.\target\release\flowpause.exe` instead of `flowpause`.

## 4. Commands

| Command | What it does |
| --- | --- |
| `flowpause login` | Authorization Code with PKCE. Opens the browser and receives the callback on `127.0.0.1:8888` |
| `flowpause status` | `GET /v1/me/player` |
| `flowpause devices` | `GET /v1/me/player/devices` |
| `flowpause pause` | `PUT /v1/me/player/pause` |
| `flowpause play --device <id>` | `PUT /v1/me/player/play?device_id=<id>` |
| `flowpause transfer --device <id>` | `PUT /v1/me/player` with `{"device_ids":["<id>"],"play":true}` |
| `flowpause watch [--match wispr]` | Detector only. Prints the capture sessions it finds and `ditado iniciou` / `ditado terminou` (dictation started / ended) with a timestamp. Does not use Spotify |
| `flowpause run [--resume-delay-ms 2000] [--max-pause-secs 300] [--match wispr]` | Detector + controller: pauses during dictation and resumes on the same device. Ctrl+C to exit |
| `flowpause autostart on` / `off` | Turns starting with Windows on or off |

The program output is in Portuguese.

The Spotify commands print the call, the HTTP status, the call time in ms, and the error body, if any. If the access token had to be refreshed, the refresh appears on its own line with its own time; the call time does not include it. The exit code is 0 for a 2xx response and 1 otherwise.

Tokens are kept in Windows Credential Manager (Windows Credentials > Generic Credentials), under the names `spotify-refresh-token.flowpause` and `spotify-access-token.flowpause`. They are never written to a file. To delete the login:

```powershell
cmdkey /delete:spotify-refresh-token.flowpause
cmdkey /delete:spotify-access-token.flowpause
```

### `watch` and `run`

The detector watches Windows audio capture sessions (WASAPI) on every active microphone. Dictation counts as active while any process whose path contains `--match` (default `wispr`, case-insensitive) has an active capture session. There is no polling: the program reacts to Windows events. To see every capture session from any app, use `flowpause watch --match ""`.

`run` prints, with a timestamp, each dictation event, each state transition (`Ocioso` idle, `Pausado` paused, `SemRetomada` no resume, `Aguardando` waiting), and each HTTP call with status and ms. Rules:

- Before pausing, it checks the playback state and pauses only if music is playing. Phase 0 did not confirm that pausing an already paused player returns 403, so it does not pause blindly.
- Resumes only on the device where it paused, `--resume-delay-ms` after dictation ends.
- Does not resume if, at that moment, music is already playing, another device is active, or the pause lasted more than `--max-pause-secs`.
- A new dictation during the wait cancels the resume.
- On a 404 from play, it tries one transfer to the same device.
- Ctrl+C while the music is paused by the program: it tries to resume before exiting.

### Start with Windows

```powershell
flowpause autostart on    # turn on
flowpause-bg              # start now, without restarting
flowpause autostart off   # turn off
```

- `autostart on` writes the value `flowpause` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, pointing to `flowpause-bg.exe`.
- At login, `flowpause-bg.exe` starts `flowpause run` with no window, using the defaults (`--resume-delay-ms 2000`, `--max-pause-secs 300`, `--match wispr`).
- Output goes to `%LOCALAPPDATA%\flowpause\logs\flowpause.log`. The file only grows; log rotation comes in phase 3. To follow it live:

  ```powershell
  Get-Content "$env:LOCALAPPDATA\flowpause\logs\flowpause.log" -Wait -Tail 20
  ```

- Only one copy runs at a time. While the background copy is running, `flowpause run` in a terminal refuses to start and points to the log.
- With no network at login, it does not exit: it logs a warning and keeps going. Spotify calls work again once the network is back.
- There is no tray icon yet. To stop the background copy: `Stop-Process -Name flowpause`. Stopping it this way does not resume the music if the program paused it at that moment.
- Kaspersky may quarantine `flowpause-bg.exe`: add it to the exclusions.

## 5. Phase 0 test plan

Hypotheses:

- **H1:** the Echo Dot accepts `pause` and `play` through the Web API (it is not a restricted device).
- **H2:** pausing when already paused returns 403.

Record each result in `docs/FASE0-RESULTADOS.md`.

1. `flowpause login`
2. With music playing on the Echo: `flowpause status`. Note the `id` and `is_restricted`.
3. `flowpause pause`. Note the HTTP status and ms; check that the Echo stopped and estimate the time until silence.
4. `flowpause pause` again. Note the HTTP status (validates H2).
5. `flowpause play --device <id>`. Check that the Echo resumed.
6. Pause (`flowpause pause`), wait 5 min, `flowpause devices`. Is the Echo still listed? Then `flowpause play --device <id>`; on a 404, `flowpause transfer --device <id>`.
7. Repeat 2 to 5 with the music playing in Spotify on the PC.

## 6. Phase 1 and 2 test plan

Record each result in `docs/FASE1-2-RESULTADOS.md`.

1. `flowpause watch` and 5 dictations: each one produces exactly one `ditado iniciou` and one `ditado terminou`.
2. `flowpause run` with music on the Echo: dictate. The music stops and comes back on the Echo about 2 s later.
3. The same with the music in Spotify on the PC.
4. No music: dictate. Nothing starts playing.
5. Three dictations less than 2 s apart: one pause, one resume.
6. Dictate and press play manually during the pause: the program sends no command.
7. Dictate with music on the Echo and switch to the PC during the pause: it does not resume on the Echo.
