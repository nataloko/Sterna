#!/usr/bin/env bash
# Move Sterna to a new version, everywhere it is written down.
#
#   ./packaging/bump-version.sh 0.5.2   prepare that version, in every file
#   ./packaging/bump-version.sh --check every file agrees with Cargo.toml
#
# The version lives in six places and the release workflow used to check two of
# them, so the two packaging READMEs and PLAN.md's commit count could drift for
# releases at a time with nothing saying so. `--check` is the other half of the
# fix: `.github/workflows/release.yml` runs it before it builds anything, so a
# site this script forgets to move fails the tag build instead of shipping a
# document that names a version nobody can download. One implementation, the
# way `toolchain.env` is one file — a second copy of this list in YAML is the
# thing that goes stale.
#
# It does not commit, tag or push. Read the diff, then `packaging/RELEASING.md`
# says what to do with it.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$PWD

# --- the six sites -----------------------------------------------------------
#
# Adding one means adding it to `sites` *and* to the rewrite below. `--check` is
# generated from this list, so a site named here cannot be silently unchecked.
#
#   file : a grep -E pattern that must match, with @V@ standing for the version
#
sites=(
    "crates/Cargo.toml:^version = \"@V@\"\$"
    "shell/CMakeLists.txt:^project\(sterna-shell VERSION @V@ LANGUAGES CXX\)\$"
    "packaging/update/README.md:^\./packaging/release\.sh v@V@\$"
    "packaging/windows/README.md:sterna-@V@-x86_64-setup\.exe"
    "CHANGELOG.md:^## \[@V@\] - [0-9]{4}-[0-9]{2}-[0-9]{2}\$"
    "CHANGELOG.md:^\[@V@\]: https://github\.com/nataloko/Sterna/compare/"
)

# The one `crates/Cargo.toml` spelling every other file is measured against —
# the same extraction `release.sh` and the workflow's preflight use, so the
# three cannot read the file differently.
current() {
    sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' \
        crates/Cargo.toml | head -1
}

# Every site, against `version`. Prints what is wrong and answers 1 if anything
# is; silent and 0 when the tree agrees with itself.
check() {
    local version=$1 bad=0 entry file pattern
    for entry in "${sites[@]}"; do
        file=${entry%%:*}
        pattern=${entry#*:}
        pattern=${pattern//@V@/$version}
        if ! grep -Eq -- "$pattern" "$file"; then
            echo "bump-version: $file does not name $version" >&2
            echo "              expected a line matching: $pattern" >&2
            bad=1
        fi
    done
    # PLAN.md's count is not a version, so it has its own question: it is the
    # number of the release commit itself, which does not exist yet while this
    # runs. Checked as "not obviously stale" rather than exactly, because a
    # branch merged after the bump moves it and that is not a release failure.
    #
    # **Not asked of a shallow clone**, which is what `actions/checkout` makes:
    # `rev-list --count` answers 1 there, so every real count looks stale and
    # the preflight this feeds would fail every tag build for a statistic in a
    # roadmap file.
    local counted stated
    if [ "$(git rev-parse --is-shallow-repository 2>/dev/null)" = "true" ]; then
        return $bad
    fi
    counted=$(git rev-list --count HEAD 2>/dev/null || echo 0)
    stated=$(sed -n 's/^.*\*\*Commits:\*\* \([0-9]*\).*$/\1/p' PLAN.md | head -1)
    if [ -n "$stated" ] && [ "$counted" -gt 0 ] \
        && [ "$stated" -gt $((counted + 1)) ]; then
        echo "bump-version: PLAN.md claims $stated commits; there are $counted" >&2
        bad=1
    fi
    return $bad
}

if [ "${1:-}" = "--check" ]; then
    version=$(current)
    [ -n "$version" ] || { echo "bump-version: no version in crates/Cargo.toml" >&2; exit 2; }
    check "$version" || exit 2
    echo "bump-version: every site names $version"
    exit 0
fi

if [ "$#" -ne 1 ] || [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    [ "$#" -eq 1 ] && exit 0 || exit 2
fi

new=$1
case "$new" in
    v*) echo "bump-version: give the version without the leading v: ${new#v}" >&2; exit 2 ;;
esac
if ! [[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "bump-version: '$new' is not X.Y.Z" >&2
    exit 2
fi

old=$(current)
[ -n "$old" ] || { echo "bump-version: no version in crates/Cargo.toml" >&2; exit 2; }
if [ "$new" = "$old" ]; then
    echo "bump-version: already at $new" >&2
    exit 2
fi
# Lower is a mistake far more often than it is a deliberate rollback, and an
# accidental one is a published release that the updater will never offer.
if [ "$(printf '%s\n%s\n' "$old" "$new" | sort -V | head -1)" = "$new" ]; then
    echo "bump-version: $new is older than $old" >&2
    exit 2
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "bump-version: the working tree is not clean; commit or stash first" >&2
    exit 2
fi

# The release notes have to exist before the version does. Promoting whatever
# is under `## [Unreleased]` rather than opening an empty section is what makes
# that unforgettable: there is no way to reach a tag with an empty section,
# because the script refuses the bump instead.
notes=$(sed -n '/^## \[Unreleased\]$/,/^## \[/p' CHANGELOG.md \
    | sed '1d;/^## \[/d' | sed '/^[[:space:]]*$/d')
if [ -z "$notes" ]; then
    echo "bump-version: CHANGELOG.md has nothing under '## [Unreleased]'" >&2
    echo "              write the release notes there first — they are what" >&2
    echo "              the GitHub release page shows." >&2
    exit 2
fi

today=$(date +%F)
commits=$(( $(git rev-list --count HEAD) + 1 ))

# --- the rewrite -------------------------------------------------------------

sed -i "s/^version = \"$old\"\$/version = \"$new\"/" crates/Cargo.toml
sed -i "s/^project(sterna-shell VERSION $old /project(sterna-shell VERSION $new /" \
    shell/CMakeLists.txt
sed -i "s|^\./packaging/release\.sh v$old\$|./packaging/release.sh v$new|" \
    packaging/update/README.md
sed -i "s/sterna-$old-x86_64-setup\.exe/sterna-$new-x86_64-setup.exe/g" \
    packaging/windows/README.md
# The number of the release commit this bump is for, which is the count now
# plus that commit. `5d33a2f` is commit 817 and PLAN.md says 817.
sed -i "s/\*\*Commits:\*\* [0-9]*/**Commits:** $commits/" PLAN.md
sed -i "s/^\*\*Last updated:\*\* [0-9-]*/**Last updated:** $today/" PLAN.md

# `## [Unreleased]` keeps its place and empties; its contents become the new
# dated section directly below it.
python3 - "$new" "$today" <<'PY'
import re
import sys

version, today = sys.argv[1], sys.argv[2]
text = open("CHANGELOG.md", encoding="utf-8").read()

start = text.index("## [Unreleased]\n")
body_at = start + len("## [Unreleased]\n")
next_at = text.index("\n## [", body_at) + 1
body = text[body_at:next_at].strip("\n")

text = (
    text[:body_at]
    + "\n"
    + f"## [{version}] - {today}\n\n"
    + body
    + "\n\n"
    + text[next_at:]
)

# The compare links at the foot: Unreleased now starts from this version, and
# this version compares against whatever Unreleased used to.
previous = re.search(
    r"^\[Unreleased\]: (\S+)/compare/v(\S+)\.\.\.HEAD$", text, re.M
)
if not previous:
    sys.exit("bump-version: no [Unreleased] compare link in CHANGELOG.md")
base, prior = previous.group(1), previous.group(2)
text = text.replace(
    previous.group(0),
    f"[Unreleased]: {base}/compare/v{version}...HEAD\n"
    f"[{version}]: {base}/compare/v{prior}...v{version}",
)

open("CHANGELOG.md", "w", encoding="utf-8").write(text)
PY

# The lockfile carries every workspace crate's version. `cargo metadata` is the
# cheapest thing that rewrites it and needs no network.
(cd crates && cargo metadata --format-version 1 --offline >/dev/null)

# --- and it is only done if it checks -----------------------------------------

if ! check "$new"; then
    echo "bump-version: the rewrite above did not take; the tree is half-moved" >&2
    exit 1
fi
if git grep -nF -- "$old" -- \
    crates/Cargo.toml shell/CMakeLists.txt packaging/update/README.md \
    packaging/windows/README.md >/dev/null 2>&1; then
    echo "bump-version: $old is still named somewhere:" >&2
    git grep -nF -- "$old" -- \
        crates/Cargo.toml shell/CMakeLists.txt packaging/update/README.md \
        packaging/windows/README.md >&2
    exit 1
fi

echo "bump-version: $old -> $new, commit $commits, dated $today"
echo
git -C "$root" diff --stat
echo
echo "Read the diff, then packaging/RELEASING.md."
