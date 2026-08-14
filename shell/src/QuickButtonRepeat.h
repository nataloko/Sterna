// The clock behind a button that sends more than once.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QVector>

#include "sterna.h"

class QTimer;

/// The runs in progress: which buttons are repeating, how many sends each has
/// left, and when the next one is due.
///
/// It lives here rather than in the core for the same reason the bell's
/// governor does: the engine is a function of its bytes, and a repeat is a
/// function of the clock. The core says what a button was *asked* for; this
/// decides when, and the window does the sending.
///
/// It holds indices into the bar's list and no buttons, so anything that can
/// renumber that list has to stop every run — `MainWindow::reloadQuickButtons`
/// does, rather than trying to follow a button that may have moved, been
/// renamed, or stopped being the same command.
class QuickButtonRepeat : public QObject {
    Q_OBJECT

public:
    explicit QuickButtonRepeat(QObject *parent = nullptr);

    /// Begin a run for `index`, **after** the caller has made the first send.
    ///
    /// `count` is the total including that send, or
    /// `TT_QUICK_BUTTON_REPEAT_FOREVER`; a count of one therefore schedules
    /// nothing, and neither does starting a run that is already going — the
    /// old one is stopped first, so a press can never leave two clocks on one
    /// button.
    ///
    /// `withoutEnter` is the Shift+click variant, carried so that every send
    /// in a run is the one that was asked for rather than the plain form.
    void start(int index, bool withoutEnter, quint32 count, int intervalMs);
    void stop(int index);
    void stopAll();
    bool isRunning(int index) const { return find(index) >= 0; }
    /// Sends still to come for `index`: -1 for a run with no end, and 0 when
    /// it is not running at all.
    int remaining(int index) const;
    bool isIdle() const { return m_runs.isEmpty(); }

signals:
    /// Send it again.
    void fire(int index, bool withoutEnter);
    /// A run started, ticked or ended — `remaining` is 0 for ended. The bar's
    /// face and the terminal's stop key follow this.
    void changed(int index, int remaining);

private:
    struct Run {
        int index = -1;
        bool withoutEnter = false;
        /// -1 for a run with no end.
        int remaining = 0;
        /// True across the `fire` for this run. A send can spin a nested event
        /// loop — a message box, a modal prompt — and a level-triggered timer
        /// will happily fire again inside it, which would spend two sends'
        /// worth of the count on one interval.
        bool firing = false;
        QTimer *timer = nullptr;
    };
    int find(int index) const;
    void tick(int index);

    QVector<Run> m_runs;
};
