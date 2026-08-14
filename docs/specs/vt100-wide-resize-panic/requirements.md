# requirements.md — vt100-wide-resize-panic

> vt100 crate の wide 文字パニックで ccmux プロセス全体が落ちる問題の要求仕様。

## 1. 背景と目的

vt100-0.16.2 (`screen.rs:870`) の `.unwrap()` が `None` を踏んで panic した。release
プロファイルは `panic = "abort"`（`Cargo.toml:28`）のため、1 ペインの PTY reader スレッドの
panic が ccmux プロセス全体を即死させ、全タブ・全ペインのセッションを同時に失う。
根本原因は vt100 内部（`Row::resize` が縮小時に wide 文字の継続セルを後始末しない）で、
upstream (doy/vt100-rust) は Issue #28 を Open のまま 1 年以上更新停止している。

## 2. 利用者と保守者（条件(3)）

- 利用者: ccmux を日常的に使う開発者（自分自身含む）。Claude Code をペイン内で起動・終了・
  リサイズする操作を頻繁に行う
- 保守者: このリポジトリの単独メンテナ（doi）。vt100 が今後アップデートされない前提で
  vendored パッチを自前で維持する

## 3. 成功基準（条件(1)）

- ペイン幅が 1 桁まで縮んだ状態で全角文字を描画してもプロセスが落ちない
- alternate screen（Claude Code）使用中にリサイズし、終了後にシェルが描画してもプロセスが
  落ちない
- 上記 2 経路の自動回帰テストが `cargo test` / `cargo test -p vt100` で green
- 万一未知の panic が発生しても、クラッシュログにペイン特定可能な情報（thread 名・location）
  が残る

## 4. 受け入れ基準（EARS 記法）

- WHEN ペイン幅が `saturating_sub` 等で 1 未満に計算される THE SYSTEM SHALL PTY/vt100 へは
  最低 2 桁（rows は最低 1 行）を渡す
- WHEN 幅 1〜2 桁のペインに全角文字が描画される THE SYSTEM SHALL panic せず描画を継続する
- WHEN alternate screen 中にペインがリサイズされ、その後 alternate screen を抜けて描画が
  発生する THE SYSTEM SHALL panic しない
- WHEN `Pane::resize` に同一サイズが連続して渡される（毎フレームの再計算含む） THE SYSTEM
  SHALL 実際のリサイズ処理（PTY resize / vt100 set_size / 画面クリア）を再実行しない
- WHEN 何らかの理由で未知の panic が発生する THE SYSTEM SHALL 端末状態を復元し、
  `<config_dir>/ccmux/crash.log` に時刻・バージョン・スレッド名・location・メッセージを
  追記する
- WHEN クラッシュログが 1MB を超える THE SYSTEM SHALL 既存ログを破棄してから新規に書き出す
  （無制限増殖させない）

## 5. 制約（条件(5)）

- 技術制約: `Cargo.toml` の `[profile.release]`（`lto/codegen-units/strip/panic=abort/
  opt-level=s`）は変更しない。バイナリサイズ最適化を裏切らない
- 技術制約: vt100 のバージョンアップやマージ待ちに依存しない（upstream 停止中のため）
- 技術制約: vendoring は upstream (Shin-sibainu/ccmux) への PR 対象に含めない
- 法令/規程: 該当なし（クラッシュログに個人情報は含まれない。パスとスレッド名のみ）
- 期限: なし

## 6. スコープ外

- `catch_unwind` によるパニック封じ込め（ユーザー決定により見送り）
- クラッシュログのバックトレース収集（strip 設定と競合するため見送り）
- scrollback 行のリサイズ対応（表示上のギャップのみで panic しないため対応不要と確認済み）
- 非アクティブ grid（alternate/normal のどちらか）をリサイズ時に個別クリアする回避策
  （vendored パッチで不要になる。かつ vt100 に公開 API がない）

## 7. 未知の未知の決定事項（Phase 2 相当の確認結果）

- vt100 バグ本体の直し方 → vendored コピーにパッチ（`[patch.crates-io]`）。fork や
  マージ待ちは upstream 停止のため不採用
- panic 全般への保険 → catch_unwind は入れず、パニックフックでの記録強化のみ
- vendored パッチの内容 → 自前で書き直さず upstream PR #30 の diff をそのまま採用
  （将来のマージ時に再適用コストを最小化するため）
