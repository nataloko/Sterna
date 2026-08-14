// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonRepeat.h"

#include <QTimer>

QuickButtonRepeat::QuickButtonRepeat(QObject *parent) : QObject(parent) {}

int QuickButtonRepeat::find(int index) const
{
    for (int i = 0; i < m_runs.size(); i++) {
        if (m_runs[i].index == index) {
            return i;
        }
    }
    return -1;
}

int QuickButtonRepeat::remaining(int index) const
{
    const int at = find(index);
    return at < 0 ? 0 : m_runs[at].remaining;
}

void QuickButtonRepeat::start(int index, bool withoutEnter, quint32 count,
                              int intervalMs)
{
    stop(index);

    Run run;
    run.index = index;
    run.withoutEnter = withoutEnter;
    if (count == TT_QUICK_BUTTON_REPEAT_FOREVER) {
        run.remaining = -1;
    } else if (count <= 1) {
        // The caller's own send was the whole of it. Zero lands here too: it
        // is not a count anybody means, and reading it as "one less than one"
        // would give -1 — a run with no end, from the value that looks most
        // like none at all.
        return;
    } else {
        run.remaining =
            static_cast<int>(qMin<quint32>(count, TT_QUICK_BUTTON_MAX_REPEAT)) - 1;
    }

    run.timer = new QTimer(this);
    run.timer->setInterval(qBound(static_cast<int>(TT_QUICK_BUTTON_MIN_INTERVAL_MS),
                                  intervalMs,
                                  static_cast<int>(TT_QUICK_BUTTON_MAX_INTERVAL_MS)));
    // The lambda captures `index` and not the run: `fire` reaches the window,
    // which may stop this very run — the button pressed again, the link
    // dropping, a dialog the send put up — so anything holding a pointer or an
    // iterator across that emit is holding a corpse. Everything below looks
    // the run up again afterwards for the same reason.
    connect(run.timer, &QTimer::timeout, this, [this, index] { tick(index); });
    m_runs.append(run);
    run.timer->start();
    emit changed(index, run.remaining);
}

void QuickButtonRepeat::tick(int index)
{
    int at = find(index);
    if (at < 0 || m_runs[at].firing) {
        return;
    }
    m_runs[at].firing = true;
    emit fire(index, m_runs[at].withoutEnter);

    at = find(index);
    if (at < 0) {
        return;
    }
    m_runs[at].firing = false;
    if (m_runs[at].remaining > 0 && --m_runs[at].remaining == 0) {
        stop(index);
        return;
    }
    emit changed(index, m_runs[at].remaining);
}

void QuickButtonRepeat::stop(int index)
{
    const int at = find(index);
    if (at < 0) {
        return;
    }
    QTimer *timer = m_runs[at].timer;
    // Off the list before the timer dies, so a slot on `changed` that asks
    // what is running gets the answer this call is making true.
    m_runs.remove(at);
    if (timer) {
        timer->stop();
        // `deleteLater`: stopping is reachable from inside this timer's own
        // timeout, and deleting a timer while it is delivering one is not.
        timer->deleteLater();
    }
    emit changed(index, 0);
}

void QuickButtonRepeat::stopAll()
{
    const QVector<Run> runs = m_runs;
    m_runs.clear();
    for (const Run &run : runs) {
        if (run.timer) {
            run.timer->stop();
            run.timer->deleteLater();
        }
        emit changed(run.index, 0);
    }
}
