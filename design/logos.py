#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Sterna's logo marks and mascot, as drafts. Regenerates every sheet and icon.

The mascot is an arctic tern (Sterna paradisaea): black cap, coral bill and
legs, deeply forked tail. Run this after editing a path; nothing here is
hand-edited SVG.
"""

import subprocess
from pathlib import Path

OUT = Path(__file__).parent / "logos"
OUT.mkdir(exist_ok=True)

INK = "#14181d"
PAPER = "#f7f5f0"
CORAL = "#e35336"
GREY = "#b9c2cc"
LIGHTINK = "#eceff3"


# ---------------------------------------------------------------- marks

def m_tern(ink, accent):
    """Flight silhouette; also a prompt chevron."""
    return f"""
  <path d="M 14,15 Q 46,26 69,45 L 69,51 Q 44,38 14,15 Z" fill="{ink}"/>
  <path d="M 14,81 Q 46,70 69,51 L 69,45 Q 44,58 14,81 Z" fill="{ink}"/>
  <path d="M 52,45 L 20,36 L 34,49 L 20,62 L 52,52 Z" fill="{ink}"/>
  <path d="M 44,42 Q 62,39 80,45 Q 62,55 44,54 Z" fill="{ink}"/>
  <path d="M 82,43 L 96,41.5 L 82,48 Z" fill="{accent}"/>
"""


def m_swallowtail(ink, accent):
    """Block cursor with the tail notch bitten out."""
    return f'<path d="M 20,24 L 84,24 L 66,50 L 84,76 L 20,76 Z" fill="{ink}"/>'


def m_dive(ink, accent):
    """Plunge-dive; the silhouette is a caret."""
    return f"""
  <path d="M 50,70 Q 34,44 12,14 L 22,12 Q 44,40 55,66 Z" fill="{ink}"/>
  <path d="M 50,70 Q 66,44 88,14 L 78,12 Q 56,40 45,66 Z" fill="{ink}"/>
  <path d="M 44,54 L 39,20 L 50,33 L 61,20 L 56,54 Z" fill="{ink}"/>
  <path d="M 43,52 Q 50,44 57,52 L 55,80 Q 50,84 45,80 Z" fill="{ink}"/>
  <circle cx="50" cy="80" r="6" fill="{ink}"/>
  <path d="M 46,84 L 50,98 L 54,84 Z" fill="{accent}"/>
"""


def m_tt(ink, accent):
    """Two t's, crossbar swept into wings, feet into a forked tail."""
    return f"""
  <path d="M 6,30 Q 28,46 50,46 Q 72,46 94,30 L 92,40 Q 70,54 50,54 Q 30,54 8,40 Z" fill="{ink}"/>
  <path d="M 41,16 L 48,16 L 48,66 Q 48,74 40,80 L 34,73 Q 41,69 41,62 Z" fill="{ink}"/>
  <path d="M 52,16 L 59,16 L 59,62 Q 59,69 66,73 L 60,80 Q 52,74 52,66 Z" fill="{ink}"/>
"""


MARKS = [
    (m_tern, "the tern", "flight silhouette = prompt chevron"),
    (m_dive, "the dive", "plunge-dive = the caret"),
    (m_swallowtail, "the swallowtail", "block cursor, tail notched out"),
    (m_tt, "the tt", "two t's, crossbar swept into wings"),
]


# ---------------------------------------------------------------- mascot

def bird(wing_up=False, mood="calm"):
    """A chunky arctic tern. 100x100 box, feet on y=94."""
    wing = (
        '<path d="M 33,48 Q 50,40 60,50 Q 48,58 30,58 Z"'
        if wing_up
        else '<path d="M 32,52 Q 50,48 60,58 Q 48,70 30,66 Z"'
    )
    eye_y = 30 if mood != "sleepy" else 31
    eye = (
        f'<circle cx="68" cy="{eye_y}" r="3.6" fill="{PAPER}"/>'
        f'<circle cx="69" cy="{eye_y}" r="1.9" fill="{INK}"/>'
        if mood != "sleepy"
        else f'<path d="M 64,31 Q 68,34 72,31" fill="none" stroke="{PAPER}" '
        f'stroke-width="2.4" stroke-linecap="round"/>'
    )
    return f"""
  <path d="M 26,50 L 2,40 L 13,58 L 2,74 Z" fill="{PAPER}" stroke="{INK}"
        stroke-width="2.6" stroke-linejoin="round"/>
  <path d="M 44,78 L 44,92" stroke="{CORAL}" stroke-width="3.4" stroke-linecap="round"/>
  <path d="M 54,78 L 54,92" stroke="{CORAL}" stroke-width="3.4" stroke-linecap="round"/>
  <path d="M 38,93 L 50,93 M 48,93 L 60,93" stroke="{CORAL}" stroke-width="3.4"
        stroke-linecap="round"/>
  <ellipse cx="45" cy="58" rx="26" ry="22" fill="{PAPER}" stroke="{INK}" stroke-width="2.6"/>
  {wing} fill="{GREY}" stroke="{INK}" stroke-width="2.4" stroke-linejoin="round"/>
  <circle cx="59" cy="33" r="17" fill="{PAPER}" stroke="{INK}" stroke-width="2.6"/>
  <path d="M 42.6,29 A 17,17 0 0 1 75.4,29 Z" fill="{INK}"/>
  <path d="M 42.6,29 L 42.6,33 Q 50,31 58,31 Z" fill="{INK}"/>
  {eye}
  <path d="M 74,31 L 97,35 L 74,40 Z" fill="{CORAL}"/>
"""


def bird_flying():
    """Same character, wings out, seen from the side."""
    return f"""
  <path d="M 22,52 L 0,42 L 10,58 L 0,72 Z" fill="{PAPER}" stroke="{INK}"
        stroke-width="2.6" stroke-linejoin="round"/>
  <path d="M 40,54 Q 34,28 20,10 Q 46,20 54,48 Z" fill="{GREY}" stroke="{INK}"
        stroke-width="2.4" stroke-linejoin="round"/>
  <ellipse cx="44" cy="58" rx="24" ry="14" fill="{PAPER}" stroke="{INK}" stroke-width="2.6"/>
  <path d="M 44,60 Q 40,80 30,94 Q 58,84 60,62 Z" fill="{GREY}" stroke="{INK}"
        stroke-width="2.4" stroke-linejoin="round"/>
  <circle cx="63" cy="46" r="14" fill="{PAPER}" stroke="{INK}" stroke-width="2.6"/>
  <path d="M 49.5,42 A 14,14 0 0 1 76.5,42 Z" fill="{INK}"/>
  <circle cx="70" cy="43" r="3" fill="{PAPER}"/>
  <circle cx="71" cy="43" r="1.6" fill="{INK}"/>
  <path d="M 76,44 L 97,47 L 76,52 Z" fill="{CORAL}"/>
"""


POSES = [
    (lambda: bird(False, "calm"), "idle", "sits on the prompt"),
    (lambda: bird(True, "calm"), "wing up", "connected / sending"),
    (lambda: bird(False, "sleepy"), "sleepy", "idle session, no traffic"),
    (bird_flying, "in flight", "transfer in progress"),
]


# ---------------------------------------------------------------- sheets

def g(inner, x, y, size):
    return f'<g transform="translate({x},{y}) scale({size / 100:.4f})">{inner}</g>'


def txt(x, y, s, size=13, fill="#8a8478", family="DejaVu Sans"):
    return f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" fill="{fill}">{s}</text>'


def marks_sheet():
    W, H = 1020, 700
    xs = [20, 270, 520, 770]
    p = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
        f'<rect width="{W}" height="{H}" fill="{PAPER}"/>',
        txt(20, 44, "four marks, none of them a bug", 26, INK),
    ]
    for (fn, name, note), x in zip(MARKS, xs):
        p.append(f'<rect x="{x}" y="70" width="230" height="230" fill="#fff" stroke="#e2ded6"/>')
        p.append(g(fn(INK, CORAL), x + 25, 95, 180))
        p.append(f'<rect x="{x}" y="310" width="230" height="230" fill="{INK}"/>')
        p.append(g(fn(LIGHTINK, CORAL), x + 25, 335, 180))
        p.append(f'<rect x="{x}" y="550" width="230" height="62" fill="#fff" stroke="#e2ded6"/>')
        p.append(g(fn(INK, CORAL), x + 14, 557, 48))
        p.append(g(fn(INK, CORAL), x + 76, 565, 32))
        p.append(g(fn(INK, CORAL), x + 126, 573, 16))
        p.append(txt(x + 156, 588, "48/32/16", 11))
        p.append(txt(x, 642, name, 19, INK))
        p.append(txt(x, 664, note, 12))
    p.append("</svg>")
    return "\n".join(p)


def mascot_sheet():
    W, H = 1020, 620
    xs = [20, 270, 520, 770]
    p = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
        f'<rect width="{W}" height="{H}" fill="{PAPER}"/>',
        txt(20, 44, "the mascot: an arctic tern", 26, INK),
        txt(20, 66, "black cap, coral bill, forked tail — pole to pole, the longest "
                    "migration of any animal", 13),
    ]
    for (fn, name, note), x in zip(POSES, xs):
        p.append(f'<rect x="{x}" y="86" width="230" height="230" fill="#fff" stroke="#e2ded6"/>')
        p.append(g(fn(), x + 25, 111, 180))
        p.append(txt(x, 344, name, 18, INK))
        p.append(txt(x, 364, note, 12))

    # in-terminal strip
    p.append(f'<rect x="20" y="392" width="980" height="200" rx="8" fill="{INK}"/>')
    mono = "DejaVu Sans Mono"
    p.append(txt(48, 432, "$ sterna myrouter", 17, LIGHTINK, mono))
    p.append(txt(48, 462, "connecting 10.0.0.1:22 ...", 17, "#7d8894", mono))
    p.append(g(bird_flying(), 44, 480, 54))
    p.append(txt(110, 522, "sending  firmware.bin   64%  ▓▓▓▓▓▓▓▓░░░░", 17, LIGHTINK, mono))
    p.append(txt(48, 562, "$", 17, LIGHTINK, mono))
    p.append(g(bird(False, "sleepy"), 66, 540, 30))
    p.append(f'<rect x="104" y="548" width="11" height="20" fill="{CORAL}"/>')
    p.append("</svg>")
    return "\n".join(p)


def icon(inner, size=256):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
        f'viewBox="0 0 100 100">{inner}</svg>'
    )


written = [("marks.svg", marks_sheet()), ("mascot.svg", mascot_sheet())]
for fn, name, _ in MARKS:
    written.append((f"mark-{name.removeprefix('the ')}.svg", icon(fn(INK, CORAL))))
written.append(("mascot-idle.svg", icon(bird(False, "calm"))))
written.append(("mascot-flying.svg", icon(bird_flying())))

for name, body in written:
    (OUT / name).write_text(body)

# PNGs for the two sheets, so they can be looked at without a renderer.
for name in ("marks", "mascot"):
    if subprocess.run(["which", "rsvg-convert"], capture_output=True).returncode == 0:
        subprocess.run(
            ["rsvg-convert", "-w", "1020", str(OUT / f"{name}.svg"),
             "-o", str(OUT / f"{name}.png")],
            check=True,
        )

print("\n".join(sorted(p.name for p in OUT.iterdir())))
