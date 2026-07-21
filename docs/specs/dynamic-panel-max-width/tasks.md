# tasks — サイドバー/プレビュー幅ドラッグ上限の動的化

各タスクの完了条件は機械検証可能なコマンドで示す。

## T1: 定数と純粋関数の追加（src/app.rs）

- モジュールレベルに追加:
  - `pub(crate) const MIN_PANE_AREA_WIDTH: u16 = 20;`
  - `const MIN_FILE_TREE_WIDTH: u16 = 10;`
  - `const MIN_PREVIEW_WIDTH: u16 = 15;`
- 純粋関数 `fn max_side_panel_width(term_w: u16, other_panel_w: u16, min: u16)
  -> u16` を追加。`saturating_sub` ×2 → `.max(min)`。doc コメントに
  「戻り値は常に min 以上（clamp panic 防止契約）」を明記。
- **完了条件**: `cargo build` 成功

## T2: App ヘルパー追加（src/app.rs・relayout_panes 近く）

- `fn max_file_tree_width(&self) -> u16` — 相手幅は
  `self.ws().last_preview_rect.is_some()` なら `self.preview_width`、else 0
- `fn max_preview_width(&self) -> u16` — 相手幅は
  `self.ws().last_file_tree_rect.is_some()` なら `self.file_tree_width`、else 0
- **完了条件**: `cargo build` 成功

## T3: ドラッグハンドラ書き換え（src/app.rs:1368-1382）

- `FileTreeBorder`: `self.file_tree_width =
  col.clamp(MIN_FILE_TREE_WIDTH, self.max_file_tree_width());`
- `PreviewBorder`: `max` を先に取得 → swapped 分岐で `new_width` 算出 →
  `new_width.clamp(MIN_PREVIEW_WIDTH, max)`
- ペイン分割比（0.15〜0.85）・Scrollbar 分岐は変更しない
- **完了条件**: `cargo build` 成功・`git diff` に update_ratio /
  split_rect の変更が含まれない

## T4: MIN_PANE_AREA_WIDTH 一本化

- app.rs relayout_panes 内のローカル `const MIN_PANE_AREA_WIDTH` 削除
  （モジュール const を参照）
- ui.rs:27 の const 削除 → use に `crate::app::MIN_PANE_AREA_WIDTH` 追記
- **完了条件**: `grep -n "MIN_PANE_AREA_WIDTH: u16" src/*.rs` の定義が
  app.rs の 1 箇所のみ・`cargo build` 成功

## T5: ユニットテスト（src/app.rs 既存 mod tests）

| テスト名（test_max_side_panel_width_*） | 入力 → 期待値 |
|---|---|
| wide_screen | (200, 40, 10) → 140 |
| other_hidden | (200, 0, 10) → 180 |
| preview_symmetry | (120, 20, 15) → 80 |
| exact_boundary | (70, 40, 10) → 10 |
| degenerate_clamps_to_min | (40, 40, 10) → 10・`100u16.clamp(10, ret)` が panic しない |
| zero_term | (0, 0, 15) → 15 |

- **完了条件**: `cargo test` 全緑（既存テスト含む）

## T6: evaluator レビュー（プロジェクト必須）

- evaluator エージェントで静的レビュー: diff 分析・clamp panic 経路・
  ui.rs / relayout_panes のレイアウト整合・エッジケース
- **完了条件**: 指摘ゼロ、または指摘反映後に `cargo test` 全緑

## T7: 実機確認とコミット

- `cargo run`: 広い端末で tree を 60 超・preview を 80 超まで拡大できる。
  端末を極端に狭めてドラッグしても panic しない
- コミット: `feat(ui): サイドバー/プレビュー幅のドラッグ上限を画面幅から動的算出`
  （`local/v0.6.2-patched` 上・メッセージは `-F` ファイル渡し）
- **完了条件**: `git log -1 --oneline` に上記コミットが存在
