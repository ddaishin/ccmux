// Regression tests for doy/vt100-rust#28: a shrinking resize can orphan a
// wide (CJK) character's continuation cell, leaving a cell with the wide
// flag set but no `col+1` neighbor. Untreated, that either panics inside
// Row::clear_wide (index out of bounds) or later at screen.rs's
// drawing_cell_mut(col+1).unwrap() when something draws over the orphaned
// cell. See vendor/vt100/PATCHES.md for the full writeup.

// Minimal reproduction from the upstream issue.
#[test]
fn resize_truncating_wide_char_then_erase_line_does_not_panic() {
    let mut parser = vt100::Parser::new(2, 4, 0);
    parser.process("你".as_bytes());
    parser.screen_mut().set_size(2, 1);
    parser.process(b"\x1b[K");
    assert_eq!(parser.screen().size(), (2, 1));
}

// The orphaned wide cell must actually lose its wide flag and contents, not
// just survive the resize without panicking.
#[test]
fn resize_shrink_clears_orphaned_wide_cell() {
    let mut parser = vt100::Parser::new(2, 4, 0);
    parser.process("你".as_bytes());
    assert!(parser.screen().cell(0, 0).unwrap().is_wide());

    parser.screen_mut().set_size(2, 1);

    let cell = parser.screen().cell(0, 0).unwrap();
    assert!(!cell.is_wide(), "orphaned wide cell must be cleared");
    assert!(!cell.has_contents());
}

// ccmux's actual crash path: the resize happens while the alternate screen
// is active (Claude Code running in the pane), so the `\x1b[2J` that
// ccmux's Pane::resize sends afterward only clears the *alternate* grid —
// Screen::set_size resizes both grids, but the clear only touches whichever
// one is current. The corruption stays in the normal grid until the app
// exits the alternate screen and the shell draws over it.
#[test]
fn resize_in_alternate_screen_leaves_normal_grid_usable() {
    let mut parser = vt100::Parser::new(4, 4, 100);
    parser.process("你好".as_bytes());
    parser.process(b"\x1b[?1049h"); // enter alternate screen (Claude Code)
    parser.screen_mut().set_size(4, 3);
    parser.process(b"\x1b[2J\x1b[H"); // clears the alternate grid only
    parser.process(b"\x1b[?1049l"); // back to the shell's normal grid
    parser.process(b"\x1b[K"); // used to panic in Row::clear_wide
    parser.process("x".as_bytes()); // used to panic at screen.rs's unwrap
}
