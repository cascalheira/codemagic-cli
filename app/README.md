# gantry

Cross-platform (desktop + mobile) Codemagic.io client built with
[Dioxus](https://dioxuslabs.com). Shares its entire data layer — API client,
models, and config — with the terminal client via the `gantry-core` crate.

## Status

Wired end-to-end against the live API:

- **Onboarding** — paste and validate an API token (persisted via `gantry-core`).
- **Builds** — a master-detail screen:
  - **Sidebar** — the build list with status pills; click to select; refresh / sign out.
  - **Detail pane** — status, workflow, branch, version, timing, and commit;
    a **Steps** list with per-step status/duration and a **View log** viewer
    (fetched with the auth token and shown in an overlay); and an **Artifacts**
    list with per-file **Download**, **Download all**, and **Convert to APK**
    for `.aab` artifacts. Every download opens a native Save As / folder dialog.

- **App & workflow IDs** — a searchable browser for the IDs `codemagic.yaml`,
  the API, and CI scripts refer to, each copyable in one click.

- **Updates** — on launch, checks GitHub Releases for a newer version and
  offers a link to it. Toggleable in Settings, with a manual "Check now".

**AAB → APK** conversion is shared with the CLI via `gantry-core` and runs
`bundletool` (desktop only — needs a `bundletool` binary or Java; the JAR is
auto-downloaded and cached on first use).

## Keyboard shortcuts

`⌘` is `Ctrl` outside macOS. The single-letter forms match the terminal
client's bindings.

| Keys | Action |
| --- | --- |
| `↑` `↓` · `j` `k` | Move through builds |
| `r` · `⌘R` | Refresh |
| `n` · `⌘N` | New build |
| `f` · `/` · `⌘F` | Filter by workflow |
| `s` · `⌘,` | Settings |
| `o` | Open the build on codemagic.io |
| `i` | App & workflow IDs |
| `?` | Show this list |
| `Esc` | Close |

Single-letter shortcuts are ignored while a text field has focus.

Next up: a mobile share sheet for downloads instead of a native dialog, and
secure token storage (Keychain / Keystore).

## Running

Install the Dioxus CLI once:

```sh
cargo install dioxus-cli
```

Then, from the repository root:

```sh
# Desktop (macOS / Windows / Linux) — the default feature
dx serve --package gantry

# iOS (requires Xcode + a simulator or device)
dx serve --package gantry --platform ios

# Android (requires Android SDK + NDK)
dx serve --package gantry --platform android
```

A plain `cargo run -p gantry` also launches the desktop build.

## Layout

```
app/
  src/
    main.rs              # app shell: routes between onboarding and build list
    state.rs             # AppState (token, API client) shared via context
    components/
      onboarding.rs      # token entry + validation
      build_list.rs      # home screen
  assets/main.css        # responsive, light/dark-aware styles
```
