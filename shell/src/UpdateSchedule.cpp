// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "UpdateSchedule.h"

#include <QDateTime>

bool updateCheckDue(const QString &recorded, const QDateTime &now)
{
    const QDateTime last = QDateTime::fromString(recorded.trimmed(), Qt::ISODate);
    if (!last.isValid()) {
        return true;
    }
    // Both sides in UTC before anything is compared: a stamp written by this
    // program carries `Z` and one typed by hand does not, so the two are not
    // otherwise the same kind of instant. `addDays` on a UTC datetime is a
    // plain 24 hours, which is what "once a day" is being taken to mean —
    // calendar days would make a launch at 23:59 and one at 00:01 two checks.
    const QDateTime then = last.toUTC();
    const QDateTime here = now.toUTC();
    if (then > here) {
        return true;
    }
    return then.addDays(1) <= here;
}

QString updateCheckStamp(const QDateTime &now)
{
    return now.toUTC().toString(Qt::ISODate);
}
