---
title: release.yml に aarch64-linux ビルドを追加する
type: enhancement
priority: medium
status: open
created: 2026-07-14
updated: 2026-07-14
---

## Description

`.github/workflows/release.yml` は現状 Linux 向けを x86_64 のみ（gnu / musl）ビルドしており、
**aarch64-linux（ARM Linux）のバイナリを配布していない**。このため #0002 の install.sh は
Linux + aarch64 を検出したら非0終了し `cargo install` を案内する仕様になっている
（#0002 の決定事項参照）。ARM Linux ユーザーも curl ワンライナーで導入できるよう、
release.yml に aarch64-linux ターゲットを追加する。

想定する作業:

- release.yml のビルドマトリクスに `aarch64-unknown-linux-musl`（および必要なら
  `aarch64-unknown-linux-gnu`）を追加。#0002 の決定に合わせ **musl 静的を主**とする。
- クロスコンパイル手段の選定: `cross`（Docker ベース）か、GitHub Actions の
  ARM runner、または `cargo` + クロスリンカ。純Rust（image + imageproc、OpenCV 非依存）
  なのでネイティブ依存は無く、クロスコンパイルのハードルは低い見込み。
- アセット名は既存規則 `slidewarp-${GITHUB_REF_NAME}-<target>.tar.gz`（+ `.sha256`）を踏襲。
- 追加後、#0002 の install.sh から aarch64-linux の「未対応エラー」分岐を外し、
  通常のダウンロード対象に含める（本チケット完了時に #0002 側の追従が必要）。

## 補足

- 関連: #0002（curl ワンライナー導入）、#0004（musl 一本化＋mimalloc）。#0004 で
  リリースを musl 静的に一本化する方針と整合させること（x86_64/aarch64 とも musl 主）。
- 検証: 可能なら QEMU / ARM 実機で生成バイナリの起動と基本動作（1枚処理）を確認する。
