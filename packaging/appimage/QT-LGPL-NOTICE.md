# Qt in this AppImage, and what that obliges

This image bundles **Qt 6** under the **GNU Lesser General Public License,
version 3**. Sterna itself is 3-clause BSD (`LICENSE`); Qt is not ours and is
carried here under different terms, which this file exists to honour rather
than to summarise.

The full LGPLv3 text is in `LGPL-3.0.txt` beside this file, and the GPLv3 it
incorporates by reference is in `GPL-3.0.txt`.

## The three things the licence actually requires

**Qt is dynamically linked and shipped as separate shared libraries.** Nothing
here is statically linked against Qt. The `libQt6*.so.*` files live in
`usr/lib/` inside the image, exactly as they would in a distribution's own
packaging.

**You may replace them with your own build.** An AppImage is a read-only
SquashFS with the runtime in front of it, so the substitution is:

```sh
./sterna-x86_64.AppImage --appimage-extract    # gives ./squashfs-root
cp /path/to/your/libQt6Widgets.so.6 squashfs-root/usr/lib/
appimagetool squashfs-root sterna-x86_64.AppImage
```

`squashfs-root/AppRun` also runs directly, so a modified tree needs no repacking
to test. Nothing in the image pins a Qt build by hash, refuses an unexpected
version, or otherwise gets in the way of that.

**The source is on offer.** The Qt in this image is unmodified upstream Qt as
packaged by the distribution named in `BUILD-INFO.txt`, which records the exact
version and the source it came from. Complete corresponding source for those
libraries is available from that distribution's source archive, and from
<https://download.qt.io/official_releases/qt/>. If neither is reachable to you,
ask via <https://github.com/nataloko/Sterna/issues> and it will be provided.

## What is *not* under the LGPL

Everything in `usr/bin/sterna` and `usr/lib/libsterna.so` is Sterna's own
code, 3-clause BSD, including the parts derived from Tera Term — which is BSD
too, so the shipped image carries one licence text for all of it rather than
two. See `ATTRIBUTION.md` in the source tree for what came from where.
