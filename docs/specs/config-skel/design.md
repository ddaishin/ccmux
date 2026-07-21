# design.md — config-skel(config.toml の初回 skel 自動生成)

## 1. 全体構成(条件⑦)

```
Config::load()                       ← 変更(src/config.rs 内で完結)
  ├─ read OK        → parse(従来どおり)
  ├─ Err(NotFound)  → ensure_skel_at(&path) ─ 成功 → (default, Some(生成通知))
  │                                        └ 失敗 → (default, None)  ※fail-open
  └─ Err(その他)    → (default, None)(従来どおり・生成しない)

main.rs / app.rs / ui.rs: 変更なし(load() の戻り値 .1 → status_message の既存配線を再利用)
```

## 2. 出力契約(条件④)

| パス | 目的 |
|------|------|
| `src/config.rs` | `CONFIG_SKEL` 定数・`ensure_skel_at()`・`load()` の NotFound 分岐・テスト 4 件 |
| `dist/ccmux-0.6.2-patched-win-x64/config.toml.sample` | CONFIG_SKEL と同内容に更新(乖離防止・コミット外) |

## 3. 設計判断(ADR 形式)

### ADR-1: 生成の判定と書き込み方式

- **案A check-then-write**(`Path::exists()` → `fs::write`): 素直だが判定と書き込みの間に
  別プロセスが作ったファイルを上書きする TOCTOU 窓がある
- **案B NotFound 分岐 + `OpenOptions::create_new`**: read の ErrorKind で不在を判定し、
  作成は create_new(既存があれば AlreadyExists で失敗)で原子的に行う。上書きが構造的に不可能
- **決定**: 案B。設定ファイルはユーザー資産であり「絶対に上書きしない」を構造で保証する

### ADR-2: 生成失敗時の扱い

- **案A エラー通知**: 権限等で書けない場合にステータスバーへ警告
- **案B 無視(fail-open)**: 通知なしで従来挙動(空設定)のまま起動継続
- **決定**: 案B。雛形は利便機能であり、書けない環境(読み取り専用 HOME 等)で毎回警告を
  出すのはノイズ。external-opener 本体の fail-open 方針(パース失敗警告とは別)とも整合

### ADR-3: 生成ロジックの置き場所

- **案A main.rs で load 前に生成**: 呼び出し側の変更が必要になり配線が増える
- **案B Config::load() 内に統合**: 既存シグネチャ (Config, Option<String>) を維持し、
  通知は既存の status_message 配線に乗る。main/app/ui 無変更
- **決定**: 案B。変更面を config.rs 1 ファイルに閉じ込める(テスト容易・PR 差分最小)

## 4. 参照実装(条件⑥)

- fail-open と警告返しのパターン: `src/config.rs` の `Config::load()`(4f1c282)
- 一時通知: `App::status_message`(次のキーで消える一回性メッセージ、34ac3da で共通化済み)
- 雛形の文面: `dist/ccmux-0.6.2-patched-win-x64/config.toml.sample`(実機検証済みの説明文)

## 5. データとライフサイクル

- CONFIG_SKEL: バイナリ埋め込み定数 → 初回のみファイルへ書き出し → 以後ユーザーが編集・所有
- 生成後の再起動では read OK 経路に乗り、二度と生成されない
- 個人情報なし

## 6. 障害モードと回復

| 障害 | 振る舞い |
|------|----------|
| 書き込み権限なし / ディレクトリ作成失敗 | 生成スキップ・無通知・空設定で継続(fail-open) |
| TOCTOU(生成競合) | create_new が AlreadyExists で失敗 → 既存ファイル無傷・無通知 |
| read が権限拒否(NotFound 以外) | 生成を試みない(従来挙動) |
| 雛形自体の TOML 破損(実装ミス) | テスト (a) で担保: CONFIG_SKEL が空 openers にパース可能であること |
