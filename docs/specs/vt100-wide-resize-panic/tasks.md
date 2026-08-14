# tasks.md — vt100-wide-resize-panic

> 各タスクに機械検証可能な完了条件を付与。T1/T2 は vendoring 非依存で先に独立 PR 可能。

## 実装順序

T1 → T2（並行可）→ T3（T1 完了後、vendoring 本体）→ T4（統合検証）

## タスク一覧

### T1: `clamp_pty_size` を追加し PTY/vt100 入力の最小サイズを保証する

- **内容**: `src/pane.rs` に `MIN_PTY_ROWS=1` / `MIN_PTY_COLS=2` と
  `fn clamp_pty_size(rows: u16, cols: u16) -> (u16, u16)` を追加。
  `Pane::new_with_cwd`（`:47` 冒頭で引数をシャドウ）と `Pane::resize`（`:172-183`、既存の
  `rows==0||cols==0` 早期 return の直後・dedup 判定の前）に適用。`last_rows`/`last_cols` には
  clamp 後の値を保存する
- **依存**: なし
- **完了条件（機械検証）**: `cargo test clamp_pty_size` が新規追加した以下のケースを含めて
  exit 0 — `clamp_pty_size(10,1)==(10,2)` / `clamp_pty_size(10,0)==(10,2)` /
  `clamp_pty_size(0,40)==(1,40)` / `clamp_pty_size(10,40)==(10,40)`（恒等）。加えて
  `next_pty_size`（`resize` のサイズ決定部を切り出した純関数）の dedup テスト:
  同一の生値を 2 回渡すと 2 回目は `None`（実処理をスキップ）を返すこと

### T2: PTY reader スレッドの命名とクラッシュログ

- **内容**: `src/pane.rs:107-117` の `thread::spawn` を
  `thread::Builder::new().name(format!("ccmux-pty-{}", id)).spawn(...)` に変更し
  `.context("Failed to spawn PTY reader thread")?` を付与。`src/main.rs:44-52` の panic hook に
  `write_crash_record` を追加（`dirs::config_dir()` はフック生成前に解決して move、
  `default_hook` 呼び出しの前に記録、1MB 超で切り詰め、`O_APPEND` で 1 回の `write_all`）
- **依存**: なし（T1 と並行可）
- **完了条件（機械検証）**: `cargo build` が成功し `Pane::new_with_cwd` の戻り値型が変わらず
  既存呼び出し元（`app.rs`）が無修正でコンパイルが通ること。手動: release ビルドに一時的に
  `panic!("hook test")` を仕込んで起動し、`%APPDATA%\ccmux\crash.log` に
  `thread=ccmux-pty-` を含む 1 レコードが追記され、端末が正常復元されることを確認
  （確認後、仕込んだ `panic!` は削除する）

### T3: vt100 を vendored 化し PR #30 相当のパッチを当てる

- **内容**: `~/.cargo/registry/src/index.crates.io-*/vt100-0.16.2/` から `src/*.rs` 8 ファイル
  + LICENSE/README/CHANGELOG を `vendor/vt100/` へコピー（`Cargo.toml.orig`/`Cargo.lock`/
  `.cargo_vcs_info.json`/`.cargo-ok` は持ち込まない）。`Cargo.toml` は `Cargo.toml.orig` の
  内容を採用し `[dev-dependencies]` と `include` を削除（**`autotests=false` を残さないこと
  — 残すと `tests/` が無言で無視される**）。`vendor/vt100/src/row.rs` の `erase`/`resize`/
  `clear_wide` を upstream PR #30 の diff で改変し、同 PR のインラインテストも取り込む。
  `vendor/vt100/tests/wide_resize.rs` を新規作成（Issue #28 最小再現・孤立セルの is_wide
  解除確認・ccmux 実経路の 3 テスト）。`vendor/vt100/patches/0001-*.patch` に diff を保存し
  `vendor/vt100/PATCHES.md` に由来・根拠・既知の未修正・再適用手順を記載。ルート
  `Cargo.toml` 末尾に `[workspace] members=["vendor/vt100"]` と
  `[patch.crates-io] vt100={path="vendor/vt100"}` を追加
- **依存**: T1（clamp が先に入っていないと `wide_resize.rs` の一部シナリオが T1 未適用の
  ccmux 側実装を前提にしないよう独立して成立するか要確認。実際には vendored テストは
  vt100 単体で完結するため厳密な依存ではないが、統合検証の順序として T1 の後に置く）
- **完了条件（機械検証）**: `cargo test -p vt100` が `0 tests` ではなく実テスト名を出力し
  green（**`autotests=false` 罠の検知を兼ねる**）。ルート `cargo test` で既存 95 テスト +
  T1 の新規テストが green。`cargo tree -p ccmux -i vt100` の出力に `vendor/vt100` パスが
  含まれること。`git diff Cargo.lock` に vt100 の `source`/`checksum` 行削除が含まれること

### T4: 統合検証と手動回帰

- **内容**: T1〜T3 適用後のバイナリで手動シナリオを実施
- **依存**: T1, T2, T3
- **完了条件（機械検証 + 手動）**:
  - `cargo build --release` が成功
  - 手動(Issue #37 経路): 3 分割して 1 ペイン幅を 3 桁程度まで縮小 → `echo あ` → プロセス生存
  - 手動(Issue #28 経路): ペインで Claude Code 起動 → 端末幅を縮小 → Claude 終了 →
    `echo 你` と `clear` → プロセス生存
  - `cargo clippy --all-targets` が新規警告を出さない（vendored crate は対象外でよい）

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| 幅1未満計算時にPTY/vt100へ最低2桁を渡す | `cargo test clamp_pty_size` | ☐ |
| 幅1〜2桁+全角文字でpanicしない | 手動シナリオ(T4, Issue #37) | ☐ |
| alternate screen中リサイズ後の描画でpanicしない | 手動シナリオ(T4, Issue #28) / `cargo test -p vt100 resize_in_alternate_screen` | ☐ |
| 同一サイズ連続時にリサイズ処理を再実行しない | `cargo test next_pty_size` dedup ケース | ☐ |
| 未知panic時にcrash.logへ記録し端末復元 | T2の手動確認（panic!仕込みテスト） | ☐ |
| crash.logが1MB超で切り詰められる | `write_crash_record`のユニットテスト（1MB超ファイルを用意し実行後サイズが縮小することを確認） | ☐ |

## 実装セッションへの引き継ぎ事項

- **触ってはいけない領域**: `Cargo.toml` の `[profile.release]`（lto/codegen-units/strip/
  panic=abort/opt-level）は一切変更しない。`Screen::text` 側（vt100 本体）にガードを足さない
- **使用してよい既存資産**: `unwrap_or_else(|e| e.into_inner())` の Mutex 復旧パターン、
  `.context(...)?` のエラー処理スタイル
- **やらないと決めたこと**: `catch_unwind`、リサイズ時の両 grid クリア回避策、scrollback
  行のリサイズ対応、バックトレース収集（`design.md` 6章・`requirements.md` 6章参照）
- **コミット分割**: T1・T2 は upstream (Shin-sibainu/ccmux) へ fork 経由で PR 可能。T3
  （vendoring）は `local/v0.6.2-patched` にのみ統合し upstream には出さない
- **vendored コードのスタイル**: `vendor/vt100/src/row.rs` は upstream の rustfmt スタイルに
  合わせる（ccmux 本体の rustfmt 非適用ルールは vendored ディレクトリには適用しない）
- 実装完了後、CLAUDE.md の運用ルールに従い evaluator エージェントの静的レビューを通すこと
