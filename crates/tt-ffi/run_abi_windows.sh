#!/usr/bin/env bash
# Cross-compile the generated header's real Win32 consumers and run the
# focused seam smoke under Wine. Native Windows remains the authority for
# ConPTY, serial hardware and the named-pipe namespace operations Wine lacks.
set -euo pipefail

cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

triple=${WINDOWS_TARGET:-x86_64-pc-windows-gnu}
target=${CARGO_TARGET_DIR:-$PWD/../target}
profile=${PROFILE:-debug}
cc=${CC_WINDOWS:-x86_64-w64-mingw32-gcc}
cxx=${CXX_WINDOWS:-x86_64-w64-mingw32-g++}

cargo build -p tt-ffi --target "$triple" ${PROFILE:+--profile "$profile"}

lib=$target/$triple/$profile
[ -f "$lib/sterna.dll" ] || { echo "no $lib/sterna.dll" >&2; exit 1; }
[ -f "$lib/libsterna.dll.a" ] || {
    echo "no $lib/libsterna.dll.a" >&2
    exit 1
}

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

"$cc" -std=c11 -Wall -Wextra -Werror -pedantic \
    -I include tests/abi_windows.c -o "$out/abi-windows.exe" \
    -L "$lib" -lsterna

printf '#include <sterna.h>\nint main() { return tt_version() == nullptr; }\n' \
    > "$out/compat.cpp"
"$cxx" -std=c++17 -Wall -Wextra -Werror -I include "$out/compat.cpp" \
    -o "$out/compat-windows.exe" -L "$lib" -lsterna

cp "$lib/sterna.dll" "$out/sterna.dll"

wine=${WINE:-}
if [ -z "$wine" ]; then
    if command -v wine64 >/dev/null 2>&1; then
        wine=wine64
    elif command -v wine >/dev/null 2>&1; then
        wine=wine
    elif [ -x /usr/lib/wine/wine64 ]; then
        wine=/usr/lib/wine/wine64
    else
        echo "wine64 is required to run the Windows ABI smoke" >&2
        exit 127
    fi
fi

export WINEPREFIX=${WINEPREFIX:-/tmp/sterna-wine-runtime}
export WINEDEBUG=${WINEDEBUG:--all}
export WINEDLLOVERRIDES=${WINEDLLOVERRIDES:-winemenubuilder.exe=d}

(cd "$out" && "$wine" ./compat-windows.exe)
(cd "$PWD" && "$wine" "$out/abi-windows.exe")
