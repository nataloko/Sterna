#!/usr/bin/env bash
# Ask a real Win32 profile implementation what it does, and record the answers.
#
#   ./run.sh              # build, run, and diff against win32.txt
#   ./run.sh --record     # ...and rewrite win32.txt with what came back
#
# Wine is not Windows; see README.md for what that means for the answers.
set -euo pipefail

cd "$(dirname "$0")"

WINE=${WINE:-/usr/lib/wine/wine64}
CC=${CC:-x86_64-w64-mingw32-gcc}
BUILD=build
export WINEPREFIX="${WINEPREFIX:-$PWD/$BUILD/prefix}"
export WINEDEBUG="${WINEDEBUG:--all}"

record=0
[[ ${1:-} == --record ]] && record=1

for tool in "$CC" "$WINE"; do
    if ! command -v "$tool" >/dev/null && [[ ! -x $tool ]]; then
        echo "ini-audit: no $tool — apt install wine64 mingw-w64" >&2
        exit 127
    fi
done

mkdir -p "$BUILD"
"$CC" -std=c11 -Wall -Wextra -O1 -o "$BUILD/exercise.exe" exercise.c

# The profile API resolves a bare filename against the Windows directory, not
# the working directory, so the fixture is named absolutely. Wine maps Z: to /.
win_dir="Z:${BUILD_ABS:-$PWD/$BUILD}"
win_dir=${win_dir//\//\\}

# First run creates the prefix, which is noisy and slow and says nothing.
if [[ ! -d $WINEPREFIX ]]; then
    echo "ini-audit: creating a wine prefix in $WINEPREFIX" >&2
    "$WINE" wineboot --init >/dev/null 2>&1 || true
fi

"$WINE" "$BUILD/exercise.exe" "$win_dir" < cases.txt > "$BUILD/win32.txt" 2>"$BUILD/wine.log"

if (( record )); then
    cp "$BUILD/win32.txt" win32.txt
    echo "recorded $(grep -c . win32.txt) answers from $("$WINE" --version 2>/dev/null)"
    exit 0
fi

if [[ ! -f win32.txt ]]; then
    echo "ini-audit: no win32.txt — run ./run.sh --record" >&2
    exit 1
fi

if diff -u win32.txt "$BUILD/win32.txt"; then
    echo "ini-audit: $(grep -c . win32.txt) answers, unchanged"
else
    echo "ini-audit: the implementation under Wine no longer agrees with the record" >&2
    exit 1
fi
