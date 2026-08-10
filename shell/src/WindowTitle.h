// The six-bit `TitleFormat` word, kept out of MainWindow so every arm can be
// tested without opening a serial port or an SSH server.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>

#include "sterna.h"

struct WindowTitleState {
    /// `terminal.title` combined with the host's OSC title by the core.
    QString title;
    /// The file value before an OSC title is considered.
    QString configuredTitle;
    /// Upstream's product-name default and this program's replacement for it.
    QString upstreamDefaultTitle;
    QString productTitle;
    /// Canonical `window.title_change` spelling, for replacing the configured
    /// component without touching an OSC title which happens to contain the
    /// same words.
    QString titleChange;

    QString endpoint;
    quint16 tcpPort = 0;
    quint32 serialBaud = 0;
    quint32 sessionNumber = 1;
    TtLinkKind linkKind = TT_LINK_NONE;
    bool connected = false;
    bool connecting = false;
    int format = 13;
};

/// Compose the visible caption the way `ttwinman.c:89` does.
QString formatWindowTitle(const WindowTitleState &state,
                          const QString &connectingLabel,
                          const QString &disconnectedLabel);
