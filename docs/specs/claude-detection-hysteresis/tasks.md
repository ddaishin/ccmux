# tasks.md — Claude 検知のヒステリシス化

## 実装順序

T1 → T2 → T3。各タスク＝1 コミット。全タスクで既存テストが green であること。

## タスク一覧

### T1: `title_is_shell_prompt` 関数と単体テスト

- **内容**: `src/pane.rs` に `fn title_is_shell_prompt(title: &str) -> bool` を新設。小文字化して
  `mingw64:` / `mingw32:` / `msys:` 始まり、または `.exe` 終わりを判定。単体テストで
  「claude 系（false を返す）/ シェル系（true）/ セッション名（false）/ 空文字（false）」を網羅
- **依存**: なし
- **完了条件（機械検証）**: `cargo test` exit 0（新規テスト green）/ `cargo clippy` 新規警告 0

### T2: `claude_active` ヒステリシスの実装

- **内容**: `Pane` に `claude_active: Arc<AtomicBool>` を追加。`new_with_cwd` で生成し reader スレッドへ
  clone を渡す（`scrollback_counter` と同じ方式）。`pty_reader_thread` の title 検知ブロックで
  `title_indicates_claude(t)` なら `true`、`title_is_shell_prompt(t)` なら `false` をストア（else は据え置き）。
  `is_claude_running()` を `self.claude_active.load(Relaxed)` へ変更
- **依存**: T1
- **完了条件（機械検証）**: `cargo test` exit 0 / `cargo clippy` 新規警告 0 / `cargo build --release` exit 0 /
  grep で `is_claude_running` が `title.lock()` を直接見ていないこと（claude_active 参照へ移行済み）

### T3: 実機確認と evaluator レビュー

- **内容**: `CCMUX_TITLE_LOG` デバッグ版で使っていた npm バイナリを、本実装のリリースビルドへ差し替え
  （既存の rename 方式・`ccmux.exe.perf-nodebug` から更に退避）。手動で受け入れ基準を確認。
  CLAUDE.md の Workflow Rules に従い evaluator agent の静的レビューを通す
- **依存**: T2
- **完了条件**: 統合検証チェックリストが全て ☑ / evaluator が Critical/Major なし

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| セッション名付き claude が claude 表示 | `next_claude_active` ライフサイクルテスト（named→held）で自動実証済み。実機は新バイナリ起動後に目視 | ☑（単体）/ ☐（実機） |
| claude 終了で shell 表示へ | ライフサイクルテスト（`MINGW64:`→OFF）で自動実証済み。実機は目視 | ☑（単体）/ ☐（実機） |
| 過去 JSONL cwd で誤検知なし | 検知は title のみ由来で JSONL 不参照（設計上構造的に成立）。実機で目視 | ☑（設計）/ ☐（実機） |
| 名前無し claude（既存挙動）維持 | `next_claude_active(_, "✳ Claude Code")` テストで実証済み | ☑ |
| 2 枚目のタブでも検知 | 検知は pane 単位で 1 枚目と同一経路（元不具合の原因はタイトル判定であってタブではないと確定）。実機で目視 | ☑（設計）/ ☐（実機） |

**実機目視**は、現在動作中の ccmux を次回起動し直したとき（新バイナリ配置済み）に、このセッション自身の枠がオレンジになることで確認できる。

## 実装セッションへの引き継ぎ事項

- `title_indicates_claude` は無変更（ON 判定に流用）。スピナー strip 済みの既存ロジックを尊重
- `claude_active` は `Arc<AtomicBool>` / `Ordering::Relaxed`（`total_scrollback` と同じ）
- title 自体は引き続き `Arc<Mutex<String>>` に保持（OSC 追跡・将来の表示用）。ヒステリシスは別フィールド
- デバッグ用の `CCMUX_TITLE_LOG` / alt screen ログは `debug/title-detection-log` ブランチにあり、本実装には含めない
- evaluator レビュー必須（Rust TUI・Playwright 不可のため静的レビュー）
