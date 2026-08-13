// Highlight rules as the window holds them, over the core's list ABI.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QColor>
#include <QString>
#include <QVector>

#include "sterna.h"

/// One rule, owned by Qt.
///
/// A copy rather than a `TtHighlight`, for the same reason `QuickButton` is
/// one: the core's strings die at the next call that changes the list, and a
/// dialog holding a rule outlives that by a long way.
///
/// `fore` and `back` are invalid `QColor`s when the rule leaves that channel
/// alone, which `QColor::isValid()` already expresses — so there is no separate
/// "has a colour" flag to keep in step.
struct QuickHighlight {
    QString label;
    QString pattern;
    bool literal = false;
    bool ignoreCase = false;
    QColor fore;
    QColor back;
    quint32 style = 0;
    bool wholeLine = false;
    quint32 group = 0;
    bool enabled = true;

    /// What the editor's list shows. The pattern when there is no label, since
    /// for a short pattern that is the better name anyway.
    QString caption() const;
    /// A sentence for a tooltip: what it matches and what it does about it.
    QString describe() const;
    /// Whether it could change a single pixel. A rule with no colours and no
    /// style is one somebody is still writing.
    bool paints() const;
};

/// Read the rules out of a settings file. A file that is not there is a first
/// run and has no rules, not an error.
QVector<QuickHighlight> loadHighlights(const QString &settingsPath);

/// Build the core's list out of Qt's copy. The caller frees it with
/// `tt_highlights_free`. Null only if the core would not allocate one.
///
/// Shared by the three callers that hand a rule set across the ABI — saving,
/// arming a session, and the editor's preview — so there is one place that
/// knows how a `QColor` becomes a packed colour.
TtHighlights *buildHighlightList(const QVector<QuickHighlight> &rules);

/// Write them back, leaving every other line of the file alone. False on
/// failure, with the core's message in `error`.
bool saveHighlights(const QString &settingsPath, const QVector<QuickHighlight> &rules,
                    QString *error);

/// Whether the engine will accept this pattern, and what it said if not.
///
/// The editor asks as the user types. `literal` and `ignoreCase` matter because
/// they change what the engine is handed.
bool checkHighlightPattern(const QString &pattern, bool literal, bool ignoreCase,
                           QString *error);

/// One line of text as rich text, coloured by `rules`.
///
/// Through the core's own engine, so the editor's sample box shows what the
/// terminal will actually do rather than a second implementation's opinion of
/// it. `fg` and `bg` stand in for the terminal's colours where a reversed rule
/// needs one and the rule did not give it.
QString highlightPreviewHtml(const QVector<QuickHighlight> &rules, const QString &text,
                             const QColor &fg, const QColor &bg);
