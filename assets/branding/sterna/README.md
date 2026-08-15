# Sterna brand assets

The mark is an arctic tern banking through an S-shaped flight path. `svg/` is
the source of truth; the PNG and ICO application assets are generated from it.

## Files

| Path | Use |
|---|---|
| `svg/sterna-app.svg` | Blue tern over bright terminal rows, for launchers and window icons. |
| `icons/sterna-{16..1024}.png` | Generated application icons. |
| `sterna.ico` | Windows icon with 16 through 256 px frames. |

The application tile supplies its own dark background. The tile is readable
with light and dark desktop themes.

## Palette

```text
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
convert icons/sterna-{16,24,32,48,64,128,256}.png sterna.ico
```

Requires `librsvg2-bin` and ImageMagick.
