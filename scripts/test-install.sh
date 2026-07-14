#!/bin/sh
# scripts/install.sh のネットワーク非依存な回帰テスト。
#
# ダミーの Release レイアウトを file:// で用意し、install.sh の主要パス
# （正常系・sha256 不一致・未対応 arch・macOS 判定）を検証する。
#
# 実行: sh scripts/test-install.sh
# 全通過で "ALL TESTS PASSED" を出して exit 0、失敗時は exit 1。

set -eu

# このスクリプトと install.sh の場所
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
INSTALL_SH="${SCRIPT_DIR}/install.sh"

[ -f "$INSTALL_SH" ] || {
	echo "FAIL: install.sh が見つかりません: $INSTALL_SH" >&2
	exit 1
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT INT TERM

VERSION="vTEST"
FAILED=0

# sha256 コマンドの選択（sha256sum -> shasum -a 256）
sha256_of() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1"
	else
		shasum -a 256 "$1"
	fi
}

# 指定 target のダミー Release アセットを releases/download/<ver>/ 配下に作る
make_fixture() {
	target="$1"
	rel_dir="${WORK}/releases/download/${VERSION}"
	mkdir -p "$rel_dir"

	stage="${WORK}/stage-${target}"
	dist="slidewarp-${VERSION}-${target}"
	mkdir -p "${stage}/${dist}"

	# 実行可能なダミー slidewarp。実バイナリ同様 --help を持ち（0 終了）、
	# 引数なしでは "slidewarp vTEST" を出す（install.sh のポストインストール確認と整合）。
	cat >"${stage}/${dist}/slidewarp" <<'EOF'
#!/bin/sh
case "${1:-}" in
--help) echo "slidewarp: usage ..."; exit 0 ;;
esac
echo "slidewarp vTEST"
EOF
	chmod +x "${stage}/${dist}/slidewarp"
	echo "dummy readme" >"${stage}/${dist}/README.md"
	echo "dummy license" >"${stage}/${dist}/LICENSE"

	# tar.gz 化（dist ディレクトリごと）
	(cd "$stage" && tar czf "${dist}.tar.gz" "$dist")
	cp "${stage}/${dist}.tar.gz" "${rel_dir}/${dist}.tar.gz"

	# .sha256 は tar.gz の basename 形式（<hash>  <basename>）で生成
	(cd "$rel_dir" && sha256_of "${dist}.tar.gz" >"${dist}.tar.gz.sha256")
}

pass() { echo "PASS: $1"; }
fail() {
	echo "FAIL: $1" >&2
	FAILED=1
}

BASE_URL="file://${WORK}/releases"

# ---------------------------------------------------------------------------
# a. fixture: Linux x86_64 (musl) と macOS arm64
# ---------------------------------------------------------------------------
make_fixture "x86_64-unknown-linux-musl"
make_fixture "aarch64-apple-darwin"

# ---------------------------------------------------------------------------
# b. 正常系: Linux x86_64
# ---------------------------------------------------------------------------
BIN_DIR="${WORK}/bin-linux"
if SLIDEWARP_OS=Linux SLIDEWARP_ARCH=x86_64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_BASE_URL="$BASE_URL" SLIDEWARP_INSTALL_DIR="$BIN_DIR" \
	sh "$INSTALL_SH" >/dev/null 2>&1; then
	if [ -x "${BIN_DIR}/slidewarp" ]; then
		out=$("${BIN_DIR}/slidewarp" 2>/dev/null || true)
		if [ "$out" = "slidewarp vTEST" ]; then
			pass "正常系(Linux x86_64): インストール・実行OK"
		else
			fail "正常系(Linux x86_64): 実行出力が想定外: '$out'"
		fi
	else
		fail "正常系(Linux x86_64): バイナリが実行可能で存在しない"
	fi
else
	fail "正常系(Linux x86_64): install.sh が非0終了した"
fi

# ---------------------------------------------------------------------------
# c. sha256 不一致系: .sha256 を改竄 → 非0終了を期待
# ---------------------------------------------------------------------------
BAD_DIR="${WORK}/bad-releases"
mkdir -p "${BAD_DIR}/download/${VERSION}"
cp "${WORK}/releases/download/${VERSION}/slidewarp-${VERSION}-x86_64-unknown-linux-musl.tar.gz" \
	"${BAD_DIR}/download/${VERSION}/"
# 改竄した sha256（不正なハッシュ）
echo "0000000000000000000000000000000000000000000000000000000000000000  slidewarp-${VERSION}-x86_64-unknown-linux-musl.tar.gz" \
	>"${BAD_DIR}/download/${VERSION}/slidewarp-${VERSION}-x86_64-unknown-linux-musl.tar.gz.sha256"

BIN_DIR2="${WORK}/bin-bad"
if SLIDEWARP_OS=Linux SLIDEWARP_ARCH=x86_64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_BASE_URL="file://${BAD_DIR}" SLIDEWARP_INSTALL_DIR="$BIN_DIR2" \
	sh "$INSTALL_SH" >/dev/null 2>&1; then
	fail "sha256不一致系: 改竄されているのに 0 終了してしまった"
else
	if [ -x "${BIN_DIR2}/slidewarp" ]; then
		fail "sha256不一致系: 非0終了したがバイナリが配置されてしまった"
	else
		pass "sha256不一致系: 非0終了しバイナリ未配置"
	fi
fi

# ---------------------------------------------------------------------------
# d. 未対応 arch 系: Linux + aarch64 → 非0終了 & "cargo install" を含む
# ---------------------------------------------------------------------------
err_out=$(SLIDEWARP_OS=Linux SLIDEWARP_ARCH=aarch64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_BASE_URL="$BASE_URL" SLIDEWARP_INSTALL_DIR="${WORK}/bin-arm" \
	sh "$INSTALL_SH" 2>&1 || true)
rc_marker="${WORK}/rc-d"
if SLIDEWARP_OS=Linux SLIDEWARP_ARCH=aarch64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_BASE_URL="$BASE_URL" SLIDEWARP_INSTALL_DIR="${WORK}/bin-arm" \
	sh "$INSTALL_SH" >/dev/null 2>&1; then
	fail "未対応arch系(Linux aarch64): 0 終了してしまった"
	rm -f "$rc_marker"
else
	case "$err_out" in
	*"cargo install"*)
		pass "未対応arch系(Linux aarch64): 非0終了し cargo install を案内"
		;;
	*)
		fail "未対応arch系(Linux aarch64): 出力に cargo install が無い: '$err_out'"
		;;
	esac
fi

# ---------------------------------------------------------------------------
# e. macOS 判定: Darwin + arm64 → 正常系（aarch64-apple-darwin に解決）
# ---------------------------------------------------------------------------
BIN_DIR3="${WORK}/bin-mac"
if SLIDEWARP_OS=Darwin SLIDEWARP_ARCH=arm64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_BASE_URL="$BASE_URL" SLIDEWARP_INSTALL_DIR="$BIN_DIR3" \
	sh "$INSTALL_SH" >/dev/null 2>&1; then
	if [ -x "${BIN_DIR3}/slidewarp" ] && \
		[ "$("${BIN_DIR3}/slidewarp" 2>/dev/null || true)" = "slidewarp vTEST" ]; then
		pass "macOS判定(Darwin arm64): aarch64-apple-darwin を取得・インストールOK"
	else
		fail "macOS判定(Darwin arm64): バイナリの配置/実行に失敗"
	fi
else
	fail "macOS判定(Darwin arm64): install.sh が非0終了した"
fi

# ---------------------------------------------------------------------------
# f. SLIDEWARP_TARGET 明示上書き（gnu へ切替できること）
#    fixture を用意し、target を明示指定して正常系になるか確認
# ---------------------------------------------------------------------------
make_fixture "x86_64-unknown-linux-gnu"
BIN_DIR4="${WORK}/bin-gnu"
if SLIDEWARP_OS=Linux SLIDEWARP_ARCH=x86_64 SLIDEWARP_VERSION="$VERSION" \
	SLIDEWARP_TARGET="x86_64-unknown-linux-gnu" \
	SLIDEWARP_BASE_URL="$BASE_URL" SLIDEWARP_INSTALL_DIR="$BIN_DIR4" \
	sh "$INSTALL_SH" >/dev/null 2>&1; then
	if [ -x "${BIN_DIR4}/slidewarp" ]; then
		pass "SLIDEWARP_TARGET上書き: gnu target で取得・インストールOK"
	else
		fail "SLIDEWARP_TARGET上書き: バイナリ未配置"
	fi
else
	fail "SLIDEWARP_TARGET上書き: install.sh が非0終了した"
fi

# ---------------------------------------------------------------------------
if [ "$FAILED" -eq 0 ]; then
	echo "ALL TESTS PASSED"
	exit 0
else
	echo "SOME TESTS FAILED" >&2
	exit 1
fi
