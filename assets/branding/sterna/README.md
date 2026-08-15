# Sterna brand assets

The mark is an arctic tern banking through an S-shaped flight path. `svg/` is
the source of truth; the PNG and ICO application assets are generated from it.

## Files

| Path | Use |
|---|---|
| `svg/sterna-wide.svg` | Primary wide mark, closest to the selected concept. |
| `svg/sterna.svg` | Primary mark in a square canvas, for large placements. |
| `svg/sterna-small.svg` | Steeper small-size composition that keeps the S silhouette legible. |
| `svg/sterna-app.svg` | Blue tern over bright terminal rows, for launchers and window icons. |
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
| `samples/full-field-terminal-studies.{svg,png}` | Review sheet for six full-field terminal row patterns. |
| `samples/sparse-output-cursor-studies.{svg,png}` | Review sheet for sparse-output cursor and bird treatments. |
| `samples/tall-cursor-bird-studies.{svg,png}` | Review sheet for tall cursor widths with wire and solid birds. |
| `samples/row-strength-bird-studies.{svg,png}` | Review sheet for terminal row strengths with wire and solid birds. |
| `samples/blue-compare.{svg,png}` | Review sheet for blue wire and solid birds over bright terminal rows. |
| `samples/bird-colour-compare.{svg,png}` | Review sheet comparing blue and green birds over original and bright rows. |

The primary and small marks use negative space for the white body. Use them on
light backgrounds. Use a mono mark on colored backgrounds. The application
tile supplies its own dark background. The tile is readable with light and
dark desktop themes.

## Palette

```text
primary wing  #28292A
terminal tile #03110A
phosphor      #73F59B
bird blue     #7DA8FF
beak          #F99E2A
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
