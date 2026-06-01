# DBM TUI Submit Key Audit Report v1.0

## Scope

Target: `apps/cli/src/tui`

Objective:

- Observe key events before submit decision without rendering diagnostics to the UI.
- Add low terminal-dependency submit fallbacks.
- Preserve UI boundary isolation.

## Observation Sink

Key events are recorded to the internal trace buffer through:

```text
render_trace::record_key_event(...)
```

Recorded format:

```text
KEY={:?} MOD={:?}
```

The buffer is not rendered by Design Specification, Evaluation, or Analysis Result panes.

## Implemented Submit Mapping

Submit now accepts:

```text
Ctrl+D
Ctrl+Enter
Command+Enter
```

Modifier checks use `contains(...)`, so combinations such as `SUPER | SHIFT` and
`CONTROL | SHIFT` still reach submit.

## Local Verification

Automated tests verified:

- `Ctrl+D` submits editor content.
- `Ctrl+Enter` submits editor content.
- `Command+Enter` remains supported.
- Additional modifiers do not block modified Enter submit.
- Key events are captured in the internal trace buffer.
- Footer displays all submit fallbacks.
- TUI trace boundary tests still pass.

## Terminal Observation Status

Direct physical key observation in Ghostty and WezTerm was not performed in this
workspace session. The runtime observation hook is now available for those
manual checks.

Expected observation outcomes:

```text
Enter          -> KEY=Enter MOD=NONE
Command+Enter  -> KEY=Enter MOD=SUPER, if terminal reports Command
Ctrl+Enter     -> KEY=Enter MOD=CONTROL, if terminal reports Ctrl+Enter
Ctrl+D         -> KEY=Char('d') MOD=CONTROL
```

## Current Decision

Because Command+Enter has been reported as newline-only in both Ghostty and
WezTerm, the primary safe fallback is:

```text
Ctrl+D
```

If a terminal reports `Ctrl+Enter` as `CONTROL + Enter`, that path is also
accepted.
