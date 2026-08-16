// The counter field's detail, on a popover over the status strip.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QFrame>

#include "sterna.h"

class QLabel;
class QTimer;
class Session;

/// Everything the status strip's counter field has no room for: bytes each
/// way, lines, breaks, queued output, and — on a serial link only — the four
/// control lines.
///
/// It is a popover rather than four more labels in the strip because a window
/// can be showing nine terminals at once, and four permanently reserved serial
/// captions on an SSH tab is eight tiles' worth of width spent saying nothing.
/// `docs/yat-ideas.md` said as much before this was built.
///
/// **The serial lines are read only while this is on screen.** There is no
/// cache and no notification behind `tt_session_modem_lines`, so a live
/// reading means asking the port: one ioctl on Linux and four kernel calls on
/// Windows, per tab, per second. Its timer therefore runs on `showEvent` and
/// stops on `hideEvent`, which is the same rule `TerminalView::m_repaint` and
/// `Session::m_retry` follow — a timer that exists while something is
/// happening and not otherwise.
class CountersPopover : public QFrame {
    Q_OBJECT

public:
    explicit CountersPopover(QWidget *parent = nullptr);

    /// Read the session and repaint. Safe on a session connected to nothing:
    /// every number reads zero and the serial row goes away, which is the
    /// state an idle tab is in and the state a failed connect leaves behind.
    void refresh(const Session *session);

    /// Show over `anchor`, and read the session at once so the first frame is
    /// never a screenful of zeros.
    ///
    /// `show()` and not `exec()`: a `Qt::Popup` spins no nested event loop, and
    /// a nested loop here would re-enter the session the way a host-key dialog
    /// does. Position is a request only — a Wayland compositor is entitled to
    /// put this somewhere else, so nothing may assert where it landed.
    void popUp(QWidget *anchor, const Session *session);

protected:
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;

private:
    void setLamp(QLabel *label, bool on);

    const Session *m_session = nullptr;
    QTimer *m_poll = nullptr;

    QLabel *m_connected = nullptr;
    QLabel *m_received = nullptr;
    QLabel *m_sent = nullptr;
    QLabel *m_rateIn = nullptr;
    QLabel *m_rateOut = nullptr;
    QLabel *m_lines = nullptr;
    QLabel *m_breaks = nullptr;
    QLabel *m_queued = nullptr;

    /// The separator and the four lamps, shown together or not at all.
    QWidget *m_serial = nullptr;
    QLabel *m_cts = nullptr;
    QLabel *m_dsr = nullptr;
    QLabel *m_cd = nullptr;
    QLabel *m_ri = nullptr;
};
