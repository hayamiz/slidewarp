---
title: Rust版のスライド領域認識方式をドキュメント化する
type: docs
priority: medium
status: open
created: 2026-07-14
updated: 2026-07-14
---

## Description

Rust 本体（`src/detect.rs` / `src/geometry.rs` / `src/warp.rs` 等）が実装している
スライド矩形領域の認識方式を、独立したドキュメントとして整理・記述する。

現状、認識アルゴリズムの設計要点は `CLAUDE.md` と `docs/tech-stack.md`（後者は
言語非依存の設計方針）に断片的に記載されているが、**Rust 本体が正**であるにも
かかわらず Rust 実装に即したまとまった解説が無い。以下を含むドキュメントを
`docs/` 配下（例: `docs/detection-rust.md`）に作成する。

- 候補生成3系統の全体像: `contour`（明度マスク＋Canny の輪郭 approxPolyDP）/
  `hough`（imageproc 標準 Hough の極線交点・帯域層化）/ `minrect`（緩いフォールバック）
- 統合スコア `score_quad` の各項（area / rect / aspect / contrast / fill / edge / cut）
  と重み、特に sub-slide 誤り対策の主役である **cut** と **方向付き edge_support** の役割
- 上下辺リファイン（`refine_top_edge` / `refine_bottom_edge`）の発火条件と損失非対称の考え方
- 出力アスペクト決定（Zhang-He 透視補正、`decide_output_aspect` の「確度が高くない限り 16:9」方針）
- Rust 固有の実装差・落とし穴（imageproc warp の `from_control_points(src,dst)` 方向、
  自前 Douglas-Peucker、EXIF 回転の明示適用 `load_oriented` 等）

図や処理フローがあると望ましい。既存の `CLAUDE.md` の記述との重複は要約＋参照で整理する。

## Triage

- Complexity: low
- Mechanical fix: no
- Requires user decision: no
- Affected files: 1（`docs/detection-rust.md` 新規。`CLAUDE.md`/`docs/tech-stack.md`/`src/*.rs` は参照のみ）
- Fix strategy: worktree
- Notes: コード変更ゼロ・新規 doc 1枚で回帰リスクは実質なし（low）。素材は CLAUDE.md §検出の設計要点／落とし穴／残課題と tech-stack.md §5 にほぼ揃い、`score_quad`/`refine_top_edge`/`refine_bottom_edge`/`decide_output_aspect`/`rectified_aspect` 等の関数も実在確認済み。ただし散在記述を Rust 実装視点で1本に再構成する執筆に一意な正解が無いため mechanical=no。文言・密度は実装者裁量に収まり user-decision=no。

## Implementation Notes

### 方針
`docs/detection-rust.md` を新規作成する。Rust 本体が正であるため、記述の一次情報源は
`src/detect.rs` / `src/geometry.rs` / `src/warp.rs` / `src/main.rs` とし、`CLAUDE.md`
（§検出の設計要点・§Rust 実装の落とし穴・§既知の残課題）と `docs/tech-stack.md` §5 を
既存の設計文脈として参照・再構成する。既存2文書は削除・改変せず、本ドキュメントから
リンクする（tech-stack.md は言語非依存の方針、detection-rust.md は Rust 実装に即した解説、
と役割を分ける）。

### 章立て（ドラフト）
1. 概要 — 目的（暗所中の明るいスライド矩形の頑健検出）と多段フォールバック思想。
2. 前処理 — `main.rs::load_oriented`（EXIF 回転の明示適用）、明度マスク / Canny / blur。
3. 候補生成3系統 — `contour`（明度マスク＋Canny 輪郭を自前 Douglas-Peucker で四角形化）/
   `hough`（imageproc 標準 Hough=極線 r,θ、明部 bbox を 18% 拡張した ROI、H/V 分離＋位置
   クラスタで重複除去、帯域層化で総当り）/ `minrect`（緩いフォールバック、信頼度係数 0.6）。
4. 統合スコア `score_quad` — 各項と重み（area0.12 / rect0.05 / aspect0.06 / contrast0.12 /
   fill0.20 / edge0.25 / cut0.20）。特に `_edge_profile` が法線サンプリングで同時算出する
   方向付き edge_support（内側明・外側暗で満点、内部線は係数0.5）と cut（明部素通しの切断辺
   検出、`cut_score=1-min(1,1.5*cut)`）の sub-slide 誤り対策としての役割を明記。fill は
   `fill_holes` 版を使う点も。
5. 上下辺リファイン — `refine_top_edge` / `refine_bottom_edge` の発火条件（帯に Canny
   エッジ密度で判定した「コンテンツ」がある時のみ、空余白/レターボックスは触らない=損失
   非対称）と gray 値ベースの前提・帯継続判定、候補生成・スコアを変えない後処理である点。
6. 出力アスペクト決定 — `geometry::decide_output_aspect` と Zhang-He 透視補正
   `rectified_aspect`（主点=画像中心・正方画素仮定、消失点直交条件で焦点距離推定）。
   「確度が高くない限り 16:9」ゲート（persp<0.12 かつ比<1.45 のときだけ 4:3）と、
   見かけ比 `estimate_aspect` を使わない理由。
7. Rust 固有の実装差・落とし穴 — imageproc の warp は `Projection::from_control_points(src,dst)`
   （逆にすると崩れる）/ Hough は極線で交点計算（Python の確率的 Hough 線分と違う）/
   approxPolyDP は自前実装 / `(x as f64) < y` の括弧 / EXIF 自前適用 / release は LTO で約2分。

### 代替案・トレードオフ
- 別案A: `docs/tech-stack.md` に Rust 節を追記して1文書に統合。→ 却下寄り。tech-stack は
  言語非依存の方針という役割で、Rust 実装詳細を混ぜると肥大・関心の分離が崩れる。
- 別案B: rustdoc（`cargo doc`）のコメント拡充で代替。→ 却下。設計要点・重み根拠・落とし穴の
  横断的解説はソースコメントに収まらず、API doc とは目的が異なる。
- 数値（重み・しきい値）は本文に書くと乖離しうるので、各値の直後に出典関数名を併記し
  「値はソースが正」と明記して将来の drift を運用で吸収する。

### 検証
コード非変更のため `cargo build`／サンプル再評価は不要（CLAUDE.md §検証の eval-output
再生成ルールの対象外）。記述の正しさは対象関数の実装と突き合わせて確認（重み・しきい値・
関数名・発火条件がソースと一致するか）。Markdown のリンク切れ・見出し構造を目視確認。
