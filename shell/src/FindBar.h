// The find bar: one search over one terminal's page and scrollback.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QStringList>
#include <QWidget>

#include "Session.h"

class QCheckBox;
class QComboBox;
class QLabel;
class QTimer;
class QToolButton;
class TerminalView;

/// A strip floating over the bottom of one [`TerminalView`].
///
/// **Floating, not stacked.** A bar in the page's layout would take a row from
/// the terminal, and `Session::resize` sends a scrolled-back view live and
/// rewrites `TerminalSize` on the way — so closing the bar would throw away the
/// scrollback position somebody had just searched to, which is the one thing a
/// find feature must not do. Over the last row costs a row of *view* while it
/// is open and nothing at all when it is closed.
///
/// One per terminal rather than one per window, the same argument
/// `PageStatusBar` makes: a search is about one session's history, and a tiled
/// window can be showing nine of them.
///
/// It owns the interaction and asks the session; the window owns only the
/// remembered patterns, because those are shared across tabs.
class FindBar : public QWidget {
    Q_OBJECT

public:
    explicit FindBar(TerminalView *view, Session *session);

    /// Height only — **the bar never decides how wide the terminal is**, the
    /// same rule `PageStatusBar` keeps, or a long pattern would widen the page.
    QSize sizeHint() const override;

    /// Show it and put the keyboard in the field, selecting what is there.
    /// Called again while open, that is what it does — which is what pressing
    /// the shortcut twice means in every other program.
    void open();
    /// Hide it, stop the search, and give the keyboard back to the terminal.
    void close();

    /// The patterns offered by the dropdown, newest first. The window owns the
    /// list because it is remembered across tabs and across runs.
    void setHistory(const QStringList &patterns);
    QStringList history() const { return m_history; }
    /// The three boxes, as the settings left them.
    void setOptions(bool caseSensitive, bool wholeWord, bool regex);

    /// What is in the field and the boxes right now.
    FindQuery query() const;

    /// Step to the next match, or the previous one. Public so a test — and, in
    /// time, a key binding — can drive them without synthesising clicks.
    void findNext() { step(false); }
    void findPrevious() { step(true); }

signals:
    /// A pattern was committed, so it belongs at the top of the remembered
    /// list. Emitted on Enter and on stepping, not on every keystroke: a
    /// history of every prefix somebody typed is not a history.
    void patternUsed(const QString &pattern);
    /// A box was ticked. The window writes the three settings.
    void optionsChanged(bool caseSensitive, bool wholeWord, bool regex);

protected:
    /// Escape closes, whatever has the focus inside the bar.
    void keyPressEvent(QKeyEvent *event) override;

private:
    /// Give the session the pattern in the field, if it does not have it
    /// already. False when the engine refused it, in which case the previous
    /// search is still running and the label says why.
    bool install();
    /// Install, then search from the top of the window and report — everything
    /// that has to happen when the pattern or a box changes.
    void apply();
    /// The debounced half of `apply`, so a pattern that matches nothing does
    /// not scan the whole scrollback once per keystroke.
    void patternEdited();
    /// One step, and what to say when there is nowhere to step to.
    void step(bool backwards);
    /// Select and scroll to a match.
    void showMatch(const FindMatch &match);
    /// `3 of 17`, `No matches`, or the reason the pattern would not compile.
    void report();
    void setStatus(const QString &text, bool problem);
    /// Which match `match` is, and how many there are — in one walk, because
    /// both numbers are about the buffer as it is at this instant. False when
    /// the walk hit its bound, so the label can say "more than" rather than a
    /// number it did not finish working out.
    bool locate(const FindMatch &match, int *ordinal, int *total);

    TerminalView *m_view = nullptr;
    Session *m_session = nullptr;

    QComboBox *m_pattern = nullptr;
    QCheckBox *m_case = nullptr;
    QCheckBox *m_word = nullptr;
    QCheckBox *m_regex = nullptr;
    QLabel *m_status = nullptr;
    QToolButton *m_previous = nullptr;
    QToolButton *m_next = nullptr;

    /// Fires `apply` a moment after typing stops. A pattern nobody has
    /// finished typing usually matches nothing, and "matches nothing" is the
    /// case that costs a pass over the whole history.
    QTimer *m_debounce = nullptr;

    QStringList m_history;
    /// What the session was last given, so typing and stepping cannot install
    /// the same pattern twice — each install throws away where the last search
    /// had got to.
    FindQuery m_applied;
    bool m_haveApplied = false;
    /// Where the last step landed, so the count can say which one it is and
    /// the next step knows where to resume from.
    FindMatch m_current;
    bool m_haveCurrent = false;
};
