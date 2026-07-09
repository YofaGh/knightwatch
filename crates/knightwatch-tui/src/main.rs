mod app;
mod events;
mod pollers;
mod tabs;
mod ui;
mod utils;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui_image::picker::Picker;
use std::io;

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

    // Read this before `picker` is moved into `App::new` below.
    match picker.protocol_type() {
        ratatui_image::picker::ProtocolType::Halfblocks => {
            eprintln!("image protocol: halfblocks (fallback, always safe)");
        }
        proto => {
            eprintln!(
                "image protocol: {proto:?} (detected/guessed — may not render correctly on all terminals, e.g. VS Code)"
            );
        }
    }

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let base_url = std::env::var("KW_URL").unwrap_or_else(|_| "http://localhost:8083".to_string());
    let token = std::env::var("KW_TOKEN").ok();
    let api = std::sync::Arc::new(kw_clients::ApiClient::new(base_url.clone(), token));

    let info = api.info().await.expect(&format!(
        "Failed to connect to knightwatch server at: {base_url}"
    ));

    let mut app = app::App::new(picker, (&info).into());

    // Single channel, single event enum, one receiver in the main loop.
    // Every producer below just sends into a clone of `tx`.
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    // --- Keyboard / mouse input ---
    pollers::spawn_input(tx.clone());

    // --- Screen tab ---
    if !info.blind {
        pollers::spawn_screen_poller(tx.clone(), api.clone());
    }

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

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
