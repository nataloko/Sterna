// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "MainWindow.h"

#include <QAction>
#include <QFontDialog>
#include <QLabel>
#include <QMenuBar>
#include <QMessageBox>
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
    setCentralWidget(m_view);

    tt_serial_params_default(&m_lastParams);

    m_status = new QLabel(this);
    statusBar()->addPermanentWidget(m_status);

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

void MainWindow::updateStatus()
{
    const bool connected = m_session->isConnected();
    m_status->setText(connected ? m_session->describe() : tr("not connected"));
    if (m_disconnectAction) {
        m_disconnectAction->setEnabled(connected);
    }
    if (m_breakAction) {
        m_breakAction->setEnabled(connected);
    }
}
