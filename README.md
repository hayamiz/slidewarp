# slidewarp

学会などで撮影したスライド写真から、**スライド領域を自動検出してトリミング＋台形補正
（透視補正）＋シャープ化**し、きれいな 4:3 / 16:9 の画像として書き出す CLI ツールです。

スライドが画角からはみ出す・観客の頭や講演者が被る・暗所や明るい会場・強い斜め撮影と
いった条件でも、頑健に「スライドらしい矩形」を推定します。

- **本体は Rust 実装**（`image` + `imageproc`）。OpenCV 非依存で、**依存ライブラリのない
  単一バイナリ**として配布できます。
- `python/` に**実験用の Python 実装**（OpenCV ベース）を同梱。新機能の試作やアルゴリズム
  検討に使います（`--remove-people` 人物除去など、まだ Rust 未移植の機能もこちら）。

## インストール / ビルド

### ビルド不要（curl ワンライナー）

ビルド済みバイナリを GitHub Releases から取得して導入します。

```bash
curl -fsSL https://raw.githubusercontent.com/hayamiz/slidewarp/master/scripts/install.sh | sh
```

- **対応プラットフォーム**: Linux x86_64 / macOS arm64 (Apple Silicon) / macOS x86_64 (Intel)。
- **既定インストール先**: `~/.local/bin`（PATH に無ければ shell 別の追記コマンドを案内）。
- **Linux x86_64 は既定で musl 静的バイナリ**（依存なし）。
- 環境変数で上書き可能:
  - `SLIDEWARP_VERSION`: バージョン固定（既定 latest。例 `SLIDEWARP_VERSION=v1.2.3`）
  - `SLIDEWARP_INSTALL_DIR`: インストール先（既定 `~/.local/bin`）
  - `SLIDEWARP_TARGET`: target 明示上書き（例 glibc 版が要るとき
    `SLIDEWARP_TARGET=x86_64-unknown-linux-gnu`）
- **aarch64 の Linux と Windows は非対応**です。下記の `cargo` ビルドを使ってください。

### ソースからビルド（Rust）

```bash
# Rust 版（本体）
cargo build --release
# → target/release/slidewarp が単一バイナリ

# 実行
./target/release/slidewarp ./input-samples -o ./out
```

### musl 静的バイナリのビルド・検証（リリースと同じ完全静的版）

GitHub Releases の Linux バイナリは **musl でリンクした依存ゼロの完全静的版**（glibc
バージョンを問わずどこでも動く）。手元で同じものをビルド・検証するには、musl ターゲットと
**musl 用の C ツールチェーン**が要る（アロケータの `mimalloc` が C コードを `cc` で
コンパイルするため、`cc` だけでなく musl 向けの C コンパイラが必要）。

```bash
# 1. Rust の musl ターゲットを追加
rustup target add x86_64-unknown-linux-musl

# 2. musl 用 C ツールチェーンを導入（mimalloc の C ビルドに必要）
sudo apt-get install musl-tools      # Debian/Ubuntu（musl-gcc を提供）
# apk add musl-dev gcc               # Alpine の場合

# 3. musl 静的バイナリをビルド
cargo build --release --target x86_64-unknown-linux-musl
# → target/x86_64-unknown-linux-musl/release/slidewarp

# 4. 完全静的か確認（下記のように出れば OK）
ldd  target/x86_64-unknown-linux-musl/release/slidewarp   # → "not a dynamic executable"
file target/x86_64-unknown-linux-musl/release/slidewarp   # → "statically linked"
```

> ネイティブ（glibc）版で十分なら手順 1〜2 は不要で `cargo build --release` だけでよい。
> `musl-tools` を入れずに musl ターゲットをビルドすると、mimalloc の C ビルドが
> C コンパイラ不在で失敗する点に注意。

## 使い方

```bash
# ファイル/フォルダ混在で入力可（フォルダは再帰探索）
slidewarp ./photos -o ./out

# 露出/色調補正も行う
slidewarp ./photos -o ./out --exposure --color

# 低信頼の写真は原本を out/_review へ退避（既定はスキップ）
slidewarp ./photos -o ./out --on-low-confidence copy

# 全オプション
slidewarp --help
```

主なオプション: `--margin`（周辺マージン, 既定 0.03）, `--sharpen`, `--exposure`,
`--color`, `--min-confidence`, `--on-low-confidence`, `--max-long-side`, `-j/--jobs`,
`--no-report`。

処理後、出力ディレクトリの `report.html` をブラウザで開くと、元画像／処理後を並べて
人手評価（切り出し位置・見た目を 1〜5 で採点、コメント、JSON/CSV 入出力、全消去）できます。

## 主な特徴（検出アルゴリズム）

- **多段フォールバック検出**: 明度事前分布（暗所中の明るい矩形）＋輪郭四角形＋**Hough
  直線交点**（辺の一部から四隅を外挿。はみ出し・オクルージョンに強い）＋最小外接矩形。
- **統合スコア `score_quad`**: 内部の明るさ（fill）／方向付き edge_support（内部線を減点し
  本物の枠を優遇）／cut（スライドの一部だけ切り出す誤りを局所評価で減点）／contrast。
- **上下辺リファイン**: 暗いタイトル帯や下部ロゴが切れないよう、真の枠まで辺を延ばす。
- **出力は 4:3 か 16:9**: Zhang-He 透視補正で真の縦横比を復元し、確度が高くない限り 16:9。
- **EXIF 回転対応**、**画角外は黒塗り**、低信頼時は壊れた出力を出さずスキップ/原本退避。

## プロジェクト構成

```
slidewarp/
  Cargo.toml, src/        Rust 本体（main/detect/geometry/warp/enhance/report）
  input-samples/          検証用写真を置く場所（写真自体はリポジトリ非同梱。各自配置）
  docs/tech-stack.md      技術選定の検討ドキュメント
  python/                 実験用 Python 実装（OpenCV。--remove-people 等）
  CLAUDE.md               開発ガイド（AI エージェント向け）
```

## 実験用 Python 実装（`python/`）

OpenCV ベースで、アルゴリズムの試行錯誤や、Rust 未移植の機能（`--remove-people` による
人物セグメンテーション＋inpaint など）に使います。詳細は [`python/README.md`](python/README.md)。

```bash
cd python && uv sync
uv run slidewarp ../input-samples -o out/
```

## ライセンス

MIT License（[LICENSE](LICENSE)）。
