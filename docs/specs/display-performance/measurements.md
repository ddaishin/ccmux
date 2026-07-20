# measurements.md — 表示の高速化 before/after 計測記録

計測日: 2026-07-21 / 環境: Windows 11 Enterprise・ConPTY (portable-pty ハーネス)・release ビルド

## 手法

`examples/perf_harness.rs`（ローカル計測ツール）が ccmux を ConPTY 内に起動し、初期ペインの
Git Bash に `cat <32MB テキスト>` を注入してバースト出力を発生させ、`CCMUX_PERF_LOG` の
frame/sec 行を解析（latency_us は PTY 出力イベント発生→draw 開始、pty_ev>0 のフレームのみ集計）。
コマンド本文と Enter は分離送信（一括だとブラケットペースト化され bash が実行しない）。

## 結果

| 指標 | before (T1 時点 = 現行 33ms poll) | after (T3+T4) |
|---|---|---|
| latency_us 中央値 | 44,831 (44.8ms) | **15,937 (15.9ms)** |
| latency_us p90 | 46,028 (46.0ms) | **16,853 (16.9ms)** |
| バースト中 fps | 20〜22 | **47〜55** |
| draw_us 中央値 / 最大 | 1,647 / 6,952 | 1,264 / 4,070 |
| 総フレーム数（同一負荷） | 117 | 274 |

- after の latency 中央値 ≈16ms は FramePacer の最小フレーム間隔（60fps 上限）による設計上の下限。
  受け入れ基準「中央値 ≤16ms」を達成（アイドル状態からの単発イベントはペーシング待ちなしで
  さらに短い）
- バースト fps 上限 55 は ConPTY スループット律速（60fps ペーシング上限の内側）
- draw 時間は before から軽微改善（T4 の再計算排除）。支配項は元々ループ起床であり、
  parser.lock スコープ変更（スコープ外項目)を持ち出す必要がないことを draw_us が裏付ける

## 再現手順

```
cargo build --release && cargo build --release --example perf_harness
./target/release/examples/perf_harness.exe ./target/release/ccmux.exe <32MB テキスト> <出力log>
```
