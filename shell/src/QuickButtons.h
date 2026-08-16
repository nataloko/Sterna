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
    /// Which page of the panel it is on, counting from 1. A page is a filter
    /// on what is drawn and never a renumbering: this button's place in the
    /// list is the same on every page, which is what a repeat in progress and
    /// an installed shortcut depend on.
    quint32 page = 1;

    /// **Every field, because a rebuild is skipped when this says equal.**
    /// `QuickButtonBar::setButtons` destroys and recreates every widget, which
    /// is a new size hint for the panel holding them; a field left out here is
    /// a change that never reaches the screen. Same rule as
    /// `ConnectBar::Entry::operator==`.
    bool operator==(const QuickButton &other) const
    {
        return label == other.label && kind == other.kind
               && value == other.value && text == other.text
               && shortcut == other.shortcut && confirm == other.confirm
               && repeat == other.repeat && intervalMs == other.intervalMs
               && page == other.page;
    }
    bool operator!=(const QuickButton &other) const { return !(*this == other); }

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

/// The whole `[Sterna Buttons]` section: the buttons, and what its pages are
/// called.
///
/// One type rather than two calls, so that saving cannot write the buttons and
/// lose the page names — which is the shape that bug takes every time.
struct QuickButtonSet {
    QVector<QuickButton> buttons;
    /// Page names, index 0 being page 1's. An empty entry is a page nobody has
    /// named, which shows as `Page N`; the list stops at the last named page,
    /// so it is not a second answer to how many pages there are.
    QStringList pageNames;

    bool operator==(const QuickButtonSet &other) const
    {
        return buttons == other.buttons && pageNames == other.pageNames;
    }
    bool operator!=(const QuickButtonSet &other) const { return !(*this == other); }

    /// How many pages there are: the highest any button names, the highest
    /// that has a name of its own, and never fewer than one.
    ///
    /// **A named page with nothing on it counts.** That is what lets somebody
    /// make a page and then fill it, and it is why the names are not merely
    /// decoration.
    int pageCount() const;
    /// What page `page` is called, or the default spelling `Page 2`.
    QString pageLabel(int page) const;
};

/// Read the section out of `settingsPath`. A file that is not there is a first
/// run and has no buttons, which is not a failure.
QuickButtonSet loadQuickButtons(const QString &settingsPath);

/// Write it back, replacing `[Sterna Buttons]` and leaving every other line
/// in the file alone.
bool saveQuickButtons(const QString &settingsPath, const QuickButtonSet &set,
                      QString *outError = nullptr);

/// Remove `page`, moving its buttons to the page beside it.
///
/// **Removing a page never removes a command.** The buttons land on the page
/// before it — or, for the first page, on what was the second. Removing a
/// command is its own act and the one that asks first, so this needs no
/// question in front of it.
///
/// It goes through the core rather than editing the vectors here, because the
/// rule for where the pages above end up belongs with the format and not with
/// whichever dialog happened to want it.
QuickButtonSet removeQuickButtonPage(const QuickButtonSet &set, int page);

/// Move a page and everything on it, the way dragging a tab would.
QuickButtonSet moveQuickButtonPage(const QuickButtonSet &set, int from, int to);
