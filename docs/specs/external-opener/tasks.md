# tasks.md — external-opener(拡張子別・外部ビューワー起動機構)

## 実装順序

T1 → T2 → T3 → T4 → T5(直列。T2 完了時点から cargo test が回る)

## タスク一覧

### T1: 依存追加

- **内容**: `Cargo.toml` に `toml = "0.8"`、`serde = { version = "1", features = ["derive"] }` を追加
- **依存**: なし
- **完了条件(機械検証)**: `cargo build` が exit 0

### T2: config モジュール新規作成

- **内容**: `src/config.rs` 新規。`Opener { extensions: Vec<String>, command: String, args: Vec<String> }`、
  `Config { openers: Vec<Opener> }`(serde Deserialize)。
  `Config::load() -> (Config, Option<String>)`(戻り値 .1 は警告メッセージ。ファイルなし=空 config・警告なし、
  パース失敗=空 config+警告 = fail-open)。`config_path()` は `dirs::config_dir()/ccmux/config.toml`。
  `find_opener(&self, path: &Path) -> Option<&Opener>`(拡張子を小文字化して比較)。
  `build_args(opener: &Opener, abs_path: &str) -> Vec<String>`(`{file}` 置換・なければ末尾付加)。
  `main.rs` に `mod config;` 追加
- **依存**: T1
- **完了条件(機械検証)**: 新規ユニットテストを含め `cargo test config::` が exit 0。テスト最低 6 件:
  正常 TOML パース / 不正 TOML で fail-open+警告 / find_opener 大文字拡張子マッチ /
  find_opener 未登録 None / build_args {file} 置換 / build_args 自動付加

### T3: App 統合(Enter 分岐+spawn)

- **内容**: `src/main.rs` で `Config::load()` し `App::new()` へ渡す(パース警告は status_message 初期値に)。
  `src/app.rs`: `App` に `config: Config` と `status_message: Option<String>` を追加。
  `handle_file_tree_key` の Enter 分岐(892-901 付近)で、ファイル選択時に `find_opener` ヒットなら
  `spawn_external_opener()`、None または spawn Err なら従来の `preview.load()` 経路。
  `spawn_external_opener`: 絶対パス化(`std::fs::canonicalize`、失敗時は元パス)→ `build_args` →
  `Command` 構築(stdin/stdout/stderr すべて `Stdio::null()`、`#[cfg(windows)]` で
  `creation_flags(0x0800_0000)`)→ spawn 成功時は `std::thread::spawn(move || { let _ = child.wait(); })`。
  結果を status_message にセット(成功: 起動コマンド名 / 失敗: エラー要約)
- **依存**: T2
- **完了条件(機械検証)**: `cargo build` exit 0 かつ `cargo test` 全件 exit 0(既存テスト退行なし)

### T4: ステータスバー表示

- **内容**: `src/ui.rs` の `render_status_bar`(823-880 付近)で `app.status_message` が Some なら
  通常ヒントより優先して表示。`app.rs` のキー入力ハンドラ先頭で status_message を消去
  (次のキー操作で消える方式。タイマー不使用)
- **依存**: T3
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑

### T5: evaluator 静的レビュー+実機確認

- **内容**: evaluator エージェントによる静的レビュー(diff・エッジケース・キー競合・レイアウト整合)。
  指摘があれば修正。その後ユーザー実機確認(下表)
- **依存**: T4
- **完了条件(機械検証)**: `cargo test` 全緑+evaluator 合格報告

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| 登録拡張子で外部起動 | config.toml に md→Typora 登録 → README.md に Enter → Typora 起動・TUI 無事 | ☐ |
| {file} 置換 | args=["{file}"] で対象ファイルが Typora で開く | ☐ |
| {file} なし自動付加 | args=[] で同上 | ☐ |
| 大文字拡張子 | README.MD(テスト用に作成)で外部起動 | ☐ |
| 未登録拡張子 | .rs ファイルに Enter → 従来 TUI プレビュー | ☐ |
| 設定ファイルなし | config.toml をリネーム → 全ファイル従来挙動 | ☐ |
| パース失敗 fail-open | config.toml に不正 TOML → 起動継続+警告表示 | ☐ |
| spawn 失敗フォールバック | command="no-such-cmd" → エラー表示+TUI プレビュー | ☐ |
| ディレクトリ Enter | ディレクトリ展開/折りたたみのみ(従来どおり) | ☐ |

## 実装セッションへの引き継ぎ事項

- rustfmt 非適用(cargo fmt 禁止)。周辺コードに手合わせで整形
- コミットは Conventional Commits+日本語、`git commit -F <file>` で作成
- /init 由来の未コミット変更(.gitignore / CLAUDE.md / .claude/ / examples/)はコミットに含めない
- 実装ブランチ: `local/v0.6.2-patched` 直接(従来パッチと同運用)。PR 切り出しは別途指示
- `.cmd` シム(VS Code)は `code.cmd` と拡張子まで書く必要がある旨を config 例コメントに記載
