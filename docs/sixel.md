# Sixel graphics

Sterna renders DEC sixel bitmaps inline on Linux and Windows. The decoder is
in the Rust core; the flat C ABI exposes borrowed RGBA8888 rasters, and the Qt
shell paints them with the text grid. A frontend using the ABI does not need a
sixel parser of its own.

For a quick check with Netpbm:

```sh
ppmtosixel -7bit picture.ppm
```

The `-7bit` form uses `ESC P` and `ESC \`, so it also survives links which do
not pass 8-bit C1 controls unchanged.

## Protocol

Sterna accepts the DEC form:

```text
DCS Pa ; Pb ; Ph q <sixel data> ST
```

`Pa` selects the pixel aspect macro, `Pb=1` leaves untouched pixels
transparent, and other `Pb` values fill them with the configured terminal
background. `Ph`, the horizontal grid parameter, is accepted and ignored as
it is by xterm. The data stream supports:

- `?` through `~` for six vertical pixels;
- `!` repeat introducers;
- `"` raster attributes and declared geometry;
- `#` color selection and definitions in DEC HLS or RGB percentages;
- `$` graphics carriage return and `-` graphics newline.

Each graphic starts with the VT340 16-color palette and has 256 registers.
Color definitions belong to that graphic; xterm's private/shared color-register
mode 1070 is not implemented.

## Placement and text

Sixel scrolling is on after reset. A graphic starts at the text cursor, follows
the text into scrollback, and leaves the cursor at the same column on the first
complete row below the image. `DECSET ?80` selects Sixel Display Mode instead:
the graphic starts at the page origin, is clipped to the page, and does not
move the cursor. `DECRST ?80` restores scrolling mode.

The bitmap covers text which was already in its cells. Text or an erase written
later clears the corresponding cell-sized image tile, so the new grid contents
show through. The cursor is always painted above the image. Main-screen images
survive a visit to the alternate screen; alternate-screen images are discarded
when that screen is left. Selection and copy operate on text, not pixels.

## Capability queries

Sterna implements xterm's `XTSMGRAPHICS` query:

```text
CSI ? 1 ; 1 ; 0 S    read the color-register count
CSI ? 1 ; 4 ; 0 S    read the maximum color-register count
CSI ? 2 ; 1 ; 0 S    read current sixel geometry in pixels
CSI ? 2 ; 4 ; 0 S    read maximum sixel geometry in pixels
```

The replies report 256 registers. Current geometry is the text area's cell
geometry, clipped to the decoder limit; maximum geometry is 4096 by 4096.
Reset requests are successful no-ops because the limits are fixed, set requests
return failure, and ReGIS queries return unsupported.

Primary Device Attributes deliberately remains Tera Term-compatible and does
not add xterm's `;4` sixel marker. Software which requires that marker instead
of trying `XTSMGRAPHICS` must be configured to emit sixel explicitly.

## Resource limits

One decoded raster is limited to 4096 by 4096 pixels, or 64 MiB as RGBA8888.
Oversized declared dimensions and repeats are clipped while the rest of the
stream is still parsed. Stored graphics share a 128 MiB cache and evict oldest
first. An unterminated DCS is decoded incrementally rather than buffered, so it
cannot grow a second unbounded copy of its input.

Sterna does not implement ReGIS, Tektronix graphics, Kitty graphics, or the
iTerm2 inline-image protocol.

The wire grammar follows the
[VT330/VT340 Graphics Programming manual](https://vt100.net/docs/vt3xx-gp/chapter14.html),
and discovery follows
[xterm's XTSMGRAPHICS control](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).
