# requirements.md — external-opener(拡張子別・外部ビューワー起動機構)

## 1. 背景と目的

MD ファイルを Mermaid 対応の外部ビューワー(Typora 等)で見たまま編集したいが、ccmux の
TUI プレビューは syntect ハイライトのみで Mermaid 描画・WYSIWYG 編集は実現不能。
拡張子ごとに外部アプリを設定できる汎用オープナー機構を新設する。

## 2. 利用者と保守者(条件③)

- 利用者: ccmux 利用者(当面は doi。upstream マージ後は全ユーザー)
- 保守者: doi(ローカルパッチ運用)。upstream PR 後は Shin-sibainu

## 3. 成功基準(条件①)

- `%APPDATA%\ccmux\config.toml` に md → Typora のオープナーを書き、ファイルツリーで
  README.md に Enter すると Typora が起動し、TUI 表示が壊れない
- 設定ファイルが存在しない環境では、全ファイルで従来どおり TUI プレビューが開く(挙動不変)
- `cargo test` 全緑(既存テスト+新規テスト)

## 4. 受け入れ基準(EARS 記法)

正常系:
- WHEN 設定に拡張子が登録されたファイルをファイルツリーで Enter する
  THE SYSTEM SHALL 設定されたコマンドを null stdio で spawn し、フォーカスをツリーに残し、
  ステータスバーに起動結果を表示する
- WHEN args に `{file}` を含むオープナーが発火する
  THE SYSTEM SHALL `{file}` を対象ファイルの絶対パスに置換して渡す
- WHEN args に `{file}` を含まないオープナーが発火する
  THE SYSTEM SHALL 絶対パスを args の末尾に自動付加して渡す
- WHEN 拡張子の大文字小文字が設定と異なる(例: README.MD)
  THE SYSTEM SHALL 小文字化比較でマッチさせ外部起動する

異常系・フォールバック:
- WHEN 設定に未登録の拡張子のファイルを Enter する
  THE SYSTEM SHALL 従来どおり TUI プレビューを開く
- WHEN 設定ファイルが存在しない
  THE SYSTEM SHALL オープナー空として動作し、従来挙動を完全に維持する
- WHEN 設定ファイルの TOML パースに失敗する
  THE SYSTEM SHALL 起動を継続し(fail-open)、ステータスバーに警告を表示し、従来挙動とする
- WHEN 外部コマンドの spawn が失敗する(コマンド不在・引数エスケープ拒否等)
  THE SYSTEM SHALL TUI プレビューにフォールバックし、ステータスバーにエラーを表示する
- WHEN ディレクトリを Enter する
  THE SYSTEM SHALL 従来どおり展開/折りたたみのみ行う(オープナー判定をしない)

## 5. 制約(条件⑤)

- 技術制約: Rust stable / ratatui + crossterm。rustfmt 非適用(cargo fmt 禁止)。
  新規依存は toml + serde(derive) のみ。シェル文字列組み立て禁止(args ベクタ直接渡し)
- upstream 制約: 設定なし環境で挙動が完全不変であること(PR 前提)
- 法令/規程: 該当なし(個人情報を扱わない)

## 6. スコープ外

- 設定のホットリロード(読み込みは起動時 1 回)
- プレビューパネル内からの外部起動キー(トリガーはファイルツリーの Enter のみ)
- TUI 内での Markdown 装飾レンダリング・Mermaid 描画
- オープナー以外の設定項目(将来の config.toml 拡張は本機能の範囲外)

## 7. 未知の未知の決定事項(Phase 2 の承認結果)

- .cmd シム(VS Code 等) → PATHEXT 非解決のため `code.cmd` と拡張子まで書く旨をドキュメント明記。
  Rust 1.77.2+ の BatBadBut 対応エスケープに委ね、拒否時はフォールバック経路で処理
- コンソール巻き込み → 全 stdio を `Stdio::null()` + Windows は `creation_flags(CREATE_NO_WINDOW)`
- Unix ゾンビ → 使い捨てスレッドで `child.wait()`(reaper)
- 拡張子マッチ → 小文字化比較・ドットなし表記
- 設定パース失敗 → fail-open(警告のみ)
- 起動後フォーカス → ファイルツリーに留まる
