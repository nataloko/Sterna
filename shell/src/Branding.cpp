// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Branding.h"

#include <QResource>

QIcon sternaIcon()
{
    Q_INIT_RESOURCE(branding);

    // SVG support is a runtime plugin rather than part of Qt Gui. Keep the
    // application icon self-contained so a system build and the Windows stage
    // do not silently lose it when that optional plugin is absent.
    QIcon icon;
    icon.addFile(QStringLiteral(":/branding/sterna-16.png"), QSize(16, 16));
    icon.addFile(QStringLiteral(":/branding/sterna-24.png"), QSize(24, 24));
    icon.addFile(QStringLiteral(":/branding/sterna-32.png"), QSize(32, 32));
    icon.addFile(QStringLiteral(":/branding/sterna-48.png"), QSize(48, 48));
    icon.addFile(QStringLiteral(":/branding/sterna-64.png"), QSize(64, 64));
    icon.addFile(QStringLiteral(":/branding/sterna-128.png"), QSize(128, 128));
    icon.addFile(QStringLiteral(":/branding/sterna-256.png"), QSize(256, 256));
    icon.addFile(QStringLiteral(":/branding/sterna-512.png"), QSize(512, 512));
    return icon;
}
