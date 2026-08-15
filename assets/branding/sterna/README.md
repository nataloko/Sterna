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
