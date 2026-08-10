// The C ABI, wrapped in something Qt can connect to.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QString>
#include <QVector>

#include "sterna.h"

#ifdef Q_OS_WIN
class QWinEventNotifier;
#else
class QSocketNotifier;
#endif
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

/// Where a running file transfer has got to, copied out of the ABI's borrowed
/// strings so a dialog can hold it.
///
/// **`bytes == 0` does not mean "not started".** The protocols report their
/// own progress and throttle it to ten updates a second (`zmodem.c:197`), so a
/// transfer that finishes quickly finishes having said almost nothing.
struct TransferProgress {
    QString protocol;
    QString file;
    bool sending = true;
    qint64 bytes = 0;
    qint64 packets = 0;
    /// Position in the current file and its size. `total` is 0 when the size
    /// is unknown, which is always true of XMODEM — it never learns one.
    qint64 done = 0;
    qint64 total = 0;
    /// A whole-percent high-water mark, or -1 for "no meaningful bar".
    int percent = 0;
    quint32 elapsedMs = 0;
};

/// How a transfer ended.
struct TransferResult {
    bool success = false;
    bool cancelled = false;
    /// What the protocol said when it failed — "Cannot create file". Often the
    /// only account of the failure there is.
    QString message;
    qint64 bytes = 0;
    quint32 elapsedMs = 0;
};

/// A copied `TtKeyCodeResult`; the ABI's macro path is borrowed and cannot
/// survive the next core call made while the window handles it.
struct KeyCodeAction {
    TtKeyCodeKind kind = TT_KEY_CODE_UNMAPPED;
    quint32 value = 0;
    QString text;
};

/// Owns one `TtSession` and drives its loop from the Qt event loop.
///
/// **There is no timer in the idle path**, which is the whole design of this
/// class. The core hands out a native wakeup when there is something to do;
/// the platform's Qt notifier waits on it, and only then does the session
/// pump. A terminal spends nearly all of its life with nothing
/// arriving, and a window that wakes 60 times a second to discover that is a
/// window that costs battery for no reason.
///
/// Two things a readability wakeup cannot cover, and each has a timer of its
/// own.
///
/// The first is output the far end refused to take. Flow control holds the
/// line, the write comes up short, and the remainder waits for a pump that will
/// never come — because a device asserting backpressure is not sending anything
/// to wake us with. That timer runs *only* while `tt_session_pending_out` is
/// non-zero, and stops the moment the queue drains.
///
/// The second is the line going **quiet**, which is not an event at all. Telnet
/// sends `IAC NOP` after `TelKeepAliveInterval` of silence, and silence is
/// exactly what a readability notifier cannot report — so `tt_session_tick`
/// runs on a once-a-second coarse timer for the life of the window. It is the
/// one wakeup in the idle path, and the cheapest honest way to have that
/// setting mean anything.
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
    /// One line by absolute number, in view or not. Null once it has been
    /// evicted from the scrollback, or before it has been printed.
    ///
    /// A row number is where a line *is* and changes with every line the host
    /// prints; this is which line it *is*, and does not. Anything that has to
    /// survive output — the selection — holds one of these.
    const TtCell *line(quint64 n, size_t *outLen) const;
    /// The absolute number of the line at viewport row `y`.
    quint64 lineAt(int y) const;
    /// The URL-marked run containing one cell, or an empty string when the
    /// cell is not a URL. `line` is absolute, like `line()`.
    QString urlAt(quint64 line, int x);
    /// The absolute number of the top line of the *live* page.
    quint64 topLine() const;
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
    /// One entry from this terminal's live palette. Unlike the ABI's
    /// sessionless fallback, the first sixteen entries reflect `ANSIColor`.
    bool paletteRgb(uint32_t index, uint8_t *r, uint8_t *g, uint8_t *b) const;
    TtTracking mouseTracking() const;
    /// Whether a wheel notch belongs to the host as a cursor key rather than
    /// to the window's own scrollback. Four terms deep on the core's side, so
    /// ask rather than assemble it — and ask only after `mouse()` has declined
    /// the wheel.
    bool wheelToCursor(TtModifiers mods) const;
    QString title() const;
    /// DECBKM. False means the Backspace key sends DEL rather than BS.
    bool backspaceSendsBs() const;

    // --- the connection -----------------------------------------------------

    bool isConnected() const;
    /// Whether `sendBreak` will do anything. False when nothing is connected,
    /// and false over SSH — which has no break at all.
    bool supportsBreak() const;
    /// What is attached — the nearest thing to upstream's `cv.PortType`.
    ///
    /// Several of Tera Term's settings are conditioned on it rather than on
    /// anything the transport does: `ConfirmDisconnect` asks only about a TCP
    /// session, and so does `BeepOnConnect`.
    TtLinkKind linkKind() const;
    QString describe() const;
    /// The address-side pieces Tera Term's `TitleFormat` presents. They are
    /// retained across a disconnect, although the formatter hides them then.
    QString connectionHost() const { return m_connectionHost; }
    quint16 connectionPort() const { return m_connectionPort; }
    quint32 serialBaud() const;
    /// Open a serial port. `path` should be a `TtPortInfo::open_path`.
    bool connectSerial(const QString &path, const TtSerialParams &params,
                       QString *outError);
    void disconnectPort();
    /// Open a telnet (or raw TCP) connection. Synchronous, unlike SSH: telnet
    /// asks no questions — a login prompt is terminal output, not a dialog.
    bool connectTelnet(const QString &host, quint16 port,
                       const TtTelnetParams &params, QString *outError);
    /// Fork a shell onto a local pty. `argv` empty runs the user's login
    /// shell, which is what the menu item does.
    bool connectPty(const QStringList &argv, QString *outError);
    /// Why the last connection ended, when there is more to say than
    /// "disconnected" — "bash exited with status 1". Empty otherwise, which is
    /// the usual case.
    QString closeNote() const;

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
    /// Load a Tera Term `KEYBOARD.CNF`. Duplicate legacy scan codes are copied
    /// to `duplicates` so the window can report them without parsing the file.
    bool loadKeyMap(const QString &path, QVector<quint16> *duplicates,
                    QString *outError);
    /// Dispatch one PC/AT set-1 scan code, modifier bits included.
    KeyCodeAction sendKeyCode(quint16 scan);
    void sendText(const QString &text);
    /// Raw bytes: no UTF-8 encoding or LNM. `Meta8Bit=raw` is the frontend
    /// caller; macro binary sends use the same core path directly.
    void sendBytes(const QByteArray &bytes);
    void paste(const QString &text);
    /// Returns whether the terminal consumed it; if not, the click belongs to
    /// the frontend and means selection.
    bool mouse(TtMouseEvent event, uint8_t button, int px, int py,
               TtModifiers mods);
    void focus(bool focused);
    void resize(int cols, int rows);
    void setCellPixels(int w, int h);
    /// How long the line is held is `SendBreakTime`'s, so there is nothing to
    /// pass — see `tt_session_send_break`.
    void sendBreak();

    // --- session logging ----------------------------------------------------

    /// Start logging to `path` with whatever the settings say — the mode, the
    /// timestamp, appending and rotation are all `TERATERM.INI` keys, and a
    /// window that assembled its own would be a second copy of the schema.
    /// Returns false and fills `outError` on failure.
    bool startLog(const QString &path, QString *outError);
    /// The file a log would be opened under: `requested` expanded against the
    /// template rules, or `LogDefaultName` when it is empty.
    ///
    /// **Ask for it at the moment it is needed.** A name holding `%H` or `&h`
    /// is an expansion of the clock and the connection, not a constant.
    QString logName(const QString &requested = QString()) const;
    void stopLog();
    /// These three const_cast the session pointer, because the ABI's
    /// `tt_session_log_path` caches the string it hands back and so takes a
    /// mutable session. The observable state is not changed.
    bool isLogging() const;
    QString logPath() const;
    quint64 logBytes() const;

    // --- file transfer ------------------------------------------------------
    //
    // The terminal is deaf and mute while one runs: keystrokes are dropped and
    // the protocol's traffic never reaches the parser. That is the core's
    // doing, not the window's, and it is what upstream's modal transfer dialog
    // achieves by other means — so a window that forgets to disable its input
    // is untidy rather than dangerous.

    /// Send files. False and `outError` when it could not be started at all.
    bool sendFiles(const TtXferJob &job, const QStringList &paths, QString *outError);
    /// Receive into `dir`. `name` is XMODEM's alone — its wire format carries
    /// no filename — and is ignored by every other protocol, which is what
    /// upstream's receive dialog does with the same field.
    bool receiveFiles(const TtXferJob &job, const QString &dir, const QString &name,
                      QString *outError);
    /// Ask it to stop. It does not stop here: the protocol sends its cancel
    /// sequence and finishes on its own terms, so wait for `transferFinished`.
    void cancelTransfer();
    bool isTransferring() const;
    /// Where it has got to. Only meaningful while `isTransferring`.
    TransferProgress transferProgress() const;

    /// Feed bytes as though they had arrived from the far end.
    void feed(const QByteArray &bytes);
    /// Handle Shift+Escape. False leaves Escape available to ordinary key
    /// encoding because debug cycling is disabled in the settings.
    bool cycleDebugMode();

    // --- macros -------------------------------------------------------------

    /// The ABI handle, for the one caller that needs it directly.
    ///
    /// `tt_macro_service` takes the session rather than remembering it — the
    /// same rule `tt_ssh_connect_poll` follows and for the same reason — so a
    /// macro has to be handed the pointer this class owns. Nothing else should
    /// want this; everything else here is a method.
    TtSession *handle() const { return m_session; }
    /// Detach a macro from the terminal, so it stops collecting output for a
    /// ring nobody reads.
    void unlinkMacro();
    /// Pump once and turn what came out into signals — what the notifier does,
    /// exposed for the one thing that changes the session from outside its own
    /// descriptor. A macro's `send`, `connect` and `dispstr` happen on the
    /// *macro's* wakeup, and without this nothing would repaint, nothing would
    /// notice the transport had changed, and the notifier would still be
    /// watching the descriptor of a connection that has gone.
    void poll();

    // --- settings -----------------------------------------------------------
    //
    // Addressed by name, because the list of settings lives in the core's
    // schema and nothing here should hold a second copy of it. `SettingsDialog`
    // walks `tt_settings_field` and asks for these by the names it finds.

    /// One setting, in the INI's own spelling. Empty for a name the schema
    /// does not have.
    QString setting(const QString &name) const;
    /// What a double-click stops at — `DelimList`, decoded out of the `$xx`
    /// escape it is stored in. Not reachable through `setting`, which gives
    /// the file's own spelling and would hand back a list with no space in it.
    QString wordDelimiters() const;
    /// Set one and apply it to the running terminal. The value is parsed the
    /// way the file would parse it, so an out-of-range number is corrected
    /// rather than refused.
    bool setSetting(const QString &name, const QString &value, QString *outError);
    /// Read a `TERATERM.INI` and apply all of it. A file that is not there is
    /// a first run, not a failure.
    bool loadSettings(const QString &path, QString *outError);
    /// Write every setting back, leaving comments, ordering and any setting
    /// this project does not know about alone.
    bool saveSettings(const QString &path, QString *outError) const;
    /// The same save, with the live position of the window that owns this
    /// session. `positionValid` is false on Wayland, where a client does not
    /// own a position and a reported `(0,0)` must not replace a useful line.
    bool saveSettingsForWindow(const QString &path, int x, int y,
                               bool positionValid, QString *outError) const;
    /// Upstream's close-time `SaveVTPos`: when enabled, touch only VTPos and
    /// TerminalSize rather than pinning every schema value into the file.
    bool saveWindowGeometry(const QString &path, int x, int y,
                            bool positionValid, QString *outError) const;

    // --- the command line ---------------------------------------------------

    /// Write a parsed Tera Term command line into the settings, and through
    /// them into the running terminal. Call it once, after the settings file
    /// has been loaded — which is upstream's order, since `_ParseParam` writes
    /// `ts` and everything downstream reads `ts` back.
    bool applyCommandLine(TtCmdLine *cmd, QString *outError);
    /// What that command line says to open — `OnCommStart`'s answer, which is
    /// one of five and only one of which is a connection.
    ///
    /// The terminal's current size goes into the target, so ask once the
    /// window has settled: that size is what goes out as `NAWS`.
    TtStartupKind startup(TtCmdLine *cmd, TtStartup *out);

signals:
    /// The screen changed and wants repainting.
    void damaged();
    /// The terminal wants a bell. `visual` is `bell.mode` being `visual`:
    /// invert the screen for `bell.visual_wait_ms` rather than making a noise.
    ///
    /// Already governed by the core, which has thinned a runaway host down to
    /// the bells Tera Term would have sounded — so this wants no rate limit of
    /// its own.
    void bellRang(bool visual);
    void titleChanged(const QString &title);
    /// Something worth saying in the status bar — a break, a corrupt byte, a
    /// failed write.
    void notice(const QString &text);
    /// Connecting, connected, disconnected, or dropped by the far end.
    void connectionChanged();
    /// `AutoWinClose` after a network connection ended. The window decides
    /// whether it can close now; serial and local-pty sessions never ask.
    void closeRequested();
    /// A setting changed, so anything derived from one is stale — the colours
    /// the painter resolves with, and the terminal's size. Emitted once per
    /// applied change rather than per field, since the dialog applies on OK.
    void settingsChanged();

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

    /// A file transfer moved. Emitted once per pump while one is running.
    void transferProgressed(const TransferProgress &progress);
    /// A file transfer ended, for any reason: finished, cancelled, refused by
    /// the peer, or cut off by the connection going away.
    void transferFinished(const TransferResult &result);

    /// The **far end** says the terminal should be this size — telnet's NAWS,
    /// arriving backwards from a console server describing the equipment
    /// behind it. Nothing has resized yet: the window owns its own size.
    void remoteResize(int cols, int rows);

private slots:
    void onReadable();
    void onRetryPending();
    void onTransferDeadline();

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
    /// Tell the core what a log name's `&h` and `&p` expand to.
    void setConnectionName(const QString &host, quint16 port);

    TtSession *m_session = nullptr;
#ifdef Q_OS_WIN
    QWinEventNotifier *m_notifier = nullptr;
#else
    QSocketNotifier *m_notifier = nullptr;
#endif
    QTimer *m_retry = nullptr;
    /// Runs only while a transfer is up.
    ///
    /// **The descriptor is not enough for a transfer.** The protocols retry by
    /// timeout — an XMODEM receiver that hears nothing re-sends its `NAK`
    /// after ten seconds — and a line that has gone quiet produces no wakeup
    /// at all, so nothing would ever fire it. This is the second and last
    /// timer in the class, and like the first it exists for a case a
    /// descriptor genuinely cannot cover.
    QTimer *m_xferTimer = nullptr;
    /// The third and last timer, and the only one that runs while nothing is
    /// happening: `tt_session_tick`, which is what lets a transport act on the
    /// line having gone *quiet*. See `kTickIntervalMs` for why it is cheap.
    QTimer *m_tick = nullptr;
    QString m_title;
    QString m_connectionHost;
    quint16 m_connectionPort = 0;

    /// Read the window title back from the core and emit an edge.
    ///
    /// A settings change moves it without a byte arriving: the title is
    /// `terminal.title` and whatever the host set, combined the way
    /// `window.title_change` says, so two of the three inputs are settings.
    /// The core only looks for the edge when it pumps, which a dialog's OK
    /// button does not.
    void refreshTitle();

    /// The connection being set up, or null.
    TtSshConnect *m_ssh = nullptr;
    /// True between emitting a question and being answered.
    ///
    /// A dialog spins a nested event loop, so the notifier fires again while
    /// one is open. Without this, `pollSsh` re-enters, invalidates the strings
    /// the open dialog is showing, and asks the same question twice.
    bool m_sshWaiting = false;
};
