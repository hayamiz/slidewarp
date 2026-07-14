---
title: Windows 対応（バイナリ配布・インストーラ）
type: feature
priority: low
status: open
created: 2026-07-14
updated: 2026-07-14
---

## Description

現状 slidewarp は Windows をサポートしていない。#0002（curl ワンライナー導入）の grill で
「Windows は対象外」と決定した際に、将来対応として本チケットに切り出したもの。

対応するとした場合に必要な作業（いずれも未着手）:

- **リリースビルドの追加**: `.github/workflows/release.yml` に Windows ターゲット
  （`x86_64-pc-windows-msvc` および必要なら `aarch64-pc-windows-msvc`）を追加し、
  `.zip`（+ sha256）で Release に添付する。tar.gz より zip が Windows では自然。
- **インストーラ**: #0002 の `scripts/install.sh` は POSIX sh 前提で Windows ネイティブでは
  動かない。PowerShell インストーラ（`scripts/install.ps1`、`irm https://.../install.ps1 | iex`
  形式）を用意するか、`winget` / `scoop` などパッケージマネージャ対応を検討する。
- **動作検証**: 純Rust（image + imageproc）構成なので OpenCV 等のネイティブ依存は無く、
  クロスコンパイル自体のハードルは低い見込み。EXIF 回転（`load_oriented`）やパス処理など
  Windows 固有の挙動差を実機/CI で確認する必要がある。

## 補足

- 優先度 low: 学会撮影スライドの処理という用途上、当面の主対象は Linux/macOS。
  Windows ユーザーは当面 WSL / Git Bash 上で Linux バイナリを使うか、`cargo build` で対応可能。
- 関連: #0002（curl ワンライナー導入・Linux/macOS 対象）。ARM Linux ビルド追加は別途要検討
  （#0002 の決定事項参照）。
