# slidewarp 自己更新（--system-update）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `slidewarp --system-update` で GitHub Releases の最新版を確認し、現在より新しければ確認のうえ実行中バイナリを自己置換して更新する。加えて確認のみの `--check-update` を提供する。

**Architecture:** `self_update` クレート（GitHub Releases バックエンド、TLS=rustls）で最新確認・DL・自己置換を行う。更新ロジックは新規 `src/update.rs` に隔離し、`main.rs` は Args にフラグを足して更新モードへ早期分岐する。既存の必須引数 `inputs`/`out_dir` は更新モードで不要なため optional 化し、通常処理では手動検証して従来の UX を保つ。

**Tech Stack:** Rust / clap(derive) / anyhow / self_update(rustls, archive-tar, compression-flate2)。

## Global Constraints

- リリース資産の命名は `slidewarp-<tag>-<target>.tar.gz`、中身は `slidewarp-<tag>-<target>/slidewarp`（サブディレクトリ入れ子）。owner=`hayamiz`, repo=`slidewarp`, bin=`slidewarp`。
- Linux x86_64 のリリースは **musl 静的**。TLS は **rustls** を使い（native-tls/openssl 不可）、musl 静的ビルドを壊さないこと。
- 現在バージョンは `env!("CARGO_PKG_VERSION")`。リリースタグは `vX.Y.Z`（比較時は先頭 `v` を除去）。
- 既存の画像処理 CLI の UX（`-o/--out-dir` 未指定時のエラー等）を回帰させないこと。
- `Cargo.lock` の更新を必ずコミットする（リリース CI は `cargo build --release --locked`）。
- sha256 照合は行わない（HTTPS の転送整合性に依存）— 設計合意済みの既知トレードオフ。

---

## File Structure

- Create: `src/update.rs` — 自己更新ロジック（`newer_available` / `check` / `run_update` と共通設定ヘルパ）。
- Modify: `src/main.rs` — `mod update;`、Args にフラグ追加、`inputs`/`out_dir` の optional 化、`main()` の更新分岐と out_dir 手動検証、`process_image` のシグネチャ変更。
- Modify: `Cargo.toml` — `self_update` 依存を追加。
- Modify: `Cargo.lock` — 依存追加に伴う更新（コミット対象）。
- Modify: `README.md` — 自己更新の使い方を追記。

---

## Task 1: self_update 依存の追加とビルド確認

**Files:**
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: なし
- Produces: `self_update` がリンクされ、`cargo build` が通る。`Cargo.lock` が更新される。

- [ ] **Step 1: `Cargo.toml` に依存を追加**

`[dependencies]` の末尾（`mimalloc = "0.1"` の次の行）に追加:

```toml
self_update = { version = "0.42", default-features = false, features = ["rustls", "archive-tar", "compression-flate2"] }
```

- [ ] **Step 2: ビルドして依存解決を確認**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished` で成功（`self_update` とその依存が解決・コンパイルされる）。`Cargo.lock` が更新される。

- [ ] **Step 3: （可能なら）musl 静的ビルドを確認**

musl ターゲットとツールがある場合のみ:

Run: `rustup target add x86_64-unknown-linux-musl 2>/dev/null; command -v musl-gcc >/dev/null && cargo build --release --target x86_64-unknown-linux-musl 2>&1 | tail -3 || echo "musl ツール無し: CI に委ねる"`
Expected: `Finished`（rustls 採用のため musl 静的でもリンク可能）。ツールが無ければスキップ（リリース CI で担保）。

- [ ] **Step 4: コミット**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: 自己更新用に self_update(rustls) 依存を追加"
```

---

## Task 2: 更新モジュール src/update.rs

**Files:**
- Create: `src/update.rs`

**Interfaces:**
- Consumes: `self_update`（外部クレート）、`env!("CARGO_PKG_VERSION")`。
- Produces:
  - `pub fn newer_available(current: &str, latest: &str) -> anyhow::Result<bool>` — latest 先頭の `v` を除去して semver 比較。
  - `pub fn check() -> anyhow::Result<()>` — 最新確認し新旧を表示（置換しない）。
  - `pub fn run_update(assume_yes: bool) -> anyhow::Result<()>` — 新しければ確認（assume_yes で省略）→ 自己置換。

- [ ] **Step 1: バージョン比較ヘルパの失敗テストを書く**

`src/update.rs`（新規）に、まずヘルパとテストのみ:

```rust
//! GitHub Releases からの自己更新（--system-update / --check-update）。

use anyhow::{anyhow, Result};

const OWNER: &str = "hayamiz";
const REPO: &str = "slidewarp";
const BIN: &str = "slidewarp";

/// current より latest が新しければ true。latest 先頭の 'v' は無視する。
pub fn newer_available(current: &str, latest: &str) -> Result<bool> {
    let latest = latest.trim_start_matches('v');
    self_update::version::bump_is_greater(current, latest)
        .map_err(|e| anyhow!("バージョン比較に失敗しました: {e}"))
}

#[cfg(test)]
mod tests {
    use super::newer_available;

    #[test]
    fn detects_newer() {
        assert!(newer_available("0.1.0", "v0.2.0").unwrap());
    }

    #[test]
    fn same_is_not_newer() {
        assert!(!newer_available("0.1.0", "0.1.0").unwrap());
    }

    #[test]
    fn older_is_not_newer() {
        assert!(!newer_available("0.2.0", "v0.1.0").unwrap());
    }

    #[test]
    fn strips_v_prefix() {
        assert!(!newer_available("1.0.0", "v1.0.0").unwrap());
    }
}
```

そして `src/main.rs` の `mod` 宣言群（`mod report;` の近く）に `mod update;` を追加（テストをビルドするため）。

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test --lib update:: 2>&1 | tail -15`（または `cargo test newer_available`）
Expected: `newer_available` 未参照の関数は使われるが、この時点では `check`/`run_update` が未実装。テスト自体は 4 件が PASS するはず。もし `mod update;` 追加で `check`/`run_update` 未定義の警告が出ても、この Step ではテストが通ることを確認する（未使用警告は次 Step で解消）。

Run: `cargo test update 2>&1 | tail -8`
Expected: `test result: ok. 4 passed`

- [ ] **Step 3: check / run_update と共通設定ヘルパを実装**

`src/update.rs` の `newer_available` の下（`#[cfg(test)]` の前）に追加:

```rust
/// 共通設定で self_update の Updater を組み立てる。
fn configure() -> Result<Box<dyn self_update::update::ReleaseUpdate>> {
    let target = self_update::get_target();
    self_update::backends::github::Update::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name(BIN)
        .target(&target)
        // 資産の中身は `slidewarp-<tag>-<target>/slidewarp` と入れ子。
        .bin_path_in_archive("slidewarp-{{ version }}-{{ target }}/slidewarp")
        .current_version(env!("CARGO_PKG_VERSION"))
        .show_download_progress(true)
        .no_confirm(true) // 確認は run_update 側で行う
        .build()
        .map_err(|e| anyhow!("自己更新の初期化に失敗しました: {e}"))
}

fn latest_version(updater: &dyn self_update::update::ReleaseUpdate) -> Result<String> {
    let rel = updater
        .get_latest_release()
        .map_err(|e| anyhow!("最新リリースの取得に失敗しました: {e}"))?;
    Ok(rel.version)
}

/// 新しいバージョンの有無を表示する（置換しない）。
pub fn check() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let updater = configure()?;
    let latest = latest_version(updater.as_ref())?;
    if newer_available(current, &latest)? {
        println!(
            "新しいバージョンがあります: 現在 {current} -> 最新 {}",
            latest.trim_start_matches('v')
        );
        println!("`slidewarp --system-update` で更新できます。");
    } else {
        println!(
            "最新です（現在 {current} / 最新リリース {}）。",
            latest.trim_start_matches('v')
        );
    }
    Ok(())
}

/// 新しければ確認のうえ自己置換して更新する。
pub fn run_update(assume_yes: bool) -> Result<()> {
    use std::io::{IsTerminal, Write};

    let current = env!("CARGO_PKG_VERSION");
    let updater = configure()?;
    let latest = latest_version(updater.as_ref())?;
    let latest_disp = latest.trim_start_matches('v').to_string();

    if !newer_available(current, &latest)? {
        println!("最新です（現在 {current}）。更新は不要です。");
        return Ok(());
    }

    println!("新しいバージョン {latest_disp} が見つかりました（現在 {current}）。");

    if !assume_yes {
        if !std::io::stdin().is_terminal() {
            return Err(anyhow!(
                "非対話環境では確認できません。`-y/--yes` を付けて実行してください。"
            ));
        }
        print!("最新版に置き換えますか？ [y/N]: ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            println!("中止しました。");
            return Ok(());
        }
    }

    let status = updater.update().map_err(|e| {
        anyhow!(
            "更新に失敗しました: {e}\n\
             このプラットフォーム向けの自己更新用バイナリが無い場合は \
             `cargo install --git https://github.com/{OWNER}/{REPO}` をご利用ください。"
        )
    })?;
    println!("更新しました: {}", status.version());
    Ok(())
}
```

- [ ] **Step 4: ビルドとテストを再確認**

Run: `cargo build 2>&1 | tail -3 && cargo test update 2>&1 | tail -5`
Expected: `Finished` かつ `test result: ok. 4 passed`。未使用警告なし。

> **実装時の検証必須事項（外部クレート挙動）**: `self_update::get_target()` の戻り値型（`String`/`&str`）、`bin_path_in_archive` テンプレートの `{{ version }}` がタグ `v0.1.0` 形か `0.1.0` 形か、`Release.version` の `v` 有無は、Task 4 の `--check-update` 実行で実データと突き合わせて確定し、必要なら `.target(&target)` / テンプレート / `trim_start_matches` を微調整する。

- [ ] **Step 5: コミット**

```bash
git add src/update.rs src/main.rs
git commit -m "feat(update): GitHub Releases からの自己更新モジュールを追加"
```

---

## Task 3: CLI 配線（フラグ追加・引数 optional 化・main 分岐）

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `update::check` / `update::run_update`（Task 2）。
- Produces: `--system-update` / `--check-update` / `-y,--yes` フラグ。更新モードは入力不要で動作。通常処理は従来どおり。

- [ ] **Step 1: Args にフラグを追加し inputs/out_dir を optional 化**

`src/main.rs` の `struct Args` を編集。`inputs` の `#[arg(required = true)]` を削除し、`out_dir` を Option 化、末尾にフラグ3つを追加:

```rust
    /// 写真ファイル または フォルダ（再帰探索）
    inputs: Vec<PathBuf>,
    /// 出力ディレクトリ
    #[arg(short, long)]
    out_dir: Option<PathBuf>,
```

`dump_geom` フィールドの直後（`struct Args` の末尾）に追加:

```rust
    /// GitHub Releases の最新版を確認し、新しければ自分自身を置き換えて更新する
    #[arg(long)]
    system_update: bool,
    /// 新しいバージョンの有無を確認するだけ（置換しない）
    #[arg(long)]
    check_update: bool,
    /// 更新時の確認プロンプトを省略する（非対話/CI 用）
    #[arg(short = 'y', long)]
    yes: bool,
```

- [ ] **Step 2: `main()` 冒頭に更新分岐を追加**

`fn main() {` の `let args = Args::parse();` の直後（`if args.jobs > 0` の前）に挿入:

```rust
    if args.check_update {
        if let Err(e) = update::check() {
            eprintln!("エラー: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.system_update {
        if let Err(e) = update::run_update(args.yes) {
            eprintln!("エラー: {e}");
            std::process::exit(1);
        }
        return;
    }
```

- [ ] **Step 3: out_dir を手動検証し process_image / report へ渡す**

(a) `dump_geom` 分岐の `return;`（`}` の直後、`let jobs = ...` の前）に out_dir 解決を挿入:

```rust
    let out_dir = match args.out_dir.clone() {
        Some(d) => d,
        None => {
            eprintln!("出力先が未指定です。-o/--out-dir を指定してください。");
            std::process::exit(2);
        }
    };
```

(b) `process_image` 呼び出しを変更:

```rust
    let results: Vec<ProcResult> = files.par_iter().map(|f| process_image(f, &args, &out_dir)).collect();
```

(c) report 節の `&args.out_dir` 3 箇所を `&out_dir` に置換:
- `src: rel_path(&out_dir, &r.src),`
- `out: r.out_path.as_ref().map(|p| rel_path(&out_dir, p)),`
- `match report::write_report(items, &out_dir) {`

- [ ] **Step 4: `process_image` のシグネチャと本体を更新**

`fn process_image(src: &Path, args: &Args) -> ProcResult {` を次に変更:

```rust
fn process_image(src: &Path, args: &Args, out_dir: &Path) -> ProcResult {
```

本体内の `args.out_dir` 参照 3 箇所を `out_dir` に置換:
- `let review = out_dir.join("_review");`
- `if let Err(e) = std::fs::create_dir_all(out_dir) {`
- `let op = output_path(src, out_dir, "", &ext);`

- [ ] **Step 5: ビルドと既存 UX の回帰確認**

```bash
cargo build 2>&1 | tail -3
# 通常処理が従来どおり動く
./target/debug/slidewarp input-samples -o /tmp/_sw_check 2>&1 | tail -3
# 出力先未指定はエラー(終了コード2)
./target/debug/slidewarp input-samples; echo "exit=$?"
# --dump-geom は out_dir 不要で動く
./target/debug/slidewarp --dump-geom input-samples 2>&1 | head -2
```
Expected: 通常処理は集計出力、未指定時は「出力先が未指定です。」＋exit=2、`--dump-geom` は幾何行を出力。

- [ ] **Step 6: コミット**

```bash
git add src/main.rs
git commit -m "feat(cli): --system-update/--check-update/-y を追加し引数を optional 化"
```

---

## Task 4: 実 GitHub での動作確認（check と自己置換 E2E）

**Files:**
- （コード変更なし。検証のみ。必要なら一時的な版下げビルドを使う）

**Interfaces:**
- Consumes: Task 2/3 の実装、実 GitHub Releases（`v0.1.0` が存在）。

- [ ] **Step 1: `--check-update` を実 GitHub で確認**

Run: `cargo build 2>&1 | tail -1 && ./target/debug/slidewarp --check-update 2>&1 | tail -5`
Expected: 現在 0.1.0 = 最新のため「最新です（現在 0.1.0 / 最新リリース 0.1.0）。」。ここで `Release.version` の `v` 有無・比較結果が想定どおりかを確認し、ズレていれば `src/update.rs` の `trim_start_matches`/テンプレートを Task 2 の注記に従い調整して再確認。

- [ ] **Step 2: `--system-update`（同一版）が置換しないことを確認**

Run: `./target/debug/slidewarp --system-update -y 2>&1 | tail -3`
Expected: 「最新です（現在 0.1.0）。更新は不要です。」で終了。バイナリは置換されない。

- [ ] **Step 3: 自己置換 E2E（版を一時的に下げて実置換を確認）**

開発バイナリ自体は壊さないよう、**version を下げた release バイナリを temp にコピーして**実行する:

```bash
# 1) version を一時的に 0.0.1 にした release バイナリを作る（musl。glibc上でも動く）
sed -i.bak 's/^version = "0.1.0"/version = "0.0.1"/' Cargo.toml
rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
cargo build --release --target x86_64-unknown-linux-musl 2>&1 | tail -2
# 2) temp にコピーして、そのコピー自身を自己更新させる
TMP=$(mktemp -d); cp target/x86_64-unknown-linux-musl/release/slidewarp "$TMP/slidewarp"
"$TMP/slidewarp" --system-update -y 2>&1 | tail -6
# 3) 置換後のバージョンが 0.1.0 になっていることを確認
"$TMP/slidewarp" --check-update 2>&1 | tail -2
# 4) 後片付け（Cargo.toml を元に戻す）
mv Cargo.toml.bak Cargo.toml; rm -rf "$TMP"
```
Expected: (2) で v0.1.0 資産を DL し「更新しました: 0.1.0」。(3) で「最新です（現在 0.1.0 ...）」。開発ツリーの `Cargo.toml` は 0.1.0 に戻る。

> 注: dev ホストは gnu だが、リリース資産は musl のみ。musl バイナリは glibc 上でも動作するため、上記のように musl でビルドしたバイナリで検証する。`self_update::get_target()` が musl を返さない環境では、検証時に `SLIDEWARP` 相当の target 上書きは無いため、必要なら `configure()` に `.target("x86_64-unknown-linux-musl")` を一時挿入して確認する（本採用はしない）。

- [ ] **Step 4: 変更が無ければコミット不要（検証のみ）**

コード調整が発生した場合のみ:
```bash
git add src/update.rs
git commit -m "fix(update): 実リリースに合わせてバージョン解析/資産パスを調整"
```

---

## Task 5: README に自己更新の使い方を追記

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: Task 3 の CLI。

- [ ] **Step 1: インストール節の近くに使い方を追記**

`README.md` の「インストール / ビルド」節の末尾付近に追加:

```markdown
### 更新（自己アップデート）

インストール済みの `slidewarp` は、自分自身を GitHub Releases の最新版へ更新できます。

```bash
slidewarp --check-update    # 新しいバージョンの有無を確認（置換しない）
slidewarp --system-update   # 新しければ確認のうえ自分自身を最新版に置き換える
slidewarp --system-update -y  # 確認プロンプトを省略（CI 等）
```

- 対応は install.sh と同じ Linux x86_64(musl) / macOS arm64 / macOS x86_64。
- それ以外（aarch64 Linux / Windows 等）は自己更新用の配布バイナリが無いため、
  `cargo install --git https://github.com/hayamiz/slidewarp` で更新してください。
```

- [ ] **Step 2: コミット**

```bash
git add README.md
git commit -m "docs(readme): 自己更新(--system-update/--check-update)の使い方を追記"
```

---

## Self-Review 結果（計画作成者による確認）

- **Spec coverage**: 依存追加/musl→Task1、更新ロジック(check/run_update/バージョン比較)→Task2、CLI(3フラグ・optional化・分岐・UX維持)→Task3、実GitHub確認と自己置換E2E→Task4、ドキュメント→Task5。設計書の全項目に対応。
- **Placeholder scan**: 具体コード・具体コマンド・期待出力を各 Step に記載。外部クレート挙動の残不確実点（`get_target` 型 / テンプレート版形 / `v` 有無）は Task 2 の注記＋Task 4 の実データ検証で確定する明示手順にしてある（放置 TODO ではない）。
- **Type consistency**: `newer_available`/`check`/`run_update`/`configure`/`latest_version` は Task2 で定義し Task3/4 で同名使用。`process_image(src,&args,&out_dir)` の新シグネチャは Task3 内で定義・呼び出し・本体を整合。`out_dir: Option<PathBuf>` の解決を通常処理パスに限定し、更新/`--dump-geom` は不要という分岐も一貫。
- **既知トレードオフ**: sha256 非照合・依存増・target 判定は設計書に記載済みで Global Constraints にも反映。
