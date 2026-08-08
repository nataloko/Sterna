// The C ABI, wrapped in something Qt can connect to.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QString>

#include "termitta.h"

class QSocketNotifier;
class QTimer;

/// Owns one `TtSession` and drives its loop from the Qt event loop.
///
/// **There is no timer in the idle path**, which is the whole design of this
/// class. The core hands out a descriptor that becomes readable when there is
/// something to do; a `QSocketNotifier` waits on it, and only then does the
/// session pump. A terminal spends nearly all of its life with nothing
/// arriving, and a window that wakes 60 times a second to discover that is a
/// window that costs battery for no reason.
///
/// The one thing a descriptor cannot cover is output the far end refused to
/// take. Flow control holds the line, the write comes up short, and the
/// remainder waits for a pump that will never come — because a device
/// asserting backpressure is not sending anything to wake us with. So there is
/// a second timer that runs *only* while `tt_session_pending_out` is non-zero,
/// and stops the moment the queue drains.
class Session : public QObject {
    Q_OBJECT

public:
    Session(int cols, int rows, QObject *parent = nullptr);
    ~Session() override;

    // --- reading the screen -------------------------------------------------
    //
    // Everything here is borrowed from the grid and dies at the next call that
    // can change it. Read a row, paint it, then pump — not the other way
    // round.

    int cols() const;
    int rows() const;
    const TtCell *row(int y, size_t *outLen) const;
    TtCursor cursor() const;
    bool reverseVideo() const;
    TtTracking mouseTracking() const;
    QString title() const;
    /// DECBKM. False means the Backspace key sends DEL rather than BS.
    bool backspaceSendsBs() const;

    // --- the connection -----------------------------------------------------

    bool isConnected() const;
    QString describe() const;
    /// Open a serial port. `path` should be a `TtPortInfo::open_path`.
    bool connectSerial(const QString &path, const TtSerialParams &params,
                       QString *outError);
    void disconnectPort();

    // --- input --------------------------------------------------------------

    void sendKey(TtKey key);
    void sendText(const QString &text);
    void paste(const QString &text);
    /// Returns whether the terminal consumed it; if not, the click belongs to
    /// the frontend and means selection.
    bool mouse(TtMouseEvent event, uint8_t button, int px, int py,
               TtModifiers mods);
    void focus(bool focused);
    void resize(int cols, int rows);
    void setCellPixels(int w, int h);
    void sendBreak(int ms);
    /// Feed bytes as though they had arrived from the far end.
    void feed(const QByteArray &bytes);

signals:
    /// The screen changed and wants repainting.
    void damaged();
    void titleChanged(const QString &title);
    /// Something worth saying in the status bar — a break, a corrupt byte, a
    /// failed write.
    void notice(const QString &text);
    /// Connected, disconnected, or dropped by the far end.
    void connectionChanged();

private slots:
    void onReadable();
    void onRetryPending();

private:
    /// Pump, then turn what came out into signals.
    void pumpAndDispatch(uint32_t budgetMs);
    /// Point the notifier at whatever descriptor the session has *now*, and
    /// run the retry timer only if output is stuck.
    void rearm();

    TtSession *m_session = nullptr;
    QSocketNotifier *m_notifier = nullptr;
    QTimer *m_retry = nullptr;
    QString m_title;
};
