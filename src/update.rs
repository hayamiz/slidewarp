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

/// 共通設定で self_update の Updater を組み立てる。
fn configure() -> Result<Box<dyn self_update::update::ReleaseUpdate>> {
    let target = self_update::get_target();
    self_update::backends::github::Update::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name(BIN)
        .target(target)
        // asset をターゲット三つ組の完全一致で選ぶ。identifier を付けないと
        // self_update のフォールバック（name が OS と ARCH を含めば一致）が働き、
        // 同一 arch の別 libc 資産（例: musl 環境で linux-gnu 資産）を誤選択しうる。
        .identifier(target)
        // 資産の中身は `slidewarp-v<version>-<target>/slidewarp` と入れ子。
        // self_update の {{ version }} は tag の先頭 'v' を除いた値になるため、
        // アーカイブ内ディレクトリ名（タグ = v 付き）に合わせてリテラル 'v' を前置する。
        .bin_path_in_archive("slidewarp-v{{ version }}-{{ target }}/slidewarp")
        .current_version(env!("CARGO_PKG_VERSION"))
        .show_download_progress(true)
        .show_output(false) // self_update の英語ログを抑制（出力は自前の日本語に統一）
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

