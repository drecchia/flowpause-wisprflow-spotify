# Phase 0 — Results

Test date: 2026-11-27
Spotify app version on the PC: not recorded
Echo model: Echo Dot ("<echo-name>")

Times are cold calls: each CLI run opens a new HTTPS connection (TLS handshake included).

## Echo Dot

| # | Step | Command | HTTP | ms | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | Login | `flowpause login` | — | — | Done with `scripts\install.ps1` + login; worked |
| 2 | State with music on the Echo | `flowpause status` | 200 | 1250 | `id`: `<echo-id>` · `is_restricted`: false · `type`: Speaker |
| 3 | Pause | `flowpause pause` | 200 | 784 | Report: "everything worked". Time to silence: not measured |
| 4 | Pause again (H2) | `flowpause pause` | not run | | |
| 5 | Resume | `flowpause play --device <id>` | 200 | 631 | Report: "everything worked" |
| 6a | Pause before the wait | `flowpause pause` | not run | | |
| 6b | Device list after 5 min | `flowpause devices` | not run | | |
| 6c | Resume after 5 min | `flowpause play --device <id>` | not run | | |
| 6d | Transfer (only if 6c returned 404) | `flowpause transfer --device <id>` | not run | | |

## Spotify on the PC

| # | Step | Command | HTTP | ms | Notes |
| --- | --- | --- | --- | --- | --- |
| 2 | State with music on the PC | `flowpause status` | 200 | 1098 | `id`: `<pc-id>` · `name`: <pc-name> · `is_restricted`: false |
| 3 | Pause | `flowpause pause` | not recorded | | |
| 4 | Pause again (H2) | `flowpause pause` | not run | | |
| 5 | Resume | `flowpause play --device <id>` | not recorded | | |

Extra: `flowpause status` with no active playback returned HTTP 204 in 1183 ms.

## Conclusion

| Item | Result |
| --- | --- |
| H1: Echo accepts `pause` and `play` | Confirmed: pause 200, play 200, `is_restricted: false` |
| H2: `pause` when already paused returns 403 | Not tested. That is why the phase 2 controller checks the state before pausing |
| Time between the command and silence on the Echo | Not measured |
| How long the Echo stays in the list after being paused | Not measured |
| Differences from `docs/spec.md` | None. Pause returned 200 (the spec accepts 200 or 204) |
