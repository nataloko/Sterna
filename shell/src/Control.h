// The window's control socket — what DDE used to be.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QStringList>

#include "sterna.h"

class MainWindow;
#ifdef Q_OS_WIN
class QWinEventNotifier;
#else
class QSocketNotifier;
#endif
class Session;

/// This window's `ttctl` socket, and the other half of what it can be asked.
///
/// The same arrangement as `Macro`, one level out: the core owns a listener
/// thread and a thread per client, all of which block; this class waits on the
/// core's descriptor with a `QSocketNotifier` and calls `tt_ctl_service` when
/// it fires, which runs the pending requests **on this thread**. So a
/// `connect` that needs an SSH host-key dialog raises an ordinary modal
/// dialog, and the client is parked on its own thread until it closes — which
/// is what its own blocking request already promised.
///
/// What it answers is nine methods, listed in `crates/tt-ctl/src/dispatch.rs`.
/// Most of them need only the session and never reach this class; what is here
/// is the four that are about the *window* — its macro, its connection, its
/// title and its closing. Windows waits on the same contract through a native
/// event and `QWinEventNotifier`.
class Control : public QObject {
    Q_OBJECT

public:
    /// `window` is what a request acts on. It may not be null: unlike a
    /// macro's dialogs, every callback here is about the window itself, and a
    /// `/V` session with no window still has a `MainWindow` behind it.
    Control(Session *session, MainWindow *window, QObject *parent = nullptr);
    ~Control() override;

    /// Bind and start listening. `name` is a `/D=` topic; empty is the pid.
    ///
    /// False and `outError` when the socket could not be bound — a name
    /// another window already has, or a runtime directory that cannot be
    /// written. **Not fatal**: a window with no control socket is a window,
    /// and refusing to start over one would make a stale file somebody's
    /// morning.
    bool start(const QString &name, QString *outError);

    /// Where it is listening, or empty. Goes into the environment of anything
    /// the window launches, so a shell running inside the terminal can drive
    /// the window it is running in.
    QString path() const { return m_path; }

    /// Follow the window's active tab. The control endpoint is still
    /// window-wide at this layer; changing the borrowed session keeps it from
    /// retaining a page that has been closed.
    void setSession(Session *session) { m_session = session; }

    // --- what a request asks of the window -----------------------------------
    //
    // Called from the ABI's callbacks, on this thread, from inside `service`.
    // Public because the callbacks are free functions with C linkage; nothing
    // else should call them.

    /// Non-zero leaves its message in [`lastError`], which is what the
    /// callback hands back.
    TtStatus runMacro(const QStringList &argv);
    bool macroRunning() const;
    int macroExitCode() const;
    void stopMacro();
    TtStatus connectLine(const QByteArray &line);
    bool closeWindow();
    /// Borrowed until the next call. The core copies it before the callback
    /// that returned it has returned, so a member is enough — a local would
    /// be freed with the callback's frame and read afterwards.
    const char *title();
    const char *lastError() const { return m_error.constData(); }

private slots:
    void onServiceable();

private:
    void service();

    Session *m_session;
    MainWindow *m_window;
    TtCtl *m_ctl = nullptr;
#ifdef Q_OS_WIN
    QWinEventNotifier *m_notifier = nullptr;
#else
    QSocketNotifier *m_notifier = nullptr;
#endif
    QString m_path;
    /// The last message a callback handed back. One socket, one service call
    /// at a time, and the core copies it before the callback returns — so one
    /// buffer is enough and it does not have to outlive the call.
    QByteArray m_error;
    QByteArray m_title;
};
