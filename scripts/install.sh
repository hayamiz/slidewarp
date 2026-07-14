#!/bin/sh
# slidewarp インストールスクリプト
#
# GitHub Releases の tar.gz アセットを OS/arch 自動判定でダウンロードし、
# sha256 検証のうえ slidewarp バイナリを PATH 上（既定 ~/.local/bin）へ配置する。
#
# 使い方（ビルド不要のワンライナー）:
#   curl -fsSL https://raw.githubusercontent.com/hayamiz/slidewarp/master/scripts/install.sh | sh
#
# 対応プラットフォーム:
#   - Linux  x86_64 （既定 musl 静的。glibc 版が要る場合は SLIDEWARP_TARGET で上書き）
#   - macOS  arm64 (Apple Silicon) / x86_64 (Intel)
#   - Linux  aarch64 は未対応（cargo install を案内して終了）
#
# 上書き用の環境変数:
#   SLIDEWARP_VERSION      取得するバージョン（既定 latest。例: v1.2.3）
#   SLIDEWARP_INSTALL_DIR  インストール先ディレクトリ（既定 ~/.local/bin）
#   SLIDEWARP_TARGET       target 三つ組を明示上書き（例 x86_64-unknown-linux-gnu）
#   SLIDEWARP_OS           uname -s の代わりに使う OS 名（テスト/検証用）
#   SLIDEWARP_ARCH         uname -m の代わりに使う arch 名（テスト/検証用）
#   SLIDEWARP_BASE_URL     ダウンロード元 URL のベース（テスト用。file:// も可）

set -eu

OWNER="hayamiz"
REPO="slidewarp"

# エラーメッセージを stderr へ出して終了
die() {
	echo "error: $*" >&2
	exit 1
}

info() {
	echo "$*" >&2
}

# OS/arch から target 三つ組を決定する（結果は変数 TARGET へ）
detect_target() {
	os="${SLIDEWARP_OS:-$(uname -s)}"
	arch="${SLIDEWARP_ARCH:-$(uname -m)}"

	# arch の正規化
	case "$arch" in
	x86_64 | amd64) arch="x86_64" ;;
	arm64 | aarch64) arch="aarch64" ;;
	*) die "未対応の CPU アーキテクチャです: $arch" ;;
	esac

	case "$os" in
	Linux)
		case "$arch" in
		x86_64)
			# Linux x86_64 の既定は musl 静的。glibc が要る場合は
			# SLIDEWARP_TARGET=x86_64-unknown-linux-gnu で上書き可能。
			TARGET="x86_64-unknown-linux-musl"
			;;
		aarch64)
			die "Linux aarch64 向けのビルド済みバイナリはありません。
ソースから導入してください:
  cargo install --git https://github.com/${OWNER}/${REPO}"
			;;
		*)
			die "未対応の Linux アーキテクチャです: $arch"
			;;
		esac
		;;
	Darwin)
		case "$arch" in
		aarch64) TARGET="aarch64-apple-darwin" ;;
		x86_64) TARGET="x86_64-apple-darwin" ;;
		*) die "未対応の macOS アーキテクチャです: $arch" ;;
		esac
		;;
	*)
		die "未対応の OS です: $os（このスクリプトは Linux / macOS のみ対応）"
		;;
	esac
}

# URL からファイルを取得して第2引数のパスへ保存する。
# curl -> wget の順にフォールバック。file:// は cp も許容（テスト用）。
download() {
	url="$1"
	dest="$2"

	# file:// はローカルコピーで扱う（テスト用フックのため）
	case "$url" in
	file://*)
		src=$(printf '%s' "$url" | sed 's#^file://##')
		cp "$src" "$dest" 2>/dev/null && return 0
		return 1
		;;
	esac

	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "$url" -o "$dest"
	elif command -v wget >/dev/null 2>&1; then
		wget -q "$url" -O "$dest"
	else
		die "curl も wget も見つかりません。どちらかをインストールしてください。"
	fi
}

# tar.gz の sha256 を .sha256 ファイル（<hash>  <basename> 形式）で検証する。
# tar.gz と .sha256 は同じディレクトリの basename で置かれている前提。
# ツール不在チェックは呼び出し前（メインフロー）で行うこと（サブシェル内 die は
# 呼び出し元へメッセージが伝播しないため）。
verify_sha256() {
	dir="$1"
	tarball="$2" # basename

	(
		cd "$dir"
		if command -v sha256sum >/dev/null 2>&1; then
			sha256sum -c "${tarball}.sha256"
		else
			shasum -a 256 -c "${tarball}.sha256"
		fi
	)
}

# INSTALL_DIR が PATH に含まれるか確認し、無ければ追記コマンドを案内する
check_path() {
	dir="$1"
	case ":${PATH}:" in
	*":${dir}:"*)
		return 0
		;;
	esac

	info ""
	info "注意: ${dir} が PATH に含まれていません。"
	shell_name=$(basename "${SHELL:-}")
	case "$shell_name" in
	zsh)
		info "以下を ~/.zshrc に追記してください:"
		info "  export PATH=\"${dir}:\$PATH\""
		;;
	bash)
		info "以下を ~/.bashrc に追記してください:"
		info "  export PATH=\"${dir}:\$PATH\""
		;;
	*)
		info "お使いの shell の設定ファイルに以下を追記してください:"
		info "  export PATH=\"${dir}:\$PATH\""
		;;
	esac
}

main() {
	base_url="${SLIDEWARP_BASE_URL:-https://github.com/${OWNER}/${REPO}/releases}"
	version="${SLIDEWARP_VERSION:-latest}"
	install_dir="${SLIDEWARP_INSTALL_DIR:-$HOME/.local/bin}"

	# target の決定（明示上書き優先）
	if [ -n "${SLIDEWARP_TARGET:-}" ]; then
		TARGET="$SLIDEWARP_TARGET"
	else
		detect_target
	fi

	# アセット名にはバージョン（タグ名）が埋め込まれるため、latest の場合も実タグを
	# 解決してから download/<tag>/<asset> を組み立てる（latest リダイレクトの
	# Location からタグを取り出す。GitHub API は使わずレート制限を回避）。
	if [ "$version" = "latest" ]; then
		version=$(resolve_latest_tag "$base_url") ||
			die "最新バージョンの解決に失敗しました。SLIDEWARP_VERSION でバージョンを明示してください。"
	fi
	dl_base="${base_url}/download/${version}"

	asset="slidewarp-${version}-${TARGET}.tar.gz"
	tar_url="${dl_base}/${asset}"
	sha_url="${dl_base}/${asset}.sha256"

	info "slidewarp ${version} (${TARGET}) をインストールします..."
	info "  取得元: ${tar_url}"

	tmp=$(mktemp -d)
	# shellcheck disable=SC2064
	trap "rm -rf \"$tmp\"" EXIT INT TERM

	download "$tar_url" "${tmp}/${asset}" || die "アーカイブのダウンロードに失敗しました: ${tar_url}"
	download "$sha_url" "${tmp}/${asset}.sha256" || die "sha256 のダウンロードに失敗しました: ${sha_url}"

	# sha256 ツールの存在チェックはサブシェルの外（ここ）で行い、正しいメッセージで終了させる
	if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
		die "sha256sum も shasum も見つかりません。sha256 検証ができません。"
	fi

	info "sha256 を検証中..."
	verify_sha256 "$tmp" "$asset" >/dev/null 2>&1 || die "sha256 検証に失敗しました。ダウンロードが破損しているか改竄されています。"

	info "展開中..."
	tar xzf "${tmp}/${asset}" -C "$tmp" || die "アーカイブの展開に失敗しました。"

	bin_path="${tmp}/slidewarp-${version}-${TARGET}/slidewarp"
	[ -f "$bin_path" ] || die "アーカイブ内に slidewarp バイナリが見つかりません: ${bin_path}"

	mkdir -p "$install_dir"
	# install があれば使い、無ければ cp + chmod（常に上書き）
	if command -v install >/dev/null 2>&1; then
		install -m 0755 "$bin_path" "${install_dir}/slidewarp"
	else
		cp "$bin_path" "${install_dir}/slidewarp"
		chmod 0755 "${install_dir}/slidewarp"
	fi

	info ""
	info "インストール完了: ${install_dir}/slidewarp"

	# 軽い疎通確認（失敗しても致命的にはしない）。slidewarp は --version を持たず
	# --help を持つので --help で確認する。実行不能なら警告のみ。
	if [ -x "${install_dir}/slidewarp" ]; then
		if "${install_dir}/slidewarp" --help >/dev/null 2>&1; then
			info "  動作確認: slidewarp --help OK"
		else
			info "  注意: ${install_dir}/slidewarp の起動確認に失敗しました（配置は完了）。"
		fi
	else
		info "  注意: ${install_dir}/slidewarp が実行可能ではありません。"
	fi

	check_path "$install_dir"
}

# GitHub の latest リダイレクトから実タグ名を解決する。
# releases/latest は releases/tag/<tag> へ 302 する。Location からタグを取り出す。
# file:// ベース（テスト）の場合は latest というリテラルタグを使う想定。
resolve_latest_tag() {
	base="$1"

	case "$base" in
	file://*)
		# テスト用: file:// では latest 解決を行わず、固定版指定を前提とする。
		# ここに来た場合は latest ディレクトリを実タグとして扱う。
		echo "latest"
		return 0
		;;
	esac

	loc=""
	if command -v curl >/dev/null 2>&1; then
		loc=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${base}/latest" 2>/dev/null || true)
	elif command -v wget >/dev/null 2>&1; then
		loc=$(wget -q -S --max-redirect=0 "${base}/latest" 2>&1 | awk '/[Ll]ocation:/ {print $2}' | tr -d '\r' | tail -n 1 || true)
	fi

	[ -n "$loc" ] || return 1
	# .../releases/tag/vX.Y.Z の末尾を取り出す
	tag=$(printf '%s' "$loc" | sed 's#.*/tag/##; s#/.*##')
	[ -n "$tag" ] || return 1
	echo "$tag"
}

main "$@"
