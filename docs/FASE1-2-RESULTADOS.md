# Phases 1 and 2 — Results

Test date:
Wispr Flow version:
Command: `flowpause run` (defaults: `--resume-delay-ms 2000`, `--max-pause-secs 300`, `--match wispr`)
Active pause strategy: CheckFirst (checks the state before pausing)

## Phase 1: detector (`flowpause watch`)

| Dictation | `ditado iniciou` (started) lines | `ditado terminou` (ended) lines | Notes |
| --- | --- | --- | --- |
| 1 | | | |
| 2 | | | |
| 3 | | | |
| 4 | | | |
| 5 | | | |

## Phase 2: acceptance criteria 1 to 6 (spec section 9)

| # | Criterion | Passed? | Time to stop | Time to resume | Notes / log excerpt |
| --- | --- | --- | --- | --- | --- |
| 1 | Music on the Echo Dot: stops within 1 s when dictation starts; comes back on the Echo about 2 s after it ends | | | | |
| 2 | Music in Spotify on the PC: same behavior, resuming on the PC | | | | |
| 3 | Nothing playing: dictating does not start music on any device | | — | — | |
| 4 | Three dictations less than 2 s apart: the music stops once and comes back once, at the end of the third | | | | |
| 5 | Manual play during the pause: the program sends no command when dictation ends | | | — | |
| 6 | Device switch during the pause: the program does not resume on the old device | | | — | |
