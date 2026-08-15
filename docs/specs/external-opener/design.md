# design.md — external-opener(拡張子別・外部ビューワー起動機構)

## 1. 全体構成(条件⑦)

```
main.rs ── Config::load() ──▶ App { config, status_message }
filetree Enter/クリック ─▶ open_selected_file ─▶ find_opener
   ├─ Some ─▶ spawn_external_opener(mode で command / 既定ランチャを分岐)
   │             └ 失敗 → status_message + preview.load() フォールバック
   ├─ None ─▶ preview.load()(従来経路そのまま)
   ├─ o キー ─▶ open_selected_with_os_default(file: 関連付け判定 / dir: 設定判定)
   └─ r キー ─▶ reload_config(parse 成功時のみ差し替え。失敗は据え置き+警告)
```

## 2. 出力契約(条件④)

| パス | 目的 |
|------|------|
| `src/config.rs` | `Config`/`Opener`、`load()`、`find_opener()`、`build_args()`。v2: `OpenerMode`・`filenames`・末尾一致 |
| `src/app.rs` | Enter 分岐、`spawn_external_opener()`、`status_message`。v2: `o`/`r` キー、`reload_config()`。v3: `has_default_handler()`(Windows実装+非Windowsスタブ)、`should_try_external()`、`fallback_to_preview()`。v4: `os_default_action()`/`OsDefaultAction`、`open_selected_with_os_default()`(旧 `open_with_os_default` を改名・統合) |
| `src/ui.rs` | ステータスバーに status_message を優先表示。v2: ツリーヒントに `o`/`r` を追加 |
| `src/main.rs` | 起動時 `Config::load()` → `App::new()` へ受け渡し |
| `Cargo.toml` | `toml = "0.8"` / `serde = { version = "1", features = ["derive"] }`。v2 は依存追加なし |

設定ファイル: `dirs::config_dir()/ccmux/config.toml`(Windows: `%APPDATA%\ccmux\config.toml`)

## 3. 設計判断(ADR 形式)

### ADR-1: Mermaid 対応 WYSIWYG の実現形

案A 外部アプリ起動 / 案B ブラウザプレビュー内蔵(HTTP+Mermaid.js) / 案C TUI 内レンダリング強化
→ **決定 案A**。案C は Mermaid 図と WYSIWYG 編集が原理的に不可、案B は新規システム級

### ADR-2: 設定形式

案A TOML(コメント可・依存追加あり) / 案B JSON(依存ゼロだがコメント不可・手書きしにくい)
→ **決定 案A**。upstream 受けと設定ファイルとしての書きやすさを優先

### ADR-3: 発火トリガー

案A 別キー追加 / 案B Enter で外部起動(マッチ時のみ)
→ **決定 案B**。未登録拡張子・設定なしでは従来どおりプレビューが開きフォールバックが常に存在する
(v2 で `o` を追加したが Enter の挙動は不変)

### ADR-4: spawn の安全策

- 案A シェル経由(`cmd /c` / `sh -c`): PATHEXT 解決が効くがインジェクション面と黒窓リスク
- 案B Command 直接 + null stdio + CREATE_NO_WINDOW + reaper thread: エスケープを std に委譲
- **決定**: 案B。TUI 巻き込み防止と安全性を優先。.cmd は拡張子明記が必要だがドキュメントで許容

## 4. 参照実装(条件⑥)

- 純粋関数切り出し+テスト: `app.rs` の `max_side_panel_width`、`pane.rs` の `next_claude_active` を踏襲
- キーハンドラ `app.rs handle_file_tree_key` / ステータスバー `ui.rs render_status_bar` / 設定パス `dirs`

## 5. データとライフサイクル

- config.toml: 起動時 1 回 + `r` キーで読み込み → メモリ保持。書き込みは初回の雛形生成だけ(`create_new`)
- status_message: 発火時にセット → 次のキー入力で消去(セッション内のみ)。個人情報なし

## 6. 障害モードと回復

| 障害 | 振る舞い |
|------|----------|
| 設定ファイルなし | openers 空で正常動作(従来挙動) |
| TOML パース失敗 | 起動時は fail-open(空設定+警告)、`r` 時は**直前の設定を据え置き**+警告(ADR-7) |
| spawn 失敗(不在/エスケープ拒否) | エラー表示 + TUI プレビューにフォールバック |
| 外部アプリが即死 | 関知しない(spawn 成功=起動成功と扱う。reaper が回収のみ) |
| 既定アプリ未登録(Windows、`mode = "default"`) | spawn せず TUI プレビューにフォールバック(v3・ADR-8/9) |
| 既定アプリ有無の判定 API が想定外の結果を返す | fail-open で「あり」扱い、従来どおり spawn を試みる(v3・ADR-9) |
| ディレクトリ + `open_directory_with_os_default = false` | spawn せず展開/折りたたみへフォールバック + ステータス表示(v4・ADR-10/11) |

## 7. v2 の追加設計(2026-08-03)

### ADR-5: MSIX のバージョン依存をどう断つか

| 手段(実環境で実測) | 可否 |
|------|------|
| WindowsApps をグロブ列挙してバージョン解決 | 不可(ACL で列挙拒否) |
| AppExecutionAlias | なし(対象アプリが未登録) |
| HKLM AppModel Repository からパス解決 | 不可(権限不足) |
| `explorer.exe shell:AppsFolder\<PFN>!<AppId>` | 起動可だが**ファイル引数を渡せない** |
| AUMID + `ActivateForFile`(COM) | 可だが windows crate 依存が増える |
| **OS 既定アプリへ委譲** | **可**(.md の既定が対象アプリ本体を指すことを実測) |

- **決定**: 既定アプリ委譲(`mode = "default"`)。依存ゼロ・バージョン非依存・クロスプラットフォーム。
  ランチャは Windows `explorer.exe` / macOS `open` / Linux `xdg-open`。`args` は無視する
  (`--foo` を explorer.exe へ渡さないよう `build_args` を呼ばず呼び出し側で分岐)

### ADR-6: マッチ優先順位

- 案A 記述順で先着(実装は最小だが "gz" が "tar.gz" を食う) /
  案B 最長一致(`filenames` 完全一致 > 最長 `extensions` 一致 > 記述順)
- **決定**: 案B。判定は `.{ext}` で終わることを要求し(ドット境界)、"atar.gz" の誤マッチを防ぐ

### ADR-7: リロード失敗時の設定の扱い

- 案A `Config::load()` を呼び直す: パース失敗時に既定値(openers 空)へ落ちるため、編集途中の
  壊れた TOML で**ユーザー設定が黙って消える**。雛形生成の副作用も `r` に付く
- **決定**: `parse()` を直接呼び `Ok` のときだけ差し替える。`Err` は設定据え置き + 警告

v2 の新キーはどちらも `Ctrl`/`Alt` 併用時は不発火とする。リネーム中(`rename_input.is_some()`)は
ファイルツリー分岐より先に全入力が吸われるため、タブ名に "order" と打っても発火しない。

## 8. v3 の追加設計(2026-08-15)

### ADR-8: 既定アプリ有無の判定手段

| 案 | 内容 | 可否 |
|---|---|---|
| A. 起動後のプロセス監視 | spawn 後に対象プロセスの実在を確認 | **却下**。explorer.exe は成功・失敗いずれも即終了し判別不能(実測済み) |
| B. `FindExecutableW` | `.exe` パスを取り出す | **却下**。`.md`(MSIX 版 MarkMaid Viewer、`DelegateExecute` のみで `.exe` パスを持たない)が偽陰性になる。実測: `hr=0x80070483`(未関連付けと同じ失敗) |
| C. `AssocQueryStringW`(`ASSOCSTR_PROGID`) | 関連付けの ProgID 有無を問う | **採用**。`.md` は `hr=0` で成功(`AppX7e80ypkf...`)、未関連付け拡張子は一貫して `hr=0x80070483`。同じ shlwapi.dll なので依存制約(requirements.md §5)は不変。`ASSOCSTR_EXECUTABLE` は未関連付けで `OpenWith.exe` を返す偽陽性の既知パターンがあるため不採用 |

実測は本機で `Add-Type` + P/Invoke により実施(tasks.md T0)。`.txt`(`AppX4ztfk9wxr86...`)・
`.exe`(`exefile`)・`.rs`(`VSCode.rs`)は成功、未関連付け拡張子(`.ccmuxprobe1`〜`3`,
`.zzznotreal`)は一貫して `hr=0x80070483`。ウォーム時の所要時間は 0.5〜0.7ms。

### ADR-9: fail-open 方針

**明示的な no-association HRESULT(`0x80070483` = `HRESULT_FROM_WIN32(ERROR_NO_ASSOCIATION)`)
のときだけ「無い」と判定し、それ以外(成功・空文字列で `E_INVALIDARG` = `0x80070057` が
返るケース含む・その他一切の想定外 HRESULT)は「ある」とみなして従来どおり spawn する**。
これが他人の実環境で本変更が非退行であることの根拠。

判定をそもそもスキップする既知ケース(fail-open とは別レイヤ):
- 拡張子なし(`filenames` マッチの Makefile 等) — `Path::extension()` が `None`
- ディレクトリ(`o` キー) — 関連付けの概念が無く、常に起動が正しい(v4 で条件付きに変更)
- macOS/Linux — `#[cfg(not(windows))]` スタブが常に `true` を返す(型レベルで保証)

## 9. v4 の追加設計(2026-08-15)

### ADR-10: 既定値と配置

- **状況**: ディレクトリでの `o` キー外部起動を config.toml で制御可能にする。既定値と
  TOML 上の配置を決める必要があった
- **案A**: 既定 false(安全側)。headless/xdg-open 未導入環境での誤爆を最初から防ぐ
- **案B**: 既定 true(現状維持)。config.toml 未設定の既存ユーザーは何も変わらない
- **決定**: 案B。requirements.md §5 の upstream 制約(設定なし環境で挙動が完全不変)を
  最優先する。配置はトップレベル・フラット(`scrollback_lines` と同格)。`[general]`
  のようなサブテーブル新設は既存キーの移動を招くため見送り。`serde_aux` 等は使わず
  `default_true()` を自前定義(依存追加禁止の制約を維持)

### ADR-11: 分岐位置とフォールバックの実装手段

- **状況**: `open_directory_with_os_default = false` のとき、ディレクトリでの `o` を
  展開/折りたたみへフォールバックさせる実装方法を決める必要があった
- **案A**: キーハンドラ側で分岐する。`filetree.rs` は無変更で済むが、設定ゲートが
  opener 経路の外に出るため、将来 `open_with_os_default` が別経路から呼ばれると
  ゲートを素通りする
- **案B**: `FileTree::toggle_dir`(private)を `pub(crate)` 化し、パス指定でトグルする。
  だが `toggle_dir` は `entry.path == path` の完全一致で探す一方、`FileTree` の根は
  `Workspace::new` の `FileTree::new(cwd.clone())`(非 canonicalize)で構築され、
  `open_with_os_default` 側は `std::path::absolute()` で正規化するため、両者が
  一致しない環境があり得る(検証済み。`initial_cwd` は別途 canonicalize されるが
  `file_tree` の根には使われていない)
- **決定**: 案C(新設)。`open_with_os_default(path)` を
  `open_selected_with_os_default(&mut self)` に変更し、パス引数を廃止して選択
  インデックス経由で `toggle_or_select()` を呼ぶ。パス比較が発生せず、`filetree.rs`
  は無変更、設定ゲートは opener 経路の中に残る。
  **判定は `selected_entry()` が返す `FlatEntry::is_dir`(ツリーのキャッシュ値)を使い、
  `abs.is_dir()` のような fs 再 stat はしない** — `toggle_or_select()` 内部の分岐も
  同じキャッシュ値を見るため、両者を同一の情報源に揃えることで戻り値(常に `None`)の
  破棄が構造的に安全になる(fs stat と別のキャッシュ値を混在させると、選択後に外部
  プロセスが同名パスをファイル/ディレクトリへ入れ替えるレースで両者が食い違い、
  `Some(path)` が握り潰されて無反応になり得ることをレビューで指摘・修正した)

ステータス表示を出す理由: 空ディレクトリ・折りたたみ済みディレクトリでは画面表示が
変わらず、トグルが無反応に見える唯一のケースになる。Enter キーが無言でよいのは
Enter が外部起動そのものを約束していないためで、`o` は「常に何かを起動する」という
既存の期待があるぶん非対称。

据え置き決定として記録: `ui.rs` の `o 外部` ヒント文言(既定 true では引き続き正しい
ため変更しない)、`reload_config` のメッセージ文言(`scrollback_lines は再起動時に
反映` は依然として真)、ツリーが古くファイルシステムとずれている場合の空打ち許容
(`toggle_or_select` が `Some` を返しても無視して構わない設計)。
