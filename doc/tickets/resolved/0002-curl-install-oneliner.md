---
title: GitHub Releases の artifact を curl ワンライナーで導入する手順を整備
type: enhancement
priority: medium
status: resolved
created: 2026-07-14
updated: 2026-07-14
---

## Description

`.github/workflows/release.yml` が `vX.Y.Z` タグ push で 4 ターゲット
（linux gnu / linux musl 静的 / macOS arm64 / macOS x86_64）のバイナリを
tar.gz ＋ sha256 で Release に添付する。これを利用者が **curl ワンライナー**で
簡単に導入できる手順を整備する。

想定する成果物:

- OS/アーキテクチャを自動判定して最新 Release から適切な tar.gz を取得・展開し、
  `slidewarp` バイナリを PATH 上（例: `~/.local/bin` or `/usr/local/bin`）へ配置する
  インストール用 shell script（例: `scripts/install.sh`）。sha256 で整合性検証する。
- `curl -fsSL https://.../install.sh | sh` 形式のワンライナーを README に追記。
  バージョン指定（環境変数 `VERSION` 等）とインストール先の上書きにも対応。
- musl 静的版を Linux の既定にするか、gnu 版を既定にするかは要検討
  （静的版は依存が無く導入が確実）。

## Triage

- Complexity: medium
- Mechanical fix: yes（grill 2026-07-14 で全決定点を解消。以下「決定事項」に従えば一意に実装可能）
- Requires user decision: no（grill 済み）
- Affected files: 2〜3（新規 `scripts/install.sh`、`README.md` のインストール節、任意で `CLAUDE.md` リリース節）
- Fix strategy: worktree
- Notes: アセット名は release.yml で `slidewarp-${GITHUB_REF_NAME}-<target>.tar.gz`（+`.sha256`）と決定的。当初は既定ビルド・インストール先・aarch64-linux 未ビルドの扱いが未決だったが、grill で musl 静的既定 / `~/.local/bin` 既定 / aarch64-linux は非対応（エラー案内、release.yml 拡張は別チケット）/ 常に上書き / Windows 対象外 に確定し、機械的に実装可能になった。

## Implementation Notes

- 判定対象: `uname -s`（Linux/Darwin）と `uname -m`（x86_64/arm64,aarch64）を
  release.yml のターゲット名（tar.gz のファイル名規則）へマッピングする。
  実際のアセット名は release.yml の命名を確認して合わせること。
- 最新版取得は GitHub API（`/repos/hayamiz/slidewarp/releases/latest`）か、
  `/releases/latest/download/<asset>` のリダイレクトを利用。API レート制限に注意。
- `set -eu`、ダウンロード失敗・sha256 不一致時の明確なエラー終了、`curl` or `wget`
  のフォールバックなど、パイプ実行される install script の定石を踏襲する。
- 検討点: PATH に無い場合の案内、Windows は対象外でよいか、既存インストールの上書き挙動。

### 実アセット名（release.yml を確認して確定・2026-07 時点）
Package ステップは `dist="slidewarp-${GITHUB_REF_NAME}-<target>"` で命名し、
`tar czf "$dist.tar.gz"` と sha256（Linux は `sha256sum`、macOS は `shasum -a 256`）を
`"$dist.tar.gz.sha256"` へ出力、`softprops/action-gh-release@v2` で Release に添付する。
よって実アセットはタグ `vX.Y.Z` に対し以下の8ファイル（tar.gz 4種＋各 .sha256）:

- `slidewarp-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`（+ `.sha256`）
- `slidewarp-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz`（+ `.sha256`）← 完全静的
- `slidewarp-vX.Y.Z-aarch64-apple-darwin.tar.gz`（+ `.sha256`）← Apple Silicon
- `slidewarp-vX.Y.Z-x86_64-apple-darwin.tar.gz`（+ `.sha256`）← Intel Mac

tar.gz 内は `slidewarp-vX.Y.Z-<target>/` ディレクトリで、中身は `slidewarp`（実行ファイル）,
`README.md`, `LICENSE` の3点。.sha256 の中身は「<hash>  <tar.gz の basename>」なので、
tar.gz と同じ basename のカレントで `sha256sum -c` / `shasum -a 256 -c` がそのまま通る。

### OS/arch → target マッピング（uname ベース）
- `uname -s`: Linux→linux系, Darwin→macOS系
- `uname -m`: x86_64|amd64 → x86_64, arm64|aarch64 → aarch64
- 対応表:
  - Linux + x86_64  → `x86_64-unknown-linux-{musl|gnu}`（既定は下記の決定点）
  - Darwin + arm64  → `aarch64-apple-darwin`
  - Darwin + x86_64 → `x86_64-apple-darwin`
  - Linux + aarch64 → ★ビルド無し。エラー終了し `cargo install` 等を案内（下記決定点）
  - それ以外        → エラー終了

### install.sh の構造（案）
1. `set -eu`（可能なら `set -o pipefail`）、`main` 関数化、`trap` で一時ディレクトリ削除。
2. detect_platform: `uname -s`/`-m` を上表で target へ写像。未対応は即エラー。
3. 取得URLは GitHub API を使わず latest リダイレクトを既定にしてレート制限回避:
   `https://github.com/hayamiz/slidewarp/releases/latest/download/<asset>`
   （固定版指定用に `SLIDEWARP_VERSION` 環境変数で `download/vX.Y.Z/<asset>` に切替可能に）。
4. downloader: `curl -fsSL` 優先、無ければ `wget` にフォールバック。tar.gz と .sha256 を取得。
5. verify: `sha256sum -c`（無ければ `shasum -a 256 -c`）。不一致は非0で終了。
6. extract: `tar xzf` → 展開ディレクトリ内の `slidewarp` を install 先へ配置。
7. install 先決定と PATH 案内（下記決定点）。完了後 `slidewarp --version` 等で軽く確認。

### README 追記（案）
「インストール / ビルド」節の冒頭に、ビルド不要のワンライナーを追記:
`curl -fsSL https://raw.githubusercontent.com/hayamiz/slidewarp/main/scripts/install.sh | sh`
既存の `cargo build --release` 手順は残す。

### 決定事項（grill 済み・2026-07-14）
- **Linux 既定ビルド = musl 静的**。依存無しで可搬・「単一バイナリ配布」方針と整合するため。
  glibc 版が要る場合のみ `SLIDEWARP_TARGET` 環境変数で `x86_64-unknown-linux-gnu` に上書き可能にする。
- **既定インストール先 = `~/.local/bin`**。sudo 不要で `| sh` 非対話実行と相性が良いため。
  `SLIDEWARP_INSTALL_DIR` 環境変数で上書き可能。インストール後、`~/.local/bin` が PATH に
  無ければ shell 別（bash/zsh 等）の追記コマンドを案内する。
- **aarch64-linux は未対応**。install.sh は Linux + aarch64 を検出したら非0終了し、
  `cargo install --git https://github.com/hayamiz/slidewarp` 等の代替手段を案内する。
  release.yml への ARM Linux ターゲット追加は本チケットのスコープ外とし、別チケットで扱う。
- **既存インストールは常に上書き**。同名バイナリを無条件で置換（再実行＝アップデート）。
  非対話な curl パイプに最も単純。バージョン比較や確認プロンプトは行わない。
- **Windows は対象外**。install.sh は POSIX sh 前提。WSL/Git Bash 上では Linux バイナリとして
  動くが公式サポートはしない。ネイティブ Windows は cargo ビルドを案内する。

（残決定点はすべて grill で解消済み。実装は上記「決定事項」に従う。）

## Resolution

### 変更点

- **新規 `scripts/install.sh`**（`#!/bin/sh` + `set -eu`、`main "$@"` 末尾呼び出し）:
  - `detect_target`: `SLIDEWARP_OS`/`SLIDEWARP_ARCH`（無ければ `uname -s`/`-m`）から target を決定。
    x86_64|amd64→x86_64、arm64|aarch64→aarch64 に正規化。Linux+x86_64→`x86_64-unknown-linux-musl`（既定）、
    Darwin+arm64→`aarch64-apple-darwin`、Darwin+x86_64→`x86_64-apple-darwin`。
    Linux+aarch64 は非0終了し `cargo install --git https://github.com/hayamiz/slidewarp` を案内。
    その他 OS/arch も明確なメッセージで `exit 1`。
  - latest 取得は GitHub API を使わず `releases/latest` のリダイレクト先 Location から実タグを解決
    （`resolve_latest_tag`）→ `releases/download/<tag>/<asset>` を構築。`SLIDEWARP_VERSION` で固定版指定可。
  - `download`: `curl -fsSL`→`wget` フォールバック。`file://` は `cp` へフォールバック（テスト用）。
  - sha256 検証（`sha256sum -c`→`shasum -a 256 -c`、どちらも無ければエラー）→ `tar xzf` 展開 →
    `install -m 0755`（無ければ cp+chmod）で `SLIDEWARP_INSTALL_DIR`（既定 `~/.local/bin`）へ常に上書き配置。
  - 完了後 `--version` で軽く動作確認（失敗は非致命）、PATH 未登録なら bash/zsh 別に追記例を案内。
  - `mktemp -d` + `trap 'rm -rf "$tmp"' EXIT INT TERM` で一時ディレクトリ確実削除。`local` 不使用・全変数クォート。
  - テストフック環境変数: `SLIDEWARP_OS` / `SLIDEWARP_ARCH` / `SLIDEWARP_BASE_URL` / `SLIDEWARP_TARGET` /
    `SLIDEWARP_INSTALL_DIR` / `SLIDEWARP_VERSION`。本番既定挙動は不変。
- **新規 `scripts/test-install.sh`**: ネットワーク非依存の回帰テスト。`file://` のダミー Release レイアウトを
  作り、正常系(Linux x86_64) / sha256 不一致 / 未対応 arch(Linux aarch64, "cargo install" 案内含む) /
  macOS 判定(Darwin arm64) / `SLIDEWARP_TARGET` 上書き(gnu) を assert。全通過で "ALL TESTS PASSED"。
- **`README.md`**: 「インストール / ビルド」節冒頭に curl ワンライナーと、対応プラットフォーム・既定
  インストール先・上書き環境変数・aarch64-linux/Windows 非対応の案内を追記。既存の cargo ビルド手順は保持。
- `scripts/*.sh` は実行ビット（100755）付与済み。

### 追加テスト

`scripts/test-install.sh`（上記 5 ケース）。テストフレームワーク未導入のプロジェクト方針に合わせ、
POSIX sh 単体で動く自己完結の回帰テストとした（macOS/Linux 双方で sha256 コマンドをフォールバック）。

### 検証結果

- `shellcheck scripts/install.sh scripts/test-install.sh` → 警告なし（OK）。
- `sh -n scripts/install.sh && sh -n scripts/test-install.sh` → 構文 OK。
- `sh scripts/test-install.sh` → 全 5 ケース PASS、"ALL TESTS PASSED"（exit 0）。

### grill 決定事項への準拠

- Linux 既定 = musl 静的、`SLIDEWARP_TARGET` で gnu 上書き可 ✓
- 既定インストール先 `~/.local/bin`、`SLIDEWARP_INSTALL_DIR` で上書き、PATH 案内 ✓
- Linux+aarch64 は非対応・cargo install 案内で非0終了 ✓
- 既存インストールは無条件上書き（プロンプトなし）✓
- Windows 対象外（POSIX sh 前提）✓
- バージョン既定 latest / `SLIDEWARP_VERSION` 固定指定、GitHub API 不使用の latest リダイレクト方式 ✓
- downloader は curl 優先・wget フォールバック ✓

### release.yml との整合性

worktree 内の `.github/workflows/release.yml` を確認し、アセット名
`slidewarp-${GITHUB_REF_NAME}-<target>.tar.gz`（+`.sha256`）・tar 内部構造
`slidewarp-<ver>-<target>/{slidewarp,README.md,LICENSE}`・sha256 生成方法（`sha256sum`/`shasum -a 256`、
basename 形式）が Implementation Notes と一致することを確認した。乖離なし。
