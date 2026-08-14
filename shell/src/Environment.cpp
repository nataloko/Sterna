// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Environment.h"

#include <QByteArray>
#include <QByteArrayList>

namespace environment {

void unshadowBundledLibraries()
{
    const QByteArray appdir = qgetenv("APPDIR");
    if (appdir.isEmpty()) {
        return;
    }
    const QByteArray path = qgetenv("LD_LIBRARY_PATH");
    if (path.isEmpty()) {
        return;
    }

    QByteArrayList kept;
    for (const QByteArray &entry : path.split(':')) {
        // An empty entry means the working directory to `ld.so`, and it is not
        // one of ours: dropping it would change what a child resolves. Only
        // the bundle's own directories go.
        if (entry == appdir || entry.startsWith(appdir + '/')) {
            continue;
        }
        kept.append(entry);
    }

    // Restore what was there before the launcher prepended to it, which
    // includes the case of nothing at all: an empty `LD_LIBRARY_PATH` is not
    // the same as an absent one, and a child that inherits the empty spelling
    // searches the working directory.
    if (kept.isEmpty()) {
        qunsetenv("LD_LIBRARY_PATH");
    } else {
        qputenv("LD_LIBRARY_PATH", kept.join(':'));
    }
}

} // namespace environment
