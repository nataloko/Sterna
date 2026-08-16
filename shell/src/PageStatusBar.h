// One terminal's own status line.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QWidget>

class QLabel;
class QTimer;

/// The strip along the bottom of one [`TerminalPage`].
///
/// There is one of these per connection rather than one per window, because
/// everything on it — the name, the link, the recording counter, the message a
/// transfer leaves behind — is a fact about *a* session, and a window can be
/// showing nine of them at once. A single `QMainWindow::statusBar()` saying
/// "connected to router1, REC 4.2 MB" is true of one tile and silent about
/// which, which is exactly the confusion this replaces.
///
/// It doubles as the active-pane marker. That is why the pane header this
/// replaced is gone rather than sitting above it: a tile gets one row of
/// chrome, and it is this one.
class PageStatusBar : public QWidget {
    Q_OBJECT

public:
    explicit PageStatusBar(QWidget *parent = nullptr);

    /// Height only: **the strip never decides how wide the page is.** The
    /// terminal above it does. A status label that quoted its own text as its
    /// width would grow the page's hint the moment a long host name or a
    /// serial device path arrived, and the window would resize on connect.
    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;

    /// What this connection is called: the same string the tab carries.
    void setName(const QString &name);

    /// What the link is doing.
    ///
    /// An enum rather than a run of booleans because the states are exclusive
    /// and a third flag beside `connected`/`connecting` would let a caller
    /// spell a combination that means nothing.
    enum class Link {
        /// Nothing connected, and nothing being done about it. A red chip.
        Down,
        /// An attempt is under way and asking questions — an SSH handshake.
        Connecting,
        /// A serial port that went away is being waited for. Still a red chip:
        /// the link *is* down, and only the words change.
        Reopening,
        /// Connected; the text is `describe()`.
        Up,
    };
    void setConnection(Link state, const QString &text);
    /// `REC <size>`, blinking red, or nothing when this session is not
    /// logging. `Session::damaged` drives the count; a small local timer drives
    /// only the warning blink.
    ///
    /// Paused it says `PAUSED <size>` in amber and stops blinking: a steady
    /// label is the honest shape for a counter that has stopped moving, and
    /// the blink is there to say a recording is running.
    void setLogging(bool logging, quint64 bytes, bool paused = false);

    /// The counter field: how long the connection has been up and how fast
    /// bytes are moving each way, or nothing at all when `on` is false.
    ///
    /// `live` is part of the state rather than something derived from the
    /// numbers — a connection that ended keeps its totals, so the digits alone
    /// cannot say whether it is still running, and the field dims to say so.
    ///
    /// Reached from `Session::damaged` and from the one-second tick, so an
    /// unchanged reading must cost a string compare and nothing else.
    void setCounters(bool on, qint64 connectedMs, quint64 rateIn, quint64 rateOut,
                     bool live);
    /// Whether the counter field is showing. `MainWindow` asks before it reads
    /// the serial control lines, which is a syscall per tab per second.
    bool countersVisible() const;

    /// Say something for `ms` milliseconds, over the name. Upstream's
    /// `QStatusBar::showMessage`, scoped to the terminal it happened in.
    void showMessage(const QString &text, int ms = 5000);
    /// Dismiss `text` if it is still the message being shown. A lifecycle edge
    /// uses this form so it cannot erase a newer notice that arrived while the
    /// operation was in progress.
    void clearMessage(const QString &text);
    QString currentMessage() const;

    /// Paint the highlight that says window-level actions go here. Only ever
    /// true when more than one pane is visible: with one terminal there is
    /// nothing to disambiguate and a highlighted strip is noise.
    void setActive(bool active);

signals:
    /// The recording indicator was clicked.
    ///
    /// Tera Term's Pause button is on its logging window, and this program
    /// deliberately has no such window — the counter in this strip is what
    /// replaced it, so the counter is where the button goes. Only emitted
    /// while something is being counted.
    void logClicked();

    /// The counter field was clicked. The window opens the popover over it —
    /// which is also the only thing that makes the serial control lines get
    /// read at all.
    void countersClicked();

protected:
    /// Re-elide the name: how much of it fits is a function of the width the
    /// tile was just given.
    void resizeEvent(QResizeEvent *event) override;
    /// Re-reserve the counter field's width. The style's font arrives here on
    /// first show, after the constructor has already measured one.
    void changeEvent(QEvent *event) override;
    /// The indicator is a `QLabel`, which has no clicked signal of its own, so
    /// the press is caught here rather than by subclassing one label.
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void applyPalette();
    void applyLogAppearance();
    void showName();
    void elideInto(QLabel *label, const QString &text);
    /// Hold the counter field's width at the widest reading it expects.
    ///
    /// **Not a measurement of the current text.** The only item in this layout
    /// that stretches is the name, so a field whose width followed its digits
    /// would take its pixels out of the host name — which would re-elide every
    /// time a rate crossed from `999` to `1.2k`. A floor rather than a fixed
    /// width, so that a reading past the reservation grows the field instead of
    /// being clipped into a wrong number.
    void reserveCounterWidth();

    QLabel *m_name = nullptr;
    QLabel *m_log = nullptr;
    QLabel *m_counters = nullptr;
    QLabel *m_connection = nullptr;
    QTimer *m_messageTimer = nullptr;
    QTimer *m_logBlinkTimer = nullptr;
    QString m_nameText;
    QString m_connectionText;
    QString m_message;
    bool m_active = false;
    bool m_linkDown = true;
    bool m_logging = false;
    bool m_logPaused = false;
    bool m_logBlinkOn = false;
    bool m_countersOn = false;
    bool m_countersLive = false;
};
