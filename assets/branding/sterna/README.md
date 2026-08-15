# Sterna brand assets

The mark is an arctic tern banking through an S-shaped flight path. `svg/` is
the source of truth; the PNG and ICO application assets are generated from it.

## Files

| Path | Use |
|---|---|
| `svg/sterna-wide.svg` | Primary wide mark, closest to the selected concept. |
| `svg/sterna.svg` | Primary mark in a square canvas, for large placements. |
| `svg/sterna-small.svg` | Steeper small-size composition that keeps the S silhouette legible. |
| `svg/sterna-app.svg` | Small composition on a warm-white tile, for launchers and window icons. |
| `svg/sterna-mono*.svg` | Single-colour marks using `currentColor`. |
| `icons/sterna-{16..1024}.png` | Generated application icons. |
| `sterna.ico` | Windows icon with 16 through 256 px frames. |
| `sterna-master.png` | 1254 px render of the primary square mark. |
| `size-test-{light,dark}.png` | Review sheets; not application resources. |
| `samples/computery-concepts.{svg,png}` | Review sheet for four computer-related icon concepts. |
| `samples/terminal-tile-concepts.{svg,png}` | Review sheet for six dark terminal tile concepts. |
| `samples/phosphor-brand-concepts.{svg,png}` | Review sheet for nine phosphor brand concepts. |
| `samples/phosphor-prompt-sizes.{svg,png}` | Review sheet for six prompt sizes. |
| `samples/phosphor-pixel-tern.png` | 32 px source for the pixel tern concept. |
| `samples/wire-bird-prompt-placements.{svg,png}` | Review sheet for six 200% prompt placements. |
| `samples/wire-bird-terminal-rows.{svg,png}` | Review sheet for six phosphor terminal row treatments. |
| `samples/wire-bird-background-rows.{svg,png}` | Review sheet for six background terminal row treatments. |

The primary and small marks use negative space for the white body, so use them
on light backgrounds. Use a mono mark on coloured backgrounds. The application
tile deliberately supplies its own background so it remains readable under
both light and dark desktop themes.

## Palette

```text
wing  #28292A
beak  #F99E2A
tile  #F8F7F2
```

## Regenerating

```sh
cd assets/branding/sterna
for s in 16 24 32 48 64 128 256 512 1024; do
    rsvg-convert -w "$s" -h "$s" svg/sterna-app.svg -o "icons/sterna-$s.png"
done
rsvg-convert -w 1254 -h 1254 svg/sterna.svg -o sterna-master.png
convert icons/sterna-{16,24,32,48,64,128,256}.png sterna.ico
```

Requires `librsvg2-bin` and ImageMagick.
