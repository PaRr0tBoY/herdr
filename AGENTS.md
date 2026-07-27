# herdr (personal fork)

Terminal based agent runtime for coding agents. This is a personal fork of
[ogulcancelik/herdr](https://github.com/ogulcancelik/herdr) for customizing the
experience on Windows. Changes here are for personal use; they are not intended
for upstream contribution unless explicitly stated.

## Architecture Principles

These principles describe how herdr is built. Follow them when making changes.

- **State is separated from runtime.** `AppState` is pure data, testable without PTYs or async. `PaneState` is separate from `PaneRuntime`. Workspace logic doesn't need real terminals.
- **Render is pure.** `compute_view()` handles geometry and mutations. `render()` takes `&AppState` and only draws. Never mutate state during render.
- **No god objects.** If a module is doing too many things, split it. `app/` is already split into state, actions, and input. Keep it that way.
- **Platform code is isolated.** OS-specific behavior lives in the matching `src/platform/<os>.rs` file, with only shared traits, types, wrappers, and testable contracts in `src/platform/mod.rs`. Core modules don't have `#[cfg(target_os)]`.
- **Detection is decoupled.** The detector reads a screen snapshot, never touches the parser or viewport state.
- **Screen detection is evidence-based.** When changing `src/detect/manifests/`, first capture the relevant bottom-buffer state with `herdr agent read <pane> --source detection --format text` and, when styling or alternate screen behavior matters, `--format ansi`. Decide which visible controls are invariant, which are alternatives, and encode them as explicit AND/OR gates. Do not match whole-pane incidental text, and do not use the user-visible viewport for agent status because users can scroll it.
- **UI patterns should be reused.** Herdr is a mouse-first TUI. New dialogs, onboarding, settings, and post-update flows should follow the existing UI/UX language and interaction patterns instead of inventing one-off screens.

### Runtime/client boundary guardrail

Herdr is migrating toward a server-owned runtime protocol with the TUI as one client. New work should not deepen the current server/TUI coupling.

Before adding state, API fields, events, commands, or socket messages, classify the feature:

- Shared runtime/session fact: belongs in server state and should be exposed through the JSON API/event path when practical.
- TUI presentation state: belongs only in the TUI/client layer.

Do not add new shared behavior that only works through the private TUI client socket. Use neutral server/API names, not UI-surface names like sidebar, row, card, or widget.

Examples:

- Pane/agent metadata, process state, terminal state, events: server/runtime.
- Sidebar layout, token placement, colors, selection, modals, mouse/viewport state: TUI/client.
- Workspace/tab/pane remain shared session organization for now, but avoid making them mandatory identity for unrelated runtime features.

## Development

### Debugging server-side input issues

When a feature works in monolithic mode (`cargo run -- --no-session`) but not in
client/server mode (`cargo run`), the root cause is usually one of:

1. **Mouse events in headless mode skip chrome.** `route_client_events_from`
   only calls `handle_pane_mouse_only`, not the full mouse handler. Tab bar
   clicks, note button clicks, and sidebar interactions need their own hit-test
   branches in the `RawInputEvent::Mouse` arm.
2. **Windows clipboard requires a window handle.** `GetConsoleWindow()` returns
   NULL in headless server processes. Pass `NULL` to `OpenClipboard` instead;
   it uses the current task's implicit window.
3. **Debug builds use `herdr-dev` config directory.** Logs, session files, and
   note files go to `%APPDATA%/herdr-dev/` (not `herdr/`). Check
   `src/config/io.rs::app_dir_name()` for the current mapping.

**Effective input debugging process:**

1. Add `tracing::info!` at the **highest event entry point** (e.g. right after
   `coalesce_bracketed_paste` in `route_client_events_from`, logging every
   `Key`/`Mouse`/`Paste` event with `code`, `kind`, `note_active`, `mode`).
2. Use `info!` level, never `debug!` — default log level is INFO.
3. Build and run. Reproduce the issue.
4. Grep the log for the prefix. The log will show exactly what events arrived
   and what state they saw.
5. Remove verbose logging after the fix is confirmed.

**Avoid:** theorizing about what events "should" arrive, writing state machines
(like `coalesce_bracketed_paste`) before seeing the actual event stream,
checking the wrong log file, assuming the user's actions didn't happen because
the log is silent (check whether the handler even runs for that input path).

### Syncing upstream

This fork tracks `upstream/master`. To pull in upstream changes:

```bash
git fetch upstream
git merge upstream/master
```

Resolve conflicts favoring personal customizations where they conflict with
upstream changes. After major upstream merges, rebuild and smoke-test.

### Testing

Run tests before committing changes:

```bash
cargo test
```

Or with nextest if installed:

```bash
cargo nextest run
```

Unit tests live next to the code (`#[cfg(test)] mod tests`). New `AppState` or `Workspace` behavior should be testable with `AppState::test_new()` and `Workspace::test_new()` without PTYs.

When testing a new Herdr build from inside an existing Herdr session, clear
inherited socket overrides so the debug binary talks to the debug `herdr-dev`
server instead of the installed stable server:

```bash
# On Unix:
env -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH cargo run -- <command>

# On Windows (PowerShell):
$env:HERDR_SOCKET_PATH = ''; $env:HERDR_CLIENT_SOCKET_PATH = ''; cargo run -- <command>
```

### Code Conventions

- Rust: no `unwrap()` in production code. Use `tracing` for logging. Use `#[allow]` only with a comment explaining why.
- Rust platform-specific code must be compile-gated. Put OS APIs and substantial OS behavior in `src/platform/`; when platform checks are needed elsewhere, use `#[cfg(windows)]`, `#[cfg(unix)]`, or target-specific `#[cfg(...)]` on imports, fields, functions, impls, and match arms so Windows-only code does not compile into Unix builds and Unix-only code does not compile into Windows builds. Use `cfg!(...)` only for pure cross-platform policy constants whose branches both compile on every target.
- Don't add dependencies without a reason. Check whether existing dependencies cover the need first.
- Integration asset versions (`HERDR_INTEGRATION_VERSION` markers and matching `*_INTEGRATION_VERSION` constants) are migration versions relative to the latest released tag, not per-commit counters on `master`. If an integration asset changes multiple times between releases, bump it once from the version in the latest release.
- When changing the server/client wire protocol, compare `src/protocol/wire.rs::PROTOCOL_VERSION` against the latest released tag. Bump it only if the current source protocol is not already greater than the latest released protocol. Update hardcoded protocol expectations and manual protocol fixtures in tests.

### Commit Style

Use lowercase conventional commits, no emojis:

```text
feat: description of what changed
fix: description of what was fixed
```

Keep commit subjects descriptive — they are the primary record of what changed
and why in this fork.

### Agent Detection Updates

To update or add an agent detection manifest:

1. Drive the agent into the target state in a live herdr pane.
2. Capture the detection buffer: `herdr agent read <pane> --source detection --format text` (add `--format ansi` when styling or alternate screen matters).
3. Inspect matching: `herdr agent explain <pane> --json`.
4. Update the manifest in `src/detect/manifests/<agent>.toml`.
5. For live testing, copy the manifest to `~/.config/herdr/agent-detection/<agent>.toml` and run `herdr server reload-agent-manifests`.
6. Once verified, remove the local override so the bundled manifest is the source of truth.

Keep Rust tests focused on manifest parsing, rule semantics, and reload behavior. Use live pane reads for agent-specific screen evidence.

### Vendored libghostty-vt

`vendor/libghostty-vt.vendor.json` records the upstream source commit currently vendored.

If you need to patch the vendored terminal library (e.g., for Windows fixes):

- Track patches in `vendor/libghostty-vt.patches.md`.
- Store patch files under `vendor/patches/libghostty-vt/`.
- Each entry should say why the patch exists, the vendored base commit, and touched files.
- When updating the vendored source from upstream, re-check every active patch.

### TUI widget coordinate mapping

When integrating third-party widgets with internal coordinate systems
(e.g. ratatui-textarea's screen↔data mapping, scroll offsets):

- **Never use scroll/viewport state as positioning input.** Scroll offsets are
  render outputs. Computing cursor position from them creates a feedback loop
  where the cursor changes the scroll, which changes the cursor position.
  Symptom: repeated identical input alternates between two wrong results.
- **Screen columns ≠ character count.** CJK characters occupy 2 display
  columns. Forward/Back navigation moves by grapheme clusters, not screen
  columns. Map through `unicode_width::UnicodeWidthChar` for conversions.
- **Use stable post-render anchors.** `widget.cursor()` +
  `widget.screen_cursor()` form a (data, screen) pair from the previous frame.
  Compute target data coordinates from this anchor, then apply with a single
  atomic `Jump`.

### Local planning

Put local PRDs, planning notes, and exploratory specs under `.local/prd/`. `.local/` is gitignored and locally controlled.
