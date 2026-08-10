// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "WindowTitle.h"

namespace {

/// `Title=` ships as the upstream product name. It means "use this program's
/// name" here, including when `ahead` or `last` leaves it beside an OSC title.
QString replaceUpstreamProduct(const WindowTitleState &state, QString title)
{
    if (state.configuredTitle != state.upstreamDefaultTitle) {
        return title;
    }
    if (title == state.configuredTitle) {
        return state.productTitle;
    }

    const QString separator = QStringLiteral(" ");
    if (state.titleChange == QLatin1String("ahead")) {
        const QString suffix = separator + state.configuredTitle;
        if (title.endsWith(suffix)) {
            title.chop(state.configuredTitle.size());
            title += state.productTitle;
        }
    } else if (state.titleChange == QLatin1String("last")) {
        const QString prefix = state.configuredTitle + separator;
        if (title.startsWith(prefix)) {
            title.remove(0, state.configuredTitle.size());
            title.prepend(state.productTitle);
        }
    }
    return title;
}

} // namespace

QString formatWindowTitle(const WindowTitleState &state,
                          const QString &connectingLabel,
                          const QString &disconnectedLabel)
{
    // A remote title is ignored while the line is not ready
    // (`ttwinman.c:101`). It cannot arrive from a real disconnected line, but
    // can remain from the connection which just closed.
    QString title = state.connected ? state.title : state.configuredTitle;
    title = replaceUpstreamProduct(state, title);

    // `TitleFormat` is a WORD. Preserve and act on its low six bits, while an
    // INI integer outside the WORD wraps the same way the C assignment does.
    const quint16 format = static_cast<quint16>(state.format);
    if (format & 1) {
        if (state.connecting) {
            title += QStringLiteral(" - ") + connectingLabel;
        } else if (!state.connected) {
            title += QStringLiteral(" - ") + disconnectedLabel;
        } else {
            QString endpoint = state.endpoint;
            if (state.linkKind == TT_LINK_SERIAL && (format & 32)) {
                endpoint += QStringLiteral(":%1bps").arg(state.serialBaud);
            } else if (state.linkKind == TT_LINK_NETWORK && (format & 16)
                       && state.tcpPort != 0) {
                endpoint += QStringLiteral(":%1").arg(state.tcpPort);
            }

            if (format & 8) {
                title = endpoint + QStringLiteral(" - ") + title;
            } else {
                title += QStringLiteral(" - ") + endpoint;
            }
        }
    }
    if (format & 2) {
        title += QStringLiteral(" (%1)").arg(state.sessionNumber);
    }
    if (format & 4) {
        title += QStringLiteral(" VT");
    }
    return title;
}
