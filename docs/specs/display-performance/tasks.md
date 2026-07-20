# tasks.md — 表示の高速化 (display-performance)

## 実装順序

T1 → T2 → T3 → T4 →（計測後任意で T5）。T1 が先なのは改善前ベースラインを同一手段で採るため。
各タスク＝1 コミット。全タスクで既存 46 テスト＋新規テストが green であること。

## タスク一覧

### T1: perf 計測基盤（コミット `perf: CCMUX_PERF_LOG フレーム計測を追加`）

- **内容**: `src/perf.rs` 新規（`PerfLog::from_env/on_event/on_frame`、frame/sec 行フォーマット関数は
  `format_*_line() -> String` に分離）。現行 `run_event_loop` にフック挿入（ローカル `Option<PerfLog>`）。
  完了後、大量出力（数 MB を cat）でベースライン計測を採取し `docs/specs/display-performance/` にメモ
- **依存**: なし
- **完了条件（機械検証）**: `cargo test` exit 0（perf フォーマットの新規単体テスト含む）/
  `cargo clippy` 新規警告 0 / `CCMUX_PERF_LOG=/tmp/p.log cargo run` でログに frame/sec 行が出力される

### T2: 時間ベース cooldown とペーシング部品（コミット `refactor: cooldown の時間ベース化とペーシング部品`)

- **内容**: `src/timing.rs` 新規（`Cooldown`: arm=max セマンティクス/is_active/take_expired/deadline、
  `FramePacer`: ready/mark_drawn/deadline_if_dirty、`earliest()`）。app.rs の `paste_cooldown`/
  `resize_cooldown: u8` を `Cooldown` に置換（150ms 定数）。ループは 33ms poll のまま
- **依存**: なし（T1 と独立、コミット順は T1 の後）
- **完了条件（機械検証）**: `cargo test` exit 0（Cooldown の max セマンティクス・take_expired 一回性・
  FramePacer の 16ms 境界・earliest の None 混在、各テスト green）/ `cargo clippy` 新規警告 0

### T3: イベント駆動メインループ（コミット `feat: イベント駆動メインループ＋入力専用スレッド`）

- **内容**: `AppEvent::Input` 追加、`spawn_input_thread`（Press フィルタ・detach）、`run_event_loop` を
  recv_timeout＋デッドライン計算＋FramePacer(16ms) に書換、`drain_pty_events` を `handle_app_event` に統合、
  ペーストバーストを 2ms 近接窓で移設
- **依存**: T2
- **完了条件（機械検証）**: `cargo test` exit 0 / `cargo clippy` 新規警告 0 / `cargo build --release` exit 0 /
  `CCMUX_PERF_LOG` 付き実行で大量出力中の `latency_us` 中央値 ≤16000（T1 ベースラインとの比較値を記録）

### T4: 描画パスの無駄排除（コミット `perf: claude monitor I/O を draw 外へ・フレーム内再計算の排除`)

- **内容**: ui.rs の `claude_monitor.update()` 呼び出しを削除し app.rs `update_claude_monitors() -> bool` を
  ループタイマー節へ。`ClaudeMonitor::update` を `-> bool` 化＋`next_check_at()` 追加（デッドライン組込み）。
  `render_terminal_content` の selection.normalized() ホイスト（`TextSelection::contains_in` 切り出し、
  既存 `contains` はラッパ化）。`is_claude_running` を render_single_pane で 1 回取得し引数渡し
- **依存**: T3
- **完了条件（機械検証）**: `cargo test` exit 0（既存 TextSelection/claude_monitor テスト無修正 or 最小修正で
  green）/ `cargo clippy` 新規警告 0 / grep で ui.rs に `claude_monitor.update` と `\.normalized()` の
  セルループ内呼び出しが存在しないこと

### T5（任意・計測後）: cooldown 短縮（コミット `perf: cooldown 時間のチューニング`）

- **内容**: T3/T4 後の perf ログと手動回帰（ペースト・リサイズのフラッシュ）を確認し、150ms を段階短縮
- **依存**: T4 ＋ 手動検証完了
- **完了条件（機械検証）**: `cargo test` exit 0。短縮値でフラッシュ回帰が出た場合は本タスクを破棄

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| PTY 出力で即起床・latency ≤16ms | 大量出力中の perf ログ latency_us 中央値を before/after 比較 | ☐ |
| バースト時 60fps 上限で安定 | perf ログ sec 行の fps ≈60・draw_us 安定 | ☐ |
| キー入力・二重入力なし | 高速タイピングでエコー遅延・重複文字がないこと（Windows） | ☐ |
| ペースト両経路の回帰なし | Windows Terminal と legacy console × PowerShell/cmd で複数行ペースト | ☐ |
| リサイズのフラッシュ防止維持 | リサイズ・split/close・sidebar トグルで旧バッファが見えないこと | ☐ |
| Claude 表示の鮮度維持 | claude 実走でボーダー・トークン集計・todo バーが従来通り更新 | ☐ |
| idle CPU ≈0% | 放置状態でタスクマネージャの CPU がほぼ 0% | ☐ |
| draw 中の I/O なし | コードレビュー（ui.rs に I/O 呼び出しなし）＋ evaluator 静的レビュー | ☐ |

## 実装セッションへの引き継ぎ事項

- `flush_paste_buffer`・`strip_bracket_paste_sequences`・`pty_reader_thread` は無変更で再利用（触らない）
- `mark_layout_change` の max() セマンティクス（app.rs:589-591 コメント）を `Cooldown::arm` が引き継ぐこと
- Press フィルタは入力スレッド側に置く（チャネル流量半減。main 側での再フィルタ不要）
- 完了報告前に evaluator agent の静的レビュー必須（CLAUDE.md Workflow Rules）
- 実運用反映は npm グローバルの `bin/ccmux.exe` 差し替え（実行中なら rename→配置の既知手順）
