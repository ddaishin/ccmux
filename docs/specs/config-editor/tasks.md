# tasks.md — config-editor(config.toml をテキストエディタで開く機能)

コミットは2本に分ける(1コミット1論理変更)。T1-T3 はリファクタ(挙動不変)、T4-T9 は機能本体。

## コミット1: `refactor` — 継ぎ目の抽出(挙動不変。既存テストが無改修で緑なのが完了条件)

### T1: `Pane::new_with_command` の新設

- **内容**: `src/pane.rs`。`PaneCommand { Shell, Program { program, args } }` enum を新設し、
  `new_with_cwd` の PTY 生成・`clamp_pty_size`・reader スレッド生成・vt100 Parser 生成を
  private ヘルパへ切り出して両者から呼ぶ。`--login` 付与・OSC 7 プロンプトフック注入・
  `clear` 送出は `Shell` 分岐にのみ残す。`new_with_command(id, rows, cols, event_tx, program,
  args, cwd, scrollback)` を公開
- **完了条件(機械検証)**: `cargo test` 全緑(既存テスト無改修)。
  `grep -n "login\|__ccmux_osc7" src/pane.rs` で該当箇所が `Shell` 分岐内にのみ存在することを確認

### T2: 分割・クローズ経路の一般化

- **内容**: `src/app.rs`。`can_split(&self, direction) -> Result<(), String>` と
  `attach_split_pane(&mut self, pane: Pane, direction)` を `split_focused_pane` から抽出。
  既存 `split_focused_pane` はこれらを呼ぶ薄いラッパに書き換え、`Ctrl+D`/`Ctrl+E` は
  従来どおり失敗時に無言 return する。`close_focused_pane` も pane_id 指定で閉じられる
  形へ一般化し、既存呼び出し(`Ctrl+W`)はそのラッパにする
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑(既存テスト無改修)。
  `Ctrl+D`/`Ctrl+E`/`Ctrl+W` の挙動差分がないことを実装で確認

### T3: `expand_file_args` の抽出

- **内容**: `src/config.rs`。`build_args` の `{file}` 置換ロジックを
  `pub fn expand_file_args(args: &[String], abs: &str) -> Vec<String>` として抽出し、
  `build_args` はそれに委譲する。`ensure_skel_at` を `pub(crate)` 化する
- **完了条件(機械検証)**: `cargo test config::` 全緑(既存 `build_args` テスト無改修)

## コミット2: `feat(editor)` — 機能本体

### T4: `EditorConfig` の追加

- **内容**: `src/config.rs`。`EditorConfig { command: Option<String>, args: Vec<String>,
  in_pane: bool }` を新設し `Config.editor: EditorConfig` を追加(`#[serde(default)]`)。
  `in_pane` は `default_true()`(既存ヘルパ)を使用。`impl Default for EditorConfig` と
  `impl Default for Config` を手書きで更新。`Config::apply_reload` に
  `self.editor = new.editor;` を追加(ドキュメントコメントの「リロード対象を増やすときは
  必ずこのメソッドに追加すること」に従う)。`CONFIG_SKEL` へ `[editor]` の説明を
  **全行コメント**で追加(GUI エディタなら `in_pane = false`、`{file}` 規約、`e` キー、
  終了で自動リロードされる旨)
- **完了条件(機械検証)**: `cargo test config::` 全緑。新規テスト最低4件
  (`[editor]` 省略時に `command=None, in_pane=true` / `command` 指定時のパース /
  `apply_reload` が `editor` を差し替え `scrollback_lines` を据え置くことの確認 /
  `test_config_skel_parses_to_empty` が `[editor]` 追記後も緑のまま)

### T5: `editor_action` 純粋関数の新設

- **内容**: `src/app.rs`。`EditorAction { Pane { program, args }, Detached { program, args },
  OsDefault, NoConfigPath }` enum と `editor_action(editor: &EditorConfig, config_path:
  Option<&Path>, visual: Option<&str>, editor_env: Option<&str>) -> EditorAction` を
  `os_default_action` の直後に追加。環境変数は呼び出し側で読んで引数として渡す(関数自体は
  FS・spawn に触れない)
- **完了条件(機械検証)**: `cargo test` で5分岐(config_path=None -> NoConfigPath /
  command設定+in_pane=true -> Pane / command設定+in_pane=false -> Detached /
  command未設定+VISUAL設定 -> Pane / どちらも未設定 -> OsDefault)が緑

### T6: `e` キーとエディタペイン生成の統合

- **内容**: `src/app.rs`。`handle_file_tree_key` に `e`(修飾キーなし限定ガード)を追加。
  押下時: `config_path()` 解決 -> `ensure_skel_at` で雛形生成 -> `editor_action` で分岐 ->
  `Pane` なら `can_split`/`attach_split_pane`(T2 の成果物)で現タブを上下分割し
  `new_with_command`(T1 の成果物)でペイン生成、`focus_target = FocusTarget::Pane` に切替、
  `editor_panes.insert(pane_id)` / `Detached` なら `spawn_detached` を再利用 / `OsDefault` なら
  `has_default_handler` 判定を経て `spawn_detached` / `NoConfigPath` なら何もせずステータス表示。
  分割不能時(`can_split` が `Err`)は理由をステータス表示。`App` に
  `editor_panes: HashSet<usize>` フィールドを追加
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑

### T7: PtyEof 結線と自動リロード

- **内容**: `src/app.rs`。既存 `AppEvent::PtyEof` ハンドラのペイン探索ループの前で
  `editor_panes.remove(&pane_id)` を評価し、ループ後にエディタペインだった場合のみ
  pane_id 指定クローズ(T2 の成果物)とリロードを行う。`reload_config` を
  `reload_config_message(&mut self) -> String` へ分割して再利用し、エディタ経路は
  `format!("エディタ終了 - {}", msg)` を `status_message` へセットする
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑。手動確認は T9 で実施

### T8: ヒント表示の更新

- **内容**: `src/ui.rs`。`focus_hints` の FileTree 分に `e` + ` 編集 ` を追加
  (`o 外部` `r 再読` の直後に配置)。`test_focus_hints_width_budget` の FileTree 上限を
  68 -> 75 に更新(根拠は design.md ADR-8。今回は端末幅との整合是正はスコープ外)
- **完了条件(機械検証)**: `cargo test ui::` 全緑

### T9: ドキュメント同期 + evaluator 静的レビュー + 実機検証

- **内容**: `CONFIG_SKEL` の内容を `dist/ccmux-0.6.2-patched-win-x64/config.toml.sample` へ
  手動同期。`README.md` の File tree mode 表と `dist/.../README.txt` に `e` を追加
  (`o`/`r` の記載漏れも併せて解消)。evaluator エージェントの静的レビュー後、下表を実機確認
- **完了条件**: `cargo test` 全緑 + evaluator 合格 + 下表の統合検証が全て済み

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| GUI エディタ(別ウィンドウ) | `[editor] command="notepad.exe"` + `in_pane=false` -> `e` -> notepad が別ウィンドウで開く | [ ] |
| CLI エディタ(ペイン内) | `[editor] command="vim"`(`in_pane` 省略=既定true) -> `e` -> 下ペインで vim が起動、キー入力が届く | [ ] |
| 終了で自動リロード | 上記 vim で `[editor]` 以外の値を編集し `:wq` -> ペインが自動で閉じ「エディタ終了 - ...」が表示され変更が反映される | [ ] |
| 環境変数フォールバック | `[editor]` 未設定 + `$env:EDITOR="vim"` -> `e` -> ペインで vim が開く | [ ] |
| OS 既定アプリフォールバック | `[editor]` 未設定 + 環境変数なし + `.toml` の関連付けあり -> `e` -> 既定アプリで開く | [ ] |
| 既定アプリ未登録 | `[editor]` 未設定 + 環境変数なし + `.toml` の関連付けなし -> `e` -> 何も起動せず「既定アプリ未登録」表示(誤った成功表示を出さない) | [ ] |
| 設定パス不明 | (環境依存につき可能なら)`dirs::config_dir()` が None の状況を再現し `e` -> ステータス表示のみ | [ ] |
| 設定ファイル不在から開く | config.toml を削除して `e` -> 雛形生成後にエディタが開く | [ ] |
| 分割不能時の表示 | ペインを MAX_PANES まで増やしてから `e` -> エラー理由がステータス表示される | [ ] |
| Ctrl+W での手動クローズ | vim ペインを `Ctrl+W` で閉じる -> panic せず、直後の PtyEof(もしあれば)も安全に処理される | [ ] |
| 設定なし環境で不変 | config.toml をリネームして起動 -> `o`/`r`/`Enter` の既存挙動が完全に不変 | [ ] |
| ヒント表示 | ファイルツリーフォーカスで `e 編集` がヒント行に表示される | [ ] |

### 実機確認の注意

- 「既定アプリ未登録」の確認は external-opener v3 と同様、未使用の拡張子で試すか
  `reg query 'HKCR\.toml'` で事前に関連付け状態を確認すること
- 「終了で自動リロード」は `status_message` が次のキー入力で消えるため、`:wq` 後すぐに
  画面を確認すること
- config.toml を削除するテストの後は、他の受け入れ基準に影響しないよう雛形の状態を確認してから
  次の項目へ進むこと

## 実装セッションへの引き継ぎ事項

- rustfmt 非適用(cargo fmt 禁止)。周辺コードに手合わせで整形
- コミットは Conventional Commits + 日本語、`git commit -F <file>` で作成
- 実装ブランチ: `local/v0.6.2-patched` 直接(従来パッチと同運用)
- CLAUDE.md の規約により、完了報告前に evaluator エージェントの静的レビューを必須とする
- ヒント幅予算の根本是正(design.md ADR-8 / 7章)は本タスクのスコープ外。将来着手する際は
  `build_right_spans` 的な切り出しと `TARGET_TERMINAL_COLS` 定数の導入から始めること
