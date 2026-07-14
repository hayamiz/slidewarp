---
title: Rust版のスライド領域認識方式をドキュメント化する
type: docs
priority: medium
status: resolved
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
- Mechanical fix: yes（grill 2026-07-14 で章立て・スコープ・図形式・CLAUDE.md 方針・数値裏取りまで確定し機械的に実装可能に。当初は「散文に一意解が無い」ため no だった）
- Requires user decision: no（grill 済み）
- Affected files: 2（`docs/detection-rust.md` 新規＋`CLAUDE.md` の検出節を参照へ圧縮。grill 2026-07-14 で決定。`docs/tech-stack.md`/`src/*.rs` は参照のみ）
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

### 決定事項（grill 済み・2026-07-14）
- **スコープ = 検出＋幾何確定（quad＋出力アスペクト決定）まで**。前処理 → 候補生成3系統 →
  `score_quad` → 上下辺リファイン → `decide_output_aspect`（真アスペクト復元）までをカバーする。
  warp は「`from_control_points(src,dst)` の方向」という落とし穴のみ触れ、実行詳細は書かない。
  **enhance（シャープ化/露出/色）・CLI/バッチ・report.html は対象外**（タイトル「認識方式」に忠実）。
  → 章立ての 1〜7 はこの境界と一致。逸脱（enhance 等）は書かない。
- **図は Mermaid**（GitHub ネイティブの ` ```mermaid ` フェンスブロック）で描く。最低限:
  ①パイプライン全体フロー（前処理→候補生成→スコアリング→リファイン→アスペクト決定）、
  ②候補生成3系統の分岐（contour / hough / minrect が並行しスコアで統合される様子）。
  Mermaid に馴染まない細部（`_edge_profile` の法線サンプリング等）は文章で補う。
  ⚠ `tech-stack.md` §5 は ASCII フローのままなので**スタイルは不揃いになる**点は許容
  （tech-stack は既存維持、新規 doc は Mermaid）。
- **CLAUDE.md の「検出の設計要点」節は参照に縮める**。新規 doc を正とし、CLAUDE.md 側は
  要点サマリ＋「詳細は `docs/detection-rust.md`」ポインタに圧縮する（変更ファイルは
  `docs/detection-rust.md` 新規＋`CLAUDE.md` 編集の 2 つ）。
  ⚠ CLAUDE.md は**エージェント向けの運用ガイド**なので、次はそのまま残すこと（縮めすぎない）:
  「⚠ Rust 実装の落とし穴」の実務的注意（`from_control_points(src,dst)` 方向・`(x as f64)<y`・
  EXIF 自前適用）、「§検証」の eval-output 再生成ルール、認識アルゴリズム変更時の運用手順。
  縮めるのは設計思想の**説明的記述**（重み根拠・cut/edge の理屈の詳説など）で、そこを doc へ移す。

### 数値のソース裏取り（grill 2026-07-14 に実施）
執筆は CLAUDE.md の値を鵜呑みにせず**ソースを正**として転記する。今回 grill で照合済み:
- ✅ ソースと一致: 重み `area0.12 / rect0.05 / aspect0.06 / contrast0.12 / fill0.20 / edge0.25 /
  cut0.20`（`detect.rs:608-614`）、`cut_score = 1 - min(1, 1.5*cut)`（`606`）、
  `edge = 0.5*mean + 0.5*min`（`488`）、Hough ROI 拡張 `0.18`（`309-310`）、
  minrect 信頼度係数 `0.6`（`958`）、面積ハードゲート下限 `0.04`（上限は `1.6`＝要記載、`587/590`）。
- ⚠ **要現地確認**: 出力アスペクトの「`persp<0.12` かつ 比 `<1.45` のとき 4:3」ゲートは
  `geometry.rs` に `1.45` リテラルが見当たらず grep で未確認。実装時に `decide_output_aspect`
  （`geometry.rs:205`）と `rectified_aspect`（`144`）を読み、**実際のしきい値を転記**すること
  （CLAUDE.md 側が drift している可能性がある）。
- 関数名の正確化: 候補生成は `contour_candidates`（`detect.rs:237`）/ `hough_candidates`（`303`）/
  `min_area_rect_of`（`250`）。章立ての `contour`/`hough`/`minrect` は略称。アスペクト系は
  `estimate_aspect`（見かけ比）/ `rectified_aspect`（透視復元）/ `decide_output_aspect`（最終決定）。

## Resolution

### 作成した doc（`docs/detection-rust.md`）
- 章立ては当初ドラフト通り 1.概要 / 2.前処理 / 3.候補生成3系統 / 4.統合スコア `score_quad` /
  5.上下辺リファイン / 6.出力アスペクト決定 / 7.Rust 固有の実装差・落とし穴 + 関連ドキュメント。
- 冒頭に本 doc・`tech-stack.md` §5（言語非依存方針）・`CLAUDE.md`（要点サマリ）の役割分担と
  「認識アルゴリズムは Rust 本体が正／数値はソースが正」を1段落で明記。
- Mermaid 図は指定通り2枚:
  ① パイプライン全体フロー（load_oriented → to_work → 前処理 → 候補生成 → score_quad →
     refine_top/bottom → 座標復元 → decide_output_aspect）。
  ② 候補生成3系統の分岐（contour/hough/minrect が並行し score_quad へ統合、minrect ×0.6）。
- Mermaid に馴染まない `edge_profile` の法線サンプリング（edge/cut の同時算出・係数0.5・cut_score）は
  §4 に文章＋重み表で補足。各数値の直後に出典（関数名・ファイル:行）を併記。

### CLAUDE.md の圧縮
- 「### 検出の設計要点」節の**設計思想の説明的記述**（fill/edge/cut/contrast の理屈詳説、
  Hough 帯域層化の手順詳細、上下辺リファインの走査ロジック詳細）を doc へ移動し、要点サマリ＋
  「詳細は `docs/detection-rust.md`」ポインタ（冒頭に引用ブロック）に圧縮（約29行→約16行）。
- **運用注意はそのまま残置**: 「### ⚠ Rust 実装の落とし穴・Python との実装差」節、「## 検証」の
  eval-output 再生成ルール、認識アルゴリズム変更時の運用手順は無改変。3系統名・主要重み・
  sub-slide 対策=cut+方向付き edge・上下辺リファインの存在といったエージェント向け即答情報は残した。

### ソース裏取り結果（特に decide_output_aspect）
- チケット記載の「`persp<0.12` かつ 比 `<1.45` のとき 4:3」は**要約であり実条件と乖離**。ソース
  （`geometry.rs:205-229`）の実ロジックは3段階:
  1. 復元値 `rec` 採用には `persp < PERSP_RECTIFIED_MAX(0.12)` **に加え**見かけ比との log 一致
     `|ln(apparent/rec)| < AGREE_LOG_TOL(0.10)` が必要。
  2. それで決まらねば `persp < PERSP_APPARENT_MAX(0.05)` のとき見かけ比 `apparent` を採用（別経路）。
  3. 4:3 判定ゲートは上限 `ASPECT_43_MAX(1.45)` **だけでなく下限 `1.05`** も持つ（`1.05 < r < 1.45`）。
- ⚠ チケットは「`geometry.rs` に `1.45` リテラルが見当たらない」としたが、**実在する**（名前付き定数
  `const ASPECT_43_MAX: f64 = 1.45;`, `geometry.rs:202`。grep がインラインリテラル前提で見落としたと推測）。
  doc §6 に「CLAUDE.md との差」注記として上記(a)(b)(c)と定数所在を明記。
- 重み `area0.12/rect0.05/aspect0.06/contrast0.12/fill0.20/edge0.25/cut0.20`（`detect.rs:608-614`）、
  `cut_score=1-min(1,1.5*cut)`（`606`）、`edge=0.5*mean+0.5*min`（`488`）、Hough ROI `0.18`（`309-310`）、
  minrect 係数 `0.6`（`957-959`）、面積ゲート下限 `0.04`/上限 `1.6`（`587`）はソースと一致確認。

### 検証
- 記載した全関数名（`contour_candidates`/`hough_candidates`/`min_area_rect_of`/`score_quad`/
  `refine_top_edge`/`refine_bottom_edge`/`fill_holes`/`brightness_mask`/`to_work`/`edge_profile`/
  `contrast_of`/`approx_quad`/`polar_intersection`/`bright_bbox`/`largest_contours`/`detect_slide`、
  `order_corners`/`rectangularity`/`estimate_aspect`/`rectified_aspect`/`decide_output_aspect`、
  `load_oriented`）と定数の実在を grep で確認。引用したファイル:行も突き合わせ済み。
- Mermaid フェンス 2 個・総フェンス 6 個で開閉一致、見出し階層 `#`→`##`→`###` 健全、相対リンク
  （`./tech-stack.md`/`../CLAUDE.md`/`src/*`）妥当を確認。コード非変更のため `cargo build` は不要。
