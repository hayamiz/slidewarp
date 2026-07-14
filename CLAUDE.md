# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> このリポジトリでの応答・ドキュメント・コメントは日本語を基本とする（グローバル設定に従う）。

## プロジェクトの目的

学会で撮影したスライド写真を一括処理する CLI コマンド（`slidewarp`）を作る。
1枚以上の写真ファイル / フォルダを入力に取り、以下を行って出力ディレクトリに書き出す:

1. 写真中の「スライドが映っていると推定される矩形領域」を検出
2. トリミング + 台形補正（透視変換）
3. シャープ化
4. optional: 露出自動補正 / 色調補正（ホワイトバランス等）

**核となる難所は検出の頑健性**: スライドが画角からはみ出す、観客の頭や講演者が
下辺・端を隠す、暗所・逆光といった撮影条件でも「矩形らしい領域」を推定できること。

## リポジトリ構成（Rust 本体 + 実験用 Python）

- **`src/` + `Cargo.toml`（リポジトリ直下）= 本体・正式版。** 純Rust（`image`+`imageproc`、
  OpenCV 非依存の単一バイナリ）。コマンド名 `slidewarp`。
  Rust 独自: EXIF 回転の明示適用（`main.rs` の `load_oriented`。`image` crate は自動適用しない。
  cv2 は適用するので合わせた）＋上下辺リファイン（`detect.rs` の `refine_top_edge`/`refine_bottom_edge`。
  暗いタイトル帯/下部ロゴの切り落とし対策）。⚠ imageproc の warp は `from_control_points(src,dst)`。
- **`python/` = 実験用 Python 実装**（OpenCV + 任意で torch/ONNX）。アルゴリズム試作・検討用。
  Rust 未移植の `--remove-people`（人物セグメンテーション＋inpaint）はこちらのみ。以下の設計
  要点・既知課題は主にこの版で確立したもの（`python/slidewarp/*.py`）。
- `input-samples/` は検証写真の置き場（**写真はリポジトリ非同梱**。README のみ追跡。各自配置）。
  `docs/tech-stack.md`, `CLAUDE.md`, `README.md` は直下。
- git 初期化済み・GitHub 公開想定（MIT）。**⚠ 認識アルゴリズムは Rust 本体が正、Python は実験。**
  Rust 側の変更は必ず `cargo build --release` ＋ サンプル再評価で確認する。

## 現状

- 検出品質は Rust・Python とも `input-samples/` 24枚で良好（人手評価でほぼ全て crop5、
  残るは超斜め・暗所の `19.55.25` 上辺のみ）。検討経緯は `docs/tech-stack.md`。
- 検出品質（`input-samples/` 全22枚を実行・コンタクトシートで目視確認済み）:
  - 大半は良好に補正できる。遠景+下辺オクルージョン（`08.44.43`）や青被り会場で
    天井とスライドが地続き（`08.45.45`）といった難ケースも検出成立。
  - **既知の失敗/弱点**（人手レビューで要確認）:
    - `2026-06-24 19.44.34`: 遠景で会場の壁/スクリーン全体を掴む（スライドが小さく
      写るケース）。**明確な失敗**。→ ML 併用が最も効く。
    - `2026-06-24 19.55.25`: タイトル上端クリップ＋下部タブバー混入。
    - `08.45.45`: 上端わずかにクリップ。`19.47.29`: minrect でやや傾き残り。
  - `--color`（gray-world WB）で青被りは実用域まで補正できることを確認。
- **出力アスペクト比は必ず 4:3 か 16:9**（`geometry.decide_output_aspect`）。**Zhang-He の
  透視補正**（`rectified_aspect`: 主点=画像中心・正方画素仮定、消失点の直交条件で焦点距離を
  推定し真の w/h を復元）を使う。見かけの辺長比（`estimate_aspect`）は斜め撮影で 16:9 でも
  1.15〜1.53 に縮み誤 4:3 になるため。方針は**「確度が高くない限り 16:9」**: 透視が弱く
  （`persp<0.12`）復元が見かけ比とも整合し、かつ比が明確に 4:3 寄り（`<1.45`）のときだけ 4:3。
  斜め・復元不能は 16:9 に倒す（真 4:3 を強斜めで撮ると 16:9 になるのは意図的トレードオフ）。
- **切り出しは検出矩形を各辺 3% 外へ広げてから行う**（`warp` の `margin` 既定 0.03、`--margin`
  で調整）。トリミング後の画像だけでスライド全体が収まっているか判断できるよう周辺マージンを
  少し含める。出力アスペクト/サイズは元 quad 基準（マージン拡大の影響を受けない）。両版共通。
- 処理後は `out/report.html`（評価用レビューUI）を既定で生成（`--no-report` で抑止）。
- git 未初期化。

### 検出の設計要点（detect.py）
- 候補生成は3系統: `contour`（明度マスク＋Cannyの輪郭 approxPolyDP）/ `hough`
  （**Canny(gray) を明部bbox拡張ROIに限定→H/V線に分けて位置クラスタで重複除去→
  水平2×垂直2の総当りで四角形化**）/ `minrect`（最後の緩いフォールバック）。
- 選択は `score_quad` の統合スコア（重み: area0.12 rect0.05 aspect0.06 contrast0.12
  fill0.20 edge0.25 cut0.20）。要点:
  - **fill**（精度）: 内部が明度マスクで埋まる割合。`fill_holes` 版で内部の暗図版を誤減点しない。
  - **edge（方向付き edge_support）** と **cut** は `_edge_profile` が各辺を**法線方向に
    サンプリング**して同時算出する:
    - edge: 各辺が実エッジに乗る割合。ただし**内側も外側も明るい内部線は係数0.5に半減**
      （表罫線・文字行で稼ぐ小矩形を抑制）。真の枠は内側明・外側暗で満点。`0.5*mean+0.5*min`。
    - cut（recall の局所版）: 辺が明部を素通しで横切る度合い（内側明∧外側明∧外側にエッジ）。
      スライドの一部だけを囲む小矩形の切断辺で 1 になる。`cut_score=1-min(1,1.5*cut)`。
      辺の外側近傍だけ見るので、大域 coverage と違い**天井・明壁で汚染されない**
      （平坦な外側は cut にならない）。
  - **contrast**: 内部と外部の明度差。スライド全体は暗い会場に対し高く、内部小矩形は低い。
- ⚠ **sub-slide 誤り（スライドの一部だけ切り出す）対策の主役は cut と方向付き edge**。
  「内部の強いエッジで囲まれた小矩形」が edge_support だけでは高得点になる問題への対処なので、
  重みを触るときはこの2項の役割を崩さないこと。
- Hough: ROI は明部 bbox を 18% 外側へ拡張。候補生成は**帯域層化**（bbox 中線で上下・左右に
  分け各帯で長い順 top_k、上帯×下帯・左帯×右帯を掛け合わせ）で本物の外周線の生存率を上げる。
- ⚠ `minAreaRect` は定義上 `rect=1.0` を取り過信を招くため信頼度に係数 0.6。
- 台形補正の画角外は黒塗り（`warp.py` は `BORDER_CONSTANT`）。

### 既知の残課題（人手評価＋fableレビューで確認済み）
- `2016-04-12 08.24.20`（90度回転入力）: 上端の凡例/タイトルがまだ僅かにクリップ。本体は改善済み。
- 人影が隅に被るケースの被写体除去は `--remove-people` で対応（下記）。未使用時は人物は残る。
- 補足: 以前失敗していた sub-slide 誤り（19.47.29 明壁 / 19.44.34 タイトル区切り線 /
  7DC・C0FB・C83EE 部分切り出し）は cut ペナルティ＋方向付き edge_support＋Hough帯域層化で解消済み。

### 人物セグメンテーション（`--remove-people`、実装済み）
- torchvision DeepLabV3(person) を任意依存 `slidewarp[ml]` として導入（`slidewarp/ml.py`
  `PersonSegmenter`）。torch は重いので base 依存には入れない。mediapipe は libGLESv2 が
  必要でこの環境不可、u2net_human_seg(ONNX) は暗いシルエット遮蔽者を取りこぼすため不採用。
- **設計の要**: 人物マスクは①**候補生成のみ**でエッジ除外に使い、②採点は必ず実エッジ
  （除外なし）で行う。両方に効かせると人物が真の枠の近くにある画像（08.44.43）で枠エッジ
  まで消え内側領域に誤ロックする。生成のみ除外なら 08.44.43 は full-slide が実エッジで勝ち、
  19.47.29（登壇者が前に立つ）は除外で初めて出る正しい候補が勝つ、を両立できる。
- warp 後、人物マスクを同じ変換で写像し切り出し内の残存人物を inpaint 除去（warp.py）。
- 効果: `19.47.29` conf 0.55→0.85 で正検出、`08.44.43` は full-slide 維持＋人影 inpaint。
- CPU で 1枚数秒。並列は自動で ≤4 に抑制。

### 次段の改善候補（fableレビュー由来・未着手）
- **2段ズーム検出**: 明部 bbox が小さい遠景スライドは面積ゲート/Hough長で落ちる。1パス目の
  bbox 周辺を元解像度で切り出し再検出すると効く（detect.py の面積ハードゲート 0.04 も要見直し）。
- **上端の枠位置の精緻化**: 08.44.43 / 09.05.40 で上端が実際の枠でなく天井の模様を拾い
  余分に天井が入る。19.44.34 は文字の途中で上端クリップ、19.47.29 は上の黒帯がクリップ、
  2016-04-12 は上端の凡例クリップ。いずれも軽微（crop 4 相当）だが top 辺の位置決めが弱点。

## 設計上の重要方針（実装前に必ず踏まえる）

- **検出は多段フォールバック**で設計する。詳細は `docs/tech-stack.md` の §5。
  - 明度事前分布（暗所中の明るい矩形）→ 輪郭四角形(approxPolyDP) / **Hough 直線の交点**
    / 明度マスク / (optional) ML セグメンテーション、を候補生成しスコアで統合。
  - **Hough 直線の交点方式が「はみ出し・オクルージョン」対策の要**。辺の一部さえ
    見えれば四隅（画角外含む）を外挿できる。
- **検出信頼度がしきい値未満なら壊れた出力を出さない**。スキップ or 原本コピー +
  レビュー用フォルダ + 警告ログ。バッチで誤補正画像を量産しないことを最優先の安全策とする。
- 露出/色調補正は **optional フラグ**で制御し、デフォルトでは幾何補正+シャープ化のみ。
- バッチは並列化前提（Rust: `rayon` / Python: `multiprocessing`）。

## 検証

- 実装の検証は `input-samples/` に写真を置いて行う（写真は非同梱。開発時は手元の 24 枚で
  検証してきた）。特に頑健性の代表ケース:
  `2026-06-23 08.44.43.jpg`（遠景・強い台形歪み・下辺に観客の頭）、
  `2026-06-24 19.47.29.jpg`（明壁投影・登壇者が前に立つ／`--remove-people`向き）、
  `2026-06-24 19.44.34.jpg`（タイトル直下の区切り線で上端クリップ）。
  近接ケースは `2026-06-25 15.50.38.jpg`。
- 出力は目視確認する。台形補正後にスライド4辺が矩形に整い、文字が読めるかを見る。

- **⚠ 認識アルゴリズム等（detect / geometry / warp / enhance / ml など出力に影響する変更）を
  調整したら、必ず `eval-output/` を削除して作り直す**。古い出力が残った評価表で判断しない
  ため、常にフレッシュに再生成すること:
  ```bash
  # Rust 本体（リポジトリ直下で）
  cargo build --release && rm -rf eval-output && \
    ./target/release/slidewarp input-samples -o eval-output/ --on-low-confidence copy
  # Python 実験版（python/ 内で）
  cd python && rm -rf eval-output && \
    uv run slidewarp ../input-samples -o eval-output/ --on-low-confidence copy
  ```
  生成後 `eval-output/report.html` をブラウザで開いて人手評価する（評価JSON/CSVも出力可）。

## コマンド

Rust 本体（リポジトリ直下。要 `~/.cargo/bin` を PATH に）:
```bash
cargo build --release                                       # 単一バイナリ target/release/slidewarp
./target/release/slidewarp input-samples -o out/            # 基本実行
./target/release/slidewarp input-samples -o out/ --exposure --color   # 露出/色補正
./target/release/slidewarp --help                           # 全オプション
```

Python 実験版（`python/` 内で実行）:
```bash
uv sync                                        # 依存導入（.venv）
uv sync --extra ml                             # 人物除去(--remove-people)用に torch も導入
uv run slidewarp ../input-samples -o out/      # 基本実行
uv run slidewarp ../input-samples -o out/ -j 1 # 逐次実行（デバッグ向き）
uv run slidewarp ../input-samples -o out/ --remove-people   # 遮蔽者を除外+inpaint（要 [ml]）
uv run slidewarp ../input-samples -o out/ --ml-model models/x.onnx  # スクリーン検出 ONNX 併用
```

- Python の単体デバッグは `-j 1`（逐次）で。並列時は各ワーカーが例外を握って ProcessResult に
  格納するためトレースが見えにくい。検出内部は
  `uv run python -c "from slidewarp import detect; ..."` で直接叩ける。
- テストフレームワークは未導入。検証は `input-samples/` への実行＋出力目視が基本。
