use std::io;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

mod api;
mod app;
mod cli;
mod config;
mod models;
mod ui;

use app::{App, AppMessage};

// ─── CLI argument schema ──────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "codemagic-cli",
    about = "Codemagic.io TUI client and CI/CD automation tool",
    long_about = "Run without arguments to open the interactive TUI.\n\
                  Pass a subcommand to run a non-interactive operation."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download build artifacts
    Download {
        #[command(subcommand)]
        sub: DownloadSub,
    },
}

#[derive(Subcommand)]
enum DownloadSub {
    /// Download (and convert from AAB if needed) the latest APK for a workflow.
    ///
    /// Searches builds newest-first until it finds one with an AAB artefact,
    /// converts it via bundletool, and writes the result to
    /// ~/Codemagic/{app}/{workflow}/last/build.apk
    Apk {
        /// Codemagic application ID (see the App IDs dialog in the TUI: press i)
        #[arg(long)]
        app_id: String,

        /// Workflow ID (see the App IDs dialog in the TUI: press i)
        #[arg(long)]
        workflow_id: String,
    },
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        // ── Non-interactive CLI mode ─────────────────────────────────────────
        Some(Cmd::Download {
            sub:
                DownloadSub::Apk {
                    app_id,
                    workflow_id,
                },
        }) => {
            cli::run_download_apk(&app_id, &workflow_id).await?;
        }

        // ── Interactive TUI mode (default) ───────────────────────────────────
        None => {
            run_tui().await?;
        }
    }

    Ok(())
}

// ─── TUI mode ─────────────────────────────────────────────────────────────────

async fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let saved_config = config::load_config().unwrap_or(None);
    let poll_secs = saved_config
        .as_ref()
        .and_then(|c| c.poll_interval_secs)
        .unwrap_or(5)
        .max(1); // guard against 0
    let refresh_secs = saved_config
        .as_ref()
        .and_then(|c| c.refresh_interval_secs)
        .unwrap_or(30)
        .max(5); // guard against unreasonably short intervals
    let (tx, mut rx) = mpsc::channel::<AppMessage>(64);
    let mut app = App::new(tx, saved_config);

    if app.screen == app::Screen::Builds {
        app.fetch_builds();
    }

    let (event_tx, mut event_rx) = mpsc::channel::<Event>(64);
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if event_tx.blocking_send(ev).is_err() {
                break;
            }
        }
    });

    let result = event_loop(
        &mut terminal,
        &mut app,
        &mut rx,
        &mut event_rx,
        poll_secs,
        refresh_secs,
    )
    .await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

// ─── TUI event loop ───────────────────────────────────────────────────────────

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: &mut mpsc::Receiver<AppMessage>,
    event_rx: &mut mpsc::Receiver<Event>,
    poll_secs: u64,
    refresh_secs: u64,
) -> Result<()> {
    let mut redraw_tick = tokio::time::interval(Duration::from_millis(250));
    redraw_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut poll_tick = tokio::time::interval(Duration::from_secs(poll_secs));
    poll_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll_tick.tick().await;

    // Full builds-list refresh in the background.
    let mut auto_refresh_tick = tokio::time::interval(Duration::from_secs(refresh_secs));
    auto_refresh_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    auto_refresh_tick.tick().await; // skip the immediate first tick

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            Some(event) = event_rx.recv() => handle_event(app, event),
            Some(msg)   = rx.recv()       => app.handle_message(msg),
            _ = redraw_tick.tick()        => {}
            _ = poll_tick.tick()          => app.poll_running_builds(),
            _ = auto_refresh_tick.tick()  => app.soft_refresh(),
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// ─── Input dispatch ───────────────────────────────────────────────────────────

fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                app.should_quit = true;
            } else {
                app.handle_key(key);
            }
        }
        Event::Resize(_, _) => {}
        _ => {}
    }
}
