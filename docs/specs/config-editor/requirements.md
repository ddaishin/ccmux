# requirements.md — config-editor(config.toml をテキストエディタで開く機能)

## 1. 背景と目的

ccmux の設定ファイルは `%APPDATA%\ccmux\config.toml`(`dirs::config_dir()/ccmux/config.toml`)に
あり、初回起動時に `CONFIG_SKEL` から雛形が自動生成される。だが**設定を編集する導線が ccmux
内に一切ない** — ユーザーはパスを覚えてエクスプローラや別の端末から開く必要がある。`r` キーに
よる手動リロードは external-opener v2 で既に入っているのに、その手前の「開いて編集する」が
欠けている。

本機能は、ファイルツリーで `e` を押すと config.toml がエディタで開き、閉じると設定が自動で
反映されるところまでを一続きにする。エディタは config.toml 自身の `[editor]` で指定でき、
vim/nano のようなターミナルエディタは ccmux のペイン内で、GUI エディタは別ウィンドウで
起動する。external-opener 機構(`docs/specs/external-opener/`)の資産をそのまま再利用するため、
対象が config.toml 固定という性質の違いから別文書として新設する。

## 2. 利用者と保守者(条件3)

- 利用者/保守者とも doi(ローカルパッチ運用)。upstream マージ後は全ユーザー/Shin-sibainu

## 3. 成功基準(条件1)

- ファイルツリーで `e` を押すと config.toml がエディタで開く
- CLI エディタは ccmux のペイン内で PTY 起動し、キー入力が届く
- GUI エディタは別ウィンドウで起動し、ccmux の画面を壊さない
- エディタ終了(ペイン経路)で設定が自動的に再読み込みされ、結果がステータスバーに出る
- `[editor]` を書いていない環境では `$VISUAL` -> `$EDITOR` -> OS 既定アプリの順にフォールバックする
- `cargo test` 全緑(既存 113 件 + 新規)
- `[editor]` を含め設定ファイルを一切書いていない環境で、既存の全機能が完全に不変であること

## 4. 受け入れ基準(EARS 記法)

正常系:
- WHEN ファイルツリーフォーカス中に `e` キーを押す(修飾キーなし)
  THE SYSTEM SHALL config.toml のパスを解決し、`[editor]` の設定に従ってエディタを起動する
- WHEN `[editor] command` が設定され `in_pane` が true(既定)
  THE SYSTEM SHALL 現在のタブを上下分割し、新しいペインで `command` を PTY 起動し、
  フォーカスをそのペインへ移す
- WHEN `[editor] command` が設定され `in_pane` が false
  THE SYSTEM SHALL 既存の `spawn_detached`(null stdio + CREATE_NO_WINDOW + reaper)で起動する
- WHEN `[editor] command` が未設定で環境変数 `$VISUAL` または `$EDITOR` が設定されている
  THE SYSTEM SHALL その値をプログラム名としてペイン内で起動する(`in_pane` 相当。
  引数分割はしない — 空白を含む値は正しく動作しない場合があることを CONFIG_SKEL に明記)
- WHEN `[editor] command` も環境変数も未設定
  THE SYSTEM SHALL 既存の OS 既定アプリ委譲経路(`has_default_handler` 判定 ->
  `spawn_detached`)で config.toml を開く
- WHEN `args` に `{file}` が含まれる
  THE SYSTEM SHALL 絶対パスへ置換する(`{file}` が無ければ末尾に自動付加。
  既存 `build_args` と同じ規約)
- WHEN エディタペイン(`in_pane = true` 経由)が終了する(PtyEof)
  THE SYSTEM SHALL そのペインを自動的に閉じ、config.toml を再読み込みし、
  結果をステータスバーへ表示する

異常系・フォールバック:
- WHEN `dirs::config_dir()` が `None` を返す環境で `e` を押す
  THE SYSTEM SHALL 何も起動せず、設定パスが決まらない旨をステータスバーへ表示する
- WHEN `e` を押した時点で config.toml が存在しない
  THE SYSTEM SHALL `ensure_skel_at` で雛形を生成してから開く(`create_new` のため
  既存ファイルがあれば no-op)
- WHEN `in_pane = true` でエディタペインの分割が `MAX_PANES` 到達や最小サイズ制約で失敗する
  THE SYSTEM SHALL 何も起動せず、分割できない理由をステータスバーへ表示する
  (既存の `Ctrl+D`/`Ctrl+E` は無反応のまま、無反応の意味が異なるため区別する)
- WHEN GUI エディタ(`in_pane = false`)経由で config.toml を編集し終了する
  THE SYSTEM SHALL 自動リロードは行わず、`r` キーでの手動反映を促す旨を起動時ステータスへ含める
- WHEN ユーザーがエディタペインを `Ctrl+W` で手動終了する
  THE SYSTEM SHALL そのペインをエディタ追跡から外し、通常のペインクローズ処理へ委譲する
  (二重クローズや panic を起こさない)

## 5. 制約(条件5)

- 技術制約: Rust stable / ratatui + crossterm。rustfmt 非適用(cargo fmt 禁止)。
  依存は toml + serde(derive) のみで新規依存を追加しない。シェル文字列組み立て禁止
  (args ベクタ直接渡し、環境変数の値も空白分割せずプログラム名としてそのまま扱う)
- upstream 制約: `[editor]` を含め設定を書いていない環境で挙動が完全不変であること(PR 前提)
- 既存資産の再利用を優先する: `spawn_detached` / `OS_DEFAULT_LAUNCHER` / `has_default_handler` /
  `build_args`(の内部 `expand_file_args`)/ `config_path` / `std::path::absolute` / `reload_config`
  (external-opener の設計を踏襲、`docs/specs/external-opener/design.md` 参照)

## 6. スコープ外

- `fallback_to_preview` への統合(config.toml はワークスペース境界外にあり、既存の境界チェックが
  拒否するため使えない。TUI プレビューへのフォールバックは行わない)
- ヒント行の幅予算と端末幅の整合性の是正(調査の結果、既存の `test_focus_hints_width_budget` の
  予算値は端末幅と未突き合わせであり Windows Terminal 既定 120 桁で既に切り詰められている
  ことが判明したが、本機能ではテスト上限の引き上げのみ行い、根本是正は別課題とする)
- 選択中の任意ファイルをエディタで開く機能(対象は常に config.toml 固定。`E` キー等への
  将来拡張の余地は残すが今回は実装しない)
- GUI エディタ終了の追跡・自動リロード(`spawn_detached` は即座に返るため技術的に不可能)
- 環境変数の値のシェル的な解釈(クォート・引数分割。空白を含む `$EDITOR` は非対応)

## 7. 未知の未知の決定事項(Phase 2 の承認結果)

- 設定形式 -> `[editor]` サブテーブル。フラットキーは `[[openers]]` より後ろに書かれると
  `deny_unknown_fields` 未使用のため無言で捨てられる構文事故があり、サブテーブルなら
  ファイル内のどこに書いても正しく効く
- `in_pane` の既定 -> true。既定 false だと `spawn_detached` の null stdio により
  CLI エディタが無音で即死し「開きました」という誤った成功表示が出る
  (v3 が既に一度解決した問題の再現)。GUI エディタを使うなら明示的に false にする
- エディタペインの配置 -> 現在タブの上下分割。新規タブは終了後の後始末が要件と噛み合わない
- 終了検知 -> `AppEvent::PtyEof` と `editor_panes: HashSet<usize>` の突き合わせ。
  複数タブ・複数エディタの同時使用にも対応する
- ヒント幅予算 -> 端末幅との不整合が判明したが、今回はテスト上限の引き上げのみで対応し、
  根本是正はスコープ外とする(ユーザー承認済み)
