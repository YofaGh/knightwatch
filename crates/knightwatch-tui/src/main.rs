mod app;
mod commands;
mod confirm;
mod connect;
mod events;
mod login;
mod poll_panel;
mod pollers;
mod process_widgets;
mod tabs;
mod ui;
mod ui_helpers;
mod utils;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui_image::picker::Picker;
use std::io;

/// Cleanly leaves raw mode / the alternate screen. Called on the normal
/// quit path as well as the early-quit-from-connect-screen path, so both
/// restore the terminal the same way.
fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    enable_raw_mode()?;

    // Make sure a panic anywhere (main loop or a spawned task) doesn't leave
    // the user's terminal stuck in raw mode / the alternate screen.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    // Query the terminal for its graphics-protocol support and font size,
    // while we're still on the "real" screen and in raw mode so the escape-
    // sequence response can be read without waiting on Enter.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| {
        // Query failed outright (no response, unsupported terminal, etc).
        // halfblocks() skips protocol detection and forces Unicode
        // half-block rendering, which works everywhere.
        Picker::halfblocks()
    });

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let token = std::env::var("KW_TOKEN").ok();
    let initial_url =
        std::env::var("KW_URL").unwrap_or_else(|_| "http://localhost:8083".to_string());
    let mut connect_state = connect::ConnectState::new(initial_url);

    // Pre-connect screen: shown before `App` exists (it can't exist yet —
    // building it needs `info`, which needs a live connection). This is a
    // plain synchronous poll/draw loop, not the `AppEvent`-driven one,
    // since there's nothing else to drive yet.
    let (api, info) = loop {
        terminal.draw(|frame| {
            let area = frame.area();
            connect_state.render(frame, area);
        })?;

        if !crossterm::event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }

        let event = crossterm::event::read()?;
        match connect_state.handle_event(&event) {
            connect::ConnectOutcome::Quit => {
                restore_terminal()?;
                return Ok(());
            }
            connect::ConnectOutcome::Submit(url) => {
                // Redraw immediately so "connecting…" shows before we
                // block this task on the request below.
                terminal.draw(|frame| {
                    let area = frame.area();
                    connect_state.render(frame, area);
                })?;

                let api = std::sync::Arc::new(kw_clients::ApiClient::new(&url, token.clone()));
                match api.info().await {
                    Ok(info) => break (api, info),
                    Err(e) => connect_state.fail(format!("failed to connect: {e}")),
                }
            }
            connect::ConnectOutcome::None => {}
        }
    };

    // Single channel, single event enum, one receiver in the main loop.
    // Every producer below just sends into a clone of `tx`.
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    let mut app = app::App::new(picker, api, &info, tx.clone());

    // Drop our own sender. If we didn't, `rx.recv()` would never return
    // `None` even after every spawned task above exits, since a sender
    // would always still technically be alive — that's the classic
    // "channel never closes, loop spins forever" bug.
    drop(tx);

    // Draw once immediately so there's something on screen before the
    // first event arrives.
    terminal.draw(|frame| ui::render(frame, &mut app))?;

    while let Some(event) = rx.recv().await {
        app.handle_app_event(event);

        if app.dirty {
            terminal.draw(|frame| ui::render(frame, &mut app))?;
            app.dirty = false;
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal()?;
    terminal.show_cursor()?;

    Ok(())
}
