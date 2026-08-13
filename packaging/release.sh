#!/usr/bin/env bash
# Finish a GitHub-built Sterna release without sending the updater key anywhere.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$PWD
repository=${GH_REPO:-nataloko/Sterna}
version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' \
    crates/Cargo.toml | head -1)
tag=${1:-v$version}

if [ "$tag" != "v$version" ]; then
    echo "release: tag $tag does not match workspace version $version" >&2
    exit 2
fi
for command in gh uv openssl; do
    command -v "$command" >/dev/null || {
        echo "release: $command is required" >&2
        exit 2
    }
done

draft=$(gh release view "$tag" --repo "$repository" --json isDraft --jq .isDraft)
if [ "$draft" != true ]; then
    echo "release: $tag is not a draft" >&2
    exit 2
fi

work=$root/release-build/$tag
signed=$work/signed
rm -rf "$work"
mkdir -p "$work" "$signed"

linux=sterna-x86_64.AppImage
zsync=$linux.zsync
windows=sterna-$version-x86_64-setup.exe
for name in "$linux" "$zsync" "$windows"; do
    gh release download "$tag" --repo "$repository" --dir "$work" --pattern "$name"
done

./packaging/update/create.py \
    --linux "$work/$linux" \
    --zsync "$work/$zsync" \
    --windows "$work/$windows" \
    --output "$signed" \
    --repository "$repository"

gh release upload "$tag" --repo "$repository" \
    --clobber \
    "$signed/latest.json" \
    "$signed/latest.json.sig" \
    "$signed/SHA256SUMS"

expected=$(printf '%s\n' "$linux" "$zsync" "$windows" \
    latest.json latest.json.sig SHA256SUMS | sort)
actual=$(gh release view "$tag" --repo "$repository" --json assets \
    --jq '.assets[].name' | sort)
if [ "$actual" != "$expected" ]; then
    echo "release: draft asset set is not exact" >&2
    diff -u <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") || true
    exit 2
fi

remote=$(mktemp -d)
trap 'rm -rf "$remote"' EXIT
for name in latest.json latest.json.sig SHA256SUMS; do
    gh release download "$tag" --repo "$repository" --dir "$remote" --pattern "$name"
    cmp "$signed/$name" "$remote/$name"
done

gh release edit "$tag" --repo "$repository" --draft=false --latest
echo "release: published https://github.com/$repository/releases/tag/$tag"
