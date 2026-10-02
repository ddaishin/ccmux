# design.md — filetree-mtime-sort(ファイルツリーの更新日時降順ソート)

## 1. 全体構成(条件7)

```
read_dir -> DirEntry::metadata() (symlink 判定 + is_dir + modified を 1 回で)
  -> FileEntry { .., modified: Option<SystemTime> }   (500 件で打ち切り: 現状維持)
  -> sort_entries(dirs, mode) / sort_entries(files, mode) -> dirs ++ files
FileTree { sort_mode: SortMode, .. }
  s キー -> toggle_sort(): モード反転 -> resort_recursive(entries) -> rebuild_flat -> restore_selection(path)
           (resort_recursive は is_expanded にかかわらず、保持しているすべての children を並べ直す)
  2 秒ごと -> auto_refresh_if_needed(): scan(sort_mode) -> merge_entries -> rebuild_flat -> restore_selection(path)
OSC 7 で作り直し(app.rs:2179) -> FileTree::new の後、prev_show_hidden と同じ形で
  「モードが違えば toggle_sort()」を呼ぶ(toggle_hidden の後。代入だけだと題と並びが最大 2 秒ずれる)
render_file_tree -> 題 " FILES 更新順 " / " FILES 名前順 "
```

## 2. 出力契約(条件4)

| パス | 目的 |
|------|------|
| `src/filetree.rs` | `SortMode { Modified, Name }`(Default = Modified)・`label()`、`FileEntry.modified`、純粋関数 `sort_entries`、`scan_directory_filtered` に `sort` 引数、`FileTree.sort_mode`・`toggle_sort()`・`restore_selection()`(`auto_refresh_if_needed` から抽出)、単体テスト |
| `src/app.rs` | `handle_file_tree_key` に `s`(`o`/`r`/`e` と同じ Ctrl/Alt ガード)、OSC 7 の作り直しで `sort_mode` を引き継ぐ |
| `src/ui.rs` | `render_file_tree` の題にモード、`focus_hints(FileTree)` に `s 並替`(予算 75 桁は据え置き) |
| `docs/content/keybindings.mdx` / `dist/ccmux-0.6.2-patched-win-x64/README.txt` / `CLAUDE.md` | `s` キーの行、機能一覧、「現在の状態」の運用中パッチ一覧(ブランチ `feat/filetree-mtime-sort`) |

`scan_directory_filtered` の本体の呼び出し 5 箇所(`new` / `toggle_hidden` / `toggle_dir_recursive` / `auto_refresh_if_needed` / `merge_entries`。後ろ 2 つは関連関数なので `sort` 引数を足す)に `sort_mode` を渡し、既存テストの 4 箇所には `SortMode::Name` を渡して従来の挙動の検査として残す。`FileEntry` は scan 側で `entry.metadata()` から `is_dir`・`modified` を取って直接組み立て、`from_dir` の `path.is_dir()` は使わない(metadata が失敗したときだけ `path.is_dir()` で判定する)。`merge_entries` は新しいスキャン結果の順を保つので、並びの責務は scan 側だけにある。

## 3. 設計判断(ADR 形式)

### ADR-1: 切替時の並べ直しに使う mtime の持ち方

- **状況**: `s` で切り替えるとき、展開中の全階層を並べ直す必要がある
- **案A**: `FileEntry` に `modified` を持ち、メモリ上で再帰的に並べ直す。I/O なしで即時に反映でき、展開状態も構造ごと残る。代償は 1 項目あたり 16 bytes 程度の増加
- **案B**: 切替時に `merge_entries` 経由で全展開ディレクトリを再スキャンする。フィールドは増えないが、切替 1 回で展開数ぶんの read_dir が走り、2 秒周期の再スキャンと重なる
- **決定**: 案A。判断軸は「切替で展開状態と選択パスを保つ」(requirements.md §7「`s` で切り替えたとき」)を I/O 失敗の影響なしに満たせること

### ADR-2: 500 件上限と並べ替えの順序

- **状況**: 現状は read_dir の順に 500 件で打ち切ってから並べ替えるので、更新順では最新のファイルが漏れることがある
- **案A**: 現状維持(打ち切り → 並べ替え)
- **案B**: 5000 件まで読んで並べ替え、先頭の 500 件を残す
- **案C**: 全件を読んで並べ替え、500 件を残す
- **決定**: 案A(Phase 2 でユーザーが選択)。DoS 防止という上限の意図と 2 秒周期の再スキャンの費用を変えない。漏れは requirements の既知の限界として記録する

### ADR-3: キーヒントの予算(75 桁)の捻出

- **状況**: FileTree のヒントはちょうど 75 桁で、`s 並替 `(7 桁)を足すと予算を超える。予算を上げると右側の情報(最大 67 桁前後)に押されて `^Q 終了` が先に消える
- **案A**: `Esc 戻る `(9 桁)を外す。Esc は端末 UI で標準の操作で、ペインのクリックでも戻れる
- **案B**: `閉じる`→`閉`、`終了`→`終` などラベルを詰める。項目は残るが、読みにくくなる
- **案C**: 予算を 82 に上げる(config-editor では 68→75 に上げた前例がある)。右端のキーが先に消える
- **決定**: 案A(予算内に収まる 73 桁)。ラベルの読みやすさと右端のキーを両方守れる。Phase 5 で変えられる

### ADR-4: モード表示の場所と文字

- **状況**: 今のモードが見えないと、並びの意味を誤読する
- **案A**: 題を ` FILES 更新順 ` / ` FILES 名前順 ` にする(全角は幅 2 で確定)
- **案B**: 題に矢印(`↓`)を付ける。U+2193 は East Asian Ambiguous 幅で、端末によって列がずれる
- **決定**: 案A。判断軸は表示幅の確定(ambiguous 幅の文字の表示が崩れた前例がこの repo にある。原因はフォントのグリフのはみ出し・2026-07-26 の task-diary)

## 4. 参照実装(条件6)

- 切替キーと状態の持ち方: `FileTree::toggle_hidden` と `.` キー(`app.rs` の `handle_file_tree_key`)
- 修飾キーのガード: 同じハンドラの `o` / `r` / `e`
- OSC 7 での引き継ぎ: `app.rs:2179-2185` の `prev_show_hidden`
- 選択の復元: `auto_refresh_if_needed` 内のパス検索とクランプ(`filetree.rs:270-291`)

## 5. データとライフサイクル

`sort_mode` はタブ(Workspace)の `FileTree` に置き、プロセスが終われば消える。永続化はしない。`modified` は走査のたびに取り直す。

## 6. 障害モードと回復

- `entry.metadata()` が失敗した → 従来の `symlink_metadata` 失敗時と同じく項目は残し(symlink の判定は false)、`is_dir` は `path.is_dir()` で決め、`modified = None` でその群の末尾へ
- 再スキャンで行が入れ替わった直後のクリック → 補正しない(プレビューと展開は非破壊。requirements §7)
- サイドバーが 16 桁未満 → 題が切れるのを許容する(requirements §7)
- `modified()` が未対応の FS → `None`(同上)
- 未来の時刻・時計のずれ → そのまま比較する(先頭に来る)。補正しない
