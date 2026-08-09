// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QMainWindow>
#include <QString>

#include "sterna.h"

#include "Session.h"

class Macro;
class QLabel;
class QScrollBar;
class TerminalView;
class XferProgressDialog;

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
    /// `settingsPath` is where this window's settings are read from and
    /// written back to. Empty means [`settingsPath()`], which is the usual
    /// case; a Tera Term command line's `/F=` is the reason it can be
    /// anything else, and it has to be known here because the file is read
    /// before the window is shown.
    explicit MainWindow(const QString &settingsPath = QString());

    /// Do what a Tera Term command line says: apply it to the settings, shape
    /// the window, and open whatever it named.
    ///
    /// The window is shown from here rather than by the caller, because `/V`
    /// means it is not shown at all and `/I` means it opens minimised — and
    /// because the terminal's size has to have settled before the target is
    /// resolved, since that size is what goes out as `NAWS`.
    ///
    /// Takes the parse rather than the arguments: `/F=` has to be read before
    /// this window exists.
    void startFrom(TtCmdLine *cmd);

    /// Connect at startup, for the command line.
    void connectSerial(const QString &path, const TtSerialParams &params);
    /// Connect at startup, for the command line. `host` may be an alias from
    /// `~/.ssh/config`; a blank `user` or a zero `port` means "whatever the
    /// config says".
    void connectSsh(const QString &host, const QString &user, int port);
    /// Connect at startup, for the command line. `params` null means the
    /// defaults for that port with the mode last chosen in the dialog.
    void connectTelnet(const QString &host, quint16 port,
                       const TtTelnetParams *params = nullptr);
    /// Fork a local shell. An empty `argv` runs the user's login shell.
    void connectPty(const QStringList &argv = {});

    /// The window's session. Exposed so a test can drive it, and because a
    /// control socket will want it long before tabs make "which session"
    /// an interesting question.
    Session *session() const { return m_session; }

    /// Where the settings are read from and written to.
    ///
    /// `$XDG_CONFIG_HOME/sterna/sterna.ini` rather than a `TERATERM.INI`
    /// beside the executable, because on Linux that is where a configuration
    /// file belongs and the executable may be inside a read-only AppImage. The
    /// *format* is Tera Term's, which is the part that matters: pointing this
    /// at a real `TERATERM.INI` is a supported thing to do, and `--ini` is how
    /// it will be spelled.
    static QString settingsPath();

private slots:
    void showConnectDialog();
    void showSshDialog();
    void showTelnetDialog();
    void disconnectPort();
    /// Ask about a host key. Raised from the session's poll, which means a
    /// nested event loop — see `Session::pollSsh` for why that is safe.
    void onSshHostKeyWanted(const HostKeyRequest &request);
    void onSshAuthWanted(const AuthRequest &request);
    void onSshFailed(const QString &error);
    /// The far end asked for a terminal size. Honoured, because a console
    /// server saying 132x43 is describing equipment the user cannot see.
    void onRemoteResize(int cols, int rows);
    void sendBreak();
    void sendFile();
    void receiveFile();
    void onTransferProgressed(const TransferProgress &progress);
    void onTransferFinished(const TransferResult &result);
    void toggleLogging();
    /// Ask for a `.ttl` and run it. Upstream's Control > Macro.
    void runMacro();
    /// Upstream's End button, which is on `ttpmacro.exe`'s own control window
    /// there and has to be somewhere here.
    void stopMacro();
    void onMacroFinished(int exitCode);
    void chooseFont();
    void showSettingsDialog();
    /// Write the settings out — upstream's `Setup > Save setup`, and the same
    /// bargain: a change applies to this session immediately and outlives it
    /// only if it is saved.
    void saveSettings();
    /// Re-read everything derived from a setting: the painter's colours, and
    /// the terminal's size.
    void onSettingsChanged();
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
    /// Start `args`' macro and complain in a box if it will not start. The one
    /// place a macro is launched, whichever of the two asked: the menu, or a
    /// `/M=` on the command line.
    void startMacro(const QStringList &args);
    /// Connect what a command line resolved to. The SSH arm goes through the
    /// same state machine the SSH dialog uses, because it has the same
    /// prompts to answer.
    void openTarget(const TtStartup &startup);
    /// The one place an SSH attempt starts, whichever of the three asked for
    /// it: the dialog, `sterna user@host`, or a `/ssh` on the command line.
    void startSsh(const TtSshParams &params, const QString &host);
    /// Say something the user has to see, even under `/V` where there is no
    /// window to say it in.
    void note(const QString &title, const QString &text);
    /// Just the log indicator. Driven by `damaged` rather than by a timer:
    /// the count changes exactly when bytes arrive, and bytes arriving is
    /// what `damaged` means — so the idle path stays free of wakeups, which
    /// is the same reason `Session` has no poll timer.
    void updateLogStatus();

    /// Where the settings came from, and where `Save setup` puts them back.
    /// Not always [`settingsPath()`] — `/F=` names another one.
    QString m_settingsPath;
    /// The title the settings ask for, which is what the window shows until a
    /// host sends an OSC title of its own. Kept so a later settings change can
    /// tell "still ours" from "the host owns it now".
    QString m_baseTitle;
    Session *m_session;
    TerminalView *m_view;
    QScrollBar *m_scroll;
    QLabel *m_status;
    QAction *m_disconnectAction = nullptr;
    QAction *m_breakAction = nullptr;
    QAction *m_logAction = nullptr;
    QAction *m_sendAction = nullptr;
    QAction *m_receiveAction = nullptr;
    QAction *m_stopMacroAction = nullptr;
    /// The macro runner, for this window's lifetime. One at a time — which is
    /// upstream's rule too, since linking a second macro takes the terminal
    /// from the first.
    Macro *m_macro = nullptr;
    /// Where the last macro was chosen from, so the dialog does not start at
    /// the process's working directory every time.
    QString m_lastMacroDir;
    QLabel *m_logStatus = nullptr;
    /// The progress dialog, while one is up. Modeless, and owned here rather
    /// than on the stack: the transfer is driven by this window's event loop,
    /// so a dialog that blocked it would block the transfer it is showing.
    XferProgressDialog *m_xferDialog = nullptr;

    // Remembered so reopening the dialog does not start from the defaults
    // again. A session profile on disk is Stage 2's, with the settings schema.
    QString m_lastPort;
    TtSerialParams m_lastParams;
    QString m_lastSshHost;
    QString m_lastSshUser;
    int m_lastSshPort = 0;
    QString m_lastSshIdentity;
    bool m_lastSshLegacy = false;
    QString m_lastTelnetHost;
    quint16 m_lastTelnetPort = 23;
    TtTelnetMode m_lastTelnetMode = TT_TELNET_NEGOTIATE;
};
