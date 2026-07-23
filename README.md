# Gantry  (Vibecoded AI slop - but hey, it works!)

A terminal UI and CLI tool for [Codemagic CI/CD](https://codemagic.io), built with [ratatui](https://ratatui.rs).

> Gantry is an **unofficial** client for Codemagic. It is not affiliated with,
> endorsed by, or sponsored by Codemagic / Nevercode Ltd. "Codemagic" is used
> only to describe what this tool talks to.

---

## Workspace layout

This repository is a Cargo workspace with three crates:

| Crate | What it is |
|---|---|
| [`core/`](core) | `gantry-core` — the shared API client, data models, and config. No UI dependencies, so it compiles for desktop **and** mobile. |
| [`cli/`](cli) | `gantry-cli` — the terminal UI + CLI (this document). |
| [`app/`](app) | `gantry` — a cross-platform Dioxus GUI (desktop + iOS/Android). See [`app/README.md`](app/README.md). |

Build everything with `cargo build`; build just the terminal client with
`cargo build -p gantry-cli`.

---

## Features

| | |
|---|---|
| **Interactive TUI** | Browse all builds, filter by workflow **and status**, live-refresh running builds |
| **Build actions** | Download artifacts, convert AAB → APK, stream logs |
| **Remote access** | SSH / VNC credentials for a running build machine |
| **New-build wizard** | Pick app → workflow → branch and trigger a build |
| **CLI mode** | `download apk` and `remote-access` subcommands for scripting and CI pipelines |
| **Clipboard** | Copy app / workflow IDs with one keypress |

---

![Gantry — Builds list](screenshots/screenshot.png)

---

## Installation

### macOS & Linux — one-liner

Paste this in your terminal. The script auto-detects your OS and CPU architecture
(x86-64 or arm64), downloads the matching binary from the latest GitHub release,
verifies the SHA-256 checksum, and installs to `/usr/local/bin`.

```bash
curl -fsSL https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.sh | sh
```

**Options** — set environment variables before the pipe:

```bash
# Install a specific version
VERSION=v1.3.0 curl -fsSL https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.sh | sh

# Install to a custom directory (no sudo needed if you own it)
INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.sh | sh
```

---

### Windows — PowerShell

Open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex
```

The script detects your CPU architecture (x86-64 or arm64), downloads the
matching `.zip` from the latest GitHub release, verifies the SHA-256 checksum,
extracts the binary to `%LOCALAPPDATA%\gantry`, and adds that directory
to your user `PATH` automatically.

**Options** — set environment variables before the pipe:

```powershell
# Install a specific version
$env:VERSION = "v1.3.0"; irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex

# Install to a custom directory
$env:INSTALL_DIR = "C:\Tools"; irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex
```

> **Execution-policy note:** if you see a security error, run
> `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` once, then retry.

---

### Manual download

Pre-built binaries for every platform are attached to every
[GitHub release](https://github.com/cascalheira/codemagic-cli/releases/latest):

| File | Platform |
|------|----------|
| `gantry-cli-macos-aarch64.tar.gz` | macOS Apple Silicon |
| `gantry-cli-macos-x86_64.tar.gz` | macOS Intel |
| `gantry-cli-linux-x86_64.tar.gz` | Linux x86-64 |
| `gantry-cli-linux-aarch64.tar.gz` | Linux arm64 |
| `gantry-cli-windows-x86_64.zip` | Windows x86-64 |
| `gantry-cli-windows-aarch64.zip` | Windows arm64 |

Each asset is accompanied by a `.sha256` checksum file.

---

### Build from source

**Prerequisites:** [Rust](https://rustup.rs) 1.85+ (`rustup update stable`)

```bash
git clone https://github.com/cascalheira/codemagic-cli.git
cd codemagic-cli
cargo build --release
# binary → target/release/gantry-cli
```

Copy the binary somewhere on your `$PATH`:

```bash
cp target/release/gantry-cli /usr/local/bin/
```

---

## Quick start

### 1. Get your API token

In the Codemagic web UI:
**Settings → Integrations → Codemagic API → Show**

### 2. First launch

```bash
gantry-cli
```

On first run the onboarding screen appears and asks for your API token.  
The token is validated against the API and saved to
`~/.config/gantry/config.toml`.

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
~/Gantry/{App Name}/{Workflow Name}/{Build Number}/{filename}
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

#### Remote Access

Only offered while a build is running. Fetches the SSH script URL and the VNC
host / port / username / password for the machine the build is executing on.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate fields |
| `Enter` | Copy the selected field to the clipboard |
| `v` | Open the VNC session in the system's default handler |
| `r` | Retry the request |
| `Esc` | Back to Build Actions |

> Remote access has to be enabled for the workflow **before** the build starts,
> and the credentials only exist while the build is running. Otherwise Codemagic
> answers "Remote access is not enabled for this build", which is shown as-is.

---

### Filter popup  (`f`)

Two columns: **Workflow** and **Status**.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate the focused column |
| `Tab` / `←` / `→` | Switch column |
| `Enter` | Apply both filters and reload builds |
| `Esc` | Cancel |

Both active filters are shown in the filter bar. **All Workflows** and **Any
status** clear their respective filter.

Filtering happens server-side, so a status filter searches your whole build
history rather than only the builds already loaded. The status column needs the
v3 API (see [Architecture](#architecture)) and is hidden when it is unavailable.

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
gantry-cli download apk \
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
✓  APK saved to /Users/you/Gantry/My Flutter App/Release Workflow/last/build.apk
```

**Output path:**

```
~/Gantry/{App Name}/{Workflow Name}/last/build.apk
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
3. Checks for a cached JAR at `~/.config/gantry/bundletool.jar`
4. Downloads the latest JAR from [GitHub Releases](https://github.com/google/bundletool/releases) and caches it (one-time download, ~80 MB)

The cached JAR is shared between the TUI and the CLI, so it is only downloaded once regardless of which mode first triggers it.

```bash
# Quick manual install if preferred:
brew install bundletool
```

### `remote-access`

Prints the SSH script URL and VNC credentials for the machine a build is
running on.

```bash
gantry-cli remote-access --build-id 6a61dd804f424ae5d8417923
```

```
SSH script : https://api.codemagic.io/remote-access/…/connect.sh
VNC URL    : vnc://builder:s3cret@1.2.3.4:5900
VNC host   : 1.2.3.4
VNC port   : 5900
VNC user   : builder
VNC pass   : s3cret
```

Add `--json` for a machine-readable object to pipe into a VNC client or an
`ssh` wrapper:

```bash
gantry-cli remote-access --build-id <id> --json | jq -r .vnc.url
```

> Remote access has to be enabled for the workflow **before** the build starts,
> and the credentials only exist while the build is running. Otherwise the
> command exits non-zero with Codemagic's own explanation.

---

## Artifact download path convention

All downloaded files follow the same directory structure:

```
~/Gantry/
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
| `~/.config/gantry/config.toml` | Stored API token |
| `~/.config/gantry/bundletool.jar` | Cached bundletool JAR |

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
  api.rs        Codemagic v1 REST API client (reqwest)
  api_v3.rs     Codemagic v3 REST API client — filtered/cursor-paged build
                listing and remote access; converts to the v1 build model
  models.rs     API response types (serde)
  config.rs     Config file read / write (toml)
```

**Async design:**

- The terminal event loop runs on the tokio runtime
- A dedicated `std::thread` reads crossterm events (blocking I/O) and forwards them via an `mpsc` channel — this prevents the tokio runtime from being blocked
- API calls are spawned as tokio tasks; results arrive via a second `mpsc` channel
- A 5-second interval ticker polls running builds to keep their status live

**Two APIs:**

Codemagic runs a v1 API (`api.codemagic.io`) and a newer v3 one
(`codemagic.io/api/v3`), both authenticated with the same token. Gantry uses v3
for the build listing — it is the only one that filters by status server-side,
pages by cursor, and exposes remote access — and v1 for everything else: build
details, logs, artifact URLs, and starting builds.

v3 lists builds per team, so Gantry resolves the token's teams on startup and
merges a page from each. If that call fails or the token sees no teams, the list
falls back to the v1 endpoint and the status filter is hidden.

---

## License

MIT
