// The other end of the media-copy sequences.
//
// `tt-vt` decides *what* to print and hands out an ordered list of jobs —
// `Open`, some `Write`s, a `Close` — because a VT engine has no printer any
// more than it has a window. This is the half that has one. It is upstream's
// `teraprn.cpp` minus the parts that were Win32 by necessity rather than by
// choice: the spool file becomes a `QString`, `GetTempFileNameW` and the
// `WriteFile`/`ReadFile` round trip disappear with it, and `PrnOpen`/`PrnWrite`
// become a `QFile` or a `QPrinter`.
//
// Two destinations, and `PassThruPort` chooses between them exactly as
// `PrnFileStart` does: a named device gets the bytes as they were sent, and an
// empty setting means the platform's own printer with the configured font and
// margins. The delay before either starts is `PassThruDelay`, and it is real —
// upstream sets that timer in `ClosePrnFile` so a host that closes and reopens
// a job in quick succession does not start a print per line.
#pragma once

#include <QObject>
#include <QString>
#include <QStringList>

#include "sterna.h"

class QTimer;
class Session;

class Printer : public QObject {
    Q_OBJECT

public:
    explicit Printer(Session *session, QObject *parent = nullptr);
    ~Printer() override;

    /// One event out of `tt_session_printer_events`, in order.
    void handle(const TtPrinterEvent &event);

    /// Print the visible screen. `CSI 0 i` arrives here, and so does
    /// File > Print — upstream's menu command and its escape sequence are the
    /// same `BuffPrint` call, differing only in which rectangle they ask for.
    void printScreen(bool scrollRegion);

    /// Whether a job is accumulating. The window asks so it can say so.
    bool busy() const { return m_open || m_pending; }

    /// Print now instead of waiting out `PassThruDelay`. Only the tests want
    /// this; a person waiting three seconds for a page is upstream's design.
    void flushNow();

signals:
    /// Something the user has to know: the device could not be opened, or the
    /// job went nowhere. Printing is not a request the host can see fail, so
    /// this is the only channel there is.
    void notice(const QString &message);

private:
    void start();

    Session *m_session = nullptr;
    QTimer *m_timer = nullptr;
    /// The job being filled — upstream's spool file, which holds code points
    /// rather than the printer's bytes for the same reason `PrnBuff` is a
    /// `char32_t` array.
    QString m_job;
    /// A job that closed and is waiting out the delay.
    QString m_pendingJob;
    bool m_open = false;
    bool m_pending = false;
};
