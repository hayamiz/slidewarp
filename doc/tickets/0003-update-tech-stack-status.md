---
title: docs/tech-stack.md を現在の採用状況に合わせて更新
type: docs
priority: medium
status: open
created: 2026-07-14
updated: 2026-07-14
---

## Description

`docs/tech-stack.md` は技術選定の**検討段階**のドキュメントのままで、実際の採用結果と
乖離している。現状を反映して更新する。

主な乖離点:

- §4 の推奨は「本命: 選択肢B（Rust + `opencv` crate）」「対抗: 選択肢A（Python + OpenCV）」
  だが、**実際に採用されたのは純Rust（`image` + `imageproc`、OpenCV 非依存の単一バイナリ）**
  であり、選択肢D 寄りの構成（ただし ML の `ort` は未使用）。この決定と理由を追記する。
- §3(d)/選択肢D の「四角形抽出・透視変換を自作、工数最大」の評価は、実際には
  自前 Douglas-Peucker・imageproc の Hough/warp・上下辺リファイン等で品質良好まで到達
  している（`CLAUDE.md` の現状参照）。この結果を反映する。
- Python 版（`python/`）は**実験用**として残り、`--remove-people`（人物セグメンテーション）
  のみ Python 実装、という現在の役割分担を明記する。
- §6「未決定事項 / 次アクション」は多くが決定済み（主軸=純Rust、出力形式、検出失敗時ポリシー
  `--on-low-confidence` 等）。決定済み項目をチェック済みにするか「決定事項」節へ移す。

「検討ドキュメント」としての履歴的価値は残しつつ、冒頭に現在の採用状況サマリを置く、
または決定を反映する形が望ましい。方針は着手時に相談してよい。

## Triage

- Complexity: medium
- Mechanical fix: no
- Requires user decision: yes
- Affected files: 1（`docs/tech-stack.md`。`CLAUDE.md`/`Cargo.toml`/`README.md` は参照のみ）
- Fix strategy: in-place
- Notes: 乖離点は裏取り済み（`Cargo.toml` の依存は image/imageproc/rayon/clap/serde/anyhow/walkdir のみで opencv crate も ort も不在。CLAUDE.md/README も純Rus t・OpenCV非依存・Python は実験で --remove-people のみ Python 実装と明記）。だが本チケットは「冒頭サマリを置く vs 各節に反映」という構造方針の選択を明示し「着手時に相談してよい」としているため mechanical=no / user-decision=yes。編集は doc 1ファイルのみで in-place。

## Implementation Notes

### 裏取りした事実（CLAUDE.md / Cargo.toml / README.md）
- **採用は純Rust**: `Cargo.toml` の依存は image / imageproc / rayon / clap / serde /
  serde_json / anyhow / walkdir のみ。`opencv` crate は不在で、選択肢B（Rust +
  opencv crate）は採用されていない。OpenCV 非依存・単一バイナリ配布（musl 静的リンク）。
- **ort は未使用**: `ort`(ONNX Runtime) は依存に存在しない。選択肢D の ML 路線は採らず、
  classical 幾何処理を自作した構成（＝「選択肢D 寄り、ただし ML なし」が実態）。
- **品質は良好まで到達**: CLAUDE.md「現状」より手元 24 枚で人手評価ほぼ全て crop5。
  難ケースも検出成立、残る弱点は 19.55.25 の上辺のみ。§3(d) の「工数最大・品質リスク最大」
  評価は実績と乖離。
- **Python の役割**: `python/` は実験用。Rust 未移植の `--remove-people`（torchvision
  DeepLabV3 の人物セグメンテーション + inpaint）のみ Python 実装。認識アルゴリズムは
  Rust 本体が正。

### 更新方針の候補（着手前に要相談）
- **案A: 冒頭に「現在の採用状況サマリ」節を追加（推奨）**: §1 の前に「## 0. 現在の採用状況
  （本ドキュメントは検討経緯として保存）」を置き実採用を要約、§2〜§6 は経緯として原文保持。
  差分が小さく経緯も残るが、サマリと本文が二重化し本文だけ見ると古い印象。
- **案B: 各節に決定を反映（in-line 更新）**: §4 を「採用: 純Rust（選択肢D 寄り、ort 不採用）」に
  書き換え、選択肢B は「検討時の第一候補」と明記。§3(d) に「実績: 品質良好まで到達」追記、
  §6 を決定済みに更新。全体が現状と一致し正確だが差分が広く、経緯が薄れる。§4 の推奨ロジック
  （②単一バイナリは妥協可）と実採用（単一バイナリ寄りの選択肢D）の食い違いに説明追記が要る。

### §6「未決定事項」の決定済み反映（両案共通）
- 主軸スタック → **純Rust（image + imageproc）に決定**（Python は実験用に併存）。
- ML を初期から入れるか → **classical で開始、ort 未導入**（人物除去のみ Python 側 torch 実装）。
- 検出失敗時ポリシー → **決定済み**: 低信頼はスキップ or 原本コピー（`--on-low-confidence`、
  既定スキップ）+ レビュー用フォルダ + report.html。
- 出力形式・命名・サイドカー → 出力アスペクトは 4:3/16:9、report.html 生成が既定
  （命名規則・JSON サイドカーの要否は実装確認が要れば残す）。

### 残る決定点
1. 構造方針（案A/案B、または折衷）。← 「着手時に相談」対象。
2. §4 の推奨ロジックと実採用の食い違いをどこまで書き換えるか（経緯として残すか上書きするか）。
3. §6 の「出力形式・命名・サイドカー」で実装未確認の項目を決定済みとするか要確認で残すか。
