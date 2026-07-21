# requirements.md — Claude 検知のヒステリシス化

## 1. 背景と目的

ccmux はペインで claude が動作中かをウィンドウタイトル（OSC 0/2）で検知し、枠色・カーソル・
ステータスバーに反映する。しかし Claude Code は UI 動作中、タイトルを `<スピナー> <セッション名>`
に書き換える（実機ログで確定: `claude` → `✳ ccmux-display-optimization` → スピナー回転）。
セッション名が付いた claude は現行の単発判定 `title_indicates_claude` で `matched=false` になり、
ペインが恒久的に "shell" 表示になる。起動時に必ず来る `claude` を捉えて状態を保持する
ヒステリシス方式で解決する。

## 2. 利用者と保守者

- 利用者: 開発者本人（claude をセッション名付き・continue 再開で日常使用）
- 保守者: 本人 + upstream (Shin-sibainu/ccmux)

## 3. 成功基準

- セッション名/トピック名が付いた claude でもペインが claude 表示（オレンジ枠）になる
- claude 終了後はシェル表示に戻る
- claude を過去に使った cwd でただのシェルを開いても claude 誤検知しない
- 既存の title 検知（名前無し claude = `Claude Code`）の挙動は維持

## 4. 受け入れ基準（EARS 記法）

- WHEN title が `claude` / `Claude Code` / `claude · ` のいずれか（現 `title_indicates_claude`）THE SYSTEM SHALL その pane を claude 動作中とマークする
- WHEN claude 系タイトルを見た後にセッション名・スピナー付きタイトルへ変わった THE SYSTEM SHALL claude 動作中の状態を維持する
- WHEN title がシェルプロンプト系（`MINGW64:` / `MSYS:` 始まり、`*.exe` 終わり等）へ戻った THE SYSTEM SHALL その pane を claude 動作中でないとマークする
- WHEN pane 起動直後でまだ claude 系タイトルを一度も見ていない THE SYSTEM SHALL claude 動作中でない（shell 扱い）とする
- WHEN 同じ cwd に過去の claude 履歴（JSONL）が残る状態でシェルを開いた THE SYSTEM SHALL claude 誤検知しない

## 5. 制約

- 既存スタック（vt100 title / Arc）のまま。**JSONL・alternate screen は検知に使わない**（却下理由は design ADR-1 参照）
- Windows / Git Bash 前提。PowerShell / zsh / カスタムプロンプトのシェルタイトル網羅は best-effort（不足時に shell init マーカーで補強＝別スコープ）
- 既存テスト（46+）を全て維持

## 6. スコープ外

- PowerShell の完全なシェルプロンプトタイトル検出（shell init フックが現状 bash/zsh のみのため。不足が判明したら別途 shell init マーカーで補強）
- `/rename` されたセッション名から claude を「名前で」特定すること（タイトルだけでは原理的に不可能。ヒステリシスで状態保持することで回避する）

## 7. 未知の未知（Phase 2 調査済み・repro で確定）

- **alternate screen 検知の却下**: claude は起動時に一瞬だけ alt screen に入って出る（ダンプ実測: enter 1 / leave 1）だけで、UI 動作中・会話中は通常スクリーンにインライン描画する。恒常的な alt screen 状態では検知できない。
- **JSONL 併用の却下**: `find_jsonl_path` は cwd をエンコードした最新 JSONL を返すだけで、claude 終了後も JSONL は残る。claude を使った cwd でシェルを開くと過去 JSONL がヒットし誤検知する（ccmux で claude を使う cwd はまさにこれに該当し頻発する）。
