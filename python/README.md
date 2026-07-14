# slidewarp — Python 実験版

> これは **実験用の Python 実装**です。正式版・配布物はリポジトリ直下の **Rust 実装**
> （[../README.md](../README.md)）。この Python 版はアルゴリズム試作や、Rust 未移植機能
> （`--remove-people` 人物除去など）の検証に使います。

学会で撮影したスライド写真を一括処理する CLI。写真からスライドの矩形領域を検出し、
トリミング + 台形補正 + シャープ化を行う。露出・色調補正は任意で有効化できる。

スライドが画角からはみ出したり、観客の頭・話者が被っている写真でも、明度事前分布・
輪郭・**Hough 直線交点**（辺の一部から四隅を外挿）・（任意で）ML セグメンテーションの
多段フォールバックで矩形を推定する。技術選定の経緯は [`../docs/tech-stack.md`](../docs/tech-stack.md)。

出力画像のアスペクト比は必ず **4:3 か 16:9** に揃える（スライドの見かけの縦横比から
推定し、判断が難しい場合はデフォルト 16:9）。処理後は出力フォルダに評価用の
`report.html` を生成し、ブラウザで元画像/処理後を並べて人手評価できる。

## セットアップ

```bash
uv sync                 # 依存を .venv に導入
# もしくは: pip install -e .

# 人物除去(--remove-people)を使う場合は ML 追加依存(torch/torchvision)を導入
uv sync --extra ml      # もしくは: pip install -e '.[ml]'
```

## 使い方

```bash
# ファイル/フォルダ混在で入力可（フォルダは再帰探索）
uv run slidewarp ../input-samples/ -o out/

# 露出・色調補正も行う
uv run slidewarp ../input-samples/ -o out/ --exposure --color

# ML セグメンテーションモデルを併用（ONNX。契約は slidewarp/ml.py 参照）
uv run slidewarp ../input-samples/ -o out/ --ml-model models/screen_seg.onnx

# 遮蔽者(人物)を検出から除外し、切り出し内の人物を inpaint 除去（要 slidewarp[ml]、低速）
uv run slidewarp ../input-samples/ -o out/ --remove-people

# 低信頼の写真は原本を out/_review へ退避（既定はスキップ）
uv run slidewarp ../input-samples/ -o out/ --on-low-confidence copy
```

処理後、`out/report.html` をブラウザで開くと一覧レビューできる。画像ごとに
「切り出し位置」「見た目（色調/露出/シャープ）」を 1〜5 で採点し、改善点コメントを
入力できる。入力は自動保存され、`JSON出力` / `CSV出力` でエクスポート、`JSON取込` で
再取込、`全消去` で入力済み評価を一括クリアできる。レポート不要時は `--no-report`。

評価は「その生成物」に紐づいて保存される（出力を再生成すると自動で新規状態になり、
前回の古いコメントが別の画像に残らない）。再生成前に評価を残したいときは `JSON出力` を。

主なオプション: `--sharpen`（強さ, 0で無効）, `--min-confidence`, `--margin`（周辺マージン,
既定0.03）, `--max-long-side`, `--ext`, `-j/--jobs`（並列数）, `--no-report`。全て `slidewarp --help`。

## 構成

| モジュール | 役割 |
|---|---|
| `slidewarp/detect.py` | 多段フォールバックのスライド矩形検出とスコアリング |
| `slidewarp/geometry.py` | 四隅の順序化・面積・矩形度・アスペクト推定・直線交点 |
| `slidewarp/warp.py` | 台形補正（透視変換）。出力を 4:3/16:9 に整える |
| `slidewarp/enhance.py` | シャープ化 / 露出補正(CLAHE) / WB(gray-world) |
| `slidewarp/pipeline.py` | 1枚分の処理と低信頼時ポリシー |
| `slidewarp/report.py` | 評価用 `report.html` の生成 |
| `slidewarp/ml.py` | ONNXスクリーン検出(任意) / DeepLabV3人物セグメンテーション(`--remove-people`) |
| `slidewarp/cli.py` | CLI・並列実行 |

## ML について

- **`--remove-people`（人物セグメンテーション）**: `slidewarp[ml]`（torch/torchvision）を
  導入すると使える。DeepLabV3 で会場の人物領域を検出し、①遮蔽者のエッジを検出（候補生成）
  から除外して誤検出を防ぎ、②切り出し内に残った人物を inpaint で除去する。採点は常に
  実エッジで行うため、人物が真の枠の近くにあっても full-slide 検出を壊さない。CPU では
  1枚あたり数秒かかるため並列数は自動で控えめ(≤4)にする。モデル重みは初回に自動取得。
- **`--ml-model`（スクリーン検出, 任意）**: ONNX セグメンテーションモデルを渡すと候補に
  合流する。契約は `slidewarp/ml.py` 冒頭を参照。未指定なら classical 検出のみで動作する。
