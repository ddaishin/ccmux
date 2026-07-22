# measurements.md — footprint(メモリ・バイナリサイズ)before/after 計測記録

計測日: 2026-07-22 / 環境: Windows 11 Enterprise・ConPTY (portable-pty ハーネス)・release ビルド

## 手法

- メモリ: `examples/mem_harness.rs` が ccmux を ConPTY 内に起動し、シナリオ実行後に
  PowerShell `Get-Process` で ccmux.exe プロセスの WorkingSet64 / PeakWorkingSet64 /
  PrivateMemorySize64 を採取(ペイン内シェルは対象外)。A/B はバイナリ 2 本を同一シナリオで
  実行して比較(config 注入はしない)。ヒープ変化に最も鋭敏な指標は PrivateMemorySize64
- シナリオ: S1 idle(起動 4 秒後)/ S2 scrollback-fill(30,000 行 cat → 10 秒静定。
  10k/2k どちらの設定でも満杯になる量)/ S3 tabs(Ctrl+T ×2 で 3 タブ)
- バイナリサイズ: `ls -l target/release/ccmux.exe`
- 描画性能ゲート: `examples/perf_harness.rs`(draw_us 中央値 基準 1,264µs / latency p90
  基準 16.9ms、+10% 以内)

## ベースライン(T0 時点 = d7ac14f 相当・profile 未設定・scrollback 10,000)

| 項目 | 値 |
|---|---|
| exe サイズ | 6,297,600 bytes |
| S1 idle (WS / Peak / Private) | 19,251,200 / 19,296,256 / 3,579,904 |
| S2 scrollback (WS / Peak / Private) | 50,577,408 / 50,581,504 / 39,223,296 |
| S3 tabs (WS / Peak / Private) | 17,768,448 / 17,768,448 / 5,443,584 |

- S2 の Private は S1 比 **+35.6MB** — 事前見積り(10,000 行 × 120 桁 × 32B ≈ 38.4MB)と整合
- S3 の Private は S1 比 +1.9MB(タブ 2 個追加 = Preview/syntect 2 セット分)

## コミット別記録

| コミット | exe bytes | Δ | 備考 |
|---|---|---|---|
| T0 ベースライン | 6,297,600 | — | profile 未設定 |
| T1 lto+cu=1+strip | 5,473,280 | −13.1% | perf: draw 中央値 963µs(基準比 −24%)・latency p90 17,606µs(+4.5%、ゲート内) |
| T2 panic=abort | 4,617,216 | −15.6% | unwind テーブル除去。test プロファイルは unwind のまま |
| T3 opt-level="s" | 3,839,488 | −16.8% | A/B: 3=1,100µs/4.6MB・"s"=1,175µs/3.84MB・"z"=1,434µs(ゲート超過・不採用)。"s" 採用 |
| T6 scrollback_lines 追加(既定 10000) | 3,839,488 | ±0 | S2 Private 39,473,152(ベースライン +0.6%、±5% 以内=挙動不変を確認) |
| T7 既定 2000 へ | 3,839,488 | ±0 | S2 Private 11,046,912(T6 比 **−28.4MB**)・WS 49.7→22.1MB。理論値 30.7MB と整合 |
| T8 syntect OnceLock 化 | 3,839,488 | ±0 | T7 バイナリ比: S1 Private 3,551,232→2,875,392(−0.7MB)・S3 5,337,088→3,608,576(−1.7MB) |

## 最終結果(T9 総括: ベースライン → T8 最終バイナリ)

| 指標 | before | after | Δ |
|---|---|---|---|
| exe サイズ | 6,297,600 | **3,839,488** | **−39.0%** |
| S1 idle WS / Private | 19.25MB / 3.58MB | 13.94MB / 2.97MB | −27.6% / −17.1% |
| S2 scrollback WS / Private | 50.58MB / 39.22MB | **21.03MB / 10.39MB** | **−58.4% / −73.5%** |
| S3 tabs WS / Private | 17.77MB / 5.44MB | 14.55MB / 3.60MB | −18.1% / −33.9% |
| perf: draw_us 中央値(pty) | 1,264µs(基準) | 1,025µs | −18.9%(改善) |
| perf: latency p90 | 16,853µs(基準) | 18,078µs | +7.3%(+10% ゲート内) |
| perf: latency 中央値 | 15,937µs(基準) | 15,431µs | −3.2%(60fps ペーシング下限) |

- メモリの主効果は scrollback 既定 10,000→2,000(S2 で −28.4MB)と syntect の
  OnceLock 共有・遅延化(S1 −0.7MB / S3 −1.7MB)
- サイズの主効果は profile 設定(lto fat + cu=1 + strip + panic=abort + opt-level "s")
- 従来挙動が必要な場合: config.toml に `scrollback_lines = 10000`

## 不採用判断の記録

- **T4: syntect regex-fancy 化(onig 除去)**: `cargo tree -i onig` 空・テスト green まで確認したが、
  exe が 3,839,488 → 4,493,312 bytes に**+654KB 増加**したため不採用(判断軸 A)。fancy-regex/regex の
  Rust コード(単形化+Unicode テーブル)が onig の C オブジェクトコードより大きい。opt-level="s" は
  Rust 側のみに効き、cc crate 経由の C 側は独立に最適化されているため C 排除がサイズ増になる
- **T5: arboard/ratatui feature 削減**: `default-features=false`(arboard: image-data 除去 /
  ratatui: all-widgets・macros 除去)でビルド・テスト green だが exe は 3,839,488 bytes で**変化なし**
  (±512B のビルドノイズのみ)。lto="fat"+strip が未使用コード(calendar/macros/GDI 画像変換)を
  既にリンカレベルで除去済みのため feature 削減の上乗せ効果ゼロ。time も syntect(onig 構成)→plist
  経由で残存するため除去不能。upstream との diff 最小化を優先し revert・不採用(判断軸 A・D)

- **ureq "json" feature 外し**: serde_json が直接依存(claude_monitor 等)で残るため削減効果ほぼゼロ。
  version_check.rs の書き換えコストのみ発生するため不採用(設計 ADR-5)
- **毎フレームアロケーション削減**: 描画セルループ(ui.rs:505-547)は既にアロケーションフリー。
  残りは短命小型確保(1 フレームあたり数百個・数 KB)で scrollback の 30MB に対し 3〜4 桁小さく
  不採用。将来候補: ui.rs:333 の rects.clone() / ui.rs:1002 の truncate_to_width String 返し
