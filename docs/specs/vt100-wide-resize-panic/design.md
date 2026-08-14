# design.md — vt100-wide-resize-panic

> 複数案＋採用理由の ADR 形式。判断軸は Plan agent による独立検証（2026-08-13）で確定。

## 1. 全体構成（条件(7)）

```
ccmux/
├── Cargo.toml                         # [workspace] members + [patch.crates-io] を追加（末尾）
├── src/
│   ├── main.rs                        # panic hook 強化（:44-52）
│   └── pane.rs                        # clamp_pty_size 追加、reader thread 命名（:47, :107-117, :172-183）
├── vendor/
│   └── vt100/                         # 新規: vendored vt100 0.16.2 + パッチ
│       ├── Cargo.toml                 # registry 版と異なり autotests=false を含まない
│       ├── PATCHES.md                 # 由来・パッチ根拠・既知の未修正・再適用手順
│       ├── patches/0001-*.patch       # upstream PR #30 の diff
│       ├── src/{row,screen,grid,...}.rs
│       └── tests/wide_resize.rs       # 新規回帰テスト
└── .github/workflows/ci.yml           # 任意: cargo test --workspace
```

## 2. 出力契約（条件(4)）

| パス | 目的 |
|------|------|
| `src/pane.rs` | `clamp_pty_size`/`next_pty_size` 追加、`new_with_cwd`/`resize` へ適用、reader thread 命名 |
| `src/main.rs` | panic hook にクラッシュログ追記を追加 |
| `Cargo.toml` | `[workspace] members`、`[patch.crates-io] vt100` 追加（既存 profile/deps は不変） |
| `vendor/vt100/*` | vt100 0.16.2 のソース一式 + row.rs パッチ + テスト + PATCHES.md |
| `Cargo.lock` | vt100 エントリの source/checksum が消える（patch 適用の結果） |
| `docs/specs/vt100-wide-resize-panic/*` | 本仕様一式 |

## 3. 設計判断（ADR 形式）

### ADR-1: vt100 バグ本体の修正方法

- **状況**: `Row::resize` が縮小時に wide 文字の継続セルを後始末せず panic。upstream は
  Issue #28 を Open のまま 1 年以上停止（最終コミット 2025-07-12）、PR #30 は未マージ
- **案A**: fork を `[patch.crates-io]` の git 参照にする。CI が外部 git と fork の維持に依存
- **案B（採用）**: vendored コピー + ローカルパッチ。依存がリポジトリ内で完結し upstream の
  死活に左右されない。CI は checkout だけで完結（4 プラットフォーム追加設定不要）
- **案C**: 第三者フォーク crate へ乗り換え。保守状況が不明でサプライチェーン評価が要り不採用
- **決定**: 案B。ユーザーが明示的に選択（推奨案）

### ADR-2: vendored パッチの中身

- **状況**: 自前で `Row::resize` の修正を書くか、upstream PR #30 の diff を移植するか
- **案A**: 自前実装。`Row::erase` の underflow・`Row::clear_wide` の OOB という
  Issue #28 の別発火点を見落とすリスクがある
- **案B（採用）**: PR #30 の diff をそのまま採用。`erase`/`resize`/`clear_wide` の 3 関数を
  まとめて直し Issue #28 の全経路を塞ぐ。将来 #30 マージ時に `git apply --3way` で
  再適用コストがほぼゼロになる
- **決定**: 案B。`clear_wide` の逆方向（`col-1` 側）境界チェックは到達不能と確認済みのため
  「防御的追加」として diff とは分離し、バグ修正ではないと明記する

### ADR-3: Issue #37（cols=1 に全角）の対処場所

- **状況**: リサイズ破損とは無関係に、ペイン幅が 1 桁まで縮むだけで単独に panic する経路が
  検証で新たに見つかった。vendored パッチでは直らない
- **案A**: vendored 側の screen.rs にもガードを足す。upstream が意図的に未定義のままにして
  いる領域に独自ルールを持ち込むことになり、将来の rebase コストが増える
- **案B（採用）**: ccmux 側で PTY/vt100 への入力を clamp する。`unicode-width` の最大値は
  2 なので「端末は最低 2 桁」は ccmux 側が持って自然な契約。vt100 の意味論には触れない
- **決定**: 案B。`clamp_pty_size` を `pane.rs` に追加し `new_with_cwd`/`resize` の入口で適用

### ADR-4: panic 全般への保険

- **状況**: 今回の 2 経路以外にも未知の vt100 panic が残りうる。`panic = "abort"` のため
  1 箇所の panic がプロセス全体を道連れにする
- **案A**: catch_unwind でスレッド単位に封じ込め。効果は高いが `panic = "abort"` を外す
  必要があり release ビルドの前提を変える。unwind テーブル分のサイズ増も伴う
- **案B（採用）**: 記録のみ強化。`panic = "abort"` を維持したまま reader thread に名前を
  付けクラッシュログを残す。根本防止にはならないがユーザーが方針として選択した
- **決定**: 案B。バックトレースは `strip = "symbols"` と競合し Windows は PDB が要るため
  見送り、`panic::Location` とスレッド名で診断は足りると判断

## 4. 参照実装（条件(6)）

- 既存の `unwrap_or_else(|e| e.into_inner())` パターン（`pane.rs:194,206,214,...`）に倣い
  Mutex 復旧は変更しない
- `Pane::new_with_cwd` の `.context(...)?` スタイル（`pane.rs:57,80,88,92`）を `spawn` にも適用
- `docs/specs/external-opener/` `docs/specs/display-performance/` と同じ 3 ファイル構成

## 5. データとライフサイクル

- クラッシュログ: `<dirs::config_dir()>/ccmux/crash.log`（`config.rs:233` と同じ親）に
  追記のみ。1MB 超で切り詰め。個人情報は含まない（時刻・version・thread 名・location・
  panic メッセージのみ）
- vendored ソース: リポジトリにコミットし、vt100 が今後更新されない前提で半永久的に保守。
  `PATCHES.md` に再適用手順を残し、upstream #30 マージ後は vendoring 自体を撤去できるようにする

## 6. 障害モードと回復

- clamp 適用後も未知の vt100 panic が発生 → クラッシュログに記録され端末状態は復元される
  （プロセス終了は避けられない。ADR-4 の意図通り）
- `crash.log` の書き込み自体が失敗（権限等） → `write_crash_record` の戻り値を `let _ =` で
  握りつぶし、端末復元と `default_hook` 委譲は継続する（記録失敗が復元をブロックしない）
- vendored パッチと upstream が将来競合 → `PATCHES.md` の再適用手順に従う。自動更新の
  仕組みは持たない（意図的にマニュアル運用）
