# design.md — external-opener(拡張子別・外部ビューワー起動機構)

## 1. 全体構成(条件⑦)

```
main.rs ── Config::load() ──▶ App { config, status_message }
                                 │
filetree Enter (app.rs handle_file_tree_key)
                                 │
                    config.find_opener(&path)
                     ├─ Some(opener) ─▶ spawn_external_opener(opener, &path)
                     │                    ├─ 成功 → status_message 表示・フォーカス維持
                     │                    └─ 失敗 → status_message + preview.load() フォールバック
                     └─ None ──────────▶ preview.load()(従来経路そのまま)
```

## 2. 出力契約(条件④)

| パス | 目的 |
|------|------|
| `src/config.rs` | 新規。`Config`/`Opener` 構造体、`Config::load()`、`find_opener()`、`build_args()` |
| `src/app.rs` | Enter 分岐、`spawn_external_opener()`、`status_message: Option<String>` フィールド |
| `src/ui.rs` | ステータスバーに status_message を優先表示 |
| `src/main.rs` | 起動時 `Config::load()` → `App::new()` へ受け渡し |
| `Cargo.toml` | `toml = "0.8"` / `serde = { version = "1", features = ["derive"] }` 追加 |

設定ファイル: `dirs::config_dir()/ccmux/config.toml`(Windows: `%APPDATA%\ccmux\config.toml`)

## 3. 設計判断(ADR 形式)

### ADR-1: Mermaid 対応 WYSIWYG の実現形

- **案A 外部アプリ起動**: 既存 GUI エディタに委譲。実装小・保守小。TUI の制約を回避
- **案B ブラウザプレビュー内蔵**: HTTP サーバ+Mermaid.js。編集同期まで作ると新規システム級
- **案C TUI 内レンダリング強化**: glow 風装飾は可能だが Mermaid 図と WYSIWYG 編集が原理的に不可
- **決定**: 案A。ユーザー要件「見たまま編集」は外部アプリでのみ充足可能(Phase 1 回答)

### ADR-2: 設定形式

- **案A TOML**: Rust TUI ツールの事実上標準。コメント可。toml+serde の依存追加が必要
- **案B JSON**: serde_json 既存で依存ゼロだがコメント不可・手書きしにくい
- **決定**: 案A(Phase 1 回答)。upstream 受けと設定ファイルとしての書きやすさを優先

### ADR-3: 発火トリガー

- **案A 別キー追加(o 等)**: TUI プレビューと外部起動を使い分け可能だがキーが増える
- **案B Enter で外部起動(マッチ時のみ)**: 操作一貫・キー追加なし。マッチ時は TUI プレビュー不可
- **決定**: 案B(Phase 1 回答)。未登録拡張子・設定なしでは従来どおりプレビューが開くため
  フォールバックは常に存在する

### ADR-4: spawn の安全策

- **案A シェル経由(`cmd /c` / `sh -c`)**: PATHEXT 解決が効くがインジェクション面と黒窓リスク
- **案B Command 直接 + null stdio + CREATE_NO_WINDOW + reaper thread**: エスケープを std に委譲
  (Rust 1.77.2+ BatBadBut 対応)。.cmd は拡張子明記が必要になるがドキュメントで許容
- **決定**: 案B(Phase 2 承認)。TUI 巻き込み防止と安全性を優先

## 4. 参照実装(条件⑥)

- 純粋関数切り出し+テストのパターン: `src/app.rs` の `max_side_panel_width`(40cb9ca)、
  `src/pane.rs` の `next_claude_active`(546e59a)を踏襲
- Enter ハンドラ: `src/app.rs:892-901`(`handle_file_tree_key`)
- ステータスバー: `src/ui.rs:823-880`(`render_status_bar`)
- 設定ディレクトリ解決: 既存依存 `dirs`(Cargo.toml:18)

## 5. データとライフサイクル

- config.toml: ユーザーが手書き → 起動時 1 回読み込み → メモリ保持のみ。書き込みしない
- status_message: 発火時にセット → 次のキー入力またはタイマーで消去(セッション内のみ)
- 個人情報なし

## 6. 障害モードと回復

| 障害 | 振る舞い |
|------|----------|
| 設定ファイルなし | openers 空で正常動作(従来挙動) |
| TOML パース失敗 | fail-open: 警告をステータスバー表示・従来挙動で継続 |
| spawn 失敗(不在/エスケープ拒否) | エラー表示 + TUI プレビューにフォールバック |
| 外部アプリが即死 | 関知しない(spawn 成功=起動成功と扱う。reaper が回収のみ) |
