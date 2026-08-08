// The C ABI, wrapped in something Qt can connect to.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QString>
#include <QVector>

#include "termitta.h"

class QSocketNotifier;
class QTimer;

/// A host key the `known_hosts` files did not already trust, copied out of the
/// ABI's borrowed strings so a dialog can outlive the poll that produced it.
struct HostKeyRequest {
    QString host;
    int port = 22;
    QString algorithm;
    /// `SHA256:…`, the form every other client prints.
    QString fingerprint;
    TtHostKeyVerdict verdict = TT_HOST_KEY_UNKNOWN;
    /// `path:line` of the entry that disagrees, for `TT_HOST_KEY_CHANGED`.
    QString recordedAt;
    QString recordedFingerprint;
    /// Algorithms already on file, for `TT_HOST_KEY_NEW_ALGORITHM`.
    QString alsoKnown;
};

/// Something that has to be typed before authentication can continue.
struct AuthRequest {
    struct Line {
        QString text;
        /// Whether to show what is typed. The server chooses — a
        /// keyboard-interactive challenge may legitimately want it echoed.
        bool echo = false;
    };
    TtSshAuthKind kind = TT_SSH_AUTH_PASSWORD;
    /// The server's own wording, where it sent any. Usually empty.
    QString name;
    QString instruction;
    /// The key file, for `TT_SSH_AUTH_PASSPHRASE`.
    QString path;
    QVector<Line> lines;
};

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
    /// One row of what is *shown* — the live screen until something scrolls
    /// back, and then history.
    const TtCell *row(int y, size_t *outLen) const;
    TtCursor cursor() const;
    /// Which viewport row to paint the cursor on, or -1 when the view has
    /// scrolled far enough back that the cursor is off the bottom.
    int cursorViewRow() const;

    // --- the viewport -------------------------------------------------------

    int scrollbackLen() const;
    /// Lines scrolled back; 0 is live. **Re-read after every pump** — the core
    /// moves it so a scrolled-back view stays on the same lines.
    int viewOffset() const;
    void setViewOffset(int offset);
    void scrollToBottom() { setViewOffset(0); }

    bool reverseVideo() const;
    TtTracking mouseTracking() const;
    QString title() const;
    /// DECBKM. False means the Backspace key sends DEL rather than BS.
    bool backspaceSendsBs() const;

    // --- the connection -----------------------------------------------------

    bool isConnected() const;
    /// Whether `sendBreak` will do anything. False when nothing is connected,
    /// and false over SSH — which has no break at all.
    bool supportsBreak() const;
    QString describe() const;
    /// Open a serial port. `path` should be a `TtPortInfo::open_path`.
    bool connectSerial(const QString &path, const TtSerialParams &params,
                       QString *outError);
    void disconnectPort();

    // --- ssh ----------------------------------------------------------------
    //
    // Connecting over SSH is a conversation, not a call: the far end asks
    // whether its host key is acceptable and what the password is, and the
    // answers come from a user. So `startSsh` returns as soon as the attempt
    // is under way and the questions arrive as signals; answer them with
    // `answerHostKey` / `answerAuth` whenever the dialog closes.
    //
    // The same `QSocketNotifier` carries the connection and then the session —
    // the core hands out one descriptor for both — so nothing is re-registered
    // at the moment output starts.

    /// Begin connecting. False and `outError` only when the attempt could not
    /// be *started*; everything after that arrives through the signals.
    bool startSsh(const TtSshParams &params, QString *outError);
    /// Give up on a connection in progress. Harmless when there is none.
    void cancelSsh();
    bool isConnecting() const { return m_ssh != nullptr; }
    /// 1 accepts and records, 2 accepts once, anything else refuses.
    void answerHostKey(int decision);
    void answerAuth(const QStringList &answers);

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

    // --- session logging ----------------------------------------------------

    /// Start logging to `path`. Returns false and fills `outError` on failure.
    bool startLog(const QString &path, const TtLogOptions &options, QString *outError);
    void stopLog();
    /// These three const_cast the session pointer, because the ABI's
    /// `tt_session_log_path` caches the string it hands back and so takes a
    /// mutable session. The observable state is not changed.
    bool isLogging() const;
    QString logPath() const;
    quint64 logBytes() const;

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
    /// Logging started, stopped, or was stopped *for* us by a write failure —
    /// which is the case that matters, because a window still claiming to log
    /// lets someone walk away from a capture that ended an hour ago.
    void logStateChanged();

    /// The far end's host key needs a decision. Answer with `answerHostKey`.
    void sshHostKeyWanted(const HostKeyRequest &request);
    /// Something has to be typed. Answer with `answerAuth`.
    void sshAuthWanted(const AuthRequest &request);
    /// The attempt is over and did not succeed. Success arrives as
    /// `connectionChanged` instead.
    void sshFailed(const QString &error);

private slots:
    void onReadable();
    void onRetryPending();

private:
    /// Pump, then turn what came out into signals.
    void pumpAndDispatch(uint32_t budgetMs);
    /// Point the notifier at whatever descriptor the session has *now*, and
    /// run the retry timer only if output is stuck.
    void rearm();
    /// Drain the connection state machine until it has nothing more to say.
    void pollSsh();
    /// Free the handle and stop watching it.
    void endSsh();

    TtSession *m_session = nullptr;
    QSocketNotifier *m_notifier = nullptr;
    QTimer *m_retry = nullptr;
    QString m_title;

    /// The connection being set up, or null.
    TtSshConnect *m_ssh = nullptr;
    /// True between emitting a question and being answered.
    ///
    /// A dialog spins a nested event loop, so the notifier fires again while
    /// one is open. Without this, `pollSsh` re-enters, invalidates the strings
    /// the open dialog is showing, and asks the same question twice.
    bool m_sshWaiting = false;
};
