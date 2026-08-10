#!/usr/bin/env bash
#
# Re-copy the vendored Tera Term language files, or check them for drift.
#
#   ./sync.sh --check    diff against upstream; exit 1 if anything differs
#   ./sync.sh            copy upstream over the vendored tree
set -u

cd "$(dirname "$0")"
TT=${TT:-../../../teraterm/installer/release/lang_utf8}

FILES=(
	Default.lng de_DE.lng en_US.lng es_ES.lng fr_FR.lng it_IT.lng ja_JP.lng
	ko_KR.lng pt_BR.lng ru_RU.lng ta_IN.lng tr_TR.lng zh_CN.lng zh_TW.lng
)

if [ ! -d "$TT" ]; then
	echo "no Tera Term language directory at $TT — set TT=" >&2
	exit 2
fi

check=0
[ "${1:-}" = "--check" ] && check=1

drift=0
for f in "${FILES[@]}"; do
	if [ ! -f "$TT/$f" ]; then
		echo "gone upstream: $f" >&2
		drift=1
		continue
	fi
	if [ "$check" = 1 ]; then
		if ! diff -q "$TT/$f" "$f" >/dev/null 2>&1; then
			echo "differs: $f"
			drift=1
		fi
	else
		cp -p "$TT/$f" "$f"
	fi
done

if [ "$check" = 1 ]; then
	if [ "$drift" = 0 ]; then
		echo "${#FILES[@]} files, all identical to $TT"
	else
		echo >&2
		echo "Upstream has moved. Re-run without --check, read the diff, and" >&2
		echo "update the revision in README.md and ATTRIBUTION.md." >&2
	fi
	exit $drift
fi

rev=$(git -C "$TT" rev-parse HEAD 2>/dev/null || echo unknown)
echo "copied ${#FILES[@]} files from $TT at $rev"
echo "now update the recorded revision, and read the diff before committing."
