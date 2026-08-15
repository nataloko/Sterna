#!/usr/bin/env python3
# Needs Python 3.11 or later. No third-party packages.
"""
Copy a Tera Term source file into build/ and apply the oracle's local fixes.

Tera Term's tree is left untouched -- everything the oracle builds from is
either unmodified or a patched copy under build/patched/. Patches are applied
as exact string replacements rather than by `patch(1)` because the upstream
files are CRLF and context matching gets fragile.

Each patch MUST match exactly once. A patch that stops applying means upstream
moved, and the oracle should fail loudly rather than silently build stale or
half-fixed behaviour.
"""

from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

# (source basename, must-match-once text, replacement, rationale)
PATCHES: list[tuple[str, str, str, str]] = [
    (
        "buffer.c",
        "\t\tif (IsBuffPadding(b)) {\r\n\t\t\tcontinue;\r\n\t\t}\r\n"
        "\t\tlen = expand_wchar(b, &buf[idx], left, &too_small);",
        "\t\tif (IsBuffPadding(b)) {\r\n\t\t\tb++;\r\n\t\t\tcontinue;\r\n\t\t}\r\n"
        "\t\tlen = expand_wchar(b, &buf[idx], left, &too_small);",
        "patches/0001-buffgetanylinedataw-padding.patch: BuffGetAnyLineDataW "
        "drops everything after the first wide character",
    ),
    (
        "buffer.c",
        "\tidx = 0;\r\n\tleft = copysize;\r\n",
        "\tidx = 0;\r\n\tleft = bufsize - 1;\r\n",
        "patches/0002-buffgetanylinedataw-left.patch: BuffGetAnyLineDataW "
        "budgets output units with a column count, truncating combining text",
    ),
    (
        "buffer.c",
        "\tNewLine(PageStart+CursorY);\r\n"
        "\tmemsetW(&(CodeLineW[XStart]),0x20, CurCharAttr.Fore,",
        "\tif (Count > NumOfColumns - XStart) {\r\n"
        "\t\tCount = NumOfColumns - XStart;\r\n"
        "\t}\r\n"
        "\tNewLine(PageStart+CursorY);\r\n"
        "\tmemsetW(&(CodeLineW[XStart]),0x20, CurCharAttr.Fore,",
        "patches/0003-bufferasecharsinline-overrun.patch: ECH writes past the "
        "end of the line, into the next one",
    ),
    (
        "buffer.c",
        "\t\tfor (j = TmpPtr + offset; j < TmpPtr + NumOfColumns - offset; j++) {\r\n"
        "\t\t\tif (!(CodeLineW[j].attr2 & Attr2Protect)) {\r\n"
        "\t\t\t\tBuffSetChar(&CodeBuffW[j], 0x20, 'H');\r\n"
        "\t\t\t\tCodeLineW[j].attr &= AttrSgrMask;\r\n",
        "\t\tfor (j = TmpPtr + offset; j < TmpPtr + NumOfColumns; j++) {\r\n"
        "\t\t\tif (!(CodeBuffW[j].attr2 & Attr2Protect)) {\r\n"
        "\t\t\t\tBuffSetChar(&CodeBuffW[j], 0x20, 'H');\r\n"
        "\t\t\t\tCodeBuffW[j].attr &= AttrSgrMask;\r\n",
        "patches/0004-buffselectederase-wrong-base.patch: DECSED indexes a "
        "line-relative pointer with an absolute offset (CurToEnd)",
    ),
    (
        "buffer.c",
        "\t\tfor (j = TmpPtr; j < TmpPtr + offset; j++) {\r\n"
        "\t\t\tif (!(CodeLineW[j].attr2 & Attr2Protect)) {\r\n",
        "\t\tfor (j = TmpPtr; j < TmpPtr + offset; j++) {\r\n"
        "\t\t\tif (!(CodeBuffW[j].attr2 & Attr2Protect)) {\r\n",
        "patches/0004-buffselectederase-wrong-base.patch: DECSED indexes a "
        "line-relative pointer with an absolute offset (HomeToCur)",
    ),
    (
        "vtterm.c",
        "\t\telse {\r\n"
        "\t\t\ttmpy[0] = ((x >> 6) & 0x1f) | 0xc0;\r\n"
        "\t\t\ttmpy[1] = (y & 0x3f) | 0x80;\r\n",
        "\t\telse {\r\n"
        "\t\t\ttmpy[0] = ((y >> 6) & 0x1f) | 0xc0;\r\n"
        "\t\t\ttmpy[1] = (y & 0x3f) | 0x80;\r\n",
        "patches/0005-mousereport-utf8-row.patch: the UTF-8 mouse report builds "
        "the row's lead byte from the column",
    ),
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()

    name = args.src.name
    args.out.parent.mkdir(parents=True, exist_ok=True)

    applicable = [p for p in PATCHES if p[0] == name]
    if not applicable:
        shutil.copy2(args.src, args.out)
        return 0

    with open(args.src, "r", encoding="utf-8", errors="surrogateescape", newline="") as f:
        text = f.read()
    for _, old, new, why in applicable:
        # Tolerate either line ending so the check does not depend on git config.
        for o, n in ((old, new), (old.replace("\r\n", "\n"), new.replace("\r\n", "\n"))):
            if text.count(o) == 1:
                text = text.replace(o, n)
                print(f"  patched {name}: {why}")
                break
        else:
            print(f"ERROR: patch no longer applies to {name}\n  {why}", file=sys.stderr)
            return 1

    with open(args.out, "w", encoding="utf-8", errors="surrogateescape", newline="") as f:
        f.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
