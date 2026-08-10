---
title: --version オプションでバージョンを表示する
type: feature
priority: medium
created: 2026-08-08
updated: 2026-08-10
status: resolved
---

## Description

`slidewarp --version`（および慣例的に `-V`）で自身のバージョンを表示できるようにする。

現状 `slidewarp` は `--version` を持たず、疎通確認は `--help` で代用している
（`scripts/install.sh` のコメントにも「slidewarp は --version を持たず --help を持つ」と
明記されている）。自己更新機能（`--system-update` / `--check-update`）で現在バージョンと
最新リリースを比較・表示するようになった今、ユーザーが手元のバージョンを単体で確認できる
`--version` がないのは不自然で、install.sh の疎通確認も `--version` に一本化できる。

期待する挙動:

- `slidewarp --version` および `slidewarp -V` が `slidewarp 0.2.0` のような
  `<name> <version>` 形式（バージョンは `Cargo.toml` = `env!("CARGO_PKG_VERSION")`）を
  標準出力に表示して終了する。
- 既存の必須引数（`inputs` / `--out-dir`）が無くても `--version` 単体で動作する
  （`--system-update` / `--check-update` と同様に、引数検証より前に処理される）。

実装メモ（参考）:

- clap derive では `#[command(version)]` を付けるだけで `--version`/`-V` が自動生成され、
  値は `CARGO_PKG_VERSION` になる。`-V` が既存の短縮フラグ（`-y` 等）と衝突しないことだけ
  確認すればよい。
- 併せて `scripts/install.sh` の疎通確認（現状 `--help`）を `--version` に更新するとよい。

## Resolution

- `src/main.rs` の `#[command(...)]` に `version` を追加し、clap が `--version` / `-V` を
  自動生成（値は `env!("CARGO_PKG_VERSION")` = `Cargo.toml`）。既存の短縮フラグ
  （`-o`/`-j`/`-y`）と `-V` は非衝突。`slidewarp --version` / `-V` とも `slidewarp 0.2.1`
  を出力することを確認。
- `scripts/install.sh` の疎通確認を `--help` から `--version` に更新（出力に版数も表示）。
- コミット: `37ebd47 feat(cli): --version と処理中の進捗表示を追加 (resolve #0007, #0008)`。
