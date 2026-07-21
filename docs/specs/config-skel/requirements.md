# requirements.md — config-skel(config.toml の初回 skel 自動生成)

## 1. 背景と目的

external-opener の設定ファイル `%APPDATA%\ccmux\config.toml` は現状ユーザーが手で作る
必要があり、不在時は黙って空設定になるため、設定の存在も書式も発見できない。
初回起動時に全コメントアウトの雛形(skel)を自動生成し、発見可能にする。

## 2. 利用者と保守者(条件③)

- 利用者: ccmux 利用者(external-opener を使い始める人)
- 保守者: doi(ローカルパッチ運用)。PR #22 再提出時に upstream へ

## 3. 成功基準(条件①)

- config.toml が無い状態で ccmux を起動すると、`%APPDATA%\ccmux\config.toml` に
  全コメントの雛形が生成され、ステータスバーに生成通知が一度表示される
- 生成された雛形はそのままでは挙動を一切変えない(全コメントのため空設定と等価)
- 既存の config.toml がある環境では何も起きない(内容・タイムスタンプ不変)
- `cargo test` 全緑(既存 74 件+新規 4 件)

## 4. 受け入れ基準(EARS 記法)

正常系:
- WHEN 起動時に config.toml が存在しない(read が ErrorKind::NotFound)
  THE SYSTEM SHALL ディレクトリを create_dir_all で作成し、create_new で雛形を書き込み、
  ステータスバーに「設定ファイルの雛形を作成しました: <パス>」を表示する
- WHEN 生成された雛形をそのままパースする
  THE SYSTEM SHALL 空の openers として読み込む(挙動不変)

異常系・境界:
- WHEN config.toml が既に存在する
  THE SYSTEM SHALL 生成を行わず、既存ファイルの内容を変更しない
- WHEN NotFound 判定と書き込みの間に別プロセスがファイルを作成した(TOCTOU)
  THE SYSTEM SHALL create_new の AlreadyExists 失敗により既存ファイルを上書きしない
- WHEN 雛形の書き込みに失敗する(権限・読み取り専用ディレクトリ等)
  THE SYSTEM SHALL 通知なしで起動を継続する(fail-open。従来挙動と同一)
- WHEN read が NotFound 以外のエラー(権限拒否等)
  THE SYSTEM SHALL 雛形生成を試みず、従来どおり空設定で継続する

## 5. 制約(条件⑤)

- 技術制約: Rust stable / 新規依存なし(テストも std のみ、tempdir crate 不使用)。
  rustfmt 非適用。既存 `Config::load() -> (Config, Option<String>)` のシグネチャを維持し
  main.rs / app.rs は変更しない
- upstream 制約: 生成物は全コメント=挙動不変であること(PR 再提出に含めるため)
- 法令/規程: 該当なし

## 6. スコープ外

- 有効な既定値入りの config 生成(挙動が変わるため不採用)
- 生成の抑止設定(--no-skel 等のフラグ)
- dist/ の config.toml.sample の自動同期機構(今回は手動で同内容に揃えるのみ)

## 7. 未知の未知の決定事項(Phase 2 は省略承認済み。既知の作法で対処)

- TOCTOU → `OpenOptions::create_new`(check-then-write でなく原子的作成)
- 生成失敗 → fail-open(起動を止めない・通知もしない)
- NotFound 以外の read エラーでは生成しない(権限問題のディレクトリへ書き込みに行かない)
