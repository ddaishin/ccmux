# requirements.md — external-opener(拡張子別・外部ビューワー起動機構)

## 1. 背景と目的

MD ファイルを Mermaid 対応の外部ビューワー(Typora 等)で見たまま編集したいが、ccmux の
TUI プレビューは syntect ハイライトのみで Mermaid 描画・WYSIWYG 編集は実現不能。
拡張子ごとに外部アプリを設定できる汎用オープナー機構を新設する。

### v2 の追加背景(2026-08-03)

v1 は `command` に絶対パスを書く方式のため MSIX アプリはアプリ更新でパスが壊れる
(実測: config の 0.3.0.0 に対しインストール版が 0.4.2.0 へ更新され起動失敗)。パスの自動解決は
成立しない(調査結果は design.md ADR-5)ため **OS の既定アプリへ委譲するモード**でバージョン依存を断つ。
あわせて複合拡張子/拡張子なしのマッチ、ディレクトリの外部起動、設定の手動リロードを入れる。

### v3 の追加背景(2026-08-15)

`mode = "default"` は explorer.exe 等の**委譲型ランチャ**への spawn であり、その成功は
「対象拡張子に既定アプリが実際にある」ことを一切保証しない(explorer.exe 自体は常に
起動できる)。結果、既定アプリの無い拡張子で Enter/`o` を押すと「開きました」という
誤った成功表示だけが出て、TUI プレビューへのフォールバックが発火しなかった。

当初 `FindExecutableW`(`.exe` パスを取り出す API)での判定を検討したが、実マシンの
レジストリで `.md`(MSIX 版 MarkMaid Viewer。v2 が解決した当の対象)を確認したところ
`DelegateExecute` のみで `(Default)` に `.exe` パスを持たず、この API では失敗を返すと
判明した。実装すれば「開かないのに成功」が「開けるのにプレビューに落ちる」新たな退行に
変わるため、ProgID の有無を問う `AssocQueryStringW` に切り替えた(design.md ADR-8)。

### v4 の追加背景(2026-08-15)

`o` キーはディレクトリに対して常に OS 既定ランチャへ委譲する(v2 の設計判断)。だが
headless 環境や `xdg-open` 未導入の Linux 環境では、この委譲自体を避けたいケースが
ある。ディレクトリには v3 の関連付け判定(拡張子ベース)が適用できないため、判定では
なく config.toml の明示設定で解く。

## 2. 利用者と保守者(条件③)

- 利用者/保守者とも doi(ローカルパッチ運用)。upstream マージ後は全ユーザー/Shin-sibainu

## 3. 成功基準(条件①)

- `%APPDATA%\ccmux\config.toml` に md のオープナーを書き、README.md に Enter で外部起動・TUI 無事
- 設定ファイルが存在しない環境では、全ファイルで従来どおり TUI プレビューが開く(挙動不変)
- `cargo test` 全緑(既存テスト+新規テスト)
- v2: config に **バージョン番号を書かず** に MSIX アプリが起動でき、アプリ更新後も設定変更なしで動く

## 4. 受け入れ基準(EARS 記法)

正常系:
- WHEN 設定に拡張子が登録されたファイルをファイルツリーで選択する(Enter キーまたは
  マウスクリック — 両経路は同一の共通メソッドを通ること)
  THE SYSTEM SHALL 設定されたコマンドを null stdio で spawn し、フォーカスをツリーに残し、
  ステータスバーに起動結果を表示する
- WHEN オープナーが発火する
  THE SYSTEM SHALL args 中の `{file}` を絶対パスに置換し、`{file}` が無ければ末尾に自動付加する
- WHEN 拡張子の大文字小文字が設定と異なる(例: README.MD)
  THE SYSTEM SHALL 小文字化比較でマッチさせ外部起動する

異常系・フォールバック:
- WHEN 設定に未登録の拡張子のファイルを Enter する
  THE SYSTEM SHALL 従来どおり TUI プレビューを開く
- WHEN 設定ファイルが無い、または TOML パースに失敗する
  THE SYSTEM SHALL オープナー空で起動を継続し(fail-open)、失敗時のみ警告を表示する
- WHEN 外部コマンドの spawn が失敗する(コマンド不在・引数エスケープ拒否等)
  THE SYSTEM SHALL TUI プレビューにフォールバックし、ステータスバーにエラーを表示する
- WHEN ディレクトリを Enter する
  THE SYSTEM SHALL 従来どおり展開/折りたたみのみ行う(オープナー判定をしない)

### v2 で追加する受け入れ基準

- WHEN `mode = "default"` のオープナーが発火する
  THE SYSTEM SHALL `command`/`args` を無視し、OS の既定ランチャ(Windows: explorer.exe /
  macOS: open / Linux: xdg-open)へ絶対パス 1 個だけを渡す
- WHEN 複合拡張子("tar.gz")とより短い拡張子("gz")の両方が登録されている
  THE SYSTEM SHALL ドット境界の末尾一致で判定し、より長い一致(tar.gz)を優先する
- WHEN `filenames` に完全一致するファイル(Makefile 等・拡張子なし)を開く
  THE SYSTEM SHALL `extensions` のどのマッチよりも優先してそのオープナーを使う
- WHEN ファイルツリーで `o` キーを押す
  THE SYSTEM SHALL 設定を参照せず選択中エントリ(ディレクトリ含む)を常に OS 既定ランチャへ渡す
- WHEN ファイルツリーで `r` キーを押す
  THE SYSTEM SHALL config.toml を再読み込みし、結果をステータスバーに表示する
- WHEN `r` による再読み込みで TOML パースに失敗する
  THE SYSTEM SHALL **直前に読み込めていた設定を保持したまま** 警告のみ表示する
- WHEN `mode = "command"`(既定)で `command` の無いオープナーがある
  THE SYSTEM SHALL そのオープナーをマッチ対象から除外し、警告を表示する

v2 で意図的に変わる既存挙動: `extensions = ["md"]` が `.md` という名のドットファイルにもマッチする
(v1 は `Path::extension()` が None で非マッチ)。改善として受け入れ、テストで固定する。

### v3 で追加する受け入れ基準

- WHEN Windows で `mode = "default"` のオープナーが発火し、その拡張子に既定アプリが
  登録されていない THE SYSTEM SHALL spawn せず TUI プレビューを開き、既定アプリ未登録
  である旨をステータスバーに表示する(Enter/クリック/`o` の全経路で同一)
- WHEN 拡張子が無いファイル・ディレクトリ・判定 API が想定外の結果を返す
  THE SYSTEM SHALL 従来どおり起動を試みる(fail-open)
- WHEN macOS または Linux THE SYSTEM SHALL v2 と完全に同一の挙動を保つ(判定しない)

### v4 で追加する受け入れ基準

- WHEN `open_directory_with_os_default` が未設定または true でディレクトリに `o`
  THE SYSTEM SHALL v2/v3 と完全に同一の挙動(OS 既定ランチャへ委譲)を保つ
- WHEN false でディレクトリに `o` THE SYSTEM SHALL 外部プロセスを一切 spawn せず、
  Enter と同じ展開/折りたたみを行い、無効である旨をステータスバーに表示する
- WHEN false でファイルに `o` THE SYSTEM SHALL v3 と同一(関連付け判定 → 起動 or
  プレビュー)。この設定はファイル経路に影響しない
- WHEN `r` キーを押す THE SYSTEM SHALL `open_directory_with_os_default` の変更を
  再起動なしで反映する

## 5. 制約(条件⑤)

- 技術制約: Rust stable / ratatui + crossterm。rustfmt 非適用(cargo fmt 禁止)。
  依存は toml + serde(derive) のみで **v2 でも新規依存を追加しない**。
  シェル文字列組み立て禁止(args ベクタ直接渡し)。v4 も新規依存を追加しない
  (`default_true()` を自前定義、`serde_aux` 等は使わない)
- upstream 制約: 設定なし環境で挙動が完全不変であること(PR 前提)。法令/規程: 該当なし

## 6. スコープ外

- 設定の**自動**ホットリロード(ファイル監視はしない。v2 は `r` キーの手動リロードのみ)
- `scrollback_lines` のリロード反映(vt100 Parser 生成時に固定されるため再起動が必要)
- プレビューパネル内からの外部起動キー(トリガーはツリーの Enter / クリック / `o` のみ)
- TUI 内での Markdown 装飾レンダリング・Mermaid 描画
- AUMID 直接指定(COM の ActivateForFile が必要で windows crate 依存が増えるため不採用)
- グロブパターンによるマッチ(末尾一致 + 完全一致で足りるため)
- 子プロセスの成否追跡は行わない(spawn 成功=起動成功として扱う)。ただし v3 で
  **Windows の `mode = "default"` に限り**、spawn 前に既定アプリの有無を事前判定する
  (機構は起動成否の追跡ではなく事前確認であり、上記の原則とは独立)
- macOS/Linux での既定アプリ有無判定(duti 等の外部ツール依存、xdg-mime + 拡張子→MIME
  変換が別途必要でコストに見合わないため v3 でも見送り。挙動は v2 のまま不変)
- ファイル側 `o` の無効化(v4 の対象はディレクトリのみ)
- `[general]` 等のサブテーブル化(v4 は既存 `Config` 構造体にトップレベルでフラット追加)

## 7. 未知の未知の決定事項(Phase 2 の承認結果)

- .cmd シム(VS Code 等) → PATHEXT 非解決のため `code.cmd` と拡張子まで書く旨をドキュメント明記
- コンソール巻き込み → 全 stdio を `Stdio::null()` + Windows は `creation_flags(CREATE_NO_WINDOW)`
- Unix ゾンビ → 使い捨てスレッドで `child.wait()`(reaper)
- 拡張子マッチ → 小文字化比較・ドットなし表記 / 設定パース失敗 → fail-open(警告のみ)
- 起動後フォーカス → ファイルツリーに留まる

### v2 の承認結果(2026-08-03)

- MSIX 依存の解消 → OS 既定アプリへの委譲(`mode = "default"`)。AUMID 直叩きは不採用
- ディレクトリ → `o` キー(Enter は展開/折りたたみのまま。ツリー操作性を落とさない)
- リロード → `r` キーの手動のみ(自動監視は保存途中の壊れた TOML を拾うため不採用)
- `o` は OS のダブルクリック相当であり、`.exe` / `.bat` を選ぶと実行される点は許容する

### v3 の承認結果(2026-08-15)

- 判定手段 → `AssocQueryStringW`(shlwapi.dll、`extern "system"` 直接 FFI、新規 crate なし)
- 対象範囲 → Windows のみ確実実装。macOS/Linux はスタブで現状維持(型レベルで保証)
- 適用箇所 → `mode = "default"` のみ、Enter/クリック/`o` の全経路
- 判定タイミング → spawn 直前で毎回(レジストリ参照のみで高速、キャッシュ不要)

### v4 の承認結果(2026-08-15)

- 既定値 → true(現状維持)。upstream 制約(設定なし環境で挙動不変)を優先
- 配置 → `Config` 構造体にトップレベルでフラット追加。サブテーブルは新設しない
- false 時の挙動 → 展開/折りたたみへフォールバック(Enter 相当)+ ステータス表示
- `r` キー → 即時反映(`openers` と同じ扱い。`scrollback_lines` とは異なる)
