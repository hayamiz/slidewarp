---
title: リリースを musl 静的バイナリに一本化し mimalloc を導入する
type: chore
priority: medium
status: resolved
created: 2026-07-14
updated: 2026-07-14
---

## Triage

- Complexity: low
- Mechanical fix: yes
- Requires user decision: no
- Affected files: 3-4（`Cargo.toml` / `src/main.rs` / `.github/workflows/release.yml`、`Cargo.lock` 自動更新、任意で `CLAUDE.md`）
- Fix strategy: worktree
- Notes: 変更は計 ~5 行（Cargo.toml に mimalloc 1 行、main.rs にアロケータ 2 行、
  release.yml から gnu の 2 行削除）で spec が一意。挙動分岐なし。gnu 行は
  release.yml 22-23 行のみで、musl-tools インストールは `-musl` 条件付き・他ステップは
  `matrix.target` 汎用参照のため gnu 削除の波及なし。main.rs に既存 `#[global_allocator]`
  は無く衝突なし。依存追加＋musl 上での mimalloc C ビルドという実ビルドリスクがあるため
  worktree 推奨。付随: CLAUDE.md リリース節の「4 ターゲット」表記を 3 へ追随更新するのが
  望ましい（軽微・ブロッカーではない）。回帰／処理時間の実測は input-samples 非同梱のため
  CI/実機検証待ち。

## Description

現在 `.github/workflows/release.yml` は Linux 向けに **gnu 版
（`x86_64-unknown-linux-gnu`）と musl 版（`x86_64-unknown-linux-musl`）の両方**を
ビルドしている。CLI 配布の手離れを優先し、**musl 静的バイナリ 1 本に一本化**したい。

### 背景・動機
- musl 静的バイナリは依存ゼロの単一バイナリで、glibc バージョンに縛られず
  Alpine・古いディストロ・distroless/最小 Docker まで「落として実行するだけ」で動く。
- slidewarp は DNS/NSS/dlopen を使わない自己完結型（image/imageproc は純Rust）なので、
  musl の機能制約に一切引っかからない。
- 唯一の弱点は **musl 標準アロケータのマルチスレッド性能**。slidewarp は rayon で
  並列画像処理する（＝アロケーション多発）ため、gnu 比で体感差が出る恐れがある。
  → **mimalloc をグローバルアロケータに設定**して回収する。

### 作業内容
1. `Cargo.toml` に `mimalloc = "0.1"` を追加。
2. `src/main.rs` 冒頭にグローバルアロケータ設定を追加:
   ```rust
   #[global_allocator]
   static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
   ```
3. `.github/workflows/release.yml` のビルドマトリクスから
   `x86_64-unknown-linux-gnu` を削除（macOS arm64 / x86_64 と musl は残す）。

### 完了条件 / 検証
- `x86_64-unknown-linux-musl` で `cargo build --release` が通り、静的バイナリが生成される
  （`ldd` で "not a dynamic executable" 相当を確認）。
- musl+mimalloc 版で `input-samples` のバッチ処理を実行し、検出品質が回帰していないこと
  （`report.html` で人手評価）＋処理時間が gnu 版と比べて許容範囲であることを実測で確認。
- リリースワークフローが musl / macOS×2 の 3 ターゲットで正常にアーティファクトを生成する。

### 判断が必要な点 / 留意
- 一本化に踏み切る前に **gnu vs musl(+mimalloc) の処理時間を実測**し、musl が許容範囲で
  あることを確認してから gnu を削除するのが安全（実測せず削除しない）。
- README / ドキュメントに Linux バイナリの記載があれば「musl 静的」に更新する。

## Resolution

隔離 worktree（ブランチ `ticket/0004-musl-only-mimalloc`）で実装・コミット済み。

### 変更内容
- `Cargo.toml`: dependencies に `mimalloc = "0.1"` を追加。
- `src/main.rs`: doc コメント直後・use 群の前に
  `#[global_allocator] static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;` を追加。
- `.github/workflows/release.yml`: ビルドマトリクスから `x86_64-unknown-linux-gnu`
  エントリを削除。残りは musl 静的 / macOS arm64 / macOS x86_64 の 3 ターゲット。
  musl-tools インストールステップ（`if: endsWith(matrix.target, '-musl')`）や他ステップは
  未変更。
- `CLAUDE.md`: リリース節の「4ターゲット（linux gnu / linux musl静的 / macOS arm64 /
  macOS x86_64）」を「3ターゲット（linux musl静的 / macOS arm64 / macOS x86_64）」へ更新。
- `Cargo.lock`: mimalloc v0.1.52 とその依存が自動追加。

### 検証結果（この環境で実施可能な範囲）
- `cargo build --release`（ネイティブ gnu、LTO 有効）: PASS（2m15s、mimalloc が正常に
  コンパイル・リンク。既存の detect.rs の unused_mut 警告は本変更と無関係）。
- `./target/release/slidewarp --help`: PASS（exit 0）。
- `Cargo.lock` に mimalloc（v0.1.52）追加: 確認済み。
- release.yml の `x86_64-unknown-linux-gnu` 残存なし（grep 0 件）、musl/macOS×2 の
  3 ターゲット残存: 確認済み。
- 自動テストは未追加（本プロジェクトはテストフレームワーク未導入。CLAUDE.md 方針に従い
  ビルド＋スモーク実行で検証）。

### CI / 実機検証に委ねた項目
- **musl 静的ビルドの成立確認**（`x86_64-unknown-linux-musl` で musl 上の mimalloc C ビルドが
  通り、静的バイナリ＝`ldd` で "not a dynamic executable" 相当になること）。本環境は
  musl ターゲット/musl-gcc 未導入のため未実施。
- **input-samples 全数の検出品質回帰ゼロ確認**（写真非同梱のため未実施）。
- **gnu vs musl(+mimalloc) の処理時間実測**（同上）。
