//! Repro harness for "claude in 2nd tab shows shell label" (local tooling).
//!
//! Spawns ccmux in a ConPTY, opens a second tab (Alt+T), runs `claude` in it,
//! waits, and dumps the host-side output so the pane title label can be
//! inspected. Usage:
//!   cargo run --release --example repro_tab2 -- <ccmux-exe> <dump-file> [use_tab2:0|1]

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let exe = args.next().expect("usage: repro_tab2 <ccmux-exe> <dump> [0|1]");
    let dump_path = args.next().expect("missing <dump>");
    let use_tab2 = args.next().as_deref() != Some("0");

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
    cmd.env_remove("CCMUX");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let dp = dump_path.clone();
    let drain = std::thread::spawn(move || {
        let mut dump = std::fs::File::create(dp).ok();
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            if let Some(f) = dump.as_mut() {
                let _ = f.write_all(&buf[..n]);
            }
        }
    });

    let launch_claude = |w: &mut Box<dyn Write + Send>| {
        w.write_all(b"claude").unwrap();
        w.flush().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        w.write_all(b"\r").unwrap();
        w.flush().unwrap();
    };

    let mut writer = pair.master.take_writer().unwrap();
    std::thread::sleep(Duration::from_secs(4)); // ccmux + tab1 shell startup

    // Launch claude in tab1 first (the realistic scenario: user already has
    // claude running in tab1 before opening tab2).
    launch_claude(&mut writer);
    std::thread::sleep(Duration::from_secs(10)); // let tab1 claude settle

    // Trust the folder prompt (1 = Yes), then send a real prompt so a
    // conversation starts and Claude Code may rewrite the terminal title
    // to a topic string.
    writer.write_all(b"1\r").unwrap();
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    writer.write_all(b"reply with just the word banana\r").unwrap();
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_secs(45)); // wait for response + any title rewrite

    let _ = use_tab2;

    // Quit ccmux directly (Ctrl+Q is intercepted by ccmux even while a
    // pane app runs) so the dump's final frames show the during-claude label.
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
    let _ = drain.join();
    eprintln!("repro_tab2: done (tab2={})", use_tab2);
}
