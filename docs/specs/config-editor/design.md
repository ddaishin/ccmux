# design.md — config-editor(config.toml をテキストエディタで開く機能)

## 1. 全体構成(条件7)

```
FileTree フォーカス中の e キー
  -> config_path() (None -> ステータス表示のみで終了)
  -> ensure_skel_at() (存在しなければ雛形生成。create_new なので既存時は no-op)
  -> editor_action(config.editor, config_path, $VISUAL, $EDITOR)
       |- Pane { program, args }     -> 現タブを上下分割 -> new_with_command
       |                                 -> focus_target = Pane
       |                                 -> editor_panes.insert(pane_id)
       |- Detached { program, args } -> spawn_detached (既存再利用) -> 「r で反映」を案内
       |- OsDefault                  -> has_default_handler 判定 -> spawn_detached へ委譲
       `- NoConfigPath               -> ステータス表示のみ

AppEvent::PtyEof(pane_id)
  -> editor_panes.remove(&pane_id) が true なら
       ペインを閉じる -> reload_config_message() -> status_message へ「エディタ終了 - ...」
```

## 2. 出力契約(条件4)

| パス | 目的 |
|------|------|
| `src/config.rs` | `EditorConfig` 構造体、`Config.editor`、`expand_file_args`(既存 `build_args` から抽出)、`apply_reload` への追加、`CONFIG_SKEL` への `[editor]` 節追加、`ensure_skel_at` の `pub(crate)` 化 |
| `src/app.rs` | `EditorAction` enum、`editor_action()` 純粋関数、`e` キーハンドラ、`can_split`/`attach_split_pane`(既存 `split_focused_pane` から抽出)、ID 指定クローズ、`editor_panes: HashSet<usize>`、PtyEof 結線、`reload_config_message()`(既存 `reload_config` から分割) |
| `src/pane.rs` | `PaneCommand` enum、`spawn_with()`(既存 `new_with_cwd` の共通部分を抽出)、`new_with_command()` |
| `src/ui.rs` | `focus_hints` の FileTree 分に `e 編集` を追加、`test_focus_hints_width_budget` の FileTree 上限を 68 -> 75 |
| `dist/ccmux-0.6.2-patched-win-x64/config.toml.sample` | CONFIG_SKEL の手動同期コピー |
| `README.md` / `dist/.../README.txt` | File tree キー表に `e` を追加(`o`/`r` の記載漏れも併せて解消) |

設定ファイル: 既存と同じ `dirs::config_dir()/ccmux/config.toml`(新規パス解決なし)

## 3. 設計判断(ADR 形式)

### ADR-1: 設定形式 — フラット3キー vs `[editor]` サブテーブル

- 案A: `editor` / `editor_args` / `editor_in_pane` をトップレベルにフラット追加
  (external-opener ADR-10 の踏襲)
- 案B: `[editor]` サブテーブル(`command` / `args` / `in_pane`)
- **決定: 案B**。`Config` にも `Opener` にも `deny_unknown_fields` が付いていないため、
  フラットキーを `[[openers]]` ブロックより後ろに書くとその opener の未知フィールドとして
  無言で捨てられる構文事故がある(実装で確認済み)。テーブルヘッダは常にルートへ戻るため
  `[editor]` はファイル内のどこに書いても正しく効く。external-opener ADR-10 が退けたのは
  `[general]` の新設 = 既存キーの移動であり、新規名前空間の追加はその理由に当たらない

### ADR-2: `in_pane` の既定値

- 案A: 既定 false(GUI として起動。安全側に見えるが…)
- 案B: 既定 true(ペイン内 PTY 起動)
- **決定: 案B**。既定 false だと `command = "vim"` とだけ書いたユーザーが `spawn_detached`
  経路に流れ、null stdio + CREATE_NO_WINDOW によりプロセス生成自体は成功するため
  「開きました」と表示されて何も起きない — external-opener v3 が解決した誤成功表示の再現になる。
  逆にペイン起動の失敗は「(x) Process exited」として画面に見える。GUI エディタ利用者は
  CONFIG_SKEL のコメントに従い明示的に `in_pane = false` を書く

### ADR-3: エディタペインの配置

- 案A: 新規タブ
- 案B: 現在タブを上下分割
- **決定: 案B**。新規タブは終了後のタブ後始末(1つしかないと閉じない等)が自動クローズ要件と
  噛み合わない。上下分割にするのは、ファイルツリー表示中に左右分割すると幅が痩せ、
  CONFIG_SKEL のコメント行が約78桁あるため折り返しで読みにくくなるため

### ADR-4: 終了検知の保持先

- 案A: `Option<usize>` 1個で最後に開いたエディタペインのみ追跡
- 案B: `HashSet<usize>`
- **決定: 案B**。複数タブ・複数エディタの同時使用でも取りこぼさない。コスト差はほぼゼロ

### ADR-5: PtyEof 結線の処理順序

- **状況**: `AppEvent::PtyEof` の既存ハンドラはペイン探索ループ内で `pane.exited = true` を
  設定するだけ(`app.rs:1957-1964`)。エディタ検知をこのループ内に書くと `&mut self.config` /
  `status_message` への書き込みで `&mut self.workspaces` の借用と衝突する
- **決定**: ループの前に `editor_panes.remove(&pane_id)` で判定を退避し、ループ後に
  リロードとペインクローズを行う3段構成にする。加えて `Ctrl+W` でユーザーがエディタペインを
  先に閉じた場合、`close_focused_pane` は `panes.remove()` -> `kill()` の順で処理するため、
  その後に届く PtyEof は「ペインが既に存在しない状態」になる。ループ内判定だとこのケースを
  取りこぼすため、`remove` ベースの自己クリーンで対応する(取りこぼしても実害はないが、
  余分なリロードが1回走る程度で無害)

### ADR-6: `Pane::new_with_command` の実装方式

- 案A: `new_with_cwd` に `Option<(program, args)>` を追加(全呼び出し箇所のシグネチャが変わる)
- 案B: 内部で `PaneCommand` enum により分岐する private ヘルパへ切り出す
- **決定: 案B**。PTY生成・`clamp_pty_size`・reader スレッド生成・vt100 Parser 生成は
  `new_with_cwd` と共通化し、シェル固有処理(`--login` の付与、OSC 7 プロンプトフック注入)は
  `Shell` 分岐にのみ残す。OSC 7 フックはシェル構文を PTY への入力として書き込む実装のため、
  エディタ(vim 等)に送ると本文へタイプされてバッファを汚す — 注入しないことが必須要件

### ADR-7: 分割不能時の可視化

- **状況**: 既存 `split_focused_pane` は `MAX_PANES`(16)到達や最小サイズ制約
  (幅20/高さ5未満)で無反応(`return Ok(())`)。`Ctrl+D`/`Ctrl+E` ではそれで許容されているが、
  `e` キーは「エディタを開く」という明確な期待があるため無反応は不親切
- **決定**: `can_split` / `attach_split_pane` へ処理を分離し、`Ctrl+D`/`Ctrl+E` は従来どおり
  無言 return を維持しつつ、エディタ起動経路だけ失敗理由をステータスバーへ表示する
  (external-opener v4 ADR-11 の「無反応に見えるケースには表示を出す」判断を踏襲)

### ADR-8: ヒント幅予算の扱い

- **調査結果**: `test_focus_hints_width_budget`(`ui.rs:1060-1073`)の予算値は端末幅と
  突き合わされていない。右側情報(モデル名・コンテキストバー・git ブランチ・更新通知)の実幅を
  構成要素から算出したところ、通常時57桁・最悪時(更新通知あり)67桁で、Ambiguous 幅文字
  (`▓` `░` `⎇` `↑`)を2桁カウントする端末ではさらに+8桁になりうる。Windows Terminal の
  `initialCols` 既定は120桁であり、FileTree(68桁)・Pane(80桁)のヒントは通常時でも
  既に右端が切り詰められている(68+57=125、80+57=137、いずれも120超)
- **決定**: 今回はテストの上限値をラベル追加分だけ引き上げる(FileTree: 68 -> 75)にとどめ、
  根本的な是正(右側情報との動的な突き合わせ、ラベルの再設計)は別課題とする(ユーザー承認済み)。
  端末幅が足りない環境では既存同様 ratatui が末尾を切り詰めるのみで、新たな退行ではない

## 4. 参照実装(条件6)

- 純粋関数切り出し+テスト: external-opener の `should_try_external` / `os_default_action`
  (`app.rs:2075-2119`)を踏襲し `editor_action` を同じ形で設計
- 既存資産の再利用(新規実装しないもの):

| 用途 | 既存関数/定数 | 場所 |
|------|------|------|
| GUI エディタ / OS 既定の spawn | `spawn_detached` | `app.rs:2252-2269` |
| OS 既定ランチャ名 | `OS_DEFAULT_LAUNCHER` | `app.rs:2050-2054` |
| 既定アプリ有無の判定 | `has_default_handler` | `app.rs:2162-2194` |
| args の `{file}` 置換 | `build_args`(内部を `expand_file_args` として抽出) | `config.rs:231-249` |
| 設定パス解決 | `config_path` | `config.rs:251-253` |
| 絶対パス化 | `std::path::absolute` | `app.rs:2234` |
| リロード | `reload_config`(`reload_config_message` へ分割して再利用) | `app.rs:695-722` |

## 5. データとライフサイクル

- `[editor]`: config.toml 読み込み時にメモリ保持。書き込みは既存の雛形生成のみ(新規なし)
- `editor_panes: HashSet<usize>`: エディタペイン生成時に挿入、PtyEof または手動クローズで削除。
  セッション内のみ、永続化なし
- `status_message`: 発火時にセット -> 次のキー入力で消去(既存の寿命と同じ)

## 6. 障害モードと回復

| 障害 | 振る舞い |
|------|----------|
| `dirs::config_dir()` が None | 何も起動せずステータス表示のみ |
| config.toml が存在しない | `ensure_skel_at` で雛形生成してから開く |
| 分割が MAX_PANES/最小サイズで失敗 | 何も起動せず理由をステータス表示 |
| ペイン生成が失敗(PTY 起動エラー等) | エラーをステータス表示。config.toml は無傷 |
| GUI エディタ(spawn_detached)の起動失敗 | 既存 spawn_detached の失敗経路をそのまま使用 |
| ユーザーが Ctrl+W でエディタペインを先に閉じる | `editor_panes` から除去され、後続 PtyEof は
  何もしない(取りこぼしても実害はない) |
| `[editor]` の TOML パース失敗 | 既存の fail-open(直前の設定を維持 + 警告)がそのまま適用される |
| `editor.command` に空文字列(`command = ""`) | 未設定として扱い `$VISUAL` -> `$EDITOR` -> OS 既定アプリへフォールバック(`$VISUAL`/`$EDITOR` の空文字列と同じ扱い) |

### 既知の限界(evaluator 静的レビューで指摘・低リスクのため対応を保留)

- **非アクティブタブでエディタペインが自動クローズされたときの一瞬の空白表示**:
  `close_pane` はペイン ID が属するワークスペースを横断的に探索して正しくクローズするが、
  末尾の `mark_layout_change()` -> `relayout_panes()` は常にアクティブタブのみを
  再計算する。タブ A でエディタを開いたまま Alt+Left/Right でタブ B へ切り替え、
  その間にタブ A 側のエディタが終了すると、タブ A の残存ペインの PTY サイズは
  即座には更新されない。ユーザーがタブ A へ戻った時点で `render_panes` が
  初めてそのワークスペースの resize を行うため機能的には自己修復するが、
  `Pane::resize` は resize 時に画面をクリアする実装のため、タブ復帰直後の
  1フレームが空白表示になりうる(子プロセスの再描画待ち)。
  機能停止や誤動作ではなく見た目のみの問題であり、修正には
  `relayout_panes` を任意のワークスペース向けに一般化する必要があり
  影響範囲が本機能を超えるため、今回は対応を見送り記録として残す。

## 7. 幅予算の実測記録(ADR-8 の根拠、将来の是正着手時の起点)

`ui.rs:917-972` の右側スパン構築を要素ごとに算出した結果:

| 要素 | 書式 | 幅 |
|------|------|-----|
| モデル名 | ` [emoji] sonnet ` | 11(`short_model()` は opus/sonnet/haiku、未知名は切り詰めなしで素通し) |
| コンテキスト | ` [bar6] 199.9k/200.0k ` | 22(bar幅6固定、`format_tokens` は最大6桁) |
| git ブランチ | ` [icon] <20桁まで> ` | 24(`truncate_to_width(branch, 20)`) |
| 更新通知 | ` [icon] v0.6.3 ` | 10(新バージョン検出時のみ) |

- 通常時(更新通知なし) = 57桁 -- 既存コメント「55-60」はこの値
- 最悪時(更新通知あり) = 67桁
- `ui.rs:964` のコメントは「更新通知が上位要素を override する」と書いているが、実装は
  Vec への push のみで何も取り除いていない(コメントと実装の不一致)
- 4項目は互いに独立して push されるため、Claude 実行中 かつ git ブランチあり かつ
  更新通知ありの場合に最悪値へ到達する
