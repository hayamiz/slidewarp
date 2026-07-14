# input-samples

検証用のスライド写真を置くディレクトリです。**写真ファイル自体はリポジトリに含めません**
（各自でここに配置してください）。この README 以外の中身は `.gitignore` で除外されます。

ここに任意のスライド写真（`.jpg` / `.jpeg` / `.png` など）を置いて、次のように実行します:

```bash
# Rust 本体（リポジトリ直下）
cargo build --release
./target/release/slidewarp input-samples -o eval-output/ --on-low-confidence copy

# Python 実験版
cd python && uv run slidewarp ../input-samples -o eval-output/ --on-low-confidence copy
```

生成された `eval-output/report.html` をブラウザで開くと結果を確認できます。
