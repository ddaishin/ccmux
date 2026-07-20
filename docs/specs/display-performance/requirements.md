# requirements.md — 表示の高速化 (display-performance)

## 1. 背景と目的

ccmux の表示に「大量出力時のカクつき・キー入力遅延・スクロール/マウスの鈍さ・全体的なもっさり感」がある。
調査の結果、`event::poll(33ms)` がターミナル入力でしか起床せず PTY 出力（mpsc）はタイムアウト後にしか
回収されないため、出力反映に最大 33ms/フレームの構造遅延（実効 30fps 上限）が乗ることが根因と判明した。
イベント駆動化と描画パスの無駄排除でレイテンシと滑らかさを改善する。

## 2. 利用者と保守者（条件③）

- 利用者: 開発者本人（毎日）。ローカルの npm グローバル ccmux バイナリを差し替えて常用
- 保守者: 本人＋upstream (Shin-sibainu/ccmux) メンテナ（PR 提出を視野に入れた品質で書く）

## 3. 成功基準（条件①）

- PTY 出力・キーエコーの「イベント発生→描画開始」レイテンシ中央値が 16ms 以下（改善前 ≈33ms 級）
- バースト出力中の描画が 60fps 上限で安定（現行実効 30fps）
- 上記を `CCMUX_PERF_LOG` の before/after 計測で数値裏付けし、体感でもカクつき・遅延が解消
- 既存機能（ペースト・リサイズ・Claude 検知・トークン集計・スクロールバック）に回帰なし

## 4. 受け入れ基準（EARS 記法）

- WHEN PTY 出力イベントが mpsc チャネルに到着した THE SYSTEM SHALL 33ms のポーリング周期を待たず即時に起床して処理する
- WHEN 描画が必要（dirty）かつ前回描画から 16ms 以上経過している THE SYSTEM SHALL 直ちに再描画する
- WHEN 描画が必要だが前回描画から 16ms 未満 THE SYSTEM SHALL 16ms 経過時点まで描画を保留し、その間のイベントを 1 フレームに統合する
- WHEN キー入力（KeyEventKind::Press）が発生した THE SYSTEM SHALL 従来と同一のキーバインド処理・ペーストバースト検出を実行する
- WHEN Windows で KeyEventKind::Release/Repeat イベントが発生した THE SYSTEM SHALL それを無視する（二重入力防止）
- WHEN ペースト（bracketed/キーバースト両経路）が行われた THE SYSTEM SHALL 従来同様に単一の書き込みへ結合し、約 150ms の描画ホールド後に再描画する
- WHEN レイアウト変更（split/close/sidebar/リサイズ）が発生した THE SYSTEM SHALL 約 150ms 描画をホールドし、期限到達時に即再描画する（旧バッファのフラッシュ防止を維持）
- WHEN イベントもタイマーもない待機状態 THE SYSTEM SHALL 最大 1 秒に 1 回程度しか起床しない（idle CPU ≈0%）
- WHEN 環境変数 CCMUX_PERF_LOG にパスが設定されている THE SYSTEM SHALL フレームごとの draw 時間・イベント→描画レイテンシ・coalesce 数を当該ファイルに記録する
- WHEN CCMUX_PERF_LOG が未設定 THE SYSTEM SHALL 計測コードのオーバーヘッドを Option 判定 1 回以下に抑える
- WHEN Claude Code の JSONL が PTY 出力なしに更新された THE SYSTEM SHALL 500ms 以内にステータスバー表示を更新する（monitor 移設後も再描画契機を維持)
- WHEN 描画（draw クロージャ）実行中 THE SYSTEM SHALL ファイル I/O（claude_monitor の JSONL 読み取り）を行わない

## 5. 制約（条件⑤）

- 技術制約: 既存スタック（ratatui+crossterm+std::sync::mpsc）のまま。crossbeam/tokio 等の依存追加なし
- Windows 前提の動作保証（Press/Release 二重イベント・legacy console のキーバーストペースト経路）
- 個人情報・法令論点: なし（perf ログはローカル任意出力・数値のみ）
- 既存 46 テストを全コミットで green 維持

## 6. スコープ外

- 端末同期出力モード（CSI 2026）の導入
- parser.lock の全セルループ被覆の解消（計測で draw_us 常態 >5ms の場合に後続 PR で検討）
- cooldown 150ms からの短縮チューニング（計測後の任意コミット扱い）
- ファイルツリー走査のバックグラウンドスレッド化

## 7. 未知の未知の決定事項（Phase 2 の承認結果）

- バースト時の描画上限 → 60fps（最小フレーム間隔 16ms）
- cooldown → フレーム数から時間ベース化し現行同等の 150ms から開始、短縮は計測後
- 入力スレッドの停止 → detach しプロセス終了に任せる（event::read ブロック中の安全停止は不可能）
- CSI 2026 同期出力 → 今回見送り
- claude_monitor の I/O → draw 外（メインループのタイマー節）へ移動、別スレッド化はしない
