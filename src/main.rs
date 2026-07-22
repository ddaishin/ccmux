mod app;
mod claude_monitor;
mod config;
mod filetree;
mod pane;
mod perf;
mod preview;
mod timing;
mod ui;
mod version_check;

use std::io;
use std::panic;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> Result<()> {
    // Detect if running inside another ccmux instance
    if std::env::var("CCMUX").is_ok() {
        eprintln!("ccmux: already running inside a ccmux pane (nested instance not allowed).");
        eprintln!("       Open a new tab with Alt+T (or Ctrl+T) or split with Ctrl+D / Ctrl+E instead.");
        std::process::exit(1);
    }

    // If a directory is passed as argument, cd into it first
    if let Some(dir) = std::env::args().nth(1) {
        let path = std::path::Path::new(&dir);
        if path.is_dir() {
            std::env::set_current_dir(path)?;
        } else {
            eprintln!("ccmux: not a directory: {}", dir);
            std::process::exit(1);
        }
    }

    // Install panic hook to restore terminal state on crash
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), crossterm::event::DisableMouseCapture);
        let _ = execute!(io::stdout(), crossterm::event::DisableBracketedPaste);
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    // User config (fail-open: parse failures surface as a status
    // message and the app continues with defaults). Loaded BEFORE
    // App::new because the initial pane's scrollback size comes from it.
    let (config, config_warning) = config::Config::load();

    // Query terminal for graphics protocol support BEFORE raw mode.
    // Falls back to halfblocks if detection fails.
    let image_picker = Some(
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks()),
    );

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    execute!(stdout, crossterm::event::EnableBracketedPaste)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Get initial terminal size
    let size = terminal.size()?;

    // Create app
    let mut app = app::App::new(size.height, size.width, config)?;
    app.image_picker = image_picker;
    app.status_message = config_warning;

    // Main event loop
    let result = run_event_loop(&mut terminal, &mut app);

    // Cleanup
    app.shutdown();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture
    )?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableBracketedPaste
    )?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Minimum interval between two draws (frame pacing, ~60fps cap).
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Upper bound on how long the loop sleeps with nothing to do.
const IDLE_TIMEOUT: Duration = Duration::from_millis(1000);
/// How long to keep collecting a rapid key burst (paste detection).
/// The old inline `event::poll(1ms)` window plus 1ms of slack for the
/// input-thread → channel hop. Clipboard pastes arrive at µs intervals,
/// human typing at ≥30ms, so this cannot merge normal keystrokes.
const KEY_BURST_WINDOW: Duration = Duration::from_millis(2);

/// Reads crossterm events on a dedicated thread and forwards them into the
/// app's event channel, so the main loop can block on one channel and wake
/// immediately for both PTY output and user input.
///
/// The thread is detached: `event::read()` cannot be interrupted safely, so
/// it lives until the process exits (or until the receiver is dropped, which
/// makes the forwarding `send` fail).
fn spawn_input_thread(tx: std::sync::mpsc::Sender<app::AppEvent>) {
    std::thread::spawn(move || loop {
        match event::read() {
            // Windows delivers Key events for Press, Release and Repeat;
            // only Press must reach the app (matches the old loop).
            Ok(Event::Key(k)) if k.kind != KeyEventKind::Press => continue,
            Ok(ev) => {
                perf::mark_input_event();
                if tx.send(app::AppEvent::Input(ev)).is_err() {
                    break;
                }
            }
            Err(_) => {
                // Transient read error — avoid a hot spin.
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    });
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    let mut paste_buffer: Vec<u8> = Vec::new();
    let mut perf_log = perf::PerfLog::from_env();
    let mut pacer = timing::FramePacer::new(MIN_FRAME_INTERVAL);

    spawn_input_thread(app.event_tx.clone());

    loop {
        // ── 1. Sleep until the next event or the earliest timer ──
        // While dirty but pacing-blocked, the pacer deadline doubles as
        // the coalescing window: events arriving before next_frame are
        // all drained into the same upcoming frame.
        let now = std::time::Instant::now();
        // While a cooldown suppresses drawing, a dirty frame cannot be
        // drawn anyway — scheduling the pacer deadline would spin the
        // loop with a zero timeout until the cooldown expires. The
        // cooldown's own deadline (first two entries) guarantees the
        // post-cooldown redraw wakeup.
        let can_draw_now =
            !app.paste_cooldown.is_active(now) && !app.resize_cooldown.is_active(now);
        let deadline = timing::earliest([
            app.paste_cooldown.deadline(),
            app.resize_cooldown.deadline(),
            app.ws()
                .file_tree_visible
                .then(|| app.ws().file_tree.next_refresh_at()),
            app.next_claude_check_at(),
            pacer.deadline_if_dirty(app.dirty && can_draw_now, now),
            Some(now + IDLE_TIMEOUT),
        ])
        .expect("idle timeout always present");

        match app
            .event_rx
            .recv_timeout(deadline.saturating_duration_since(now))
        {
            Ok(ev) => {
                handle_app_event(app, ev, &mut paste_buffer)?;
                // Coalesce everything else that is already queued.
                while let Ok(ev) = app.event_rx.try_recv() {
                    handle_app_event(app, ev, &mut paste_buffer)?;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            // Cannot happen while App holds event_tx; defensive exit.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // ── 2. Timers ──
        let now = std::time::Instant::now();
        if app.paste_cooldown.take_expired(now) {
            app.dirty = true;
        }
        if app.resize_cooldown.take_expired(now) {
            app.dirty = true;
        }
        if app.ws().file_tree_visible && app.ws_mut().file_tree.auto_refresh_if_needed() {
            app.dirty = true;
        }
        // JSONL monitor I/O runs here, outside the draw closure, so a slow
        // file read can never stall inside terminal.draw.
        if app.update_claude_monitors() {
            app.dirty = true;
        }

        // ── 3. Paced render ──
        if app.dirty
            && !app.paste_cooldown.is_active(now)
            && !app.resize_cooldown.is_active(now)
            && pacer.ready(now)
        {
            app.dirty = false;
            let draw_start = perf_log.as_ref().map(|_| std::time::Instant::now());
            terminal.draw(|frame| {
                ui::render(app, frame);
            })?;
            let after_draw = std::time::Instant::now();
            pacer.mark_drawn(after_draw);
            if let (Some(p), Some(start)) = (perf_log.as_mut(), draw_start) {
                p.on_frame(start, after_draw.duration_since(start));
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Dispatch one channel event. PTY-side events go to `App::handle_pty_event`;
/// terminal input is handled here because paste-burst detection needs the
/// main loop's paste buffer.
fn handle_app_event(
    app: &mut app::App,
    ev: app::AppEvent,
    paste_buffer: &mut Vec<u8>,
) -> Result<()> {
    match ev {
        app::AppEvent::Input(event) => handle_input_event(app, event, paste_buffer),
        other => {
            app.handle_pty_event(other);
            Ok(())
        }
    }
}

fn handle_input_event(
    app: &mut app::App,
    event: Event,
    paste_buffer: &mut Vec<u8>,
) -> Result<()> {
    match event {
        // Only Press events reach us (filtered in the input thread).
        Event::Key(key) => {
            let consumed = app.handle_key_event(key)?;
            if !consumed {
                // Collect rapid key events as potential paste
                if let Some(bytes) = crate::app::key_event_to_bytes_pub(&key) {
                    paste_buffer.extend_from_slice(&bytes);
                    collect_key_burst(app, paste_buffer)?;
                    flush_paste_buffer(app, paste_buffer)?;
                }
            }
            app.dirty = true;
        }
        Event::Paste(text) => {
            app.forward_paste_to_pty(&text)?;
            app.paste_cooldown
                .arm(std::time::Instant::now(), app::PASTE_COOLDOWN);
            app.dirty = true;
        }
        Event::Mouse(mouse) => {
            app.handle_mouse_event(mouse);
            app.dirty = true;
        }
        Event::Resize(cols, rows) => {
            // Propagate the new terminal size to App so every pane's PTY
            // gets a prompt SIGWINCH, and hold the paint briefly while
            // the children redraw.
            app.on_terminal_resize(cols, rows);
        }
        _ => {}
    }
    Ok(())
}

/// Keep collecting keys that arrive within [`KEY_BURST_WINDOW`] of each
/// other into `paste_buffer` (clipboard-paste detection on terminals that
/// send pastes as a key burst). Mirrors the old nested `event::poll(1ms)`
/// loop, with two intentional differences: PTY output arriving mid-burst
/// no longer terminates the window (it only marks the screen dirty), and
/// the window is read from the unified channel.
fn collect_key_burst(app: &mut app::App, paste_buffer: &mut Vec<u8>) -> Result<()> {
    // Absolute deadline, extended only when a key arrives. PTY output
    // events must NOT extend it: during heavy output they arrive faster
    // than the window and a relative timeout would keep the window open
    // (freezing rendering and swallowing normal keystrokes into a
    // "paste") until the output pauses.
    let mut window_end = std::time::Instant::now() + KEY_BURST_WINDOW;
    loop {
        let remaining = window_end.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(()); // window closed
        }
        match app.event_rx.recv_timeout(remaining) {
            Ok(app::AppEvent::Input(Event::Key(k))) => {
                if app.handle_key_event(k)? {
                    // Shortcut consumed — flush what we have first
                    // (preserves the old ordering guarantee).
                    flush_paste_buffer(app, paste_buffer)?;
                    return Ok(());
                }
                if let Some(b) = crate::app::key_event_to_bytes_pub(&k) {
                    paste_buffer.extend_from_slice(&b);
                }
                app.dirty = true;
                window_end = std::time::Instant::now() + KEY_BURST_WINDOW;
            }
            Ok(ev @ app::AppEvent::PtyOutput(_)) => {
                // Burst may continue; just note the pending redraw.
                app.handle_pty_event(ev);
            }
            Ok(other) => {
                // Any other event ends the burst: flush, then handle it.
                flush_paste_buffer(app, paste_buffer)?;
                handle_app_event(app, other, paste_buffer)?;
                return Ok(());
            }
            Err(_) => return Ok(()), // window closed (timeout)
        }
    }
}

/// Flush accumulated key buffer to PTY. If multiple characters were collected
/// (indicating a paste), wrap in bracketed paste sequences only when the PTY
/// application has enabled the mode. Unconditional wrapping causes shells that
/// haven't opted in to display the escape sequences as literal text (issue #2).
fn flush_paste_buffer(app: &mut app::App, buffer: &mut Vec<u8>) -> Result<()> {
    if buffer.is_empty() {
        return Ok(());
    }

    let focused_id = app.ws().focused_pane_id;
    if let Some(pane) = app.ws_mut().panes.get_mut(&focused_id) {
        pane.scroll_reset();
        if buffer.len() > 6 {
            if pane.is_bracketed_paste_enabled() {
                let sanitized = app::strip_bracket_paste_sequences(buffer);
                let mut data = Vec::with_capacity(sanitized.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend_from_slice(&sanitized);
                data.extend_from_slice(b"\x1b[201~");
                pane.write_input(&data)?;
            } else {
                pane.write_input(buffer)?;
            }
            app.paste_cooldown
                .arm(std::time::Instant::now(), app::PASTE_COOLDOWN);
        } else {
            // Normal typing — send directly
            pane.write_input(buffer)?;
        }
    }
    buffer.clear();
    Ok(())
}
