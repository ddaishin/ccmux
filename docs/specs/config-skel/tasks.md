# tasks.md — config-skel(config.toml の初回 skel 自動生成)

## 実装順序

T1 → T2 → T3(直列。T1 完了時点から cargo test が回る)

## タスク一覧

### T1: config.rs への実装+テスト

- **内容**: `src/config.rs` に以下を追加・変更:
  - `pub(crate) const CONFIG_SKEL: &str = r#"..."#;` — 全コメントの雛形
    (書式説明、Typora / VS Code(code.cmd 注記) / ストアアプリ直接パスのコメント例、
    複合拡張子不可・{file} 置換・バージョン入りパスの注意書き)
  - `fn ensure_skel_at(path: &Path) -> Option<String>` — 親ディレクトリを create_dir_all →
    `OpenOptions::new().write(true).create_new(true)` で CONFIG_SKEL を書き込み →
    成功時 `Some("設定ファイルの雛形を作成しました: <パス>")`。あらゆる失敗で None
  - `Config::load()`: `std::fs::read_to_string` の Err を ErrorKind で分岐し、
    NotFound のみ `(Self::default(), ensure_skel_at(&path))` を返す。他の Err は従来どおり
  - テスト 4 件(`std::env::temp_dir()` + プロセス ID 入り一意名。後始末に remove_dir_all):
    (a) `Config::parse(CONFIG_SKEL)` が Ok(空 openers)
    (b) 不在パスで ensure_skel_at → Some・ファイル内容 == CONFIG_SKEL
    (c) 既存ファイル(別内容)で ensure_skel_at → None・内容不変
    (d) 未作成のネストディレクトリ配下パスで生成成功
- **依存**: なし
- **完了条件(機械検証)**: `cargo test config::` が exit 0(既存 8+新規 4)、`cargo test` 全緑

### T2: evaluator 静的レビュー

- **内容**: evaluator エージェントで diff 静的レビュー(TOCTOU・fail-open 経路・
  既存 load() 挙動の退行・テストの後始末漏れ)。指摘があれば修正
- **依存**: T1
- **完了条件(機械検証)**: cargo test 全緑+evaluator 合格報告

### T3: 配備と sample 同期

- **内容**: release ビルド → npm 配備差し替え(絶対パスで rename 退避→配置・sha256 検証)。
  `dist/.../config.toml.sample` を CONFIG_SKEL と同内容に更新。
  `pr/external-opener` へ cherry-pick(docs/specs はコンフリクト時 git rm)→ fork へ push
- **依存**: T2
- **完了条件(機械検証)**: sha256 一致(ビルド元 == 配備先)、pr ブランチで cargo test 全緑

## 統合検証

| 受け入れ基準 | 検証手順 | 結果 |
|--------------|----------|------|
| 不在時に skel 生成+通知 | config.toml をリネーム → ccmux 起動 → 雛形生成+ステータスバー通知 | ☐ |
| 雛形は挙動不変 | 生成直後に .md を開く → TUI プレビュー(外部起動しない) | ☐ |
| 既存ファイル不変 | リネーム戻し → ccmux 再起動 → 内容・mtime 不変・通知なし | ☐ |
| TOCTOU 上書き防止 | テスト (c) で担保(既存ファイルが create_new で無傷) | ☐ |
| 生成失敗 fail-open | テストでは (b)〜(d) の逆条件で担保。実機は読み取り専用環境が無いため省略 | ☐ |

## 実装セッションへの引き継ぎ事項

- main.rs / app.rs / ui.rs は変更しない(load() のシグネチャ維持が契約)
- rustfmt 非適用。コミットは Conventional Commits+日本語+`-F` ファイル渡し
- /init 由来の未コミット変更は含めない。実機確認はユーザー依頼(ccmux 再起動が必要)
- 実行中 ccmux がある場合の exe 差し替えは rename 退避→配置(絶対パスで実行)
