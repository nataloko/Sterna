// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "MainWindow.h"

#include <QAction>
#include <QFontDialog>
#include <QLabel>
#include <QHBoxLayout>
#include <QMenuBar>
#include <QMessageBox>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QFileDialog>
#include <QFileInfo>
#include <QLocale>
#include <QStatusBar>
#include <QTimer>

#include <QDir>
#include <QStandardPaths>

#include <cstdio>
#include <cstring>

#include "Control.h"
#include "Macro.h"
#include "SerialDialog.h"
#include "Session.h"
#include "SettingsDialog.h"
#include "SshDialog.h"
#include "SshPrompts.h"
#include "TelnetDialog.h"
#include "TerminalView.h"
#include "XferDialog.h"

namespace {

/// What the schema says a setting ships as.
///
/// Read out of the table rather than written down a second time here, which is
/// the whole point of there being a table: a default that is duplicated in the
/// frontend is a default that changes in one place.
QString settingDefault(const char *name)
{
    for (size_t i = 0, n = tt_settings_field_count(); i < n; i++) {
        TtSettingField f;
        if (tt_settings_field(i, &f) && f.name && strcmp(f.name, name) == 0) {
            return QString::fromUtf8(f.default_value);
        }
    }
    return {};
}

} // namespace

MainWindow::MainWindow(const QString &settingsPath)
    : m_settingsPath(settingsPath.isEmpty() ? MainWindow::settingsPath()
                                            : settingsPath)
{
    m_session = new Session(80, 24, this);
    m_view = new TerminalView(m_session, this);

    // A plain QWidget plus a scrollbar rather than a QAbstractScrollArea: the
    // painter draws straight onto the widget in cell coordinates, and a scroll
    // area would add a viewport child and a coordinate translation to hold a
    // scrollbar we can place in a layout for nothing.
    m_scroll = new QScrollBar(Qt::Vertical, this);
    auto *central = new QWidget(this);
    auto *layout = new QHBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(m_view, 1);
    layout->addWidget(m_scroll);
    setCentralWidget(central);

    connect(m_view, &TerminalView::viewChanged, this, &MainWindow::syncScrollBar);
    connect(m_scroll, &QScrollBar::valueChanged, this, [this](int value) {
        // The scrollbar counts down from the top of the history; the session
        // counts back from the live screen. One subtraction, in one place.
        m_view->setViewOffset(m_scroll->maximum() - value);
    });
    syncScrollBar();

    tt_serial_params_default(&m_lastParams);

    m_logStatus = new QLabel(this);
    statusBar()->addPermanentWidget(m_logStatus);
    m_status = new QLabel(this);
    statusBar()->addPermanentWidget(m_status);

    connect(m_session, &Session::logStateChanged, this, &MainWindow::updateStatus);
    connect(m_session, &Session::damaged, this, &MainWindow::updateLogStatus);

    connect(m_session, &Session::titleChanged, this, &MainWindow::onTitleChanged);
    connect(m_session, &Session::notice, this, &MainWindow::onNotice);
    connect(m_session, &Session::connectionChanged, this,
            &MainWindow::onConnectionChanged);
    connect(m_session, &Session::sshHostKeyWanted, this,
            &MainWindow::onSshHostKeyWanted);
    connect(m_session, &Session::sshAuthWanted, this, &MainWindow::onSshAuthWanted);
    connect(m_session, &Session::sshFailed, this, &MainWindow::onSshFailed);
    connect(m_session, &Session::remoteResize, this, &MainWindow::onRemoteResize);
    connect(m_session, &Session::settingsChanged, this, &MainWindow::onSettingsChanged);
    connect(m_session, &Session::transferProgressed, this,
            &MainWindow::onTransferProgressed);
    connect(m_session, &Session::transferFinished, this,
            &MainWindow::onTransferFinished);

    m_macro = new Macro(m_session, this, this);
    connect(m_macro, &Macro::finished, this, &MainWindow::onMacroFinished);
    connect(m_macro, &Macro::keyboardEnabled, m_view,
            &TerminalView::setKeyboardEnabled);
    connect(m_macro, &Macro::notice, this, &MainWindow::onNotice);

    buildMenus();

    // Before the window is shown, so the size the file asks for is the size it
    // opens at rather than a resize the user watches happen. A file that is
    // not there is a first run: every setting takes its default and nothing is
    // written until `Save setup`.
    QString error;
    if (!m_session->loadSettings(m_settingsPath, &error)) {
        // Not fatal and not a dialog. An unreadable settings file is a reason
        // to run with the defaults and say so once, not a reason to refuse to
        // open a terminal.
        onNotice(tr("Could not read the settings: %1").arg(error));
    }

    updateStatus();
    m_view->setFocus();

    // Last, because it publishes the window: once this is bound, something
    // else on the machine can ask this session for things, and everything it
    // can ask about has to exist by then.
    startControl(QString());
}

void MainWindow::startControl(const QString &name)
{
    // Rebinding is how a `/D=` topic takes effect: the constructor has
    // already bound this window under its pid, and `startFrom` calls again
    // with the name the command line asked for. Nothing can have connected in
    // between — the event loop has not started — so the old socket is simply
    // dropped.
    delete m_control;
    m_control = new Control(m_session, this, this);

    QString error;
    if (!m_control->start(name, &error)) {
        // Not fatal, and not a dialog. A window with no way in is still a
        // window, and refusing to open one because a socket file is in the way
        // would be the wrong trade for the user in front of it.
        onNotice(tr("No control socket: %1").arg(error));
        delete m_control;
        m_control = nullptr;
        return;
    }

    // So that anything this window launches can find it — the local shell
    // above all, where a script running *inside* the terminal can then drive
    // the window it is running in. That is the one thing DDE could not do.
    //
    // The process's environment rather than the child's, because
    // `TtPtyParams` has no environment array: one window per process today,
    // and tabs (Stage 3) are where this has to become per-session.
    qputenv("STERNA_CTL", m_control->path().toUtf8());
}

QString MainWindow::settingsPath()
{
    const QString dir =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return QDir(dir).filePath(QStringLiteral("sterna.ini"));
}

void MainWindow::onSettingsChanged()
{
    m_view->applySettings();

    // The *window* is resized rather than the grid, the same way a remote
    // resize is handled: the view fits the terminal to the space it has, so
    // setting the grid directly would leave the painter drawing the old number
    // of columns until the next resize event undid it.
    //
    // Only once the window is up: before it is laid out the view has no size
    // to measure against, and the first layout takes the terminal's size from
    // `TerminalView::sizeHint` instead — which is what makes a configured
    // 132x50 window *open* at 132x50 rather than resize itself in front of the
    // user.
    const int cols = m_session->setting(QStringLiteral("terminal.cols")).toInt();
    const int rows = m_session->setting(QStringLiteral("terminal.rows")).toInt();
    if (isVisible() && cols > 0 && rows > 0
        && (cols != m_session->cols() || rows != m_session->rows())) {
        const QSize want = m_view->sizeForCells(cols, rows);
        resize(size() + (want - m_view->size()));
    }

    // The title and the title *bar*, which are both `TERATERM.INI` keys and
    // both reachable from the command line — `/W=` and `/H`. They are applied
    // here rather than in the startup path so that a file which sets them and
    // a line which sets them arrive at the same place.
    //
    // The core combines `terminal.title` with whatever the host set, the way
    // `window.title_change` says (`ttwinman.c:95`), so there is nothing to
    // decide here — only the substitution below, which is ours.
    showTitle(m_session->title());

    const bool hideTitle =
        m_session->setting(QStringLiteral("window.hide_title")) == QLatin1String("on");
    if (hideTitle != windowFlags().testFlag(Qt::FramelessWindowHint)) {
        const bool wasVisible = isVisible();
        setWindowFlag(Qt::FramelessWindowHint, hideTitle);
        // Changing a window flag re-creates the native window, which hides it.
        if (wasVisible) {
            show();
        }
    }
    updateStatus();
}

void MainWindow::showSettingsDialog()
{
    SettingsDialog dialog(m_session, this);
    dialog.exec();
}

void MainWindow::saveSettings()
{
    // Back to the file it came from, which a `/F=` may have chosen. Writing to
    // the default one instead would move somebody's settings without saying so.
    const QString path = m_settingsPath;
    QDir().mkpath(QFileInfo(path).absolutePath());
    QString error;
    if (!m_session->saveSettings(path, &error)) {
        QMessageBox::warning(this, tr("Setup"),
                             tr("Could not save the settings: %1").arg(error));
        return;
    }
    onNotice(tr("Settings saved to %1").arg(path));
}

void MainWindow::buildMenus()
{
    // No `&` mnemonics anywhere in this menu bar, and that is deliberate: Qt
    // opens a menu on Alt+letter when one matches, and Alt+letter is how a
    // Linux line editor receives Meta. A menu that stole Alt+B from readline
    // would be a menu people disable the whole menu bar to escape.
    QMenu *file = menuBar()->addMenu(tr("File"));
    file->addAction(tr("Connect to serial port..."), QKeySequence(Qt::ALT | Qt::Key_N),
                    this, &MainWindow::showConnectDialog);
    file->addAction(tr("Connect over SSH..."), this, &MainWindow::showSshDialog);
    file->addAction(tr("Connect over telnet..."), this, &MainWindow::showTelnetDialog);
    // No dialog: there is nothing to ask. The shell, the size and the
    // environment are all already known, and a dialog whose only button is OK
    // is a dialog nobody wants twice.
    file->addAction(tr("Local shell"), this, [this] { connectPty(); });
    m_disconnectAction = file->addAction(tr("Disconnect"), this,
                                         &MainWindow::disconnectPort);
    file->addSeparator();
    file->addAction(tr("Quit"), QKeySequence::Quit, this, &QWidget::close);

    QMenu *edit = menuBar()->addMenu(tr("Edit"));
    edit->addAction(tr("Copy"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_C),
                    m_view, &TerminalView::copySelection);
    edit->addAction(tr("Paste"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_V),
                    m_view, &TerminalView::pasteClipboard);

    // Under File, next to the connection, because that is where upstream puts
    // it and because a transfer is a thing you do *to* a connection.
    file->insertSeparator(m_disconnectAction);
    m_sendAction = new QAction(tr("Send file..."), this);
    connect(m_sendAction, &QAction::triggered, this, &MainWindow::sendFile);
    m_receiveAction = new QAction(tr("Receive file..."), this);
    connect(m_receiveAction, &QAction::triggered, this, &MainWindow::receiveFile);
    file->insertAction(m_disconnectAction, m_sendAction);
    file->insertAction(m_disconnectAction, m_receiveAction);
    file->insertSeparator(m_disconnectAction);

    QMenu *terminal = menuBar()->addMenu(tr("Terminal"));
    m_breakAction = terminal->addAction(tr("Send break"), this, &MainWindow::sendBreak);
    terminal->addSeparator();
    m_logAction = terminal->addAction(tr("Start logging..."), this,
                                      &MainWindow::toggleLogging);

    // Upstream's Control menu, which is where a macro is started and stopped.
    // Stop is upstream's End button, which lives on `ttpmacro.exe`'s own
    // control window — there is no second window here, so it belongs on the
    // one there is.
    QMenu *control = menuBar()->addMenu(tr("Control"));
    control->addAction(tr("Run macro..."), this, &MainWindow::runMacro);
    m_stopMacroAction = control->addAction(tr("Stop macro"), this,
                                           &MainWindow::stopMacro);
    m_stopMacroAction->setEnabled(false);
    // "Setup", which is Tera Term's own name for this menu, so that someone
    // arriving from it looks in the right place.
    QMenu *setup = menuBar()->addMenu(tr("Setup"));
    setup->addAction(tr("Terminal..."), this, &MainWindow::showSettingsDialog);
    setup->addAction(tr("Font..."), this, &MainWindow::chooseFont);
    setup->addSeparator();
    setup->addAction(tr("Save setup"), this, &MainWindow::saveSettings);
}

void MainWindow::showConnectDialog()
{
    SerialDialog dialog(this);
    dialog.setInitial(m_lastPort, m_lastParams);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    if (dialog.portPath().isEmpty()) {
        QMessageBox::warning(this, tr("Connect"),
                             tr("No serial ports were found."));
        return;
    }
    connectSerial(dialog.portPath(), dialog.params());
}

void MainWindow::startFrom(TtCmdLine *cmd)
{
    // Applied first, so everything downstream reads one place — which is
    // upstream's order too: `_ParseParam` writes into `ts` and `CommOpen`
    // reads `ts` back. This is also what puts `/W=` and `/H` into effect,
    // since both are settings and `onSettingsChanged` acts on them.
    QString error;
    if (!m_session->applyCommandLine(cmd, &error)) {
        onNotice(tr("Could not apply the command line: %1").arg(error));
    }

    TtCmdLineInfo info = {};
    tt_cmdline_info(cmd, &info);

    // Upstream's `/D=` is the DDE topic a window registers so the `ttpmacro`
    // it launched can find it. Here it names the control socket, which is the
    // same job through what replaced DDE — so a shortcut written for
    // `ttermpro /D=A1B2C3D4` still produces a window `ttpmacro /D=A1B2C3D4`
    // can reach.
    if (info.dde_topic && *info.dde_topic) {
        startControl(QString::fromUtf8(info.dde_topic));
    }

    // Before showing, so the window does not jump. Upstream pairs the two:
    // giving one coordinate puts the other at 0 rather than leaving it at
    // `CW_USEDEFAULT`, because a real position in one axis and "wherever you
    // like" in the other is not a position a window manager can honour.
    if (info.has_x || info.has_y) {
        move(info.has_x ? info.x : 0, info.has_y ? info.y : 0);
    }

    // `/V` is a session with no window at all, for one driven entirely by a
    // macro. It is upstream's, and it is why nothing here assumes `show()`.
    if (!info.hide_window) {
        show();
        if (info.minimize) {
            showMinimized();
        }
    }

    // *After* showing: the terminal's size is what goes out as `NAWS` and as
    // the pty's `winsize`, and before the first layout it is still the size
    // the settings asked for rather than the one the window got.
    TtStartup startup;
    switch (m_session->startup(cmd, &startup)) {
    case TT_STARTUP_OPEN:
        // Logging first, so the banner a console prints on connect is in the
        // file. Upstream starts it at the same point, `vtwin.cpp:3631` — and
        // with the same test: `/L=` names a file, `LogAutoStart` asks for the
        // default one, and either is enough on its own.
        if (info.log_file || m_session->setting(QStringLiteral("log.auto_start"))
                                 == QStringLiteral("on")) {
            // Expanded here rather than taken as typed: `/L=&h-%Y%m%d.log` is
            // a template, and this is the moment its clock is read.
            const QString path = m_session->logName(
                info.log_file ? QString::fromUtf8(info.log_file) : QString());
            if (!m_session->startLog(path, &error)) {
                QMessageBox::critical(this, tr("Logging"),
                                      tr("Could not write %1.\n\n%2")
                                          .arg(path, error));
            }
        }
        openTarget(startup);
        break;
    case TT_STARTUP_DIALOG:
        showConnectDialog();
        break;
    case TT_STARTUP_IDLE:
        // Nothing, deliberately: a terminal with no connection is what `/DS`
        // asks for.
        break;
    default:
        QMessageBox::warning(this, tr("Connect"),
                             tr("Nothing was opened: %1")
                                 .arg(QString::fromUtf8(
                                     startup.reason ? startup.reason
                                                    : tt_last_error())));
        break;
    }

    // Last, and after the connection: a startup macro's first line is
    // usually a `wait` for the prompt of the session the same command line
    // opened. Upstream starts it from `OnCommStart` for the same reason.
    switch (info.macro_kind) {
    case TT_MACRO_UNSET:
    case TT_MACRO_CLEARED:
        // `/M=` with nothing after it cancels the settings file's
        // `StartupMacro`, which is the whole of what `TT_MACRO_CLEARED` means.
        break;
    case TT_MACRO_PROMPT:
        // `/M` on its own, or `/M=*`: upstream puts its file dialog up.
        runMacro();
        break;
    default:
        if (info.macro_file) {
            startMacro({QString::fromUtf8(info.macro_file)});
        }
        break;
    }
    if (info.unknown_count > 0) {
        QStringList bad;
        for (size_t i = 0; i < info.unknown_count; i++) {
            bad << QString::fromUtf8(tt_cmdline_unknown(cmd, i));
        }
        // A message box, which is what upstream does with these and the only
        // diagnostic in either parser.
        note(tr("SSH"), tr("Unrecognised option(s): %1").arg(bad.join(", ")));
    }
}

void MainWindow::openTarget(const TtStartup &startup)
{
    switch (startup.target) {
    case TT_TARGET_SERIAL:
        connectSerial(QString::fromUtf8(startup.path), startup.serial);
        break;
    case TT_TARGET_TELNET:
        connectTelnet(QString::fromUtf8(startup.host), startup.port,
                      &startup.telnet);
        break;
    case TT_TARGET_SSH:
        // Not opened by the core: a host key or a password is a prompt, and a
        // prompt belongs to whoever owns a window. This is the same state
        // machine the SSH dialog drives.
        startSsh(startup.ssh, QString::fromUtf8(startup.ssh.host));
        break;
    case TT_TARGET_SHELL: {
        QStringList argv;
        for (size_t i = 0; i < startup.pty.argc; i++) {
            argv << QString::fromUtf8(startup.pty.argv[i]);
        }
        connectPty(argv);
        break;
    }
    default:
        break;
    }
}

/// Say something the user has to see, whether or not there is a window to say
/// it in. `/V` means there is not, and a modal dialog nobody can find is worse
/// than a line on stderr.
///
/// **`fprintf` and not `qWarning`, which would be lost.** Fedora builds Qt with
/// journald support, so `qWarning` goes to the *systemd journal* rather than to
/// stderr whenever stderr is not a terminal — which is precisely the case a
/// windowless session is launched in: a script, a `.desktop` entry, a cron job.
/// The message would be findable with `journalctl` and nowhere a user would
/// look. `QCommandLineParser` writes its own errors the same way for the same
/// reason.
void MainWindow::note(const QString &title, const QString &text)
{
    if (isVisible()) {
        QMessageBox::information(this, title, text);
    } else {
        fprintf(stderr, "%s: %s\n", qUtf8Printable(title), qUtf8Printable(text));
    }
}

void MainWindow::connectSerial(const QString &path, const TtSerialParams &params)
{
    QString error;
    if (!m_session->connectSerial(path, params, &error)) {
        // The core distinguishes busy from unplugged from permission-denied on
        // purpose — they are the same errno to a naive layer — so the message
        // is shown as it comes rather than replaced with a generic one.
        QMessageBox::critical(this, tr("Connect"),
                              tr("Could not open %1.\n\n%2").arg(path, error));
        return;
    }
    m_lastPort = path;
    m_lastParams = params;
    updateStatus();
}

void MainWindow::showSshDialog()
{
    SshDialog dialog(this);
    dialog.setInitial(m_lastSshHost, m_lastSshUser, m_lastSshPort,
                      m_lastSshIdentity, m_lastSshLegacy);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    if (dialog.host().isEmpty()) {
        QMessageBox::warning(this, tr("SSH"), tr("Enter a host to connect to."));
        return;
    }

    TtSshParams params;
    dialog.fill(&params);
    startSsh(params, dialog.host());
}

void MainWindow::connectSsh(const QString &host, const QString &user, int port)
{
    TtSshParams params;
    tt_ssh_params_default(&params);
    const QByteArray hostUtf8 = host.toUtf8();
    const QByteArray userUtf8 = user.toUtf8();
    params.host = hostUtf8.constData();
    // Blank means "whatever ~/.ssh/config says", which is not the same as
    // empty — so it stays null rather than pointing at "".
    params.user = user.isEmpty() ? nullptr : userUtf8.constData();
    params.port = static_cast<uint16_t>(port);

    startSsh(params, host);
    m_lastSshUser = user;
    m_lastSshPort = port;
}

void MainWindow::startSsh(const TtSshParams &params, const QString &host)
{
    QString error;
    if (!m_session->startSsh(params, &error)) {
        QMessageBox::critical(this, tr("SSH"),
                              tr("Could not start the connection.\n\n%1").arg(error));
        return;
    }
    m_lastSshHost = host;
    statusBar()->showMessage(tr("Connecting to %1...").arg(host));
    updateStatus();
}

void MainWindow::onSshHostKeyWanted(const HostKeyRequest &request)
{
    HostKeyDialog dialog(request, this);
    dialog.exec();
    m_session->answerHostKey(dialog.decision());
}

void MainWindow::onSshAuthWanted(const AuthRequest &request)
{
    AuthDialog dialog(request, this);
    if (dialog.exec() != QDialog::Accepted) {
        // Cancelling has to end the attempt rather than send empty strings:
        // a device that counts failures should not be walked toward a lockout
        // by someone who changed their mind.
        m_session->cancelSsh();
        statusBar()->showMessage(tr("Connection cancelled"), 5000);
        updateStatus();
        return;
    }
    m_session->answerAuth(dialog.answers());
}

void MainWindow::showTelnetDialog()
{
    TelnetDialog dialog(this);
    dialog.setInitial(m_lastTelnetHost, m_lastTelnetPort, m_lastTelnetMode);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    if (dialog.host().isEmpty()) {
        QMessageBox::warning(this, tr("Telnet"), tr("Enter a host to connect to."));
        return;
    }
    TtTelnetParams params;
    dialog.fill(&params);
    connectTelnet(dialog.host(), dialog.port());
    m_lastTelnetMode = params.mode;
}

void MainWindow::connectTelnet(const QString &host, quint16 port,
                               const TtTelnetParams *given)
{
    TtTelnetParams params;
    if (given) {
        params = *given;
    } else {
        tt_telnet_params_default(&params, port);
        params.mode = m_lastTelnetMode;
    }
    QString error;
    if (!m_session->connectTelnet(host, port, params, &error)) {
        QMessageBox::critical(this, tr("Telnet"),
                              tr("Could not connect to %1:%2.\n\n%3")
                                  .arg(host)
                                  .arg(port)
                                  .arg(error));
        return;
    }
    m_lastTelnetHost = host;
    m_lastTelnetPort = port;
    updateStatus();
}

void MainWindow::connectPty(const QStringList &argv)
{
    QString error;
    if (!m_session->connectPty(argv, &error)) {
        QMessageBox::critical(this, tr("Local shell"),
                              tr("Could not start a local shell.\n\n%1").arg(error));
        return;
    }
    updateStatus();
}

void MainWindow::onRemoteResize(int cols, int rows)
{
    // A console server is describing equipment the user cannot see, so this is
    // honoured rather than offered. Bounded because it arrives off the wire:
    // an 800x600 terminal from a confused server is a window nobody wants and
    // a grid allocation nobody asked for.
    if (cols < 8 || rows < 2 || cols > 500 || rows > 300) {
        onNotice(tr("Ignoring a remote request for a %1x%2 terminal").arg(cols).arg(rows));
        return;
    }
    if (cols == m_session->cols() && rows == m_session->rows()) {
        return;
    }
    // The *window* is resized, not the grid: the view fits the terminal to
    // whatever space it has, so setting the grid directly would leave the
    // painter drawing 132 columns into an 80-column widget until the next
    // resize event undid it. A window manager that refuses — tiled, maximised
    // — leaves the size where it was, and the notice below is then the only
    // record that anything was asked.
    const QSize want = m_view->sizeForCells(cols, rows);
    resize(size() + (want - m_view->size()));
    onNotice(tr("The far end asked for %1x%2").arg(cols).arg(rows));
}

void MainWindow::onSshFailed(const QString &error)
{
    QMessageBox::critical(this, tr("SSH"), error);
    updateStatus();
}

void MainWindow::disconnectPort()
{
    m_session->disconnectPort();
    updateStatus();
}

void MainWindow::sendBreak()
{
    m_session->sendBreak();
}

void MainWindow::sendFile()
{
    XferOptionsDialog options(true, m_session, this);
    if (options.exec() != QDialog::Accepted) {
        return;
    }
    // The protocol first and the files second, because the protocol decides
    // whether more than one is allowed: X/YMODEM send a batch happily, and
    // Kermit's `Send` does too, but a user who picked XMODEM and three files
    // would be surprised by which one arrived.
    const bool batch = options.job().protocol != TT_XFER_PROTOCOL_X_MODEM;
    const QStringList paths =
        batch ? QFileDialog::getOpenFileNames(this, tr("Send"))
              : QStringList{QFileDialog::getOpenFileName(this, tr("Send"))};
    if (paths.isEmpty() || paths.first().isEmpty()) {
        return;
    }

    QString error;
    if (!m_session->sendFiles(options.job(), paths, &error)) {
        QMessageBox::warning(this, tr("Send file"), error);
        return;
    }

    m_xferDialog = new XferProgressDialog(
        tr("Sending — %1").arg(options.protocolName()), this);
    m_xferDialog->setAttribute(Qt::WA_DeleteOnClose);
    connect(m_xferDialog, &XferProgressDialog::cancelled, m_session,
            &Session::cancelTransfer);
    connect(m_xferDialog, &QObject::destroyed, this, [this] { m_xferDialog = nullptr; });
    m_xferDialog->show();
    updateStatus();
}

void MainWindow::receiveFile()
{
    XferOptionsDialog options(false, m_session, this);
    if (options.exec() != QDialog::Accepted) {
        return;
    }
    const QString dir =
        QFileDialog::getExistingDirectory(this, tr("Receive into"));
    if (dir.isEmpty()) {
        return;
    }

    QString name;
    if (options.needsReceiveName()) {
        // XMODEM carries no filename on the wire, so there is nothing to
        // derive a destination from and the user has to say. Asked with a save
        // dialog rather than a line edit, because it is a file name and the
        // platform has a widget for that.
        const QString chosen = QFileDialog::getSaveFileName(
            this, tr("Save the received file as"), dir + QLatin1String("/received.bin"));
        if (chosen.isEmpty()) {
            return;
        }
        name = QFileInfo(chosen).fileName();
    }

    QString error;
    if (!m_session->receiveFiles(options.job(), dir, name, &error)) {
        QMessageBox::warning(this, tr("Receive file"), error);
        return;
    }

    m_xferDialog = new XferProgressDialog(
        tr("Receiving — %1").arg(options.protocolName()), this);
    m_xferDialog->setAttribute(Qt::WA_DeleteOnClose);
    connect(m_xferDialog, &XferProgressDialog::cancelled, m_session,
            &Session::cancelTransfer);
    connect(m_xferDialog, &QObject::destroyed, this, [this] { m_xferDialog = nullptr; });
    m_xferDialog->show();
    updateStatus();
}

void MainWindow::onTransferProgressed(const TransferProgress &progress)
{
    if (m_xferDialog) {
        m_xferDialog->update(progress);
    }
}

void MainWindow::onTransferFinished(const TransferResult &result)
{
    if (m_xferDialog) {
        // Left open rather than closed. A transfer that failed has something
        // to say — often the protocol's own words, which are the only account
        // of the failure there is — and a dialog that vanished at the moment
        // of failure would say it to nobody.
        m_xferDialog->finish(result);
    }
    // The status line gets it too, because the dialog may already have been
    // dismissed and because this is the sentence that survives.
    if (result.success) {
        m_status->setText(tr("Transfer complete"));
    } else if (result.cancelled) {
        m_status->setText(tr("Transfer cancelled"));
    } else {
        m_status->setText(result.message.isEmpty() ? tr("Transfer failed")
                                                   : tr("Transfer failed: %1")
                                                         .arg(result.message));
    }
    updateStatus();
}

void MainWindow::runMacro()
{
    if (m_macro->running()) {
        // Upstream's rule, and not an arbitrary one: linking a second macro
        // takes the terminal from the first, so the first would go on running
        // against a session it can no longer reach.
        QMessageBox::information(this, tr("Macro"),
                                 tr("%1 is still running. Stop it first.")
                                     .arg(m_macro->name()));
        return;
    }
    // Two languages, one dialog: the core picks the interpreter from the
    // extension, so there is nothing here to ask the user about. `.ttl` leads
    // because that is the one a converted shortcut names.
    const QString path = QFileDialog::getOpenFileName(
        this, tr("Run macro"), m_lastMacroDir,
        tr("Scripts (*.ttl *.TTL *.lua);;Tera Term macros (*.ttl *.TTL);;Lua "
           "scripts (*.lua);;All files (*)"));
    if (path.isEmpty()) {
        return;
    }
    m_lastMacroDir = QFileInfo(path).absolutePath();
    startMacro({path});
}

void MainWindow::startMacro(const QStringList &args)
{
    QString error;
    bool busy = false;
    if (!runMacroFile(args, &error, &busy)) {
        note(tr("Macro"), tr("Could not start the macro.\n\n%1").arg(error));
    }
}

bool MainWindow::runMacroFile(const QStringList &args, QString *outError,
                              bool *outBusy)
{
    // "Already running" is separated out here rather than in `Macro::start`
    // because only the two callers know what to do with it: the menu puts a
    // box up, and the control socket has an error code of its own for the one
    // refusal a client would retry.
    if (outBusy) {
        *outBusy = m_macro->running();
    }
    if (!m_macro->start(args, outError)) {
        return false;
    }
    // Not `updateStatus`: the macro may already have finished — a two-line
    // script does — and `onMacroFinished` has then already run.
    if (m_stopMacroAction) {
        m_stopMacroAction->setEnabled(m_macro->running());
    }
    if (m_macro->running()) {
        onNotice(tr("Running %1").arg(m_macro->name()));
    }
    return true;
}

bool MainWindow::macroRunning() const { return m_macro && m_macro->running(); }

int MainWindow::macroExitCode() const { return m_macro ? m_macro->exitCode() : 0; }

bool MainWindow::openCommandLine(const QByteArray &line, QString *outError)
{
    // `tt_cmdline_parse_line`, not `tt_cmdline_parse`: this is an argument
    // that *is* a command line, with no program name in front of it, so it
    // goes through the arm that prepends upstream's dummy one and passes NULL
    // for the DDE topic. A `/D=` inside it therefore neither names a socket
    // nor cancels the startup macro, which is exactly what `ttdde.c:617` does.
    TtCmdLine *cmd = tt_cmdline_parse_line(line.constData(), 0);
    if (!cmd) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }

    QString error;
    if (!m_session->applyCommandLine(cmd, &error)) {
        onNotice(tr("Could not apply the command line: %1").arg(error));
    }

    TtStartup startup;
    const TtStartupKind kind = m_session->startup(cmd, &startup);

    switch (kind) {
    case TT_STARTUP_OPEN:
        // **Queued, not immediate**, and for the same reason the dialog arm
        // below refuses outright: opening can fail, and every one of the four
        // arms of `openTarget` reports a failure in a modal box. Called
        // straight from here that box would go up *inside* `tt_ctl_service`,
        // holding the socket's request open — and the client, which asked a
        // question it was told would be answered at once, would wait on a
        // window nobody is looking at.
        //
        // So the answer goes back first and the connection is opened on the
        // next turn of the event loop, which is where a dialog is an ordinary
        // dialog. `startup`'s strings are borrowed from `cmd`, so the handle
        // is freed by the same closure rather than here.
        QTimer::singleShot(0, this, [this, cmd, startup] {
            openTarget(startup);
            tt_cmdline_free(cmd);
        });
        return true;
    case TT_STARTUP_DIALOG:
        // **A divergence, and a deliberate one.** Upstream's `connect` with
        // nothing openable in it puts the New Connection dialog up
        // (`OnCommStart`'s dialog arm), which is right when a person asked.
        // Nobody asked here: the request came off a socket, and a modal dialog
        // would block this window — and the client with it — until somebody
        // found the window and closed it. So the refusal is reported instead,
        // and the reason is that a request from outside the process must not
        // be able to make the window wait on a person.
        if (outError) {
            *outError = tr("the command line named nothing to connect to");
        }
        break;
    case TT_STARTUP_IDLE:
        if (outError) {
            *outError = tr("the command line asked for no connection");
        }
        break;
    default:
        if (outError) {
            *outError = QString::fromUtf8(startup.reason ? startup.reason
                                                         : tt_last_error());
        }
        break;
    }
    // Only the refusing arms reach here; the opening one hands the handle to
    // the closure that will need its strings.
    tt_cmdline_free(cmd);
    return false;
}

void MainWindow::stopMacro()
{
    if (m_macro->running()) {
        m_macro->cancel();
        // It stops at its next line rather than here, so this is the only
        // acknowledgement there is until it does.
        onNotice(tr("Stopping the macro..."));
    }
}

void MainWindow::onMacroFinished(int exitCode)
{
    if (m_stopMacroAction) {
        m_stopMacroAction->setEnabled(false);
    }
    // The exit code is what `setexitcode` asked the *process* to exit with.
    // Nothing here exits on a macro's word — this window outlives its scripts
    // — so it is worth a line and nothing more.
    onNotice(exitCode == 0 ? tr("Macro finished")
                           : tr("Macro finished, exit code %1").arg(exitCode));
}

void MainWindow::toggleLogging()
{
    if (m_session->isLogging()) {
        m_session->stopLog();
        statusBar()->showMessage(tr("Logging stopped"), 3000);
        updateStatus();
        return;
    }

    // `LogDefaultName` in `LogDefaultPath`, both expanded — so a user whose
    // file says `&h-%Y%m%d.log` is offered today's file for this host rather
    // than a name this window made up.
    const QString path = QFileDialog::getSaveFileName(
        this, tr("Log session to"), m_session->logName(),
        tr("Log files (*.log);;All files (*)"));
    if (path.isEmpty()) {
        return;
    }

    QString error;
    if (!m_session->startLog(path, &error)) {
        QMessageBox::critical(this, tr("Logging"),
                              tr("Could not write %1.\n\n%2").arg(path, error));
        return;
    }
    updateStatus();
}

void MainWindow::chooseFont()
{
    bool ok = false;
    // Monospaced only: a proportional font in a character grid is not a
    // degraded look, it is an unreadable one.
    const QFont font = QFontDialog::getFont(&ok, m_view->theme().font(), this,
                                            tr("Terminal font"),
                                            QFontDialog::MonospacedFonts);
    if (ok) {
        m_view->applyFont(font);
    }
}

void MainWindow::onTitleChanged(const QString &title) { showTitle(title); }

void MainWindow::showTitle(const QString &title)
{
    // `Title=`'s default is upstream's own product name, so taking it
    // literally would put "Tera Term" in this program's title bar. It is read
    // as "no opinion" and means ours — which is the whole of what this window
    // decides about the title now that the core combines it.
    setWindowTitle(title.isEmpty() || title == settingDefault("terminal.title")
                       ? tr("Sterna")
                       : title);
}

void MainWindow::onNotice(const QString &text)
{
    statusBar()->showMessage(text, 5000);
}

void MainWindow::onConnectionChanged()
{
    updateStatus();
}

void MainWindow::syncScrollBar()
{
    const int history = m_session->scrollbackLen();
    const int offset = m_session->viewOffset();
    // Blocked because this is a *reaction* to the session moving: letting it
    // emit would turn every pump into a write back into the session, and the
    // rounding would fight the offset the core just chose.
    const QSignalBlocker block(m_scroll);
    m_scroll->setRange(0, history);
    m_scroll->setPageStep(qMax(1, m_session->rows()));
    m_scroll->setSingleStep(1);
    m_scroll->setValue(history - offset);
    // Hidden when there is nothing to scroll, so an 80x24 window is not
    // permanently a few pixels narrower than the terminal in it.
    m_scroll->setVisible(history > 0);
}

void MainWindow::updateLogStatus()
{
    if (!m_logStatus) {
        return;
    }
    // `formattedDataSize` rather than a KiB division, so a log that has only
    // just started reads "REC 44 bytes" instead of "REC 0 KiB" — the number
    // anyone actually checks is whether it is *moving*.
    m_logStatus->setText(m_session->isLogging()
                             ? tr("REC %1  ").arg(QLocale().formattedDataSize(
                                   static_cast<qint64>(m_session->logBytes())))
                             : QString());
}

void MainWindow::updateStatus()
{
    if (m_logAction) {
        m_logAction->setText(m_session->isLogging() ? tr("Stop logging")
                                                    : tr("Start logging..."));
    }
    updateLogStatus();

    const bool connected = m_session->isConnected();
    const bool connecting = m_session->isConnecting();
    m_status->setText(connected ? m_session->describe()
                      : connecting ? tr("connecting...")
                                   : tr("not connected"));
    if (m_disconnectAction) {
        // Enabled while connecting too: stopping an attempt that is waiting on
        // a slow key exchange is a thing people need to be able to do.
        m_disconnectAction->setEnabled(connected || connecting);
    }
    if (m_breakAction) {
        // Asked of the core rather than inferred from the transport: SSH has
        // no break — RFC 4335 defines one and russh does not implement it —
        // and offering the item anyway offers an error message at the moment
        // a console has stopped answering.
        m_breakAction->setEnabled(m_session->supportsBreak());
    }
    // One transfer at a time, and only over something. The core refuses both
    // anyway, but a greyed item says so before the click rather than after.
    const bool canTransfer = connected && !m_session->isTransferring();
    if (m_sendAction) {
        m_sendAction->setEnabled(canTransfer);
    }
    if (m_receiveAction) {
        m_receiveAction->setEnabled(canTransfer);
    }
}
