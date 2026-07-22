# tasks.md — footprint(メモリ使用量・バイナリサイズ最小化)

## 実装順序

T0 → (T1 → T2 → T3) → T4 → T5 → (T6 → T7 → T8) → T9。
T1〜T3 は profile の積み上げ順(効果切り分け)。T4/T5 は T1 以降なら任意。
T6〜T8 は T0 のベースライン採取後であること。各コミットで measurements.md に実測を追記。

## タスク一覧

### T0: perf: メモリ計測ハーネス mem_harness を追加
- **内容**: examples/mem_harness.rs 新規(perf_harness をコピーして開始・抽象化しない)。
  `child.process_id()` → PowerShell Get-Process で WorkingSet64/PeakWorkingSet64/
  PrivateMemorySize64 を TSV 出力。シナリオ S1 idle(起動→4 秒)/S2 scrollback-fill
  (2 万行 cat→5 秒静定)/S3 tabs(Alt+T×2)。docs/specs/footprint/measurements.md 雛形
  +現行バイナリのベースライン記録
- **依存**: なし
- **完了条件（機械検証）**: `cargo build --release --example mem_harness` exit 0 /
  実行で S1-S3 の TSV 行が出力される / `cargo test` exit 0(全 ~79 件)

### T1: build: release プロファイルに lto・codegen-units・strip を設定
- **内容**: `[profile.release]` に lto="fat"・codegen-units=1・strip="symbols"
- **依存**: T0
- **完了条件**: `cargo build --release` exit 0 / exe サイズ記録 / perf_harness の draw_us
  中央値・latency p90 が基準(1,264µs / 16.9ms)+10% 以内 / `cargo test` exit 0

### T2: build: release プロファイルに panic=abort を設定
- **内容**: panic="abort" 追加。コミットメッセージに catch_unwind 不使用 grep と
  panic フック互換の根拠を記載
- **依存**: T1
- **完了条件**: `cargo build --release` exit 0 / サイズ記録 / `cargo test` exit 0

### T3: build: opt-level の実測 A/B(退行時はコミットしない)
- **内容**: 3/"s"/"z" の 3 ビルドでサイズ+perf_harness を計測。+10% 以内で最小サイズの値を
  採用。3 が勝てば不採用を measurements.md に記録のみ
- **依存**: T2
- **完了条件**: 採用時 `cargo build --release` exit 0+ゲート通過値の記録 / 不採用時は記録のみ

### T4: build: syntect を regex-fancy 構成へ切替(onig 除去)
- **内容**: `syntect = { version="5.3", default-features=false, features=["parsing","default-syntaxes","default-themes","regex-fancy"] }`
- **依存**: T1
- **完了条件**: `cargo tree -i onig` が「nothing to print」相当 / `cargo test` exit 0
  (test_preview_highlight_rust 含む)/ サイズ記録

### T5: build: arboard/ratatui の未使用 feature を削減
- **内容**: arboard `default-features=false`、ratatui `default-features=false,
  features=["crossterm","underline-color","layout-cache"]`。効果 <1% の項目は revert し記録
- **依存**: T1
- **完了条件**: `cargo build --release` exit 0 / `cargo tree -i time` が空 / `cargo test` exit 0 / サイズ記録

### T6: feat(config): scrollback_lines 設定を追加(既定 10000 のまま=挙動不変)
- **内容**: config.rs にキー+手書き impl Default+クランプ(上限 100,000)+テスト 5 件
  (不在→既定 / 明示値 / 0 / 100,001→クランプ / 負値→fail-open)。main.rs で読み込みを
  App::new 前へ移動、app.rs / pane.rs へ引き回し。CONFIG_SKEL と dist の config.toml.sample 追記
- **依存**: T0
- **完了条件**: `cargo test` exit 0(新規 5 件 green)/ mem_harness S2 が before と ±5%(挙動不変の証明)

### T7: feat(config)!: scrollback 既定を 2000 に変更
- **内容**: default_scrollback_lines を 2000 へ+CONFIG_SKEL の既定値表記更新(bisect 可能な 1 論理変更)
- **依存**: T6
- **完了条件**: `cargo test` exit 0 / mem_harness S2 で Δ≈30MB 減を記録

### T8: perf(preview): SyntaxSet/ThemeSet をプロセス共有・遅延初期化
- **内容**: Preview の syntax_set/theme_set フィールドを削除し `OnceLock<(SyntaxSet, ThemeSet)>`
  の static + highlight_assets() へ
- **依存**: T6
- **完了条件**: `cargo test` exit 0(preview 5 件)/ mem_harness S1・S3 で減少を記録

### T9: docs: footprint 施策の before/after 計測まとめ
- **内容**: measurements.md 最終表(サイズ 6,297,600→実測、RSS 各シナリオ)+不採用判断
  (ureq json / アロケーション削減該当なし / T3・T5 の実測落選があれば)の記録
- **依存**: T0〜T8
- **完了条件**: 表に全コミットの実測値が揃っている

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| profile 適用 | `cargo build --release` 後 exe サイズが基準比減少 | ☐ |
| onig 除去 | `cargo tree -i onig` が空 | ☐ |
| time 除去 | `cargo tree -i time` が空 | ☐ |
| scrollback 既定 2000 | config 無しで S2 実測が旧版比 ≈30MB 減 | ☐ |
| scrollback 設定反映 | `scrollback_lines = 5000` の設定テストが green | ☐ |
| 0/クランプ/負値 | config テスト 5 件 green | ☐ |
| syntect 共有・遅延 | S1(未プレビュー起動)・S3(タブ×3)で減少 | ☐ |
| ハイライト互換 | test_preview_highlight_rust green | ☐ |
| 性能ゲート | perf_harness draw 中央値・latency p90 +10% 以内 | ☐ |

## 実装セッションへの引き継ぎ事項

- rustfmt 非適用(周辺コードへ手合わせ)。コミットは日本語+`git commit -F <file>`
- ConPTY ハーネスの罠: CCMUX env_remove・コマンド本文と Enter の分離送信(200-300ms)・
  reader 常時 drain+dump 保存(知見 conpty-tui-harness-nested-guard-and-paste-trap)
- A/B はバイナリ 2 本比較(config 注入不可: dirs::config_dir は env で曲げられない)
- dist/ の zip 再生成はリリース工程側。コミットには config.toml.sample 更新のみ含める
- 実装完了後に evaluator エージェントの静的レビュー必須(CLAUDE.md 規約)
