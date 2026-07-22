# design.md — footprint(メモリ使用量・バイナリサイズ最小化)

## 1. 全体構成（条件⑦）

変更は 3 系統が独立: (a) Cargo.toml のビルド設定・feature(T1〜T5)、(b) scrollback の
config 化(config.rs→main.rs→app.rs→pane.rs の引き回し、T6〜T7)、(c) preview.rs の
syntect 共有化(T8)。計測系(examples/mem_harness.rs + docs/specs/footprint/)が全体を挟む。

## 2. 出力契約（条件④）

| パス | 目的 |
|------|------|
| Cargo.toml | [profile.release] 追加・syntect/arboard/ratatui の feature 変更 |
| src/config.rs | `scrollback_lines` キー・バリデーション・CONFIG_SKEL 追記・テスト 5 件 |
| src/main.rs | config 読み込みを App::new 前へ移動し受け渡し |
| src/app.rs | Workspace::new・分割時 Pane 生成への scrollback 引き回し |
| src/pane.rs | pane.rs:88 `vt100::Parser::new(rows, cols, 10000)` の引数化 |
| src/preview.rs | SyntaxSet/ThemeSet を OnceLock の process-wide static へ |
| examples/mem_harness.rs | ConPTY 起動+PowerShell Get-Process で RSS 計測(新規) |
| docs/specs/footprint/measurements.md | 全コミットの before/after 実測と不採用判断の記録 |
| dist/ccmux-0.6.2-patched-win-x64/config.toml.sample | CONFIG_SKEL と同期 |

## 3. 設計判断（ADR 形式）

### ADR-1: release プロファイル
- **状況**: profile 未設定(全て既定)。サイズ・性能の両立が必要
- **案A**: lto="fat"+codegen-units=1+strip="symbols"+panic="abort"
- **案B**: lto="thin" 等の控えめ設定(ビルド時間優先)
- **決定**: 案A。lto fat+cu=1 はサイズと性能の両方に効く。panic="abort" は
  `catch_unwind` 不使用を grep で確認済み・main.rs:45-52 の端末復旧 panic フックは abort 前に
  実行されるため互換。挙動変化(バックグラウンドスレッド panic→全体終了)は「軽微」承認範囲

### ADR-2: opt-level
- **状況**: 既定 3。"z"/"s" はサイズに効くが描画性能とトレードオフ
- **案A**: 3/"s"/"z" を実測 A/B し、perf_harness +10% 以内で最小サイズの値を採用
- **案B**: 無条件で "z"(サイズ最優先)
- **決定**: 案A。成功基準が「性能退行 +10% 以内」なので実測でゲートする。退行時は 3 のまま
  (=コミット自体を作らず measurements.md に不採用記録)

### ADR-3: syntect の正規表現エンジン
- **状況**: 既定 feature で onig(C ライブラリ)+plist/yaml-rust/html が同梱される
- **案A**: `default-features=false, features=["parsing","default-syntaxes","default-themes","regex-fancy"]`
- **案B**: 現状維持(onig)
- **決定**: 案A。使用 API(load_defaults_newlines / ThemeSet::load_defaults /
  easy::HighlightLines)は全て残る feature でカバー(preview.rs:7-9,51-52,203 で確認済み)。
  fancy-regex の方言差によるハイライト微差は 500 行上限の閲覧用途で許容。問題時は
  regex-onig へ 1 行戻し

### ADR-4: scrollback 設定の形
- **状況**: vt100 Parser 生成時固定。既定 10,000 は過大
- **案A**: トップレベルキー `scrollback_lines`、既定 2000、0 許容、100,000 クランプ、
  負値/型不一致は既存 fail-open。`#[derive(Default)]` を手書き impl Default 化
- **案B**: [performance] セクション新設 / 実行時変更対応
- **決定**: 案A。既存 config は全て起動時読み込みで一貫しており実行時変更は過剰。
  トップレベルは既存 Opener(配列テーブル)と衝突しない。derive のままだと不在時 0 に
  なるため手書き Default で serde default と一致させる

### ADR-5: メモリ計測方式
- **状況**: before/after を再現可能に取る必要。config 注入は dirs::config_dir が環境変数で
  曲げられずユーザー実 config を汚すリスク
- **案A**: mem_harness(perf_harness コピー)+PowerShell Get-Process、A/B はバイナリ 2 本比較
- **案B**: windows-sys 依存追加で API 直叩き / 手動タスクマネージャー
- **決定**: 案A。依存追加ゼロ・実 config 非汚染。ConPTY の既知の罠(CCMUX env_remove・
  本文と Enter 分離送信・reader 常時 drain+dump)を踏襲する

## 4. 参照実装（条件⑥）

- examples/perf_harness.rs + docs/specs/display-performance/measurements.md(計測様式)
- src/config.rs の Opener/CONFIG_SKEL/テスト群(設定追加の様式)

## 5. データとライフサイクル

- 計測 TSV は docs/specs/footprint/measurements.md に転記(個人情報なし)
- config.toml はユーザー管理(%APPDATA%\ccmux)。ハーネスは読み込ませない(バイナリ A/B)

## 6. 障害モードと回復

- config 不正値 → 既存 fail-open(既定値+status_message 警告)で起動継続
- fancy-regex でハイライト崩れ → preview テストで検知、regex-onig へ 1 行 revert
- opt-level/feature 削減で性能退行 → perf_harness ゲートで検知し不採用(コミットしない/revert)
- panic=abort でスレッド panic が全体終了化 → 端末復旧フックは維持される(main.rs:45-52)
