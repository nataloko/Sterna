// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QMainWindow>
#include <QString>

#include "sterna.h"

#include "Session.h"

class Control;
class I18n;
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
class Printer;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    /// `settingsPath` is where this window's settings are read from and
    /// written back to. Empty means [`settingsPath()`], which is the usual
    /// case; a Tera Term command line's `/F=` is the reason it can be
    /// anything else, and it has to be known here because the file is read
    /// before the window is shown.
    explicit MainWindow(const QString &settingsPath = QString());

    /// Tear the macro down before Qt gets to the children.
    ///
    /// `QObjectPrivate::deleteChildren` deletes in the order they were
    /// created, and the session is created first — so with no destructor here
    /// the session is freed and *then* `~Macro` calls `unlinkMacro` on it.
    /// A use-after-free on every window that closes with a macro still
    /// running, which is a script that outlives its window and the End button
    /// not having been pressed. It presented as an intermittent
    /// `malloc_consolidate(): unaligned fastbin chunk detected` in CI, in a
    /// test that passes locally, because writing into freed memory only
    /// corrupts the heap once something else has claimed it.
    ~MainWindow() override;

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

    // --- what the control socket asks of the window --------------------------
    //
    // The same four things the menu can do, said without a person: run a
    // macro, stop it, open a connection, close the window. They return their
    // errors rather than putting them in a box, because the thing that asked
    // is on the other end of a socket and a message box would be shown to
    // nobody — and would block the window until somebody found it.

    /// Start a macro. `args` is `ttpmacro`'s command line, already split.
    ///
    /// `outBusy` distinguishes the one refusal worth retrying, which is what
    /// the socket reports as its own error code.
    bool runMacroFile(const QStringList &args, QString *outError, bool *outBusy);
    bool macroRunning() const;
    /// The last `setexitcode`, or 0.
    int macroExitCode() const;
    /// Ask the running macro to stop — upstream's End button, which is on
    /// `ttpmacro.exe`'s own control window there and has to be somewhere here.
    /// Public because the socket asks for it too, and it is the same request.
    void stopMacro();
    /// Open what a Tera Term command line describes — a macro's `connect`
    /// argument, which is a command line with no program name in it.
    ///
    /// Answering means the attempt has *started*: an SSH target goes through
    /// the same state machine and the same dialogs as everything else here,
    /// and those are answered by a person.
    bool openCommandLine(const QByteArray &line, QString *outError);

    /// The control socket, or null when none was bound. Its path goes into
    /// the environment of anything this window launches.
    Control *control() const { return m_control; }

    /// Bind the control socket under `name` — a `/D=` topic, or empty for
    /// this process's pid. Called once from the constructor and again from
    /// [`startFrom`] when a command line named a topic.
    void startControl(const QString &name);

    /// Where the settings are read from and written to.
    ///
    /// `$XDG_CONFIG_HOME/sterna/sterna.ini` rather than a `TERATERM.INI`
    /// beside the executable, because on Linux that is where a configuration
    /// file belongs and the executable may be inside a read-only AppImage. The
    /// *format* is Tera Term's, which is the part that matters: pointing this
    /// at a real `TERATERM.INI` is a supported thing to do, and `--ini` is how
    /// it will be spelled.
    static QString settingsPath();

protected:
    /// Switch between `AlphaBlendActive` and `AlphaBlend` when the desktop
    /// activates or deactivates this top-level window.
    bool event(QEvent *event) override;
    /// Ask before closing a window with a live TCP session on it —
    /// `ConfirmDisconnect`, which is on by default.
    void closeEvent(QCloseEvent *event) override;

private:
    /// Tell the terminal what its window is, for the XTWINOPS reports.
    ///
    /// Pushed on every move, resize and window-state change rather than asked
    /// for on demand: the reply to `CSI 14 t` is composed while the sequence
    /// is being parsed, and there is nowhere in there to call into Qt.
    void pushWindowMetrics();

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
    /// `CSI 1`-`10 t`: the host asked the window to move, resize in pixels,
    /// iconify, raise, lower, repaint or maximise.
    void onWindowOperation(const TtWindowRequest &request);
    void sendBreak();
    void sendFile();
    void receiveFile();
    void onTransferProgressed(const TransferProgress &progress);
    void onTransferFinished(const TransferResult &result);
    void toggleLogging();
    /// Ask for a `.ttl` or a `.lua` and run it. Upstream's Control > Macro.
    void runMacro();
    void onMacroFinished(int exitCode);
    void chooseFont();
    void chooseKeyMap();
    void showSettingsDialog();
    /// Write the settings out — upstream's `Setup > Save setup`, and the same
    /// bargain: a change applies to this session immediately and outlives it
    /// only if it is saved.
    void saveSettings();
    /// Open the ordinary menu tree as one popup. This is upstream's
    /// Ctrl+left-click replacement for a menu bar hidden by `PopupMenu` or
    /// `HideTitle`.
    void showPopupMenu(const QPoint &globalPos);
    /// Re-read everything derived from a setting: the painter's colours, and
    /// the terminal's size.
    void onSettingsChanged();
    void onTitleChanged(const QString &title);
    /// Put a title in the title bar, applying `TitleFormat` and substituting
    /// this program's name for upstream's `Title=` default.
    void showTitle(const QString &title);
    void onNotice(const QString &text);
    void onConnectionChanged();
    /// Track the viewport: the core moves the offset itself to keep a
    /// scrolled-back view on the same lines, so the scrollbar follows the
    /// session rather than the session following the scrollbar.
    void syncScrollBar();

private:
    void buildMenus();
    void updateStatus();
    /// Apply one of the two 0..255 opacity settings as Qt's 0.0..1.0 value.
    void applyWindowOpacity(bool active);
    /// Apply `VTPos` once, after the settings file is loaded and before a
    /// command line gets its later chance to override it with `/X` and `/Y`.
    void applySavedPosition();
    /// `ConfirmDisconnect` (`ttset.c:1154`, on by default): whether to go ahead
    /// with dropping the connection.
    ///
    /// **TCP only**, which is upstream's condition and not a simplification —
    /// both tests are `cv.PortType==IdTCPIP` (`vtwin.cpp:1668`, `:4448`), so a
    /// serial session closes without a word however this is set. The reasoning
    /// is visible once stated: reopening a serial port costs nothing, and
    /// reopening a session on a router four hops away costs a login.
    bool confirmDisconnect();
    /// Start `args`' macro and complain in a box if it will not start. The one
    /// place a macro is launched, whichever of the two asked: the menu, or a
    /// `/M=` on the command line.
    void startMacro(const QStringList &args);
    /// Start a macro named by `StartupMacro` or `/M=`. Relative names resolve
    /// beside the active settings file, Sterna's stable equivalent of
    /// upstream's process-wide `HomeDirW`; a leading `*` asks the user.
    void startNamedMacro(const QString &name);
    /// Install one `KEYBOARD.CNF` and report unreadable or duplicate entries.
    void loadKeyMap(const QString &path);
    /// A type-3 user key's upstream menu id, for the actions this window has.
    void invokeMenuCommand(quint16 command);
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
    /// Select the catalog named by `settings.language_file`. Missing catalogs
    /// leave the source-language UI in place, as upstream's defaults do.
    void reloadLanguage();
    /// Resolve the language keys attached to the menu actions. The key lives
    /// on each action as data, leaving the menu structure as the only list.
    void translateMenus();

    /// Where the settings came from, and where `Save setup` puts them back.
    /// Not always [`settingsPath()`] — `/F=` names another one.
    QString m_settingsPath;
    /// The active `KEYBOARD.CNF`, for reopening the file picker in its folder.
    QString m_keyMapPath;
    I18n *m_i18n = nullptr;
    QString m_languageSetting;
    Session *m_session = nullptr;
    /// The other end of the media-copy sequences, and of File > Print.
    Printer *m_printer = nullptr;
    TerminalView *m_view;
    QScrollBar *m_scroll;
    QLabel *m_status;
    QAction *m_disconnectAction = nullptr;
    QAction *m_serialConnectAction = nullptr;
    QAction *m_sshConnectAction = nullptr;
    QAction *m_telnetConnectAction = nullptr;
    QAction *m_localShellAction = nullptr;
    QAction *m_breakAction = nullptr;
    QAction *m_logAction = nullptr;
    QAction *m_sendAction = nullptr;
    QAction *m_receiveAction = nullptr;
    QAction *m_stopMacroAction = nullptr;
    /// The macro runner, for this window's lifetime. One at a time — which is
    /// upstream's rule too, since linking a second macro takes the terminal
    /// from the first.
    Macro *m_macro = nullptr;
    /// This window's `ttctl` socket, for its lifetime. Null when it could not
    /// be bound, which is not fatal — a window with no way in is still a
    /// window.
    Control *m_control = nullptr;
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
