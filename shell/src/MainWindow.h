// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QMainWindow>
#include <QString>

#include "termitta.h"

#include "Session.h"

class QLabel;
class QScrollBar;
class TerminalView;

/// One window, one session.
///
/// Tabs and multiple sessions are Stage 3. Keeping it one-to-one now means the
/// menu actions talk to a member rather than to a notion of a "current"
/// session, which is the thing that has to be threaded through everything
/// later — and threading it through six actions then is cheaper than carrying
/// the indirection through the whole of Stage 1.
class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    MainWindow();

    /// Connect at startup, for the command line.
    void connectSerial(const QString &path, const TtSerialParams &params);
    /// Connect at startup, for the command line. `host` may be an alias from
    /// `~/.ssh/config`; a blank `user` or a zero `port` means "whatever the
    /// config says".
    void connectSsh(const QString &host, const QString &user, int port);

    /// The window's session. Exposed so a test can drive it, and because a
    /// control socket will want it long before tabs make "which session"
    /// an interesting question.
    Session *session() const { return m_session; }

private slots:
    void showConnectDialog();
    void showSshDialog();
    void disconnectPort();
    /// Ask about a host key. Raised from the session's poll, which means a
    /// nested event loop — see `Session::pollSsh` for why that is safe.
    void onSshHostKeyWanted(const HostKeyRequest &request);
    void onSshAuthWanted(const AuthRequest &request);
    void onSshFailed(const QString &error);
    void sendBreak();
    void toggleLogging();
    void chooseFont();
    void onTitleChanged(const QString &title);
    void onNotice(const QString &text);
    void onConnectionChanged();
    /// Track the viewport: the core moves the offset itself to keep a
    /// scrolled-back view on the same lines, so the scrollbar follows the
    /// session rather than the session following the scrollbar.
    void syncScrollBar();

private:
    void buildMenus();
    void updateStatus();
    /// Just the log indicator. Driven by `damaged` rather than by a timer:
    /// the count changes exactly when bytes arrive, and bytes arriving is
    /// what `damaged` means — so the idle path stays free of wakeups, which
    /// is the same reason `Session` has no poll timer.
    void updateLogStatus();

    Session *m_session;
    TerminalView *m_view;
    QScrollBar *m_scroll;
    QLabel *m_status;
    QAction *m_disconnectAction = nullptr;
    QAction *m_breakAction = nullptr;
    QAction *m_logAction = nullptr;
    QLabel *m_logStatus = nullptr;

    // Remembered so reopening the dialog does not start from the defaults
    // again. A session profile on disk is Stage 2's, with the settings schema.
    QString m_lastPort;
    TtSerialParams m_lastParams;
    QString m_lastSshHost;
    QString m_lastSshUser;
    int m_lastSshPort = 0;
    QString m_lastSshIdentity;
    bool m_lastSshLegacy = false;
};
