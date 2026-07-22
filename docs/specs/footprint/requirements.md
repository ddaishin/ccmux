# requirements.md — footprint(メモリ使用量・バイナリサイズ最小化)

## 1. 背景と目的

release exe は 6,297,600 bytes で `[profile.release]` が未設定。メモリは vt100 scrollback
10,000 行/ペイン固定(満杯時 ≈38.4MB/ペイン = Cell 32B × 120 桁 × 行数)が支配項で、
syntect の SyntaxSet/ThemeSet がタブごとに複製されている。描画性能(直前のイベント駆動
+60fps 化)を退行させずに両フットプリントを削減する。

## 2. 利用者と保守者（条件③）

- 利用者: doi 本人(日常の Claude Code 多重起動で常用)
- 保守者: doi 本人(local/v0.6.2-patched で運用。upstream 追従時の diff 最小化に配慮)

## 3. 成功基準（条件①）

- バイナリサイズとメモリ(RSS)の before/after 実測値を `measurements.md` に提示できる
- 数値の必達目標は置かない。効果 <1% の施策は不採用と判断し、その記録を残す
- 描画性能ゲート: perf_harness の draw_us 中央値(基準 1,264µs)・latency p90(基準
  16.9ms)が +10% 以内であること(超えた施策は不採用)
- `cargo test` 全件(約 79 件+新規)が green であること

## 4. 受け入れ基準（EARS 記法）

### バイナリ
- WHEN `cargo build --release` を実行する THE SYSTEM SHALL lto="fat"・codegen-units=1・
  strip="symbols"・panic="abort" が適用された exe を生成する
- WHEN T4 適用後に `cargo tree -i onig` を実行する THE SYSTEM SHALL 「nothing to print」相当
  (onig 非依存)を出力する
- WHEN T5 適用後に `cargo tree -i time` を実行する THE SYSTEM SHALL time crate 非依存を出力する

### scrollback 設定
- WHEN config.toml に `scrollback_lines` が無い THE SYSTEM SHALL 既定値 2000 を使用する
- WHEN `scrollback_lines = 5000` が設定されている THE SYSTEM SHALL 新規ペインの vt100 Parser を
  scrollback 5000 行で生成する
- WHEN `scrollback_lines = 0` が設定されている THE SYSTEM SHALL 履歴なしで動作し、スクロール
  バックが総行数 > 表示行数にならない限り表示されない従来条件を維持する
- WHEN `scrollback_lines` が 100,000 を超える THE SYSTEM SHALL 100,000 にクランプして使用する
- WHEN `scrollback_lines` に負値や型不一致が設定されている THE SYSTEM SHALL 既存の fail-open
  経路(config 全体を既定値+status_message へ警告)で起動を継続する
- WHEN 起動後に config.toml を変更する THE SYSTEM SHALL 既存 config と同様に再起動まで反映しない

### syntect 共有化・遅延化
- WHEN テキストプレビューを一度も開かずに起動・終了する THE SYSTEM SHALL SyntaxSet/ThemeSet を
  ロードしない
- WHEN タブを N 個開いてプレビューを使用する THE SYSTEM SHALL SyntaxSet/ThemeSet をプロセス全体で
  1 セットのみ保持する
- WHEN Rust ファイルをプレビューする THE SYSTEM SHALL 既存テスト(test_preview_highlight_rust)を
  満たすハイライトを返す

### 計測
- WHEN `mem_harness <exe> <bigfile> <scenario>` を実行する THE SYSTEM SHALL S1(idle)/
  S2(scrollback-fill)/S3(tabs) の WorkingSet64/PeakWorkingSet64/PrivateMemorySize64 を
  TSV で出力する

## 5. 制約（条件⑤）

- 技術制約: Rust stable / Windows(win32) / rustfmt 非適用(周辺コードへ手合わせ)/
  コミットは Conventional Commits+日本語+`-F` ファイル経由
- 挙動変更は「軽微」まで: scrollback 既定変更・panic=abort(バックグラウンドスレッド panic 時に
  アプリ全体終了へ変化)は許容。機能削除は不可
- 法令/規程: 該当なし(個人情報を扱わない)
- 統合先: local/v0.6.2-patched のみ(upstream PR は今回スコープ外)

## 6. スコープ外

- 毎フレームアロケーション削減(設計調査で「効果大」該当なしと判断。ui.rs:333 rects.clone /
  ui.rs:1002 truncate_to_width を将来候補として measurements.md に記録するのみ)
- ureq "json" feature 外し(serde_json が直接依存で残るため効果ほぼゼロ)
- image crate のデコーダ削減(機能削除に該当)
- upstream への PR 分割・提出

## 7. 未知の未知の決定事項（Phase 2 の承認結果）

- Phase 2 は省略(規模判定: 小機能。ユーザー承認済み)
