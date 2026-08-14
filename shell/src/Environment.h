// What the launcher did to this process that its children must not inherit.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

namespace environment {

/// Take the AppImage's own libraries back out of `LD_LIBRARY_PATH`.
///
/// The AppImage resolves its bundled Qt by `LD_LIBRARY_PATH` rather than by
/// rpath — deliberately, because patchelf corrupts `.relr.dyn` libraries and
/// because leaving them byte-for-byte as built preserves the LGPL substitution
/// seam. The cost is that the variable is *exported*, so every process the
/// terminal starts inherits it: the login shell, everything that shell's rc
/// files run, and the browser a clicked URL opens. Those are host programs
/// built against the host's libraries, and they are handed ours — which are
/// older, because the image is built on `manylinux_2_28`. What that looks like
/// is a wall of
///
/// ```text
/// grep: /tmp/.mount_sternaXXXX/usr/lib/libpcre2-8.so.0: no version information available
/// flatpak: /tmp/.mount_sternaXXXX/usr/lib/libmount.so.1: version `MOUNT_2_40' not found
/// ```
///
/// on the first prompt, and the second of those is fatal to the program that
/// hit it. A terminal whose shell cannot run the user's own tools is broken in
/// the way that matters most.
///
/// Removing entries under `$APPDIR` is safe for *this* process because glibc
/// reads `LD_LIBRARY_PATH` once, at `exec`, into the search list `dlopen` then
/// uses; changing the variable afterwards cannot move a plugin we have not
/// loaded yet. That is why this is one call at startup rather than a scrub at
/// every place a child is spawned — the next such place would forget.
///
/// Outside an AppImage `APPDIR` is unset and this does nothing.
void unshadowBundledLibraries();

} // namespace environment
