# vendored vt100 — 由来とローカルパッチ

## 由来

- upstream: https://github.com/doy/vt100-rust
- version: 0.16.2 (crates.io, MIT)
- upstream commit (`.cargo_vcs_info.json` より): `eb66ffaf7d771c13303ef73b29f6f2a56fdacecf`
- 取得元: `~/.cargo/registry/src/index.crates.io-*/vt100-0.16.2/`
- upstream 最終コミット: 2025-07-12（このパッチ作成時点で1年以上停止中。0.16.2 が最新）

## 展開版からの変更（パッチ以外）

- `Cargo.toml` は registry 正規化版ではなく `Cargo.toml.orig` の内容を採用した。
  正規化版には `autotests = false` が入っており、これを残すと `tests/` 配下が
  無言で無視され `cargo test -p vt100` が `0 tests, ok` を返す
- `[dev-dependencies]`（nix/quickcheck/rand/serde/serde_json/terminal_size）と
  `include` を削除した（path 依存では不要。残すとルート `Cargo.lock` が無駄に膨らむ）
- `Cargo.toml.orig` / `Cargo.lock` / `.cargo_vcs_info.json` / `.cargo-ok` は
  vendored ディレクトリへ持ち込んでいない

## パッチ

### `patches/0001-row-fix-orphaned-wide-cell.patch`

- 出典: doy/vt100-rust **PR #30**（未マージ）/ **Issue #28**
- 取得: `gh pr diff 30 --repo doy/vt100-rust`（2026-08-13 取得）
- 対象: `src/row.rs` の `Row::{erase, resize, clear_wide}`、および
  `#[cfg(test)] mod tests` の追加
- 適用方法: `git apply` がこのディレクトリ配下では対象パスを解決できず
  "Skipped patch" となったため、diff の内容を手動で `src/row.rs` へ反映した
  （`patches/` にはオリジナル diff をそのまま保存してあり、内容の突合は
  この diff と実際の `src/row.rs` を diff すれば確認できる）

**症状**: 縮小 resize が wide 文字（CJK 等 2 カラム文字）の継続セルを切り落とし、
`is_wide()` が立ったまま `col+1` が存在しない最終セルが残る。以後
- `Row::clear_wide` → `cells[col+1]` で `index out of bounds`
- `screen.rs` の `drawing_cell_mut(col+1).unwrap()` → `None` を unwrap

のいずれかで panic する。ccmux は release ビルドで `panic = "abort"`
（`Cargo.toml` `[profile.release]`）のため、1 ペインの PTY reader スレッドの
panic がプロセス全体を即死させる。

**成立根拠**: `Cell::clear()`（`src/cell.rs`）は `self.len = 0` を代入し、
同じバイトにパックされた `IS_WIDE` / `IS_WIDE_CONTINUATION` フラグも同時に
落とす。これにより「孤立した wide セルを `clear()` すれば安全に後始末できる」
という修正が成立する。

**変更内容**:
1. `Row::erase` — `self.cols() - if wide {2} else {1}` を `saturating_sub` へ
   （幅 1 の行で孤立 wide セルを erase すると素の減算はアンダーフローする）
2. `Row::resize` — `cells.resize()` の**前**に旧長 `old_len` を取得し、
   縮小時のみ新しい最終セルが `is_wide()` なら `clear()` する。既存の
   `Row::truncate` が持つ後始末と同じロジック
3. `Row::clear_wide` — `col + 1` が範囲外なら自セルを `clear()` して return
   （元は無条件で `cells[col+1]` へアクセスしていた）
4. インラインテスト `erase_wide_cell_in_one_column_row_does_not_underflow`

### 追加（PR #30 diff に含まれない、防御的変更）

- なし。`clear_wide` の継続セル側（`col - 1` 側）への境界チェック追加は
  検討したが、到達可能な呼び出し元（`Row::remove` ← `Grid::delete_cells`、
  `Row::erase` ← `Grid::erase_row_*` / `erase_cells`）から `col == 0` で
  continuation が立つ状態は構成できないと確認済みのため、バグ修正ではないと
  判断し見送った

## 既知の未修正（意図的）

- **Issue #37 相当**（cols がグリフ幅より狭い、例: cols == 1 に全角文字）:
  `Screen::text` / `Grid::col_wrap` の `size.cols - width` がアンダーフローし
  panic しうる。vendored 側では直していない — upstream が「グリフが入らない
  画面幅」の意味論を意図的に未定義のままにしている領域であり、独自ルールを
  持ち込むと将来の rebase コストが増える。ccmux 側で `pane.rs` の
  `clamp_pty_size`（PTY/vt100 への入力を最低 2 カラムに固定）が対処する
- **scrollback 行は `Grid::set_size` でリサイズされない**: 書き込みは全て
  `drawing_rows_mut` 経由で、読み出しは `Option` を返すため panic はしない。
  症状は「端末を広げた直後、scrollback 部分の右端が描画されない」という
  表示上のギャップのみで、対応不要と確認済み

## 再適用手順（upstream vt100 が更新されたとき）

1. `cargo add vt100@<new>` 等で新版を registry に展開させる
2. 新版の `src/` + `LICENSE`/`README.md`/`CHANGELOG.md` を `vendor/vt100/` へ上書き
3. `Cargo.toml.orig` 相当を `Cargo.toml` として置き直し、`[dev-dependencies]` /
   `include` を除去（このファイルの「展開版からの変更」節を参照）
4. `patches/0001-row-fix-orphaned-wide-cell.patch` を手掛かりに `src/row.rs` の
   `erase` / `resize` / `clear_wide` を再度手動で当て直す
5. `cargo test -p vt100` — **テスト名が表示されること**を必ず確認する
   （`0 tests` なら手順3の `autotests` 罠が再発している）
6. upstream が PR #30 をマージ済みなら `patches/` を削除しこのファイルを更新、
   さらに vendoring 自体（ルート `Cargo.toml` の `[workspace]` /
   `[patch.crates-io]`）の撤去を検討する

## テストの実行方法とCI

`cargo test -p vt100` でこのディレクトリのテスト（`src/row.rs` インライン +
`tests/wide_resize.rs`）が実行される。**CI（`.github/workflows/`）にはテスト
ジョブが存在せず、回帰テストは CI では一切走らない。** テストは手元実行のみ。
