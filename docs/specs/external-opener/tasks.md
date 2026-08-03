# tasks.md — external-opener(拡張子別・外部ビューワー起動機構)

## v1 タスク(完了)

T1 依存追加(`toml` / `serde` derive) → T2 `src/config.rs` 新規(`Config` / `Opener` /
`load()` / `find_opener()` / `build_args()` とユニットテスト) → T3 App 統合
(`main.rs` で読み込み、Enter 分岐、`spawn_external_opener`: 絶対パス化 + null stdio +
`CREATE_NO_WINDOW` + reaper スレッド) → T4 ステータスバー表示(次のキー入力で消去) →
T5 evaluator 静的レビュー + 実機確認。各段の完了条件は `cargo build` / `cargo test` の exit 0。

実機で「Enter だけ実装されクリック経路が漏れていた」欠陥を検出し、`open_selected_file()` への
共通化で修正済み。**v2 でも両経路が同一メソッドを通る不変条件を保つこと**。

## v2 タスク(2026-08-03。T6 → T7 → T8 → T9 の直列)

### T6: config スキーマ拡張とマッチロジック

- **内容**: `src/config.rs`。`OpenerMode { Command(既定), Default }` を追加、`Opener` に
  `filenames: Vec<String>` を追加し `command` を `Option<String>` へ。`find_opener()` を
  最良スコア方式へ書き換え(`filenames` 完全一致 = 最優先、`extensions` は `.{ext}` 末尾一致の
  最長勝ち、同点は記述順)。`mode = Command` かつ `command` なしの opener は除外し警告。
  `CONFIG_SKEL` に `mode = "default"` の推奨例・`filenames` 例・`r` キー/再起動の注記を追加
- **完了条件(機械検証)**: `cargo test config::` exit 0。新規テスト最低 8 件
  (tar.gz > gz 最長一致 / filenames 優先 / 拡張子なしマッチ / atar.gz 非マッチ /
  大文字小文字 / mode 省略時 Command / mode="default" / command 欠落 opener の除外)
  かつ既存テストが全緑。`command` の `Option` 化に伴う型追従(ヘルパー `opener()` と
  `test_parse_valid_toml` のアサーション 1 行)だけは避けられないが、**判定内容は 1 件も変えない**。
  ドットファイル挙動の変更は新規テストで固定する

### T7: 既定ランチャ起動と o / r キー

- **内容**: `src/app.rs`。`spawn_external_opener` に mode 分岐を追加し、`Default` では
  OS ランチャ(`explorer.exe` / `open` / `xdg-open`)へ絶対パス 1 個だけを渡す(args 無視)。
  引数組み立ては純粋関数 `launch_argv()` に切り出す。`open_with_os_default()` と
  `reload_config()` を新設(reload は `Config::parse()` を直接呼び `Ok` のときだけ差し替え)。
  `handle_file_tree_key` に `o` / `r` を追加(`Ctrl`/`Alt` 併用時は不発火)。
  `filetree.rs` の `selected_entry()` から `#[allow(dead_code)]` を外す
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑。
  `launch_argv` のテストで `mode = "default"` が args を無視することを確認

### T8: ステータスバーのヒント更新

- **内容**: `src/ui.rs` の `FocusTarget::FileTree` ヒントに `o` 外部 / `r` 設定再読 を追加。
  横幅のため既存ラベルを短縮する
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑

### T9: evaluator 静的レビュー + 実機確認

- **内容**: evaluator エージェントの静的レビュー(diff・キー競合・エッジケース)後、下表 v2 行を実機確認
- **完了条件**: `cargo test` 全緑 + evaluator 合格 + v2 統合検証が全て ☑

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

v2 追加分:

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| MSIX がバージョン非依存で起動 | md を `mode = "default"` にして README.md に Enter → MarkMaid 0.4.2.0 が起動 | ☑ |
| ディレクトリを o | ツリーでディレクトリ選択 → `o` → エクスプローラーで開く・TUI 無事・黒窓なし | ☑ |
| ファイルを o | main.rs に `o` → 既定アプリ / 同ファイルに Enter → TUI プレビュー(使い分け) | ☑ |
| 手動リロード | `filenames = ["Makefile"]` を追記 → `r` → 再起動なしで Makefile が外部起動になる | ☑ |
| リロード失敗で据え置き | config を壊して `r` → 警告のみ・**直前の設定のまま外部起動が生きている** | ☑ |
| 複合拡張子の最長一致 | `gz` と `tar.gz` を別アプリで登録 → `a.tar.gz` が tar.gz 側で開く | ☑ |
| 設定なし環境で不変 | config.toml をリネームして起動 → 雛形生成・全ファイル TUI プレビュー | ☑ |
| リロードは openers のみ | `scrollback_lines` を変えて `r` → 分割/新タブしても新ペインの履歴長が変わらない | ☑ |
| カンマ入りパスを o | `a,b.md` を作成 → ツリーで `o` → 既定アプリで開く(explorer のカンマ解釈確認) | ☑ |

### v2 実機確認の順序と注意

- 「設定なし環境で不変」は config.toml をリネームするため**最後に実施**し、終わったら
  `config.toml.bak-v1` から戻す(戻さないと .md が TUI プレビューに逆戻りする)。
  途中で実施すると以降の行が雛形設定に対して走り、誤った理由で通る
- 「リロード失敗で据え置き」は TOML をわざと壊すため、**ccmux を終了する前に必ず直す**
  (壊れたまま終了すると次回起動は fail-open で openers 空になり、ファイルは存在するので
  雛形生成も走らず「壊れたまま」に見える)
- ステータスメッセージは次のキー入力で消える。`r` / `o` を押したら次を押す前に読むこと

## 実装セッションへの引き継ぎ事項

- rustfmt 非適用(cargo fmt 禁止)。周辺コードに手合わせで整形
- コミットは Conventional Commits+日本語、`git commit -F <file>` で作成
- /init 由来の未コミット変更(.gitignore / CLAUDE.md / .claude/ / examples/)はコミットに含めない
- 実装ブランチ: `local/v0.6.2-patched` 直接(従来パッチと同運用)。PR 切り出しは別途指示
- `.cmd` シム(VS Code)は `code.cmd` と拡張子まで書く必要がある旨を config 例コメントに記載
- v2: `%APPDATA%\ccmux\config.toml` は現在バージョン直書きで壊れている。実装完了後に
  **ユーザー確認のうえ** `mode = "default"` へ書き換える(プロジェクト外なので独断で変更しない)
- v2: 新規依存は追加しない。`o` は OS のダブルクリック相当のため `.exe` を選ぶと実行される点は許容済み
