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

#include "SerialDialog.h"
#include "Session.h"
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

    buildMenus();
    updateStatus();
    setWindowTitle(tr("termitta"));
    m_view->setFocus();
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
    terminal->addSeparator();
    terminal->addAction(tr("Font..."), this, &MainWindow::chooseFont);
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
    m_status->setText(connected ? m_session->describe() : tr("not connected"));
    if (m_disconnectAction) {
        m_disconnectAction->setEnabled(connected);
    }
    if (m_breakAction) {
        m_breakAction->setEnabled(connected);
    }
}
