// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>

class QDateTime;

/// Is a startup update check due, given what `updates.last_check` recorded?
///
/// Deliberately its own translation unit rather than a method on either side of
/// the seam. The terminal has to answer this *before* it loads `sterna_updater`
/// — the whole point of that library being on-demand is that an ordinary
/// session never maps Qt Network or a TLS backend — so the decision cannot live
/// in the updater; and it is a pure function of a string and a clock, so it
/// does not want a window either. `update_test` compiles this file directly.
///
/// A stamp that does not parse, or that is in the future, is treated as "never
/// checked". Both are how a hand-edited file and a clock moved backwards
/// arrive, and the alternative to checking is a terminal that never looks
/// again — which is the one outcome worth avoiding for a signed security
/// update.
bool updateCheckDue(const QString &recorded, const QDateTime &now);

/// `now` in the spelling [`updateCheckDue`] reads back: ISO-8601, UTC, seconds.
///
/// UTC because the file is read on whichever side of a time-zone change or a
/// DST boundary the next launch happens to be, and a local stamp makes one of
/// those into a day without a check or a day with two.
QString updateCheckStamp(const QDateTime &now);
