---
title: 処理中に spinner・処理済みファイル数・予想完了時間を表示する
type: enhancement
priority: medium
created: 2026-08-08
updated: 2026-08-10
status: resolved
---

## Description

バッチ処理の実行中に、進捗が分かる表示を出す。具体的には:

- **spinner**（処理が進んでいることが分かるアニメーション）
- **処理済み / 全体のファイル数**（例: `42/120`）
- **予想完了時間（ETA）**

現状は開始時に `対象 N 枚 / 並列 J` を表示し、各ファイルの結果は全処理完了後
（`par_iter().collect()` の後）にまとめてログ出力しているだけで、処理中は進捗が
見えない。枚数が多い / 大きい画像のときに「止まっているのか進んでいるのか」が分からない。

期待する挙動:

- 実行中、`[####----] 42/120 (35%) ETA 00:18` のような進捗が更新表示される。
- 完了時に進捗表示を消して（または完了状態にして）、既存の per-file ログと
  `集計: ...` を従来どおり出す。
- 非対話環境（stdout が TTY でない、パイプ/リダイレクト）では進捗バーを出さず、
  ログ出力を妨げない（誤動作・制御文字の混入を避ける）。

実装メモ（参考）:

- 進捗バーは `indicatif` が定番。**自己更新で導入した `self_update` が既に `indicatif` を
  依存に持つ**ため、`indicatif` を直接依存に加えても新規の重い依存は増えにくい。
- 処理は `files.par_iter().map(process_image).collect()`（rayon 並列）。ライブ進捗には
  `indicatif` の rayon 連携（`ParallelProgressIterator::progress_with`）を使うか、
  完了ごとに `AtomicUsize` を増やして `ProgressBar::inc(1)` する。全体数が既知なので
  ETA は `indicatif` が自動算出する。
- TTY 判定は `std::io::IsTerminal`。非 TTY なら `ProgressBar::hidden()` を使う。
- 既存の per-file ログ（`log(r)`）と進捗バーの行が混ざらないよう、
  `ProgressBar::suspend` などで出力を協調させる（またはログは完了後にまとめて出す現行方式を維持）。

## Resolution

- `indicatif`（`rayon` feature）を直接依存に追加（`self_update` 経由で依存ツリーには既存）。
- `src/main.rs` の並列処理 `files.par_iter()` に `ParallelProgressIterator::progress_with`
  で進捗バーを連携。テンプレートは `{spinner} {pos}/{len} ({percent}%) [{bar}] 残り {eta}`、
  `enable_steady_tick(120ms)` で spinner をアニメーション。完了後 `finish_and_clear`。
- 非対話環境は `std::io::stderr().is_terminal()` で判定し、非TTY 時は `ProgressBar::hidden()`
  にして制御文字を出さない。per-file ログは従来どおり処理完了後にまとめて出力（混在回避）。
- コミット: `37ebd47 feat(cli): --version と処理中の進捗表示を追加 (resolve #0007, #0008)`。
