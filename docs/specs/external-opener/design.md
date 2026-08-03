# design.md — external-opener(拡張子別・外部ビューワー起動機構)

## 1. 全体構成(条件⑦)

```
main.rs ── Config::load() ──▶ App { config, status_message }
filetree Enter/クリック ─▶ open_selected_file ─▶ find_opener
   ├─ Some ─▶ spawn_external_opener(mode で command / 既定ランチャを分岐)
   │             └ 失敗 → status_message + preview.load() フォールバック
   ├─ None ─▶ preview.load()(従来経路そのまま)
   ├─ o キー ─▶ open_with_os_default(設定を見ず OS 既定ランチャ。ディレクトリ可)
   └─ r キー ─▶ reload_config(parse 成功時のみ差し替え。失敗は据え置き+警告)
```

## 2. 出力契約(条件④)

| パス | 目的 |
|------|------|
| `src/config.rs` | `Config`/`Opener`、`load()`、`find_opener()`、`build_args()`。v2: `OpenerMode`・`filenames`・末尾一致 |
| `src/app.rs` | Enter 分岐、`spawn_external_opener()`、`status_message`。v2: `o`/`r` キー、`reload_config()` |
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
