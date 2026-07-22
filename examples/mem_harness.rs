//! Automated memory measurement harness (local tooling, not part of the app).
//!
//! Runs a ccmux binary inside a ConPTY, drives a scenario, then samples the
//! ccmux process RSS via PowerShell Get-Process. Only the ccmux.exe process
//! is measured (holder of scrollback buffers and syntect assets); pane shells
//! are out of scope. Output is one TSV line on stderr:
//!   label \t WorkingSet64 \t PeakWorkingSet64 \t PrivateMemorySize64
//!
//! Scenarios:
//!   s1  idle       — start, wait 4s, sample
//!   s2  scrollback — start, `cat <bigfile>` (>= 20k lines so the buffer is
//!                    full at both 10k and 2k settings), settle, sample
//!   s3  tabs       — start, Ctrl+T x2 (3 tabs total), wait, sample
//!
//! A/B is done by running two binaries (HEAD^ vs HEAD) through the same
//! scenario — config.toml injection is not possible because dirs::config_dir
//! cannot be redirected via environment variables.
//!
//! Usage:
//!   cargo run --release --example mem_harness -- <ccmux-exe> <bigfile-msys-path> <s1|s2|s3>

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::Duration;

fn sample_rss(pid: u32, label: &str) {
    let script = format!(
        "$p = Get-Process -Id {}; Write-Output \"$($p.WorkingSet64) $($p.PeakWorkingSet64) $($p.PrivateMemorySize64)\"",
        pid
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .expect("failed to run powershell");
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace();
    let ws = it.next().unwrap_or("?").to_string();
    let peak = it.next().unwrap_or("?").to_string();
    let private = it.next().unwrap_or("?").to_string();
    eprintln!("{}\t{}\t{}\t{}", label, ws, peak, private);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let exe = args
        .next()
        .expect("usage: mem_harness <ccmux-exe> <bigfile> <s1|s2|s3>");
    let bigfile = args.next().expect("missing <bigfile>");
    let scenario = args.next().expect("missing <s1|s2|s3>");

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
    // The harness itself may run inside a ccmux pane; the child must not
    // inherit the nested-instance guard variable.
    cmd.env_remove("CCMUX");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let pid = child.process_id().expect("child pid unavailable");

    // Drain ccmux's terminal output so it never blocks on a full pipe.
    // Keep a copy on disk for debugging (what did the "screen" receive?).
    let dump_path = std::env::var("MEM_HARNESS_DUMP").ok();
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

    match scenario.as_str() {
        "s1" => {
            sample_rss(pid, "s1-idle");
        }
        "s2" => {
            // Send the command text and Enter separately: a rapid burst
            // including the trailing \r gets wrapped in bracketed paste by
            // ccmux, and bash readline then inserts the newline literally
            // instead of executing the command.
            writer
                .write_all(format!("cat '{}'", bigfile).as_bytes())
                .unwrap();
            writer.flush().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            writer.write_all(b"\r").unwrap();
            writer.flush().unwrap();
            // Let the burst run and settle.
            std::thread::sleep(Duration::from_secs(10));
            sample_rss(pid, "s2-scrollback");
        }
        "s3" => {
            // Ctrl+T opens a new tab (app.rs: Ctrl+T / Alt+T — new tab).
            writer.write_all(&[0x14]).unwrap();
            writer.flush().unwrap();
            std::thread::sleep(Duration::from_secs(1));
            writer.write_all(&[0x14]).unwrap();
            writer.flush().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            sample_rss(pid, "s3-tabs");
        }
        other => panic!("unknown scenario: {} (expected s1|s2|s3)", other),
    }

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
    eprintln!("mem_harness: done, host-side bytes read: {}", total);
}
