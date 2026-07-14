---
title: GitHub Releases の artifact を curl ワンライナーで導入する手順を整備
type: enhancement
priority: medium
status: open
created: 2026-07-14
updated: 2026-07-14
---

## Description

`.github/workflows/release.yml` が `vX.Y.Z` タグ push で 3 ターゲット
（linux musl 静的 / macOS arm64 / macOS x86_64）のバイナリを
tar.gz ＋ sha256 で Release に添付する。これを利用者が **curl ワンライナー**で
簡単に導入できる手順を整備する。

想定する成果物:

- OS/アーキテクチャを自動判定して最新 Release から適切な tar.gz を取得・展開し、
  `slidewarp` バイナリを PATH 上（例: `~/.local/bin` or `/usr/local/bin`）へ配置する
  インストール用 shell script（例: `scripts/install.sh`）。sha256 で整合性検証する。
- `curl -fsSL https://.../install.sh | sh` 形式のワンライナーを README に追記。
  バージョン指定（環境変数 `VERSION` 等）とインストール先の上書きにも対応。
- Linux は musl 静的版のみ（#0004 で一本化。依存が無く導入が確実）。

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
よって実アセットはタグ `vX.Y.Z` に対し以下の6ファイル（tar.gz 3種＋各 .sha256）:

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
  - Linux + x86_64  → `x86_64-unknown-linux-musl`（#0004 で musl 静的一本化）
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
- **Linux ビルド = musl 静的のみ**（#0004 で一本化）。依存無しで可搬・「単一バイナリ配布」
  方針と整合するため。gnu 版はリリースしないので、ターゲット上書きの仕組みは設けない。
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
