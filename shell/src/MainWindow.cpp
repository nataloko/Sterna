// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

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
#include <QLocale>
#include <QStatusBar>

#include <QDir>
#include <QStandardPaths>

#include "SerialDialog.h"
#include "Session.h"
#include "SettingsDialog.h"
#include "SshDialog.h"
#include "SshPrompts.h"
#include "TelnetDialog.h"
#include "TerminalView.h"

namespace {

/// `CommSendBreak`'s duration. Long enough for a `getty` and for a Sun PROM,
/// both of which want a break of a few hundred milliseconds.
constexpr int kBreakMs = 300;

} // namespace

MainWindow::MainWindow()
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

    buildMenus();

    // Before the window is shown, so the size the file asks for is the size it
    // opens at rather than a resize the user watches happen. A file that is
    // not there is a first run: every setting takes its default and nothing is
    // written until `Save setup`.
    QString error;
    if (!m_session->loadSettings(settingsPath(), &error)) {
        // Not fatal and not a dialog. An unreadable settings file is a reason
        // to run with the defaults and say so once, not a reason to refuse to
        // open a terminal.
        onNotice(tr("Could not read the settings: %1").arg(error));
    }

    updateStatus();
    setWindowTitle(tr("termitta"));
    m_view->setFocus();
}

QString MainWindow::settingsPath()
{
    const QString dir =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return QDir(dir).filePath(QStringLiteral("termitta.ini"));
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
    updateStatus();
}

void MainWindow::showSettingsDialog()
{
    SettingsDialog dialog(m_session, this);
    dialog.exec();
}

void MainWindow::saveSettings()
{
    const QString path = settingsPath();
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

    QMenu *terminal = menuBar()->addMenu(tr("Terminal"));
    m_breakAction = terminal->addAction(tr("Send break"), this, &MainWindow::sendBreak);
    terminal->addSeparator();
    m_logAction = terminal->addAction(tr("Start logging..."), this,
                                      &MainWindow::toggleLogging);
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
    QString error;
    if (!m_session->startSsh(params, &error)) {
        QMessageBox::critical(this, tr("SSH"),
                              tr("Could not start the connection.\n\n%1").arg(error));
        return;
    }
    m_lastSshHost = dialog.host();
    statusBar()->showMessage(tr("Connecting to %1...").arg(m_lastSshHost));
    updateStatus();
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

    QString error;
    if (!m_session->startSsh(params, &error)) {
        QMessageBox::critical(this, tr("SSH"),
                              tr("Could not start the connection.\n\n%1").arg(error));
        return;
    }
    m_lastSshHost = host;
    m_lastSshUser = user;
    m_lastSshPort = port;
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

void MainWindow::connectTelnet(const QString &host, quint16 port)
{
    TtTelnetParams params;
    tt_telnet_params_default(&params, port);
    params.mode = m_lastTelnetMode;
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
    m_session->sendBreak(kBreakMs);
}

void MainWindow::toggleLogging()
{
    if (m_session->isLogging()) {
        m_session->stopLog();
        statusBar()->showMessage(tr("Logging stopped"), 3000);
        updateStatus();
        return;
    }

    const QString path = QFileDialog::getSaveFileName(
        this, tr("Log session to"), QStringLiteral("termitta.log"),
        tr("Log files (*.log);;All files (*)"));
    if (path.isEmpty()) {
        return;
    }

    TtLogOptions opts;
    tt_log_options_default(&opts);
    // Elapsed time rather than wall clock, and it is the useful one on a
    // console: the question is nearly always "how long after reset did it
    // stop", not what time it was. Both are `TERATERM.INI` keys and become
    // choices when the settings schema exists.
    opts.timestamp = TT_LOG_TIMESTAMP_ELAPSED;
    QString error;
    if (!m_session->startLog(path, opts, &error)) {
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

void MainWindow::onTitleChanged(const QString &title)
{
    setWindowTitle(title.isEmpty() ? tr("termitta") : title);
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
}
