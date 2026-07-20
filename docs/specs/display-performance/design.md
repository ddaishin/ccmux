# design.md — 表示の高速化 (display-performance)

## 1. 全体構成（条件⑦）

```
input thread (新規): crossterm event::read() ──┐ Press フィルタ
pty reader threads (既存): read 4096B ─────────┼─→ mpsc<AppEvent> ─→ main loop:
                                               │     recv_timeout(次デッドライン)
claude_monitor / filetree / cooldown (タイマー)┘     → try_recv 全 drain (coalesce)
                                                     → タイマー処理 → FramePacer(16ms) 付き draw
```

メインループの起床源を mpsc 1 本に統合し、`event::poll(33ms)` を廃止する。デッドラインは
min(cooldown 期限, filetree 2s, claude_monitor 500ms, dirty 時の next_frame, now+1s 保険)。

## 2. 出力契約（条件④）

| パス | 目的 |
|------|------|
| `src/timing.rs`（新規） | `Cooldown` / `FramePacer` / `earliest()`。全 API が `now: Instant` 引数で単体テスト可能 |
| `src/perf.rs`（新規） | `CCMUX_PERF_LOG` 計測（`PerfLog::from_env/on_event/on_frame`、frame/sec 行のテキスト出力） |
| `src/main.rs` | `run_event_loop` 全面書換、`spawn_input_thread`・`handle_app_event` 新設 |
| `src/app.rs` | `AppEvent::Input` 追加、cooldown を `Cooldown` に置換、`update_claude_monitors() -> bool`、`TextSelection::contains_in` |
| `src/ui.rs` | draw 内の monitor.update 除去、normalized() ホイスト、is_claude_running 1 回化 |
| `src/claude_monitor.rs` | `update() -> bool` 化、`next_check_at()` 追加 |

## 3. 設計判断（ADR 形式）

### ADR-1: イベントループの改修方式

- **状況**: PTY 出力で起床できない `event::poll(33ms)` が全症状の根因
- **案A**: 入力専用スレッド＋チャネル統合＋recv_timeout（ratatui 公式推奨パターン）
- **案B**: poll 短縮（5〜10ms 適応制御）。変更極小だが最大 10ms 遅延が残り idle CPU 増
- **決定**: 案A（ユーザー選択）。レイテンシ根治。detach 制約は既知の標準トレードオフ

### ADR-2: イベント型

- **案A**: 既存 `AppEvent` に `Input(crossterm::event::Event)` を追加（チャネル 1 本・順序保証が自然）
- **案B**: 別 enum ＋ crossbeam select（型分離できるが依存追加・複雑化）
- **決定**: 案A。std::sync::mpsc に select はなく、依存追加は制約⑤に反する

### ADR-3: ペーストバースト検出の移設先

- **状況**: 現行の nested `event::poll(1ms)` はメインループから poll が消えるため移設必須
- **案A**: 入力スレッド内で収集 → `handle_key_event` の consumed 判定（App 状態必要）が再現不能
- **案B**: メインループ側 `recv_timeout(2ms)` 近接窓 → 現行セマンティクス（consumed 時の先行 flush 等）をそのまま移植可能
- **決定**: 案B。窓 2ms はチャネルホップ 1 段の余裕（ペーストは µs 間隔・タイピングは ≥30ms で誤結合なし）。窓中の PtyOutput は dirty のみ立てて窓継続（現行の「非 Key で break」より正しい）

### ADR-4: cooldown の表現

- **案A**: フレーム数 u8 維持 → イベント駆動でループ周期が不定になり時間が不定化（採用不可）
- **案B**: `Cooldown`（Instant 期限、arm は max() で既存期限を縮めない＝app.rs:589-591 のセマンティクス維持）
- **決定**: 案B・150ms（現行 5×33ms≈165ms と同等の保守値）。短縮は perf 計測後に別コミット

## 4. 参照実装（条件⑥）

- 入力スレッド＋mpsc パターン: https://ratatui.rs/concepts/event-handling/（公式推奨）
- Windows Press/Release 二重イベント: https://ratatui.rs/faq/
- 既存踏襲: `flush_paste_buffer`（main.rs）・`strip_bracket_paste_sequences`（app.rs）は無変更で再利用。
  `pty_reader_thread`（pane.rs）の送信側も無変更（`AppEvent` 追加はバリアントのみ）

## 5. データとライフサイクル

- perf ログ: `CCMUX_PERF_LOG=path` 指定時のみ生成。数値のみ（個人情報なし）。BufWriter 64KB・1s ごと flush。
  ローテーションなし（計測セッション限りの使い捨て、ユーザーが削除）
- チャネル: unbounded mpsc（現行同等）。バースト時も drain で吸収し滞留しない

## 6. 障害モードと回復

- 入力スレッドの `event::read()` エラー → 50ms sleep でスピン防止し継続。`tx.send` 失敗（rx drop）→ スレッド終了
- `recv_timeout` Disconnected → 実質発生しない（App が tx 保持）が、防御的にループ break
- perf ログ書き込み失敗 → 計測を諦めるだけで本体動作に影響させない（write 失敗は握りつぶし）
- 入力スレッドは detach のため異常時も join しない。raw mode 解除は main の既存 cleanup/panic hook が担保
