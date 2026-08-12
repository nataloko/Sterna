#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Sign release artifacts and create Sterna's updater manifest."""

from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import quote


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_KEY = ROOT / ".private/updater-private.pem"
DEFAULT_PASSWORD = ROOT / ".private/updater-password.txt"
PUBLIC_KEY = ROOT / "packaging/update/public-key.txt"


def workspace_version() -> str:
    cargo = (ROOT / "crates/Cargo.toml").read_text()
    match = re.search(
        r'^\[workspace\.package\]\s*.*?^version\s*=\s*"([^"]+)"',
        cargo,
        re.MULTILINE | re.DOTALL,
    )
    if not match or not re.fullmatch(r"\d+\.\d+\.\d+", match.group(1)):
        raise RuntimeError("crates/Cargo.toml has no X.Y.Z workspace version")
    return match.group(1)


def signing_key(args: argparse.Namespace) -> tuple[Path, str]:
    key_env = os.environ.get("STERNA_UPDATE_PRIVATE_KEY")
    if key_env:
        key = Path(key_env)
    else:
        key = args.key

    password_env = os.environ.get("STERNA_UPDATE_KEY_PASSWORD")
    if password_env is not None:
        return key, f"pass:{password_env}"
    return key, f"file:{args.password_file}"


def sign(path: Path, key: Path, passin: str) -> str:
    signature = subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            str(key),
            "-passin",
            passin,
            "-in",
            str(path),
        ],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    if len(signature) != 64:
        raise RuntimeError(f"unexpected signature length for {path}: {len(signature)}")
    return base64.b64encode(signature).decode("ascii")


def artifact(path: Path, url: str, key: Path, passin: str) -> dict[str, object]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
            size += len(chunk)
    return {
        "url": url,
        "size": size,
        "sha256": digest.hexdigest(),
        "signature": sign(path, key, passin),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--linux", type=Path, required=True, help="x86-64 AppImage")
    parser.add_argument("--windows", type=Path, required=True, help="x86-64 NSIS setup")
    parser.add_argument("--output", type=Path, default=Path("release-update"))
    parser.add_argument("--repository", default="nataloko/Sterna")
    parser.add_argument("--key", type=Path, default=DEFAULT_KEY)
    parser.add_argument("--password-file", type=Path, default=DEFAULT_PASSWORD)
    args = parser.parse_args()

    for path in (args.linux, args.windows, PUBLIC_KEY):
        if not path.is_file():
            parser.error(f"not a file: {path}")
    key, passin = signing_key(args)
    if not key.is_file():
        parser.error(f"signing key not found: {key}")

    version = workspace_version()
    tag = f"v{version}"
    base = f"https://github.com/{args.repository}/releases/download/{tag}"
    manifest = {
        "format": 1,
        "version": version,
        "published_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "platforms": {
            "linux-x86_64": artifact(
                args.linux,
                f"{base}/{quote(args.linux.name)}",
                key,
                passin,
            ),
            "windows-x86_64": artifact(
                args.windows,
                f"{base}/{quote(args.windows.name)}",
                key,
                passin,
            ),
        },
    }

    args.output.mkdir(parents=True, exist_ok=True)
    manifest_path = args.output / "latest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    (args.output / "latest.json.sig").write_text(
        sign(manifest_path, key, passin) + "\n"
    )
    print(f"Updater manifest: {manifest_path}")
    print(f"Manifest signature: {manifest_path}.sig")
    return 0


if __name__ == "__main__":
    sys.exit(main())
