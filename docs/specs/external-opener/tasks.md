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

## v3 タスク(2026-08-15。T0 → T1 → T2 → T3 → T4/T5 → T6 → T7 → T8 の直列)

### T0: 実測プローブ(完了)

- **内容**: PowerShell + P/Invoke で本機の `AssocQueryStringW` 挙動を実測(プランモード外)。
  一時ファイル書き込みが非ASCIIユーザー名パスで失敗したため `C:\Windows\Temp` へ退避して実行
- **完了条件(機械検証)**: `.md`(MSIX)が `ASSOCSTR_PROGID` で `hr=0` 成功、未関連付け拡張子
  (`.ccmuxprobe1`〜`3`, `.zzznotreal`)が一貫して `hr=0x80070483` を返すこと。実測済み・確定

### T1: 判定関数の新設

- **内容**: `src/app.rs`。`AssocQueryStringW` の `extern "system"` FFI 宣言、
  `has_default_handler()`(Windows実装 + 非Windowsスタブ)、`assoc_hresult_means_absent()`
- **完了条件(機械検証)**: `cargo build` exit 0(Windows)

### T2: 判断ロジックの純粋関数化

- **内容**: `src/app.rs`。`should_try_external(mode, handler_present: impl FnOnce() -> bool)`
- **完了条件(機械検証)**: `cargo test` で Command/Default 分岐 + 遅延評価のテストが緑

### T3: プレビューフォールバックの共通化

- **内容**: `src/app.rs`。`open_selected_file` 末尾を `fallback_to_preview()` へ抽出
- **完了条件(機械検証)**: `cargo build` exit 0、既存の boundary 引数(ワークスペース外拒否)を維持

### T4/T5: `open_selected_file` / `open_with_os_default` への統合

- **内容**: `src/app.rs:579-`。spawn 前に `should_try_external` チェックを追加し、false なら
  `fallback_to_preview` へ。`open_with_os_default` はディレクトリで判定をスキップ
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑

### T6: テスト追加

- **内容**: `app.rs` の `mod tests` に追記
- **完了条件(機械検証)**: `cargo test` 全緑。新規 6 件
  (`should_try_external` 3分岐+遅延評価1件、`has_default_handler` 拡張子なし1件、
  Windows限定で HRESULT写像1件+実環境プローブ1件)

### T7: ドキュメント更新(本ファイル + requirements.md + design.md)

- **内容**: v3 背景・EARS受け入れ基準・ADR-8/9・障害モード表・v3タスクを追記
- **完了条件**: 3ファイルとも UTF-8 整合性確認済み

### T8: evaluator静的レビュー + 実機検証

- **内容**: evaluator エージェントの静的レビュー後、下表 v3 行を実機確認
- **完了条件**: `cargo test` 全緑 + evaluator 合格 + v3 統合検証が全て ☑

## v4 タスク(2026-08-15。T1 → T2 → T3 → T4 → T5 の直列)

### T1: config スキーマ追加(完了)

- **内容**: `src/config.rs`。`default_true()` ヘルパー、`Config` に
  `open_directory_with_os_default: bool`(`#[serde(default = "default_true")]`)、
  `Default` 実装、`apply_reload` への追加、`CONFIG_SKEL` への説明追記。
  `..Default::default()` を持たない唯一のテスト用リテラル
  (`test_apply_reload_replaces_openers_and_dir_flag_but_keeps_scrollback` に改名)
  を新フィールド込みで書き換え
- **完了条件(機械検証)**: `cargo test config::` exit 0。新規 4 件
  (既定true / 明示false / 非bool値パースエラー / CONFIG_SKELの新フィールド一致確認)

### T2: 純粋関数の新設(完了)

- **内容**: `src/app.rs`。`OsDefaultAction` enum(`Launch`/`ToggleDir`/`Preview`)と
  `os_default_action(is_dir, dir_launch_enabled, handler_present)` を
  `should_try_external` の直後に追加
- **完了条件(機械検証)**: `cargo build` exit 0

### T3: `open_with_os_default` → `open_selected_with_os_default` への統合(完了)

- **内容**: `src/app.rs`。パス引数を廃止し選択インデックス経由に変更(design.md ADR-11)。
  `os_default_action` の3分岐(`ToggleDir`/`Preview`/`Launch`)を処理し、`ToggleDir` では
  `toggle_or_select()` を呼びステータス表示。キーハンドラの `o` 分岐を1行に簡素化
- **完了条件(機械検証)**: `cargo build` exit 0・`cargo test` 全緑

### T4: テスト追加(完了)

- **内容**: `app.rs` の `mod tests`(v3ブロックの後)に `os_default_action` の4テストを追加
- **完了条件(機械検証)**: `cargo test` 全緑。新規4件
  (dir+enabled→Launch / dir+disabled→ToggleDir / file経路がdirフラグに非依存 /
  ディレクトリ経路でhandler_presentが呼ばれないことの確認)

### T5: ドキュメント更新 + evaluator静的レビュー + 実機検証

- **内容**: 本ファイル + requirements.md + design.md にv4背景・EARS受け入れ基準・
  ADR-10/11・障害モード表を追記。evaluatorエージェントの静的レビュー後、下表v4行を実機確認
- **完了条件**: `cargo test` 全緑 + evaluator 合格 + v4 統合検証が全て ☑

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

v3 追加分(Windows のみ):

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| .md が退行なく開く(最重要) | README.md に Enter → MarkMaid Viewer が起動、「外部アプリで開きました: 既定アプリ」 | ☐ |
| 未関連付け拡張子で Enter | 未使用拡張子ファイルを作り config に登録・Enter → プレビューが開き「既定アプリ未登録 (.ext) — プレビューで開きます」 | ☐ |
| 同ファイルをクリック | 上記と同一結果(Enter/クリックが同一メソッドを通る不変条件) | ☐ |
| 同ファイルで o | 上記と同一結果 | ☐ |
| ディレクトリで o | 判定スキップでエクスプローラが開く(従来どおり) | ☐ |
| mode="command" で Enter | 判定が介在せず従来どおり「外部起動失敗」+プレビュー | ☐ |
| 拡張子なしファイルで Enter | 判定スキップで従来どおり spawn される | ☐ |

v4 追加分:

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| 未設定で従来どおり | 設定を書かずにディレクトリで `o` → エクスプローラが開く | ☐ |
| false でトグル | `open_directory_with_os_default = false` → ディレクトリで `o` → 展開/折りたたみのみ・外部プロセスなし | ☐ |
| 空ディレクトリで無反応に見えない | 空ディレクトリで `o` → ステータスに「ディレクトリの外部起動は無効(設定)— 展開/折りたたみ」 | ☐ |
| ファイル経路は不変 | false のまま `.md` に `o` → v3 と同じ(既定アプリで起動) | ☐ |
| `r` で即時反映 | 実行中に true→false へ書き換え `r` → 再起動なしでトグル挙動になる | ☐ |
| 折りたたみ済みディレクトリ | 展開済みディレクトリで `o` → 折りたたまれる(Enter と同一) | ☐ |

### v4 実機確認の注意

`false` へ切り替える前に一度 `true`(既定)のままディレクトリで `o` を押し、従来どおり
エクスプローラが開くことを確認してから設定を変更すると、退行の有無を切り分けやすい。

### v3 実機確認の注意

未関連付け拡張子は、一度でも Windows の「開く方法を選んでください」ピッカーが出ると
`UserChoice` が書かれ次回から「あり」に反転しうる。再テストごとに拡張子名を変えるか、
事前に `reg query 'HKCR\.<ext>'` で未登録であることを確認すること。

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
