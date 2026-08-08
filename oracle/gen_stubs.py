#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Generate the Sterna oracle's stub layer from Tera Term's own headers.

vtterm.c and buffer.c reference ~136 symbols that live in translation units we
deliberately do not compile (vtdisp.c's GDI rendering, the DDE bridge, the comm
layer, printing, clipboard). The oracle needs those symbols to exist, but not
to do anything -- except the handful the grid model actually observes.

This reads the real prototypes out of the headers so the stubs cannot silently
drift from the signatures vtterm.c was compiled against. Re-run it whenever
Tera Term's headers change:

    ./gen_stubs.py --missing build/real.txt --out src/stubs_generated.c
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

TT = Path("/home/nata/agents-home/Projects/teraterm/teraterm")
HEADER_DIRS = [TT / "common", TT / "teraterm"]

def hand_written(obj: Path) -> set[str]:
    """Symbols already defined in stubs_manual.c, read from its object file.

    Deriving this from the compiled object rather than a hardcoded list means
    the two stub layers cannot drift into duplicate definitions.
    """
    import subprocess

    if not obj.exists():
        return set()
    out = subprocess.run(["nm", "--defined-only", str(obj)],
                         capture_output=True, text=True, check=True).stdout
    syms = set()
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1] not in ("t", "r", "d", "b"):
            syms.add(parts[2])
        elif len(parts) == 3:
            syms.add(parts[2])
    return syms

COMMENT_BLOCK = re.compile(r"/\*.*?\*/", re.S)
COMMENT_LINE = re.compile(r"//[^\n]*")


def load_headers() -> str:
    chunks = []
    for d in HEADER_DIRS:
        for h in sorted(d.glob("*.h")):
            try:
                chunks.append(h.read_text(errors="replace"))
            except OSError:
                pass
    text = "\n".join(chunks)
    text = COMMENT_BLOCK.sub(" ", text)
    text = COMMENT_LINE.sub(" ", text)
    # Drop preprocessor lines: a prototype guarded by #if would otherwise be
    # captured together with the directive and emitted as unbalanced #endif.
    text = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
    return text


def find_prototype(text: str, sym: str) -> str | None:
    """Return the full declaration text for `sym`, parens balanced."""
    for m in re.finditer(rf"(^|[;\}}\n])\s*([^;{{}}\n][^;{{}}]*?\b{re.escape(sym)}\s*)\(", text):
        start = m.start(2)
        # Walk forward from the '(' matching parens.
        i = m.end()
        depth = 1
        while i < len(text) and depth:
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        if depth:
            continue
        decl = text[start:i]
        decl = " ".join(decl.split())
        if decl.startswith(("typedef", "return", "if", "while", "for", "switch")):
            continue
        if "=" in decl.split("(")[0]:
            continue
        decl = re.sub(r"\b(DllExport|WINAPI|PASCAL|CALLBACK|extern\s+\"C\"|extern)\b", "", decl)
        return " ".join(decl.split())
    return None


def find_variable(text: str, sym: str) -> str | None:
    m = re.search(rf"extern\s+([A-Za-z_][A-Za-z0-9_ \*]*?)\s+\*?\b{re.escape(sym)}\s*(\[[^\]]*\])?\s*;", text)
    if not m:
        return None
    return m.group(0).replace("extern", "").strip().rstrip(";")


def return_stub(decl: str, sym: str) -> str:
    """Pick a body based on the declared return type."""
    ret = decl.split(sym)[0].strip().rstrip("*").strip()
    stars = decl.split(sym)[0].strip().endswith("*")
    if stars or ret.endswith("*"):
        return "\treturn NULL;"
    ret_l = ret.lower().replace("const", "").strip()
    if ret_l in ("void", ""):
        return ""
    if ret_l in ("bool", "int", "unsigned int", "uint", "short", "char", "long",
                 "byte", "word", "dword", "uint32_t", "size_t", "colorref",
                 "lresult", "lstatus", "wchar_t", "int64_t", "long long"):
        return "\treturn 0;"
    return "\treturn 0;"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--missing", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--manual-obj", type=Path,
                    default=Path("build/stubs_manual.o"),
                    help="object file whose symbols must not be regenerated")
    args = ap.parse_args()

    skip = hand_written(args.manual_obj)
    text = load_headers()
    symbols = [s.strip() for s in args.missing.read_text().split() if s.strip()]

    protos, variables, unknown = [], [], []
    for sym in symbols:
        if sym in skip:
            continue
        decl = find_prototype(text, sym)
        if decl:
            protos.append((sym, decl))
            continue
        var = find_variable(text, sym)
        if var:
            variables.append((sym, var))
            continue
        unknown.append(sym)

    out = [
        "/*",
        " * Sterna oracle -- GENERATED by gen_stubs.py. Do not edit by hand.",
        " *",
        " * No-op definitions for symbols vtterm.c/buffer.c reference but whose",
        " * translation units the oracle does not compile (GDI rendering, DDE,",
        " * comm, printing, clipboard). Signatures are lifted from Tera Term's",
        " * own headers so they cannot drift.",
        " *",
        " * Anything the grid model actually observes is hand-written in",
        " * stubs_manual.c instead; those symbols are excluded automatically by",
        " * reading build/stubs_manual.o, so the two layers cannot collide.",
        " */",
        "#include <windows.h>",
        "#include <stddef.h>",
        "",
        '#include "teraterm.h"',
        '#include "tttypes.h"',
        '#include "buffer.h"',
        '#include "vtdisp.h"',
        # keyboard.c is compiled (in src/keys.c) for its key table, and drags
        # in the settings loader's function-pointer types and the delayed-send
        # queue. Without these two the generated stubs do not name their own
        # parameter types.
        '#include "ttsetup.h"',
        '#include "sendmem.h"',
        "",
    ]

    if variables:
        out.append("/* ---- data ---- */")
        for sym, var in variables:
            out.append(f"{var};")
        out.append("")

    out.append("/* ---- functions ---- */")
    for sym, decl in protos:
        body = return_stub(decl, sym)
        out.append(f"{decl}")
        out.append("{")
        if body:
            out.append(body)
        out.append("}")
        out.append("")

    args.out.write_text("\n".join(out) + "\n")

    print(f"generated {len(protos)} function stubs, {len(variables)} variables")
    if unknown:
        print(f"NO DECLARATION FOUND for {len(unknown)}:", file=sys.stderr)
        for s in unknown:
            print(f"  {s}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
