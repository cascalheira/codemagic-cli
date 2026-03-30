# codemagic-cli  (Vibecoded AI slop - but hey, it works!)

A terminal UI and CLI tool for [Codemagic CI/CD](https://codemagic.io), built with [ratatui](https://ratatui.rs).

---

## Features

| | |
|---|---|
| **Interactive TUI** | Browse all builds, filter by workflow, live-refresh running builds |
| **Build actions** | Download artifacts, convert AAB → APK, stream logs |
| **New-build wizard** | Pick app → workflow → branch and trigger a build |
| **CLI mode** | `download apk` subcommand for scripting and CI pipelines |
| **Clipboard** | Copy app / workflow IDs with one keypress |

---

## Installation

### Prerequisites

- [Rust](https://rustup.rs) 1.85+ (`rustup update stable`)
- macOS, Linux, or Windows (tested on macOS)

### Build from source

```bash
git clone <repo>
cd codemagic-cli
cargo build --release
# binary → target/release/codemagic-cli
```

Copy the binary somewhere on your `$PATH`:

```bash
cp target/release/codemagic-cli /usr/local/bin/
```

---

## Quick start

### 1. Get your API token

In the Codemagic web UI:
**Settings → Integrations → Codemagic API → Show**

### 2. First launch

```bash
codemagic-cli
```

On first run the onboarding screen appears and asks for your API token.  
The token is validated against the API and saved to
`~/.config/codemagic-cli/config.toml`.

Subsequent launches jump straight to the builds list.

---

## TUI — key bindings

### Builds list

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` | Open the **Build Actions** sheet for the selected build |
| `n` | Open the **New Build** wizard |
| `f` | Open the **Workflow filter** popup |
| `l` | Load more builds (next page) |
| `r` | Refresh (reload from the top) |
| `i` | Open the **App & Workflow IDs** browser |
| `s` | Open **Settings** (change API token) |
| `q` / `Esc` | Quit |
| `Ctrl-C` / `Ctrl-D` | Force quit |

> Running builds show an animated braille spinner and a **● N live** badge in the status bar. Their status is automatically refreshed every 5 seconds.

---

### Build Actions sheet  (`Enter` on any build row)

The sheet shows build details (status, app, workflow, branch, duration, commit) inline at the top, followed by the action list:

| Key | Action |
|-----|--------|
| `↑` / `↓` / `j` / `k` | Navigate actions |
| `Enter` | Confirm selected action |
| `Esc` | Close |

**Available actions:**

#### Download Artifacts

Shows a table of all artefacts (name, type, size) for the selected build.  
Selecting one and pressing `Enter` downloads it directly to:

```
~/Codemagic/{App Name}/{Workflow Name}/{Build Number}/{filename}
```

An `.aab` file is always accompanied by a **Convert → APK** row at the bottom of the list (only shown when an AAB is present). Selecting it runs the bundletool conversion and saves the result at the same path as the other artefacts.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate |
| `Enter` | Download / convert |
| `Esc` | Back to Build Actions |

#### View Build Logs

Shows the list of build steps with their status icons (✓ ✗ ● ○).  
Pressing `Enter` on a step fetches and displays the full plain-text log for that step.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate steps |
| `Enter` | Open log for selected step |
| `Esc` | Back to Build Actions |

Inside the **Log Viewer**:

| Key | Action |
|-----|--------|
| `↑` / `↓` / `j` / `k` | Scroll one line |
| `PgUp` / `PgDn` | Scroll 20 lines |
| `Esc` | Back to step list |

---

### Workflow filter popup  (`f`)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate |
| `Enter` | Apply filter and reload builds |
| `Esc` | Cancel |

The currently active filter is shown in the filter bar. Selecting **All Workflows** clears the filter.

---

### New Build wizard  (`n`)

Three-step process:

**Step 1 — Select App**

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate |
| `Enter` | Next step |
| `Esc` | Cancel |

**Step 2 — Select Workflow**

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate workflows |
| `Enter` | Next step |
| `Esc` | Back to app selection |

An **Enter workflow ID manually…** option is always present at the bottom for `codemagic.yaml`-configured apps (which have no Workflow Editor entries). When selected, a text input appears for the workflow ID.

**Step 3 — Select Branch**

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate the filtered branch list |
| `type` | Filter branches (case-insensitive substring) |
| `Backspace` | Delete last filter character |
| `Enter` | Start build with the highlighted branch (or type a new branch name) |
| `Esc` | Back to workflow selection |

On success the TUI shows `✓ Build queued (id: …)` and reloads the list so the new build appears immediately.

---

### App & Workflow IDs browser  (`i`)

Useful when you need IDs for the CLI, CI scripts, or the new-build wizard.

```
My Flutter App
  App ID    5c9c064185dd2310123b8e96
  Workflows
    • Android Workflow              5d85f242e941e00019e81bd2
    • iOS Release                   6e96g353f052f11120f92ce3

──────────────────────────────────────────────────────────────
Another App
  App ID    6a1b234567890abcdef12345
  Workflows  (none — uses codemagic.yaml)
```

| Key | Action |
|-----|--------|
| `↑` / `↓` / `j` / `k` | Move between selectable IDs |
| `Enter` or `y` | Copy the highlighted ID to the system clipboard |
| `PgUp` / `PgDn` | Scroll content |
| `Esc` / `q` | Close |

> Clipboard access uses [`arboard`](https://github.com/1Password/arboard). On headless Linux you may need `xclip` or `xsel`.

---

### Settings  (`s`)

Change or rotate the stored API token.

| Key | Action |
|-----|--------|
| type / `Backspace` | Edit token |
| `Enter` | Validate and save |
| `Esc` | Cancel without saving |

The new token is validated with a live API call before being saved. The builds list reloads automatically on success.

---

## CLI mode

Non-interactive operations for scripts and CI pipelines.

### `download apk`

Finds the latest finished build for a workflow that contains an AAB artefact, converts it to a universal APK with [bundletool](https://developer.android.com/tools/bundletool), and saves it locally.

```bash
codemagic-cli download apk \
  --app-id      5c9c064185dd2310123b8e96 \
  --workflow-id release
```

**Example output:**

```
Fetching app info…
App: My Flutter App  ·  Workflow: Release Workflow
Searching for the latest build with an AAB artefact…
  Searched 20 builds so far, looking further back…
Found AAB in build #37: app-release.aab
Generating download link…
Downloading AAB (32.1 MB)…
Converting AAB → APK (bundletool)…
Extracting universal APK…
✓  APK saved to /Users/you/Codemagic/My Flutter App/Release Workflow/last/build.apk
```

**Output path:**

```
~/Codemagic/{App Name}/{Workflow Name}/last/build.apk
```

`last/` is always overwritten, giving you a stable path to the freshest APK.

**Recursive search:**  
If the most recent build has no AAB (e.g. it failed, was cancelled, or only produced an IPA), the command walks backwards through older builds automatically until it finds one.

**Getting IDs:**  
Press `i` in the TUI to open the App & Workflow IDs browser and copy the values you need.

#### bundletool auto-install

The command works without bundletool pre-installed:

1. Checks for `bundletool` binary on `PATH` — uses it if found
2. Checks for `java` on `PATH` — required for the JAR fallback
3. Checks for a cached JAR at `~/.config/codemagic-cli/bundletool.jar`
4. Downloads the latest JAR from [GitHub Releases](https://github.com/google/bundletool/releases) and caches it (one-time download, ~80 MB)

The cached JAR is shared between the TUI and the CLI, so it is only downloaded once regardless of which mode first triggers it.

```bash
# Quick manual install if preferred:
brew install bundletool
```

---

## Artifact download path convention

All downloaded files follow the same directory structure:

```
~/Codemagic/
  {App Name}/
    {Workflow Name}/
      {Build Number}/          ← numbered builds from TUI
        app-release.apk
        app-release.aab
        app-release.ipa
      last/                    ← always the latest, from CLI
        build.apk
```

Characters illegal in directory names (`/ \ : * ? " < > |`) are replaced with `_`.

---

## Configuration

| File | Purpose |
|------|---------|
| `~/.config/codemagic-cli/config.toml` | Stored API token |
| `~/.config/codemagic-cli/bundletool.jar` | Cached bundletool JAR |

**`config.toml` format:**

```toml
api_token = "your-token-here"
```

You can edit this file directly or use the **Settings** dialog (`s`) in the TUI.

---

## Architecture

```
src/
  main.rs       Entry point: clap dispatch → TUI or CLI
  cli.rs        Non-interactive download commands
  app.rs        TUI application state machine (screens, popups, async messages)
  ui.rs         ratatui rendering (all screens and popups)
  api.rs        Codemagic REST API client (reqwest)
  models.rs     API response types (serde)
  config.rs     Config file read / write (toml)
```

**Async design:**

- The terminal event loop runs on the tokio runtime
- A dedicated `std::thread` reads crossterm events (blocking I/O) and forwards them via an `mpsc` channel — this prevents the tokio runtime from being blocked
- API calls are spawned as tokio tasks; results arrive via a second `mpsc` channel
- A 5-second interval ticker polls running builds to keep their status live

---

## License

MIT
