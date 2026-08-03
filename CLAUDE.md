> Universal principles are loaded from `~/.claude/rules/` (global).
> This file contains project-specific configuration only.

---

# ccmux — Claude Code Multiplexer

## Overview
Rust TUI tool for managing multiple Claude Code instances in split panes.

## Tech Stack
- Rust (stable), ratatui + crossterm, portable-pty, vt100

## Build & Run
```bash
cargo build          # Debug build
cargo build --release # Release build
cargo test           # Run tests
cargo run            # Run the app
```

## Architecture
- `main.rs` — Entry point, terminal setup, event loop
- `app.rs` — App state, event dispatching, layout tree
- `pane.rs` — PTY management, vt100 terminal emulation, shell detection
- `ui.rs` — ratatui rendering, layout calculation, theme
- `filetree.rs` — File tree sidebar
- `preview.rs` — File preview panel

## Key Design Decisions
- **vt100 crate** for terminal emulation (not ANSI stripping) — needed for Claude Code's interactive UI
- **Binary tree layout** for recursive pane splitting
- **Per-PTY reader threads** with mpsc channel to main event loop
- PTY resize via both `master_pty.resize()` and `vt100_parser.set_size()`

## Shell Detection Priority
- Windows: Git Bash → PowerShell
- Unix: $SHELL → /bin/sh

## Release Process
1. `Cargo.toml` と `npm/package.json` のバージョンを同じ値に揃えて上げる
2. コミット & `git push origin master`
3. `git tag vX.Y.Z && git push origin vX.Y.Z`
4. CI (`.github/workflows/release.yml`) が自動で実行:
   - 4プラットフォーム (Windows x64, macOS x64/arm64, Linux x64) のリリースビルド
   - GitHub Release 作成 + checksums.txt 生成
   - npm publish (Trusted Publishing)
- **手動で `npm publish` や `gh release create` しないこと** — バージョン衝突の原因になる

## Workflow Rules
- **Every implementation must be reviewed by the evaluator agent** before reporting done. This is a Rust TUI app, so Playwright MCP is not available — the evaluator should perform static review (diff analysis, edge cases, logic correctness, key conflict checks, layout math consistency).

## Git運用
- Conventional Commits（`feat:` / `fix:` / `perf:` / `docs:` 等）＋日本語の説明文
- コードスタイル: **rustfmt 非適用**。既存コードは rustfmt と大きく乖離しており、`cargo fmt` を走らせるとファイル全体が再整形され upstream との diff が汚染される（実測済み）。整形は周辺コードへの手合わせで行う
- upstream (Shin-sibainu/ccmux) への PR は fork 経由（権限は READ）。ローカル運用の統合先は `local/v0.6.2-patched` ブランチ

## 現在の状態
- ベース: v0.6.2（upstream master `bdc9e79`）
- `local/v0.6.2-patched` にローカルパッチを統合し、npm グローバルの `ccmux-cli/bin/ccmux.exe` を自前ビルドで差し替え運用中（`npm i -g` で上書きされる点に注意）
  1. `fix/claude-title-detection-v2.1.140` — Claude Code v2.1.140+ の新タイトル形式（`claude` / `✳ Claude Code`）検知（Issue #15）
  2. `fix/wide-char-copy-extra-spaces` — 全角文字コピー時の空白混入修正（vt100 wide continuation セル）
  3. `perf/event-driven-display` — イベント駆動ループ＋入力専用スレッド＋60fps ペーシング（latency 中央値 44.8ms→16ms・仕様書: `docs/specs/display-performance/`）
  4. 拡張子別の外部オープナーとステータスバー修正 — ブランチを切らず `local/v0.6.2-patched` へ直接コミット（`feat(opener)` = OS 既定アプリ委譲・`o` / `r` キー・仕様書 `docs/specs/external-opener/`、`fix(ui)` = 右側情報がキーヒントを覆う問題）
- いずれも upstream 未 PR。PR 時は各ブランチを個別に fork へ push する

### グローバル exe の差し替え手順
実行中の exe は Windows がロックするため `cp` 上書きは失敗する。`ccmux-cli/bin/` で
`mv ccmux.exe ccmux.exe.prev-<id>` と退避してから `cp target/release/ccmux.exe ccmux.exe` を
配置し（退避と配置は 1 コマンドに繋いで欠損窓を作らない）、**sha256 でビルド成果物と照合**する
（サイズ・日時では判定しない — 別ビルドでサイズが一致した実績がある）。退避版がロールバック先。
現存: `ccmux.exe.prev-20260722`（直前の版）/ `ccmux.exe.prev-d7ac14f`（footprint 改修前）。
