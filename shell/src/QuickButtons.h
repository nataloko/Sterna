// The window's own list of commands, out of the settings file.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QVector>

#include "sterna.h"

/// One quick button, copied out of the ABI's borrowed strings.
///
/// The list is the core's — the file format is, and so is what a button does,
/// since the four kinds are `KEYBOARD.CNF`'s `[User keys]` types. This struct
/// exists because those strings die at the next call that changes the list,
/// and a `QAction` outlives that by a long way.
struct QuickButton {
    QString label;
    TtQuickButtonKind kind = TT_QUICK_BUTTON_TEXT;
    /// As the file holds it, still `$HH`-escaped for the two sending kinds.
    /// This is what `Session::runQuickButton` takes.
    QString value;
    /// The same value, unescaped — what the editor shows and what a tooltip
    /// puts in front of somebody deciding whether to press it.
    QString text;
    /// A Qt key sequence in portable spelling, or empty. Empty is the shipping
    /// state for every button: a shortcut is a key the terminal stops
    /// receiving, so none is assigned on a user's behalf.
    QString shortcut;
    bool confirm = false;
    /// How many times one press sends it: 1 is once, and
    /// `TT_QUICK_BUTTON_REPEAT_FOREVER` is a run only a person stops.
    quint32 repeat = 1;
    /// Milliseconds between the starts of two sends while repeating.
    quint32 intervalMs = 1000;

    /// Whether one press sends more than once.
    bool repeats() const { return repeat != 1; }
    bool repeatsForever() const
    {
        return repeat == TT_QUICK_BUTTON_REPEAT_FOREVER;
    }
    /// "10 times, every 2.5 s", or empty when it sends once. For a tooltip and
    /// for the question `confirm` puts in front of it — the count and the
    /// cadence are exactly what somebody deciding whether to press it wants.
    QString repeatSummary() const;

    /// What to write on it: the label, or the command itself when there is no
    /// label. A button with no name is still a button, and showing the command
    /// is more use than showing nothing.
    QString caption() const;
    /// The same, cut to something a toolbar can hold.
    QString shortCaption() const;
    /// One line describing what it will do, for a tooltip or a confirmation.
    QString describe() const;
    /// Whether pressing it puts a `CR` on the wire last — what the editor's
    /// "Send Enter after" box is a view of.
    bool sendsEnter() const;
    /// A copy with the trailing `CR` removed, for a Shift+click.
    QuickButton withoutEnter() const;
};

/// Read the buttons out of `settingsPath`. A file that is not there is a first
/// run and has no buttons, which is not a failure.
QVector<QuickButton> loadQuickButtons(const QString &settingsPath);

/// Write them back, replacing `[Sterna Buttons]` and leaving every other line
/// in the file alone.
bool saveQuickButtons(const QString &settingsPath,
                      const QVector<QuickButton> &buttons,
                      QString *outError = nullptr);
