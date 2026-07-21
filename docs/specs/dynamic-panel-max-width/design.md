# design — サイドバー/プレビュー幅ドラッグ上限の動的化

## 動的最大値の式（layout_swapped 非依存・対称）

```
max_tree_width    = term_w − MIN_PANE_AREA_WIDTH − (preview 描画中 ? preview_width : 0)
max_preview_width = term_w − MIN_PANE_AREA_WIDTH − (tree 描画中 ? file_tree_width : 0)
```

- `term_w = App::last_term_size.0`（ui.rs で毎フレーム `frame.area()` から更新。
  メイン領域は横幅フルなので端末幅をそのまま使える）
- `layout_swapped` は preview の位置（ツリー右隣 / 右端）だけを変え幅合計は
  不変のため、max の式に swapped 分岐は不要

## ADR-1: 相手側パネルの「描画中」判定

- 案 A: `preview.is_active()` / `file_tree_visible`（論理状態）
- 案 B: `last_preview_rect.is_some()` / `last_file_tree_rect.is_some()`（描画実態）— **採用**

理由: 狭小フォールバック時は「論理 ON だが非描画」があり得る。案 A では画面に
存在しないパネル幅まで差し引き上限が不当に縮む。案 B は描画実態と一致する
（1 フレーム遅れだがドラッグは連続イベントで実害なし）。なおドラッグ開始は
境界が描画されているときのみ成立するため、自パネル側の描画は保証される。

## ADR-2: 実装の配置

- 案 A: ドラッグハンドラにインライン計算 — 変更最小だが式と panic ガードが
  テスト不能（`App` は PTY 起動を伴い単体テストで生成困難）。式が 2 回重複
- 案 B: `App` メソッドのみ — `&self` 依存で同じくテスト不能
- 案 C: モジュールレベル純粋関数 `max_side_panel_width(term_w, other_panel_w,
  min) -> u16` ＋ App 薄ヘルパー `max_file_tree_width()` /
  `max_preview_width()` — **採用**

理由: pane.rs `next_claude_active` の前例と同型。ロジックと panic ガードを
純粋関数に集約し `cargo test` で直接検証。ヘルパーは状態読み出し 2 行のみ。

## ADR-3: clamp panic ガード

Rust の `clamp(min, max)` は min > max で panic する（例: 端末幅 40・
preview 40 描画中に tree ドラッグ → max 0 < min 10）。純粋関数内で

```rust
term_w.saturating_sub(MIN_PANE_AREA_WIDTH)
      .saturating_sub(other_panel_w)
      .max(min)
```

とし「戻り値は常に min 以上」を関数契約にする（doc コメント明記）。
`saturating_sub` で u16 アンダーフローも防止。縮退時は max == min となり
幅は min 固定 → 既存フォールバックが引き継ぐ。

## ADR-4: MIN_PANE_AREA_WIDTH の重複解消

現状 ui.rs:27（モジュール const）と app.rs relayout_panes 内ローカル const の
2 箇所（「ui と同期を保つ」コメント付き）。今回参照が増えるため:

- 案 A: 触らず 3 箇所目を作る — 同期コメント頼みの重複が悪化
- 案 B: app.rs モジュールレベル `pub(crate)` に一本化 — **採用**
- 案 C: ui.rs 側を `pub(crate)` 化 — app → ui の新規依存が発生し非推奨

理由: レイアウトのモデル側の真実（relayout_panes・新ヘルパー）が app.rs に
あり、ui.rs は描画時にミラーする側。依存方向は既存どおり ui → app のまま。
値 20 は不変で挙動リスクなし。あわせてマジックナンバー 10 / 15 も
`MIN_FILE_TREE_WIDTH` / `MIN_PREVIEW_WIDTH` として定数化する。

## 変更ファイル

- src/app.rs: 定数 3 つ・純粋関数・ヘルパー 2 つ・ドラッグハンドラ書き換え・
  ローカル const 削除・テスト 6 ケース（+70/−5 行程度）
- src/ui.rs: const 削除 → `crate::app::MIN_PANE_AREA_WIDTH` 参照（+1/−1 行）
