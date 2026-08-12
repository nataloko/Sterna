#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Create Sterna's one-time Ed25519 updater signing key.

The private half and its random password stay under the ignored `.private/`
directory. The raw public key and a signature fixture are written into the
repository so every build and test uses the same root of trust.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import os
from pathlib import Path
import secrets
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
PRIVATE_DIR = ROOT / ".private"
PRIVATE_KEY = PRIVATE_DIR / "updater-private.pem"
PASSWORD = PRIVATE_DIR / "updater-password.txt"
PUBLIC_KEY = ROOT / "packaging/update/public-key.txt"
TEST_MESSAGE = ROOT / "packaging/update/test-message.txt"
TEST_SIGNATURE = ROOT / "packaging/update/test-signature.txt"
SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")


def run(*args: str, input_bytes: bytes | None = None) -> bytes:
    return subprocess.run(
        args,
        input=input_bytes,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def write_secret(path: Path, value: bytes) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as out:
        out.write(value)


def sign(path: Path) -> bytes:
    return run(
        "openssl",
        "pkeyutl",
        "-sign",
        "-rawin",
        "-inkey",
        str(PRIVATE_KEY),
        "-passin",
        f"file:{PASSWORD}",
        "-in",
        str(path),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--force",
        action="store_true",
        help="rotate an existing key (old installations will need a manual update)",
    )
    args = parser.parse_args()

    existing = [p for p in (PRIVATE_KEY, PASSWORD, PUBLIC_KEY) if p.exists()]
    if existing and not args.force:
        joined = ", ".join(str(p.relative_to(ROOT)) for p in existing)
        parser.error(f"refusing to replace the updater root of trust: {joined}")

    PRIVATE_DIR.mkdir(mode=0o700, exist_ok=True)
    if args.force:
        for path in (PRIVATE_KEY, PASSWORD, PUBLIC_KEY, TEST_SIGNATURE):
            path.unlink(missing_ok=True)

    password = (secrets.token_urlsafe(48) + "\n").encode("ascii")
    write_secret(PASSWORD, password)
    try:
        run(
            "openssl",
            "genpkey",
            "-algorithm",
            "ED25519",
            "-aes-256-cbc",
            "-pass",
            f"file:{PASSWORD}",
            "-out",
            str(PRIVATE_KEY),
        )
        os.chmod(PRIVATE_KEY, 0o600)

        der = run(
            "openssl",
            "pkey",
            "-in",
            str(PRIVATE_KEY),
            "-passin",
            f"file:{PASSWORD}",
            "-pubout",
            "-outform",
            "DER",
        )
        if not der.startswith(SPKI_PREFIX) or len(der) != len(SPKI_PREFIX) + 32:
            raise RuntimeError("OpenSSL returned an unexpected Ed25519 public key")
        public = der[len(SPKI_PREFIX) :]
        PUBLIC_KEY.write_text(base64.b64encode(public).decode("ascii") + "\n")

        signature = sign(TEST_MESSAGE)
        if len(signature) != 64:
            raise RuntimeError("OpenSSL returned an unexpected Ed25519 signature")
        TEST_SIGNATURE.write_text(base64.b64encode(signature).decode("ascii") + "\n")
    except Exception:
        for path in (PRIVATE_KEY, PASSWORD, PUBLIC_KEY, TEST_SIGNATURE):
            path.unlink(missing_ok=True)
        raise

    fingerprint = hashlib.sha256(public).hexdigest()
    print(f"Updater public-key SHA-256: {fingerprint}")
    print(f"Private key: {PRIVATE_KEY}")
    print(f"Password:    {PASSWORD}")
    print("Back up both private files together; losing them ends automatic updates.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
