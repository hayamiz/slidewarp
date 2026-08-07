# slidewarp 自己更新（--system-update）設計書

- 日付: 2026-08-07
- 対象: `slidewarp` 本体（Rust）に GitHub Releases からの自己更新機能を追加する

## 目的

`slidewarp --system-update` で、GitHub Releases の最新リリースを確認し、現在のバージョンより
新しければ、実行中のバイナリ自身を最新版に置き換えて更新する。加えて置換せず確認だけ行う
`--check-update` を用意する。

## スコープ / 非スコープ

- **スコープ**: 最新版チェック、バージョン比較、確認プロンプト、ダウンロード＋自己置換、
  チェック専用モード、未対応プラットフォームの明確なエラー。
- **非スコープ**: 自動定期チェック、ロールバック、`.sha256` 照合（下記トレードオフ参照）、
  aarch64 Linux / Windows 向けリリース資産の新規追加。

## 主要な設計判断（ユーザー合意済み）

1. **実装方式**: `self_update` クレート採用（GitHub Releases 対応・バージョン比較・DL・自己置換を
   一括）。musl 静的バイナリを維持するため TLS は **rustls**。
2. **CLI 形**: `--system-update` フラグ（`slidewarp --system-update`）。現状必須の
   `inputs`/`out_dir` を optional 化して更新モードへ早期分岐する。
3. **UX**: 既定は確認プロンプト付き実行。`--check-update` で確認のみ、`-y/--yes` でプロンプト省略。

## CLI 設計（`src/main.rs`）

Args に追加:

- `--system-update: bool` — 最新を確認し、新しければ確認のうえ自己置換。
- `--check-update: bool` — 新しいバージョンの有無を表示するのみ（置換しない）。
- `-y, --yes: bool` — 確認プロンプトを省略（非対話 / CI 用）。

既存フィールドの変更:

- `inputs: Vec<PathBuf>` — `required = true` を解除（更新モードでは入力不要）。
- `out_dir: Option<PathBuf>` — Option 化（更新モード・`--dump-geom` では不要）。

`main()` の分岐:

1. `system_update || check_update` が真なら `update` モジュールへ委譲して return。
2. それ以外（通常処理・`--dump-geom`）は `out_dir`/`inputs` の有無を手動検証し、
   欠けていれば従来同様のエラーで終了（既存 UX を維持）。通常処理は `out_dir` 必須、
   `--dump-geom` は out_dir 不要（Option 化による副次改善）。

## 更新ロジック（新規 `src/update.rs`）

`self_update::backends::github::Update` を共通設定でビルドするヘルパを持つ:

- `repo_owner("hayamiz")`, `repo_name("slidewarp")`, `bin_name("slidewarp")`
- `current_version(env!("CARGO_PKG_VERSION"))`
- `target(self_update::get_target())`
- `bin_path_in_archive("slidewarp-{{ version }}-{{ target }}/slidewarp")`
  — リリース資産 `slidewarp-<tag>-<target>.tar.gz` の中身が
  `slidewarp-<tag>-<target>/slidewarp` とサブディレクトリ入れ子のため必須。
  （実装時に `{{ version }}` がタグ `v0.1.0` 形か `0.1.0` 形かを実資産で必ず検証する。）

関数:

- `check() -> anyhow::Result<Outcome>` — `get_latest_release()` で最新タグを取得し、
  `self_update::version::bump_is_greater(current, latest)` で新旧判定。current/latest/is_newer を返す。
- `run_update(assume_yes: bool) -> anyhow::Result<()>` — `check()` 相当で判定 → 新しければ
  確認（`assume_yes` なら省略）→ `.update()` で DL＋自己置換。同一/古い場合は「最新です」表示で正常終了。

プラットフォーム/エラー処理:

- 未対応（aarch64 Linux / Windows など、対応する資産が無い）→ 資産マッチ失敗を捕捉し、
  「このプラットフォーム向けの自己更新用バイナリがありません。`cargo install` で更新してください」
  という明確なメッセージにして終了（install.sh の未対応時案内に倣う）。
- 非対話（stdin/stdout が TTY でない）かつ `--yes` 未指定で確認が必要な場合は、
  誤操作防止のため中断し `-y` の指定を案内する。

## Cargo.toml

```toml
self_update = { version = "0.42", default-features = false, features = ["rustls", "archive-tar", "compression-flate2"] }
```

- `default-features = false` で reqwest の native-tls を外し、`rustls` を有効化（musl 静的を維持）。
- 資産は tar.gz のため `archive-tar` + `compression-flate2` が必須。
- `Cargo.lock` の更新をコミットする（CI は `--locked` でビルドするため）。

## トレードオフと既知の制約（合意済み）

1. **sha256 照合は行わない**: self_update は転送整合性を HTTPS で担保するのみで、install.sh の
   ような `.sha256` 照合はしない。GitHub＋TLS を信頼する前提とする。
2. **依存・バイナリサイズ増**: self_update は reqwest(blocking)＋rustls＋内部 tokio を引き込むため、
   単一バイナリのサイズとビルド時間が増える（self_update 採用の既知コスト）。
3. **ターゲット判定**: リリース版（musl）は `get_target()` が `-musl` を返し資産に一致。ローカル
   cargo ビルド（gnu）は対応資産が無く自己更新できない＝この機能は「リリース版バイナリ向け」。

## テスト / 検証方針

- Rust 側はテストフレームワーク未導入。純粋ロジックの単体テストは最小限（可能なら
  バージョン比較ヘルパ）に留め、主に実行確認で検証する。
- `--check-update`: 現行 v0.1.0＝最新なので「最新です」と表示されることを実 GitHub で確認。
- `--system-update`（同一版時）: 置換せず「最新です」で終了することを確認。
- **自己置換 E2E**: crate 版を一時的に `0.0.1` に下げてビルドし、**temp ディレクトリへコピーした
  バイナリ**上で `--system-update -y` を実行 → live の v0.1.0 資産を DL し置換されることを確認する
  （dev は gnu のため `target` を musl 上書き。musl バイナリは glibc 上でも動作する）。開発用
  バイナリ自体は置換しない。

## 成功基準

- `slidewarp --system-update` が最新確認→（新しければ）確認→自己置換で更新する。
- `slidewarp --check-update` が置換せず新旧を表示する。
- 通常の画像処理 CLI の既存 UX（引数必須チェック等）が回帰しない。
- musl 静的ビルド（リリース CI）が成功する。
