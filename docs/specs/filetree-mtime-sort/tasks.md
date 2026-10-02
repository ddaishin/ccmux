# tasks.md — filetree-mtime-sort(ファイルツリーの更新日時降順ソート)

## 実装順序

T0 → T1 → T2 → T3 → T4 → T5 → T6。並行はしない(T3 の題は T1 の `sort_mode`、OSC 7 の引き継ぎは T2 の `toggle_sort` に依存)。

## タスク一覧

### T0: 準備

- **内容**: `local/v0.6.2-patched` から `feat/filetree-mtime-sort` を切る。変更前の clippy の出力を保存する
- **依存**: なし
- **完了条件(機械検証)**: `git branch --show-current` が `feat/filetree-mtime-sort`。`cargo clippy 2> <scratch>/clippy-before.txt` を保存済み

### T1: 並べ替えの核(filetree.rs)

- **内容**: `SortMode { Modified, Name }`(`Default` = Modified、`label()` で `"更新順"` / `"名前順"`)、`FileEntry.modified: Option<SystemTime>`、`FileTree.sort_mode`、純粋関数 `sort_entries(&mut [FileEntry], SortMode)`(Modified: mtime 降順・同時刻は名前の昇順(大小無視)・`None` は末尾 / Name: 従来の名前昇順)。`scan_directory_filtered` に `sort` 引数を足し、`entry.metadata()` 1 回から symlink・`is_dir`・`modified` を取って `FileEntry` を直接組み立てる(失敗時は `path.is_dir()`)。本体 5 箇所に `sort_mode` を渡し(`toggle_dir_recursive` / `merge_entries` は引数を足す)、既存テスト 4 箇所には `SortMode::Name` を渡す
- **依存**: T0
- **完了条件(機械検証)**: `cargo test filetree` が exit 0。`cargo test filetree -- --list` に `test_sort_modified_desc`・`test_sort_same_mtime_by_name`・`test_sort_none_mtime_last`・`test_sort_name_mode_matches_legacy` が出る(合成の `SystemTime` を使う。`None` のテストはディレクトリ群の末尾に入ることも見る)。既存の filetree テストも緑

### T2: 切替と選択の維持(filetree.rs)

- **内容**: `toggle_sort()`(モード反転 → 保持しているすべての `children` を `is_expanded` にかかわらず再帰で並べ直す → `rebuild_flat` → 選択パスの復元)。`auto_refresh_if_needed` のパス復元とクランプを `restore_selection(Option<PathBuf>)` に抽出して両方から使う
- **依存**: T1
- **完了条件(機械検証)**: `cargo test filetree` が exit 0 で、`-- --list` に `test_toggle_sort_keeps_selection_and_expansion` が出る。テストは `std::env::temp_dir()` の下に一意のディレクトリを作り、サブディレクトリの直下に名前順と mtime 順が逆になるファイルを 2 つ以上置く。中身を作り終えてから、`File::create` / `OpenOptions::write(true)` のハンドルで**ファイルにだけ** `set_modified` をかける。展開して子のファイルを選び、切り替え後に選択パスと `is_expanded` が保たれ、**子の階層で**順序が反転することを確かめ、最後に片付ける

### T3: キーと表示(app.rs / ui.rs)

- **内容**: `handle_file_tree_key` に `KeyCode::Char('s')`(`!key.modifiers.intersects(CONTROL | ALT)` のときだけ)→ `toggle_sort()`。OSC 7 の作り直し(`app.rs:2179` 付近)では、`toggle_hidden` の後に「`prev_sort_mode` と違えば `toggle_sort()`」を呼ぶ。`render_file_tree` の題を `format!(" FILES {} ", sort_mode.label())` に。`focus_hints(FileTree)` から `Esc 戻る ` を外して `s 並替 ` を `.` の後に足す(design ADR-3)
- **依存**: T2
- **完了条件(機械検証)**: `cargo test` が exit 0(`test_focus_hints_width_budget` の FileTree 予算 75 は変えずに通る)。ui.rs のテストに「FileTree のヒントに `並替` が含まれる」表明を足す。`grep -n -A1 "KeyCode::Char('s')" src/app.rs` の結果のうち `fn handle_file_tree_key` より後ろにちょうど 1 件あり、次の行が `if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)` であること(Alt+S 用の既存の 1 件は除く)

### T4: ビルドと静的検査

- **内容**: 全体のビルドと clippy。rustfmt は使わない(CLAUDE.md)
- **依存**: T1〜T3
- **完了条件(機械検証)**: `cargo build --release` と `cargo clippy` がどちらも exit 0。T0 で保存した `clippy-before.txt` と今の出力の warning 行を diff し、新しい warning が無い

### T5: evaluator による静的レビュー

- **内容**: CLAUDE.md の Workflow Rules に従い、evaluator agent に差分を静的にレビューさせる(キーの衝突、展開の再帰、選択のクランプ、ヒント幅の計算)
- **依存**: T4
- **完了条件(機械検証)**: evaluator の指摘一覧と、各指摘への対応(直した / 直さない理由)を記録してあること。反映後に `cargo test` が exit 0

### T6: 文書の更新

- **内容**: `docs/content/keybindings.mdx` のファイルツリーの表に `s` の行、`dist/ccmux-0.6.2-patched-win-x64/README.txt` の機能一覧に 1 項目、CLAUDE.md「現在の状態」の運用中パッチ一覧に `feat/filetree-mtime-sort` を足す。その後 `local/v0.6.2-patched` へ統合する
- **依存**: T5
- **完了条件(機械検証)**: `grep -n "並替\|更新順" docs/content/keybindings.mdx dist/ccmux-0.6.2-patched-win-x64/README.txt` が両方のファイルで 1 件以上、`grep -n "feat/filetree-mtime-sort" CLAUDE.md` が 1 件以上

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| 既定が更新順・ディレクトリ先頭 | `cargo run` → ツリーの題が ` FILES 更新順 `、各群が新しい順 | ☐ |
| 同時刻は名前順・取得失敗は末尾 | T1 のテスト | ☐ |
| `s` で切替・状態を維持 | ディレクトリを展開しファイルを選んで `s` → 題が ` FILES 名前順 `、選択と展開が保たれる。T2 のテスト | ☐ |
| 名前順は従来どおり | `test_sort_name_mode_matches_legacy` | ☐ |
| 自動再スキャンで選択が追従 | 選択中のファイルを別ペインで `touch` → 2 秒以内に先頭へ移り、選択も付いていく | ☐ |
| ヒント 75 桁以内・`s 並替` | `test_focus_hints_width_budget`、目視 | ☐ |
| OSC 7 で引き継ぎ | 名前順にしてからペインで `cd` → 題が ` FILES 名前順 ` のまま、並びもすぐに名前順 | ☐ |
| Ctrl+S / Alt+S では切り替わらない | ファイルツリーにフォーカスして Ctrl+S → 題が変わらない。Alt+S → ステータスバーだけが切り替わる。T3 の grep | ☐ |

## 実装セッションへの引き継ぎ事項

- `merge_entries` は新しいスキャン結果の順を保つ。並びの責務は scan 側に置き、merge 側で並べ直さない
- `entry.metadata()` は symlink を辿らない(Windows では追加の syscall が無い)。失敗しても項目は捨てず、`is_dir` は `path.is_dir()`、`modified = None` にする(従来の `symlink_metadata` 失敗時と同じく残す)
- Windows では書き込み中で開いたままのファイルの mtime は閉じるまで進まない。統合検証の `touch` や Claude Code の保存は閉じるので影響しない(requirements §7)
- 500 件の上限(打ち切ってから並べ替え)は変えない(ADR-2)
- 題とヒントに ambiguous 幅の文字(矢印など)を使わない
- グローバル exe の差し替えは CLAUDE.md の手順(mv で退避 → cp → sha256 で照合)に従い、ユーザーの確認を取ってから行う
- green-loop 判定: 非適用(2026-10-02 観測。green-loop@kamibu 1.1.6 は user scope で有効、testCmd は `cargo test` に定まるが、lib の無いバイナリ crate で回帰テストの置き場から内部に届かず、既定の `tests/review-regression/*.sh` も cargo は収集しないため条件 (b) 不成立)。再検証トリガ: `src/lib.rs` を切り出す、または sh を収集するランナーを足す
