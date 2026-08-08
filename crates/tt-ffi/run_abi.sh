#!/usr/bin/env bash
# Compile the generated header and drive the ABI from C, the way the Qt shell
# will. Nothing else in the suite links the shared library or compiles the
# header, so this is the only thing that can catch a seam that works from Rust
# and not from C.
set -euo pipefail

cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

target=${CARGO_TARGET_DIR:-$PWD/../target}
profile=${PROFILE:-debug}
cc=${CC:-cc}
cxx=${CXX:-c++}

cargo build -p tt-ffi ${PROFILE:+--profile "$PROFILE"}

lib=$target/$profile
[ -f "$lib/libsterna.so" ] || { echo "no $lib/libsterna.so" >&2; exit 1; }

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

# -Werror because a warning in a header is the header's bug: this file is a
# stand-in for a frontend, and a frontend that has to silence warnings to
# include us will just stop including the warnings.
"$cc" -std=c11 -Wall -Wextra -Werror -pedantic \
    -I include tests/abi.c -o "$out/abi" \
    -L "$lib" -lsterna -Wl,-rpath,"$lib"

# And again as C++, because that is what actually includes it. `cpp_compat`
# in cbindgen.toml is what makes this work; without it the enums collide with
# C++'s stricter scoping rules and nobody finds out until the shell exists.
printf '#include <sterna.h>\nint main() { return tt_version() == nullptr; }\n' \
    > "$out/compat.cpp"
"$cxx" -std=c++17 -Wall -Wextra -Werror -I include "$out/compat.cpp" \
    -o "$out/compat" -L "$lib" -lsterna -Wl,-rpath,"$lib"
"$out/compat"

"$out/abi"
