# Termitta mascot assets

The mascot is a termite whose face is a shell prompt. **`svg/` is the source of
truth** — everything under `icons/`, the master PNG and the `.ico` are generated
from it, so regenerate rather than edit rasters.

## Files

| Path | What it is |
|---|---|
| `svg/termitta.svg` | **Vector master.** Full mark: head, prompt, segmented body, six legs. Use at 48px and above. |
| `svg/termitta-small.svg` | **Small-size variant.** The same traced geometry, cropped at the neck and scaled up. Use at 32px and below. |
| `svg/termitta-mono.svg`, `svg/termitta-mono-small.svg` | Single-colour versions. Body takes `currentColor`, prompt is knocked out — set `color` on the parent to recolour. |
| `icons/termitta-{16,24,32}.png` | Generated from the *small* variant. |
| `icons/termitta-{48..1024}.png` | Generated from the *full* mark. |
| `termitta.ico` | 16/24/32/48/64/128/256, each from the appropriate variant. |
| `termitta-master.png` | 1254px render of the full mark, for convenience. |
| `alternates/accessible/` | Higher-contrast palette, both variants. See below. |
| `alternates/monochrome/` | Earlier raster monochrome master, superseded by `svg/termitta-mono.svg`. |
| `termitta-master-chroma.png` | The original generated raster, kept for provenance. |
| `size-test-{light,dark}.png` | Review sheets: 16, 24, 32, 48, 64, 128, 256 on contrasting backgrounds. Not application resources. |

## Palette

```
body    #D900B5     prompt  #39FF14
```

## Why there are two size variants

The full mark becomes an unreadable magenta blob below about 48px — the legs
and segmented body turn to mush and take the `>_` with them. That is the worst
possible thing to lose, because the prompt *is* the idea.

The small variant is **not a redrawing**. The head, antennae and prompt are
disjoint from the thorax and legs in the master, so the head is isolated as a
**connected component**, keeping its own natural tapered bottom edge, and then
scaled **uniformly** to fill the frame. Head octagon, antennae and prompt are
geometrically identical to the full mark — just about three times larger than a
naive downscale would give.

Two earlier attempts got this wrong, so if it ever needs regenerating:

- **Do not hand-draw the head.** It comes out wider and squarer than the real
  one, with stubby antennae — it stops looking like the full mark.
- **Do not crop at the neck's narrowest row.** That cuts through the taper and
  the head looks chopped off. The head continues to about y=600 of the 940-unit
  master and ends in its own rounded edge; crop below that and isolate by
  connected component instead.

Rule of thumb: **≤32px use `termitta-small.svg`, ≥48px use `termitta.svg`.**

## Accessibility

Magenta and green are the pair that merges under red-green colour blindness,
which affects roughly 8% of men. Measured body-to-prompt contrast:

| Palette | Normal | Deuteranopia |
|---|---|---|
| `#D900B5` / `#39FF14` (primary) | 3.35:1 | **1.47:1** |
| `#A8008C` / `#B6FF8C` (accessible) | 5.81:1 | 2.98:1 |

At 1.47:1 the prompt is effectively invisible to a deuteranope — it reads as
brown on olive. The primary palette is kept because it is the intended look and
the mark is still *recognisable* by silhouette; `alternates/accessible/` exists
for anywhere the prompt has to be *read* rather than merely recognised. Note
that darkening the body alone makes deuteranopic contrast **worse**, not better
— both colours have to move together.

The greyscale rendering is fine, so the luminance structure of the design is
sound; this is purely a hue-pair problem.

## Regenerating

```sh
cd assets/branding/termitta
for s in 16 24 32;               do rsvg-convert -w $s -h $s svg/termitta-small.svg -o icons/termitta-$s.png; done
for s in 48 64 128 256 512 1024; do rsvg-convert -w $s -h $s svg/termitta.svg       -o icons/termitta-$s.png; done
rsvg-convert -w 1254 -h 1254 svg/termitta.svg -o termitta-master.png
convert icons/termitta-{16,24,32,48,64,128,256}.png termitta.ico
```

Needs `librsvg2-bin` and `imagemagick`.

## History

The first master was a generated raster with a blue background keyed out. That
left ~6,300 blue-contaminated edge pixels (a visible halo on light
backgrounds), 17,847 unique colours in what should be a two-colour flat mark,
and a body colour that had drifted from the declared `#D900B5` to about
`#E603A9` and varied pixel to pixel. The design was traced to vector, which
fixed all of it at once: flat exact colours, no halo, and clean output at any
size. `termitta-master-chroma.png` is that original, retained for provenance.
