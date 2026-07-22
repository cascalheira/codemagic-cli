# codemagic-app

Cross-platform (desktop + mobile) Codemagic.io client built with
[Dioxus](https://dioxuslabs.com). Shares its entire data layer — API client,
models, and config — with the terminal client via the `codemagic-core` crate.

## Status

Wired end-to-end against the live API:

- **Onboarding** — paste and validate an API token (persisted via `codemagic-core`).
- **Builds** — a master-detail screen:
  - **Sidebar** — the build list with status pills; click to select; refresh / sign out.
  - **Detail pane** — status, workflow, branch, version, timing, and commit;
    a **Steps** list with per-step status/duration and a **View log** viewer
    (fetched with the auth token and shown in an overlay); and an **Artifacts**
    list with per-file **Download** and **Download all** (saved to
    `~/Downloads/Codemagic`).

Next up: new-build wizard, AAB → APK conversion (desktop only — needs
bundletool), a mobile share sheet for downloads instead of a fixed path, and
secure token storage (Keychain / Keystore).

## Running

Install the Dioxus CLI once:

```sh
cargo install dioxus-cli
```

Then, from the repository root:

```sh
# Desktop (macOS / Windows / Linux) — the default feature
dx serve --package codemagic-app

# iOS (requires Xcode + a simulator or device)
dx serve --package codemagic-app --platform ios

# Android (requires Android SDK + NDK)
dx serve --package codemagic-app --platform android
```

A plain `cargo run -p codemagic-app` also launches the desktop build.

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
