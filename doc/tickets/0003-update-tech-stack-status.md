---
title: docs/tech-stack.md を現在の採用状況に合わせて更新
type: docs
priority: medium
status: ready-to-apply
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
- Mechanical fix: yes（2026-07-14 に構造方針＝案A を確定し機械的に実装可能に。当初は案A/案B が未決で no だった）
- Requires user decision: no（案A に決定済み）
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

### 決定事項（2026-07-14 確定）
- **構造方針 = 案A**: `docs/tech-stack.md` 冒頭（§1 の前）に「## 0. 現在の採用状況（本文は
  検討経緯として保存）」節を新設し、実採用を要約する。**§1〜§6 の本文は原文のまま保持**
  （検討経緯として残す）。→ これにより下記が自動的に決まる:
  - 残決定点1 → 案A。
  - 残決定点2（§4 の推奨ロジック書換え）→ **書き換えない**。§4 は「検討当時の推奨」として原文保持し、
    現採用との差は §0 サマリが担う（§0 に「§4 の本命=選択肢B は検討当時の判断」と一言添える）。
  - 残決定点3（§6 の未確認項目）→ **§6 本文は原文保持**。決定済み事項は §0 に集約し、実装未確認の
    細目（命名規則・JSON サイドカーの要否）は §0 で断定せず触れないか「実装準拠」とする。
- **§0 の内容**（`CLAUDE.md`/`Cargo.toml` で裏取り済みの事実を要約）:
  - 採用 = 純Rust（`image` + `imageproc`）、OpenCV 非依存の単一バイナリ（musl 静的）。
    `opencv` crate は不使用＝選択肢B は不採用、選択肢D 寄り。
  - ML は未導入（`ort` は依存に無い）。classical 幾何処理を自作。
  - Python（`python/`）は実験用で、Rust 未移植の `--remove-people`（人物セグメンテーション＋inpaint）のみ実装。
  - §6 の未決定事項の主要項目は決定済み（主軸=純Rust / ML は classical 開始 / 検出失敗時は
    `--on-low-confidence`）。
- **自己矛盾の回避**: §0 冒頭に「以降の §1〜§6 は選定検討時の記録。未決定事項は §0 で解決済み」と
  明記し、§6 の未チェック箇条書きと §0 の齟齬を枠付けで吸収する（§6 本文自体は書き換えない）。
- **スコープ**: 変更は `docs/tech-stack.md` のみ。`CLAUDE.md`/`Cargo.toml`/`README.md` は裏取り用の参照で非編集。

## Resolution

構造方針＝案Aで `docs/tech-stack.md` を更新した。変更は同ファイル1つのみ（40行追加・0行削除）。

### 追加した §0「現在の採用状況（本文は検討経緯として保存）」の要点
- 冒頭に「§0 が現在の実採用の正。§1〜§6 は選定検討時の記録で、§6 の未決定事項含め当時未確定の
  論点はすべて §0 で解決済み」と明記し、本文との齟齬を枠付けで吸収。
- 採用スタック: 純Rust（`image` + `imageproc`）、OpenCV 非依存の単一バイナリ（musl 静的リンク）。
  `opencv` crate 不使用＝§4 本命の選択肢B は不採用、実採用は選択肢D 寄り。
- ML 未導入（`ort` 不使用）、classical 幾何処理を自作。§3(d)/選択肢D の「工数最大・品質リスク最大」
  評価に対し、実績は手元 24 枚で人手評価ほぼ全て良好まで到達。
- Python（`python/`）は実験用併存。認識アルゴリズムは Rust 本体が正、Rust 未移植の
  `--remove-people` のみ Python 実装。
- §6 の未決定事項の解決を集約（主軸=純Rust / classical 開始・`ort` 未導入 / 検出失敗時は
  `--on-low-confidence`＋レビュー用フォルダ＋report.html）。命名規則・JSON サイドカーの要否は
  「実装準拠」とし断定せず。
- §4 冒頭に1行の注記（「本節の推奨は検討当時のもの。実採用は §0 参照」）のみ追加。§4 本文のロジックは非改変。

### 裏取り結果（`Cargo.toml`）
- `[dependencies]` = image / imageproc / rayon / clap / serde / serde_json / anyhow / walkdir /
  mimalloc のみ。`opencv` crate も `ort` も**不在**。§0 の主張と一致。CLAUDE.md の現状記述とも矛盾なし。

### 検証結果
- `git diff --numstat` = 40 insertions / 0 deletions（§1〜§6 の本文は原文のまま保持、削除行なし）。
- 見出し階層は `## 0.` → `## 1.` … `## 6.` の番号順を確認。既存リンク（§3 の crates.io 等）は非改変。
- コード非変更のため cargo build は不要。

### 評価者による独立レビュー
- Evaluator: PASS — Sonnet 評価者（ticket:ticket-evaluator）。Codex CLI は本セッションで 401
  Unauthorized（未認証）を確認済みのためフォールバック。評価者は §0 の全事実を独立に裏取り:
  `Cargo.toml` の依存（image/imageproc 有・opencv/ort 無）、`--remove-people` が `python/` のみで
  `src/*.rs` に無いこと、`--on-low-confidence` 既定=skip（main.rs:57）、`report.html`/`--no-report`、
  musl ターゲット（release.yml）を確認。`git diff --numstat` = 40 insertions / 0 deletions で §1〜§6
  本文の非改変、hunk が §0 挿入と §4 1行注記の2つだけであること、変更が docs/tech-stack.md 1ファイル
  のみ（CLAUDE.md/Cargo.toml/README 非変更）を確認。事実誤り・スコープ逸脱なし。
- human-review: optional（docs のみ・挙動変更/セキュリティ面なし・事実突き合わせ済み）。
  → PASS + optional のため `ready-to-apply`。`/ticket-apply` で着地可能。
