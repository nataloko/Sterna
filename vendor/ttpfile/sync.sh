#!/usr/bin/env bash
#
# Re-copy the vendored Tera Term sources, or check them for drift.
#
#   ./sync.sh --check    diff against upstream; exit 1 if anything differs
#   ./sync.sh            copy upstream over the vendored tree
#
# The file list is not maintained by hand: it is what `gcc -MM` says the
# protocol sources actually include, so a new upstream #include shows up as a
# missing file at build time rather than as a header quietly resolved out of
# the sibling checkout.
set -u

cd "$(dirname "$0")"
TT=${TT:-../../../teraterm/teraterm}

FILES=(
	ttpfile/xmodem.c ttpfile/ymodem.c ttpfile/zmodem.c ttpfile/kermit.c
	ttpfile/bplus.c ttpfile/quickvan.c ttpfile/ftlib.c ttpfile/raw.c
	ttpfile/protolog.cpp
	ttpfile/xmodem.h ttpfile/ymodem.h ttpfile/zmodem.h ttpfile/kermit.h
	ttpfile/bplus.h ttpfile/quickvan.h ttpfile/ftlib.h ttpfile/raw.h
	ttpfile/protolog.h ttpfile/filesys_io.h
	teraterm/filesys.h teraterm/filesys_log.h teraterm/filesys_proto.h
	common/i18n.h common/teraterm.h common/ttcommdlg.h common/ttlib.h
	common/ttlib_static_dir.h common/tttypes.h common/tttypes_termid.h
	common/codeconv.h common/ttcstd.h common/asprintf.h common/asprintf.cpp
)

if [ ! -d "$TT" ]; then
	echo "no Tera Term checkout at $TT — set TT=" >&2
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
		mkdir -p "$(dirname "$f")"
		cp -p "$TT/$f" "$f"
	fi
done

if [ "$check" = 1 ]; then
	if [ "$drift" = 0 ]; then
		echo "${#FILES[@]} files, all identical to $TT"
	else
		echo
		echo "Upstream has moved. Re-run without --check, read the diff, and" >&2
		echo "update the revision in README.md." >&2
	fi
	exit $drift
fi

rev=$(git -C "$TT" rev-parse HEAD 2>/dev/null || echo unknown)
echo "copied ${#FILES[@]} files from $TT at $rev"
echo "now update README.md's revision line, and read the diff before committing."
