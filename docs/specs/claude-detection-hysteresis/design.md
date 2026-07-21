# design.md — Claude 検知のヒステリシス化

## 1. 全体構成（条件⑦）

```
pty_reader_thread (pane.rs) が OSC 0/2 title イベントごとに claude_active を遷移:
  title claude系      → claude_active = true
  title シェル系      → claude_active = false
  それ以外(セッション名/スピナー) → 維持
is_claude_running() → claude_active.load()   （現行: title を毎回再判定）
ui.rs (枠色/カーソル/ステータスバー) は bool を消費するだけで無変更
```

## 2. 出力契約（条件④）

| パス | 目的 |
|------|------|
| `src/pane.rs` | `Pane.claude_active: Arc<AtomicBool>` 追加 / reader スレッドで状態遷移 / `is_claude_running` を `claude_active` 参照へ / `title_is_shell_prompt()` 新設 + テスト |

## 3. 設計判断（ADR 形式）

### ADR-1: 検知方式

- **状況**: claude が UI 動作中にタイトルを `<スピナー> <セッション名>` へ書き換えるため、単発の `title_indicates_claude` が false になる
- **案A: JSONL monitor 併用** — `find_jsonl_path` は cwd の最新 JSONL を返すのみ。claude 終了後も残り、過去に claude を使った cwd のシェルを誤検知（構造的欠陥）→ 却下
- **案B: alternate screen 検知** — claude は起動時に一瞬 alt screen に入るだけ（repro 実測 enter1/leave1）で恒常的に通常スクリーン → 却下
- **案C: title ヒステリシス** — 起動時に必ず来る `claude`（実機ログで全 pane 確認）を ON トリガーにし、状態保持 → **採用**

### ADR-2: OFF トリガー（claude 終了検知）

- **案A: 既存シェルタイトル検出** — title が `MINGW64:`/`MSYS:` 始まり・`*.exe` 終わり等に戻ったら OFF。変更小・Git Bash（主要シェル）で確実 → **採用**
- **案B: shell init マーカー注入** — shell 起動フックに OSC 0 固定マーカーを追加。確実だが bash/zsh の init 拡張が必要で PowerShell は init 自体が無い → 見送り（案A で不足時に補強）

## 4. 状態遷移の詳細

`claude_active`（初期 `false`）を title イベントごとに更新:

```
if title_indicates_claude(t)   { claude_active = true }
else if title_is_shell_prompt(t) { claude_active = false }
// else: 変更しない（セッション名・スピナー付きタイトルは維持）
```

`title_is_shell_prompt(t)`（小文字化して判定）:
- `mingw64:` / `mingw32:` / `msys:` で始まる（Git Bash / MSYS）
- `.exe` で終わる（`...bash.exe` / `powershell.exe` / `cmd.exe` / `pwsh.exe` — 起動時にプロセス名がタイトルになるケース）

## 5. データとライフサイクル

- `claude_active: Arc<AtomicBool>`（`Ordering::Relaxed`）。reader スレッドが書き、描画スレッドが読む。既存の `total_scrollback: Arc<AtomicUsize>` と同じ共有パターン。`title: Arc<Mutex<String>>` と並存（title 自体は表示・OSC 追跡に引き続き保持）
- pane ごとに独立。pane 破棄で Arc も解放

## 6. 障害モードと回復

- **未知のシェルタイトル**（PowerShell のプロンプトが `MINGW64:`/`.exe` に当たらない形式等）→ claude 終了後も `claude_active=true` が残る誤検知。best-effort と割り切り、shell init マーカー（ADR-2 案B）で将来補強可能
- **claude が `claude` を出さずに起動**（現状ありえない。実機ログで起動時 `claude` は必ず来る）→ ON にならない。リスクは実測で否定済み
- **claude 内でシェルコマンド実行**（claude は通常スクリーンでインライン描画）→ タイトルは claude 制御下のまま、シェルプロンプト系にはならないので `claude_active` は維持される

## 7. 参照実装（条件⑥）

- ON 判定は既存 `title_indicates_claude`（pane.rs）を無変更で流用
- `Arc<AtomicBool>` の共有は `scrollback_counter`（pane.rs `new_with_cwd`）と同じ渡し方（clone を reader スレッドへ move）
