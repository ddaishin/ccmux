//! Automated perf measurement harness (local tooling, not part of the app).
//!
//! Runs a ccmux binary inside a ConPTY, waits for startup, triggers a burst
//! (`cat <bigfile>`) in the initial pane's shell, lets it settle, then quits
//! ccmux with Ctrl+Q. The child ccmux writes its frame log to the path given
//! via CCMUX_PERF_LOG.
//!
//! Usage:
//!   cargo run --release --example perf_harness -- <ccmux-exe> <bigfile-msys-path> <perflog>

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let exe = args
        .next()
        .expect("usage: perf_harness <ccmux-exe> <bigfile> <perflog>");
    let bigfile = args.next().expect("missing <bigfile>");
    let perflog = args.next().expect("missing <perflog>");

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(&exe);
    cmd.env("CCMUX_PERF_LOG", &perflog);
    // The harness itself may run inside a ccmux pane; the child must not
    // inherit the nested-instance guard variable.
    cmd.env_remove("CCMUX");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    // Drain ccmux's terminal output so it never blocks on a full pipe.
    // Keep a copy on disk for debugging (what did the "screen" receive?).
    let dump_path = std::env::var("PERF_HARNESS_DUMP").ok();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let drain = std::thread::spawn(move || {
        let mut dump = dump_path.and_then(|p| std::fs::File::create(p).ok());
        let mut buf = [0u8; 8192];
        let mut total = 0usize;
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            if let Some(f) = dump.as_mut() {
                let _ = f.write_all(&buf[..n]);
            }
            total += n;
        }
        total
    });

    let mut writer = pair.master.take_writer().unwrap();

    // ccmux startup (graphics query fallback) + pane shell startup.
    std::thread::sleep(Duration::from_secs(4));

    // Trigger the burst in the focused pane's shell. Send the command text
    // and Enter separately: a rapid burst including the trailing \r gets
    // wrapped in bracketed paste by ccmux, and bash readline then inserts
    // the newline literally instead of executing the command.
    writer
        .write_all(format!("cat '{}'", bigfile).as_bytes())
        .unwrap();
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    writer.write_all(b"\r").unwrap();
    writer.flush().unwrap();

    // Let the burst run and settle.
    std::thread::sleep(Duration::from_secs(10));

    // Ctrl+Q quits ccmux.
    writer.write_all(&[0x11]).unwrap();
    writer.flush().unwrap();

    for _ in 0..50 {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    drop(pair.master);
    let total = drain.join().unwrap_or(0);
    eprintln!("perf_harness: done, host-side bytes read: {}", total);
}
