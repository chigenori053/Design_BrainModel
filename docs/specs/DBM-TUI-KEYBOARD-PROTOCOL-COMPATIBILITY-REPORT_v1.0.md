# DBM TUI Keyboard Protocol Compatibility Report v1.0

Date: 2026-06-04
Status: Investigation -> Fixed pending external terminal verification
Priority: P1
Classification: Product Hardening

## 日本語

### 調査対象

対象範囲は `apps/cli/src/tui/*`、特に `TerminalRenderer` と Product TUI input pipeline である。RuntimeCore、Canonical Reuse、Followup Resolver、Replay Engine、HolographicMemory は対象外とした。

### 原因

原因は `TerminalRenderer::enter()` が `KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES` を常時有効化していたことによる keyboard enhancement protocol 互換性問題である。

この flag が有効な場合、対応ターミナルでは Shift によって生成済みの文字ではなく、物理キーに近い `KeyCode::Char` と `KeyModifiers::SHIFT` が届く場合がある。そのため `Shift+1` は `Char('!')` ではなく `Char('1') + SHIFT` として観測される。TUI input pipeline は `KeyCode::Char(ch)` をそのまま `insert_char(ch)` するため、`1`, `[`, `]` が入力される。

判定: Cause Type A - Keyboard Enhancement Flags Incompatibility.

### 修正

`REPORT_ALL_KEYS_AS_ESCAPE_CODES` を Product TUI の通常起動から外し、`DISAMBIGUATE_ESCAPE_CODES` のみを維持した。これにより printable character input は legacy character semantics に戻り、ターミナルが生成した `A`, `!`, `{`, `}` がそのまま editor に入力される。

変更箇所:

- `apps/cli/src/tui/renderer/mod.rs`
- `apps/cli/src/tui/state.rs`

### 回帰テスト

追加したテスト:

- Renderer が `REPORT_ALL_KEYS_AS_ESCAPE_CODES` を有効化しないこと
- `a`, `A`, `!`, `{`, `}` が Product TUI editor にそのまま入力されること
- diagnostics mode が `Char('1') + SHIFT` のような enhanced physical-key event を記録できること

### 検証結果

ローカル自動検証:

- `cargo test -p design_cli tui::renderer::tests::terminal_enter_pushes_keyboard_enhancement_flags_without_forcing_printable_keys`
- `cargo test -p design_cli tui::state::tests::shifted_printable_characters_insert_as_terminal_generated_text`
- `cargo test -p design_cli tui::state::tests::diagnostics_records_shift_modified_physical_key_events`

外部ターミナル検証はこの実行環境から Ghostty、WezTerm、Terminal.app、iTerm2 の実 UI を直接操作できないため未実施。手動検証では以下を確認する。

```text
cargo run -p design_cli -- --diagnostic-input
```

入力:

```text
a
A
!
{
}
```

期待:

```text
[EVENT] Key(Char('A') ...)
[KEY]   KEY=Char('A') ...
[INPUT] insert_char('A')
```

同様に `!`, `{`, `}` が `Char('!')`, `Char('{')`, `Char('}')` として表示されること。

### Terminal Matrix

| Environment | Status | Expected result |
| --- | --- | --- |
| Ghostty | Pending manual verification | `A`, `!`, `{`, `}` are inserted as printable chars |
| WezTerm | Pending manual verification | `A`, `!`, `{`, `}` are inserted as printable chars |
| Terminal.app | Pending manual verification | `A`, `!`, `{`, `}` are inserted as printable chars |
| iTerm2 | Pending manual verification | `A`, `!`, `{`, `}` are inserted as printable chars |

## English

### Investigation Scope

The investigation was limited to `apps/cli/src/tui/*`, especially `TerminalRenderer` and the Product TUI input pipeline. RuntimeCore, Canonical Reuse, Followup Resolver, Replay Engine, and HolographicMemory were out of scope.

### Root Cause

The root cause was that `TerminalRenderer::enter()` always enabled `KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES`.

With that flag enabled, compatible terminals may report the physical key plus `KeyModifiers::SHIFT` instead of the final printable character. For example, `Shift+1` can arrive as `Char('1') + SHIFT` rather than `Char('!')`. The TUI input pipeline correctly inserts the received `KeyCode::Char(ch)` directly, so the editor receives `1`, `[`, or `]`.

Decision: Cause Type A - Keyboard Enhancement Flags Incompatibility.

### Fix

Product TUI no longer enables `REPORT_ALL_KEYS_AS_ESCAPE_CODES` during normal terminal setup. It keeps only `DISAMBIGUATE_ESCAPE_CODES`, preserving escape disambiguation while restoring legacy printable character semantics for text input.

Changed files:

- `apps/cli/src/tui/renderer/mod.rs`
- `apps/cli/src/tui/state.rs`

### Regression Tests

Added coverage for:

- Renderer does not enable `REPORT_ALL_KEYS_AS_ESCAPE_CODES`
- `a`, `A`, `!`, `{`, `}` are inserted exactly as terminal-generated printable text
- Diagnostics mode still records enhanced physical-key observations such as `Char('1') + SHIFT`

### Verification

Local automated checks:

- `cargo test -p design_cli tui::renderer::tests::terminal_enter_pushes_keyboard_enhancement_flags_without_forcing_printable_keys`
- `cargo test -p design_cli tui::state::tests::shifted_printable_characters_insert_as_terminal_generated_text`
- `cargo test -p design_cli tui::state::tests::diagnostics_records_shift_modified_physical_key_events`

Manual terminal verification remains pending because this environment cannot directly operate Ghostty, WezTerm, Terminal.app, or iTerm2 UI sessions.

Run:

```text
cargo run -p design_cli -- --diagnostic-input
```

Then enter:

```text
a
A
!
{
}
```

Expected diagnostics:

```text
[EVENT] Key(Char('A') ...)
[KEY]   KEY=Char('A') ...
[INPUT] insert_char('A')
```

The same expectation applies to `!`, `{`, and `}`.
