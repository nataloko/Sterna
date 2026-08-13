// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Branding.h"

#include <QResource>

QIcon sternaIcon()
{
    Q_INIT_RESOURCE(branding);
    return QIcon(QStringLiteral(":/branding/sterna.svg"));
}
