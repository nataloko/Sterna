// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "MainWindow.h"

#include "Branding.h"
#include "PageStatusBar.h"
#include "Printer.h"

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QCloseEvent>
#include <QDateTime>
#include <QDesktopServices>
#include <QDockWidget>
#include <QEvent>
#include <QFile>
#include <QFontDialog>
#include <QGuiApplication>
#include <QHash>
#include <QKeySequence>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QRegularExpression>
#include <QScreen>
#include <QPushButton>
#include <QFileDialog>
#include <QFileInfo>
#include <QLocale>
#include <QLibrary>
#include <QLayout>
#include <QStatusTipEvent>
#include <QTimer>
#include <QUrl>

#include <QDir>
#include <QStandardPaths>

#include <cstdio>
#include <cstring>
#include <limits>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

#include "ConnectBar.h"
#include "Control.h"
#include "HighlightsDialog.h"
#include "I18n.h"
#include "LogDialog.h"
#include "Macro.h"
#include "Plugins.h"
#include "QuickButtonBar.h"
#include "QuickButtonRepeat.h"
#include "QuickButtonsDialog.h"
#include "Session.h"
#include "SettingsDialog.h"
#include "SshPrompts.h"
#include "TerminalPage.h"
#include "TerminalView.h"
#include "UpdateSchedule.h"
#include "WindowTitle.h"
#include "XferDialog.h"

namespace {

/// How long after startup the update check waits before making its request.
///
/// Long enough that the window is up, the first frame is painted and a session
/// that opens with a password or host-key prompt has its dialog on screen —
/// that dialog is what [`MainWindow::checkForUpdatesOnStartup`] steps aside
/// for. Short enough to still happen in a terminal somebody opens and reads.
constexpr int UpdateCheckDelayMs = 3000;

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

PanelLayout panelLayout(const QString &value)
{
    // `two` and `four` are the 0.2.x spellings, from when this was a panel
    // count shown *alongside* the tab bar. The core already folds them into
    // `tiled` before the shell sees them; the arms are here so that the two
    // readings of the same table cannot drift apart.
    if (value == QLatin1String("tiled") || value == QLatin1String("two")
        || value == QLatin1String("four")) {
        return PanelLayout::Tiled;
    }
    return PanelLayout::Single;
}

QString panelLayoutSetting(PanelLayout layout)
{
    return layout == PanelLayout::Tiled ? QStringLiteral("tiled")
                                        : QStringLiteral("single");
}

/// `window.quick_buttons_area`, as a Qt dock area.
///
/// The schema already refuses an unrecognised spelling — its enum arm is
/// `top/*` — so this only has to name the four.
Qt::DockWidgetArea quickButtonArea(const QString &setting)
{
    if (setting == QLatin1String("bottom")) {
        return Qt::BottomDockWidgetArea;
    }
    if (setting == QLatin1String("left")) {
        return Qt::LeftDockWidgetArea;
    }
    if (setting == QLatin1String("right")) {
        return Qt::RightDockWidgetArea;
    }
    return Qt::TopDockWidgetArea;
}

/// ...and back, for remembering where the dock was dragged to.
QString quickButtonAreaName(Qt::DockWidgetArea area)
{
    switch (area) {
    case Qt::BottomDockWidgetArea:
        return QStringLiteral("bottom");
    case Qt::LeftDockWidgetArea:
        return QStringLiteral("left");
    case Qt::RightDockWidgetArea:
        return QStringLiteral("right");
    default:
        return QStringLiteral("top");
    }
}

/// Whether this window system gives a client coordinates it may restore.
///
/// Wayland intentionally does not: there is no set-position request in
/// xdg-shell, `move()` is ignored and `pos()` commonly reports `(0,0)`. That
/// pair must not overwrite the last useful X11/Windows position in a settings
/// file shared across sessions.
bool windowPositionIsMeaningful()
{
    return !QGuiApplication::platformName().startsWith(QLatin1String("wayland"));
}

/// The sibling name upstream's `CreateBakupFile` gives the old setup file.
QString settingsBackupPath(const QString &path)
{
    const QDateTime now = QDateTime::currentDateTime();
    int offsetMinutes = now.offsetFromUtc() / 60;
    QChar sign = QLatin1Char('+');
    if (offsetMinutes < 0) {
        sign = QLatin1Char('-');
        offsetMinutes = -offsetMinutes;
    }

    QString prefix = now.toString(QStringLiteral("yyyyMMdd'T'HHmmss"));
    prefix += sign;
    prefix += QStringLiteral("%1").arg(offsetMinutes / 60, 2, 10, QLatin1Char('0'));
    prefix += QStringLiteral("%1").arg(offsetMinutes % 60, 2, 10, QLatin1Char('0'));
    prefix += QLatin1Char('_');

    const QFileInfo info(path);
    return QDir(info.absolutePath()).filePath(prefix + info.fileName());
}

/// Tera Term's common-dialog mask is semicolon-separated (`*.txt;*.log`),
/// while Qt puts spaces between the patterns in one name-filter arm.
QString transferNameFilter(const QString &mask)
{
    if (mask.isEmpty()) {
        return QCoreApplication::translate("MainWindow", "All files (*)");
    }
    QString patterns = mask;
    patterns.replace(QLatin1Char(';'), QLatin1Char(' '));
    return QCoreApplication::translate(
               "MainWindow", "User defined (%1);;All files (*)")
        .arg(patterns);
}

/// Attach a `.lng` key without creating a second menu table. The source text
/// stays on the action as its fallback, so changing catalogs can retranslate
/// the same objects in place.
void languageAction(QAction *action, const char *key, const QString &fallback,
                    const char *section = "Tera Term")
{
    action->setText(fallback);
    action->setProperty("sternaLanguageKey", QByteArray(key));
    action->setProperty("sternaLanguageSection", QByteArray(section));
    action->setProperty("sternaLanguageFallback", fallback);
}

/// A `/K=` path in the active setup directory, with upstream's default `.CNF`
/// extension when the file name contains no dot.
QString keyboardFile(const QString &given, const QString &settingsPath)
{
    if (given.isEmpty()) {
        return {};
    }
    QString path = given;
    if (QFileInfo(path).isRelative()) {
        path = QDir(QFileInfo(settingsPath).absolutePath()).filePath(path);
    }
    if (!QFileInfo(path).fileName().contains(QLatin1Char('.'))) {
        path += QStringLiteral(".CNF");
    }
    return path;
}

/// `GetFileDir`: use an existing configured directory, after Win32-style
/// environment expansion, and otherwise fall back to the desktop's Downloads
/// location. Keeping `%NAME%` expansion matters for a TERATERM.INI shared with
/// Windows; unknown variables are left untouched and therefore fail the
/// existence check instead of silently naming a different directory.
QString transferDirectory(const Session &session)
{
    QString dir = session.setting(QStringLiteral("transfer.dir"));
    int at = 0;
    while ((at = dir.indexOf(QLatin1Char('%'), at)) >= 0) {
        const int end = dir.indexOf(QLatin1Char('%'), at + 1);
        if (end < 0) {
            break;
        }
        const QByteArray name = dir.mid(at + 1, end - at - 1).toLocal8Bit();
        if (!name.isEmpty() && qEnvironmentVariableIsSet(name.constData())) {
            const QString value = qEnvironmentVariable(name.constData());
            dir.replace(at, end - at + 1, value);
            at += value.size();
        } else {
            at = end + 1;
        }
    }
    if (!dir.isEmpty() && QDir(dir).exists()) {
        return dir;
    }
    const QString downloads =
        QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
    return downloads.isEmpty() ? QDir::homePath() : downloads;
}

} // namespace

MainWindow::MainWindow(const QString &settingsPath, const QString &pluginsPath)
    : m_settingsPath(settingsPath.isEmpty() ? MainWindow::settingsPath()
                                            : settingsPath)
    , m_pluginsPath(pluginsPath.isEmpty() ? MainWindow::pluginsPath()
                                          : pluginsPath)
{
    m_i18n = new I18n(this);
    m_panels = new PanelContainer(this);
    setCentralWidget(m_panels);

    m_page = createPage();
    m_panels->addPage(m_page, tr("Terminal"));
    activatePage(m_page);
    connect(m_panels, &PanelContainer::currentChanged, this,
            [this](QWidget *widget) {
                activatePage(static_cast<TerminalPage *>(widget));
            });
    connect(m_panels, &PanelContainer::closeRequested, this,
            [this](QWidget *widget) {
                closePage(static_cast<TerminalPage *>(widget));
            });
    connect(m_panels, &PanelContainer::visiblePagesChanged, this,
            &MainWindow::queueWindowMetrics);
    connect(m_panels, &PanelContainer::emptyConnectionRequested, this,
            // The tile's index is not consumed: a new connection appends to tab
            // order, and in tiled mode that index *is* the spare tile it was
            // started from. The signal still carries it so a test can say which
            // tile answered.
            [this](int, PanelContainer::ConnectionKind kind) {
                m_requestedNewPage = true;
                switch (kind) {
                case PanelContainer::ConnectionKind::Serial:
                    showConnectDialog(ConnectDialog::Kind::Serial);
                    break;
                case PanelContainer::ConnectionKind::Ssh:
                    showConnectDialog(ConnectDialog::Kind::Ssh);
                    break;
                case PanelContainer::ConnectionKind::Telnet:
                    showConnectDialog(ConnectDialog::Kind::Telnet);
                    break;
                case PanelContainer::ConnectionKind::Shell:
                    connectPty();
                    break;
                }
                // Accepted connection paths consume the slot while creating
                // their page. A cancelled dialog reaches this with it intact.
                m_requestedNewPage = false;
            });

    tt_serial_params_default(&m_lastParams);

    // No `statusBar()` here, and nowhere else either: it is created lazily by
    // the first call, and this window has none. Every terminal carries its own
    // `PageStatusBar` instead, because a window can be showing nine of them and
    // one shared line cannot say which session it is describing.

    // Under the menu, and connected to the same window methods the menu uses —
    // the bar decides nothing itself. `updateStatus` is what keeps its labels
    // and its enabled states honest.
    m_connectBar = new ConnectBar(m_i18n, this);
    addToolBar(Qt::TopToolBarArea, m_connectBar);
    connect(m_connectBar, &ConnectBar::recentChosen, this,
            &MainWindow::openRecent);
    connect(m_connectBar, &ConnectBar::destinationEntered, this,
            &MainWindow::connectDestination);
    connect(m_connectBar, &ConnectBar::newConnectionRequested, this,
            [this] { showConnectDialog(); });
    connect(m_connectBar, &ConnectBar::forgetRecentsRequested, this,
            &MainWindow::forgetRecents);
    connect(m_connectBar, &ConnectBar::disconnectRequested, this,
            &MainWindow::disconnectPort);
    connect(m_connectBar, &ConnectBar::localEchoRequested, this, [this](bool on) {
        if (!m_session->isConnected()) {
            updateStatus();
            return;
        }
        QString error;
        if (!m_session->setSetting(QStringLiteral("terminal.local_echo"),
                                   on ? QStringLiteral("on")
                                      : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change the local echo: %1").arg(error));
        }
    });
    connect(m_connectBar, &ConnectBar::lineEditRequested, this, [this](bool on) {
        if (!m_session->isConnected()) {
            updateStatus();
            return;
        }
        if (!on && !m_view->confirmDiscardLineEdit()) {
            // The checkbox moved before its signal arrived. The session is
            // still authoritative, so refresh it back to checked on Cancel.
            updateStatus();
            return;
        }
        QString error;
        if (!m_session->setSetting(QStringLiteral("terminal.line_edit"),
                                   on ? QStringLiteral("on")
                                      : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change line editing: %1").arg(error));
        }
    });
    connect(m_connectBar, &ConnectBar::darkModeRequested, this, [this](bool on) {
        const QString name = QStringLiteral("terminal.dark_mode");
        const QString value = on ? QStringLiteral("on") : QStringLiteral("off");
        for (int i = 0; i < m_panels->count(); i++) {
            auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
            QString error;
            if (!page->session()->setSetting(name, value, &error)) {
                onNotice(tr("Could not change terminal dark mode: %1").arg(error));
                return;
            }
        }
        // Unlike Local echo and Line edit, this is a window-wide appearance
        // preference rather than live terminal state. New tabs and the next
        // launch should not jump back to a white grid.
        rememberSettings({{name, value}});
    });

    // The user's own commands live in a dock rather than directly in a
    // QMainWindow toolbar area: dock splitters are user-resizable, toolbar
    // bands are not. Its area comes from the settings below once they have
    // been read.
    m_quickDock = new QDockWidget(tr("Quick buttons"), this);
    m_quickDock->setObjectName(QStringLiteral("quickButtonDock"));
    m_quickDock->setAllowedAreas(Qt::AllDockWidgetAreas);
    m_quickDock->setFeatures(QDockWidget::DockWidgetMovable);
    // The bar is the dock's whole widget rather than something aligned inside a
    // panel: it owns the stretches that centre its buttons, and a wrapper that
    // capped its height would leave them pinned to one end of the panel with
    // the empty room all at the other.
    m_quickBar = new QuickButtonBar(m_quickDock);
    m_quickDock->setWidget(m_quickBar);
    addDockWidget(Qt::RightDockWidgetArea, m_quickDock);
    m_quickDock->hide();
    const auto orientQuickBar = [this](Qt::DockWidgetArea area) {
        const bool horizontal = area == Qt::TopDockWidgetArea
            || area == Qt::BottomDockWidgetArea;
        m_quickBar->setOrientation(horizontal ? Qt::Horizontal : Qt::Vertical);
    };
    connect(m_quickDock, &QDockWidget::dockLocationChanged, this,
            orientQuickBar);
    orientQuickBar(Qt::RightDockWidgetArea);
    connect(m_quickBar, &QuickButtonBar::activated, this,
            &MainWindow::runQuickButton);
    connect(m_quickBar, &QuickButtonBar::addRequested, this, [this] {
        const QuickButton blank;
        editQuickButtons(-1, &blank);
    });
    connect(m_quickBar, &QuickButtonBar::editRequested, this,
            [this](int index) { editQuickButtons(index); });
    connect(m_quickBar, &QuickButtonBar::duplicateRequested, this,
            [this](int index) {
                QVector<QuickButton> buttons = m_quickBar->buttons();
                if (index < 0 || index >= buttons.size()) {
                    return;
                }
                QuickButton copy = buttons[index];
                // Not the shortcut: two buttons cannot have the same key, and
                // silently giving it to the copy would take it from the
                // original.
                copy.shortcut.clear();
                buttons.insert(index + 1, copy);
                if (storeQuickButtons(buttons)) {
                    editQuickButtons(index + 1);
                }
            });
    connect(m_quickBar, &QuickButtonBar::removeRequested, this,
            [this](int index) {
                QVector<QuickButton> buttons = m_quickBar->buttons();
                if (index < 0 || index >= buttons.size()) {
                    return;
                }
                // Asked about, and it names the button: this is the one
                // destructive thing on the bar, and undo is retyping it.
                if (QMessageBox::question(
                        this, tr("Remove quick button"),
                        tr("Remove \"%1\"?").arg(buttons[index].caption()))
                    != QMessageBox::Yes) {
                    return;
                }
                buttons.remove(index);
                storeQuickButtons(buttons);
            });
    connect(m_quickBar, &QuickButtonBar::stopRequested, this,
            [this](int index) { m_quickRepeat->stop(index); });

    m_quickRepeat = new QuickButtonRepeat(this);
    connect(m_quickRepeat, &QuickButtonRepeat::fire, this,
            &MainWindow::sendQuickButton);
    connect(m_quickRepeat, &QuickButtonRepeat::changed, this,
            &MainWindow::quickRepeatChanged);

    buildMenus();

    // `sizeHint()` is consumed before the first show so a configured grid opens
    // at that size. Polishing after show can add two pixels to Qt's menu/status
    // chrome; do it now so the pre-show hint and the laid-out client agree.
    ensurePolished();
    menuBar()->ensurePolished();
    // The status line is the page's now, so it is the page that has to be
    // polished before anyone asks how tall a terminal wants to be.
    m_page->ensurePolished();
    m_connectBar->ensurePolished();

    if (!m_plugins->error().isEmpty()) {
        onNotice(tr("Could not load Lua plugins: %1").arg(m_plugins->error()));
    }

    // Before the window is shown, so the size the file asks for is the size it
    // opens at rather than a resize the user watches happen. A file that is
    // not there is a first run: every setting takes its default and nothing is
    // written until `Save setup`.
    QString error;
    m_loadingPage = true;
    if (!m_session->loadSettings(m_settingsPath, &error)) {
        // Not fatal and not a dialog. An unreadable settings file is a reason
        // to run with the defaults and say so once, not a reason to refuse to
        // open a terminal.
        onNotice(tr("Could not read the settings: %1").arg(error));
    }
    m_loadingPage = false;
    // After the load and before anything can connect: the connect dialogs and
    // `--port` both open at what was last used, which needs the file read first.
    restoreRememberedConnection();
    loadKeyMap(QDir(QFileInfo(m_settingsPath).absolutePath())
                   .filePath(QStringLiteral("KEYBOARD.CNF")));
    reloadHighlights();
    // After the key map, so a button's shortcut can be checked against it, and
    // after the settings, which say where the bar goes.
    reloadQuickButtons();
    applySavedPosition();

    updateStatus();
    m_view->focusInput();

    // Last, because it publishes the window: once this is bound, something
    // else on the machine can ask this session for things, and everything it
    // can ask about has to exist by then.
    startControl(QString());
}

TerminalPage *MainWindow::createPage()
{
    auto *page =
        new TerminalPage(m_i18n, this, m_pluginsPath, m_settingsPath, m_panels);
    wirePage(page);
    return page;
}

void MainWindow::activatePage(TerminalPage *page)
{
    if (!page) {
        return;
    }
    const bool pageChanged = page != m_page;
    if (m_panels && m_panels->currentWidget() != page) {
        m_panels->setCurrentWidget(page);
    }
    m_page = page;
    m_session = page->session();
    m_printer = page->printer();
    m_view = page->view();
    m_macro = page->macro();
    m_plugins = page->plugins();
    if (m_control) {
        m_control->setSession(m_session);
    }
    markActiveTile();

    // The constructor reaches here before the actions and the toolbar exist.
    // Every later activation has a complete window to refresh.
    if (m_connectBar) {
        reloadLanguage();
        showTitle(m_session->title());
        // A click or context menu inside the already-active page also comes
        // through here. Do not erase a destination being typed there; only a
        // genuine page change makes another page's selector authoritative.
        if (pageChanged) {
            refreshConnectionSelector(page);
        }
        updateStatus();
        queueWindowMetrics();
        // The stop key belongs to whichever view is in front, and the runs it
        // stops belong to the window — so it is re-armed on the way in rather
        // than left on a view the user has walked away from.
        if (m_quickRepeat) {
            m_view->setStopKeyArmed(!m_quickRepeat->isIdle());
        }
        m_view->focusInput();
    }
}

void MainWindow::wirePage(TerminalPage *page)
{
    Session *session = page->session();
    TerminalView *view = page->view();
    Printer *printer = page->printer();
    Macro *macro = page->macro();
    Plugins *plugins = page->plugins();

    connect(view, &TerminalView::popupMenuRequested, this,
            [this, page](const QPoint &pos) {
                activatePage(page);
                showPopupMenu(pos);
            });
    connect(view, &TerminalView::pasteMenuRequested, this,
            [this, page](const QPoint &pos, bool pasteEnabled) {
                activatePage(page);
                showPasteMenu(pos, pasteEnabled);
            });
    connect(view, &TerminalView::keyMacroRequested, this,
            [this, page](const QString &path) {
                activatePage(page);
                startNamedMacro(path);
            });
    connect(view, &TerminalView::keyCommandRequested, this,
            [this, page](quint16 command) {
                activatePage(page);
                invokeMenuCommand(command);
            });
    // Escape, and only while the view has been told something is running. Not
    // per page: the bar is the window's, so its runs are too, and stopping
    // them from whichever terminal is in front is the point of a stop key.
    connect(view, &TerminalView::stopRequested, this, [this, page] {
        m_quickRepeat->stopAll();
        showPageMessage(page, tr("Stopped repeating"), 3000);
    });

    connect(session, &Session::logStateChanged, this, [this, page] {
        updatePageStatus(page);
        if (page == m_page) {
            updateStatus();
        }
    });
    connect(session, &Session::damaged, this, [this, page] {
        updateLogStatus(page);
    });
    // The indicator pauses the log it is counting. It belongs to *this* page,
    // so it pauses this page's log whether or not the page is the active one —
    // the menu item is the one that follows the front terminal.
    connect(page->status(), &PageStatusBar::logClicked, this, [this, page] {
        Session *s = page->session();
        if (!s->isLogging()) {
            return;
        }
        const bool paused = !s->logPaused();
        s->pauseLog(paused);
        page->status()->showMessage(paused ? tr("Logging paused") : tr("Logging resumed"),
                                    3000);
        if (page == m_page) {
            updateStatus();
        }
    });
    connect(session, &Session::titleChanged, this,
            [this, page](const QString &title) {
                updateTabTitle(page);
                if (page == m_page) {
                    showTitle(title);
                }
            });
    // Not filtered to the active page any more. A notice from a tile nobody is
    // looking at used to be dropped on the floor; it now lands on that tile's
    // own line, which is where somebody can read it.
    connect(session, &Session::notice, this, [this, page](const QString &text) {
        showPageMessage(page, text);
    });
    connect(session, &Session::connectionChanged, this, [this, page] {
        updateTabTitle(page);
        updatePageStatus(page);
        // The far edge of an SSH attempt, and the only one worth remembering:
        // `startSsh` returning true means the handshake has begun, and a host
        // whose key was refused or whose login failed should not become the
        // next dialog's default. Either outcome disarms it.
        auto pending = m_pendingSsh.find(page);
        if (pending != m_pendingSsh.end()
            && !page->session()->isConnecting()) {
            const RecentConnection recent = *pending;
            page->status()->clearMessage(
                tr("Connecting to %1...").arg(recent.host));
            m_pendingSsh.erase(pending);
            if (page->session()->isConnected()) {
                rememberSsh(recent.host, recent.user, recent.port,
                            recent.identity, recent.legacy);
                // Here rather than in `startSsh`, for the reason stated
                // there: that call returning true means an attempt has
                // *started*, and a refused host key is not a place to offer
                // going back to.
                rememberRecent(recent);
            }
        }
        // Outside the active-page test on purpose: what this window has open
        // is a fact about the *window*, and a background tab opening a port
        // takes it just as surely as the visible one.
        publishOpenPorts();
        if (page == m_page) {
            onConnectionChanged();
        }
    });
    connect(session, &Session::closeRequested, this, [this, page] {
        // Upstream checks IsWindowEnabled before AutoWinClose. A socket can
        // disappear inside a modal dialog's nested event loop; closing its
        // disabled parent out from under it would strand the dialog.
        if (isEnabled()) {
            if (m_panels->count() == 1) {
                // `close()` hides this stack-owned window; it does not delete
                // the page from inside the session's signal stack.
                close();
                return;
            }
            // Queued because this signal is emitted from the session's pump.
            // Deleting the page here would free the object whose signal stack
            // we are still unwinding through.
            QTimer::singleShot(0, this, [this, page] { closePage(page, false); });
        }
    });
    connect(session, &Session::sshHostKeyWanted, this,
            [this, page](const HostKeyRequest &request) {
                activatePage(page);
                onSshHostKeyWanted(request);
            });
    connect(session, &Session::sshAuthWanted, this,
            [this, page](const AuthRequest &request) {
                activatePage(page);
                onSshAuthWanted(request);
            });
    connect(session, &Session::sshFailed, this,
            [this, page](const QString &error) {
                auto pending = m_pendingSsh.find(page);
                if (pending != m_pendingSsh.end()) {
                    page->status()->clearMessage(
                        tr("Connecting to %1...").arg(pending->host));
                    m_pendingSsh.erase(pending);
                }
                activatePage(page);
                onSshFailed(error);
            });
    connect(session, &Session::remoteResize, this,
            [this, page](int cols, int rows) {
                activatePage(page);
                onRemoteResize(cols, rows);
            });
    connect(session, &Session::windowOperationRequested, this,
            [this, page](const TtWindowRequest &request) {
                activatePage(page);
                onWindowOperation(request);
            });
    connect(session, &Session::printerEvent, printer, &Printer::handle);
    connect(printer, &Printer::notice, this, [this, page](const QString &text) {
        showPageMessage(page, text);
    });
    connect(session, &Session::settingsChanged, this,
            [this, page] { onPageSettingsChanged(page); });
    connect(session, &Session::transferProgressed, this,
            [this, page](const TransferProgress &progress) {
                if (auto *dialog = page->transferDialog()) {
                    dialog->update(progress);
                }
            });
    connect(session, &Session::transferFinished, this,
            [this, page](const TransferResult &result) {
                if (auto *dialog = page->transferDialog()) {
                    // Left open rather than closed. A transfer that failed has
                    // something to say — often the protocol's own words — and
                    // a dialog that vanished would say it to nobody.
                    dialog->finish(result);
                }
                // A message on that page's own line, not a rewrite of the link
                // state. Writing the outcome into the connection label and then
                // calling `updateStatus()` — which is what this did — put it on
                // screen for no frames at all.
                showPageMessage(
                    page,
                    result.success     ? tr("Transfer complete")
                    : result.cancelled ? tr("Transfer cancelled")
                    : result.message.isEmpty()
                        ? tr("Transfer failed")
                        : tr("Transfer failed: %1").arg(result.message));
                if (page == m_page) {
                    updateStatus();
                }
            });

    connect(macro, &Macro::finished, this, [this, page](int exitCode) {
        if (page == m_page) {
            onMacroFinished(exitCode);
        }
    });
    connect(macro, &Macro::keyboardEnabled, view,
            &TerminalView::setKeyboardEnabled);
    connect(macro, &Macro::notice, this, [this, page](const QString &text) {
        showPageMessage(page, text);
    });
    connect(plugins, &Plugins::notice, this,
            [this, page](const QString &text) {
                showPageMessage(page, tr("Lua plugin: %1").arg(text));
            });
}

TerminalPage *MainWindow::addBlankPage()
{
    auto *page = createPage();
    m_panels->addPage(page, tr("Terminal"));
    activatePage(page);

    QString error;
    m_loadingPage = true;
    if (!m_session->loadSettings(m_settingsPath, &error)) {
        onNotice(tr("Could not read the settings: %1").arg(error));
    }
    m_loadingPage = false;
    const QString keyMap = m_keyMapPath.isEmpty()
                               ? QDir(QFileInfo(m_settingsPath).absolutePath())
                                     .filePath(QStringLiteral("KEYBOARD.CNF"))
                               : m_keyMapPath;
    loadKeyMap(keyMap);
    if (!m_plugins->error().isEmpty()) {
        onNotice(tr("Could not load Lua plugins: %1").arg(m_plugins->error()));
    }
    updateTabTitle(page);
    updateTabBar();
    return page;
}

void MainWindow::newTab() { addBlankPage(); }

void MainWindow::ensureIdlePage()
{
    if (m_requestedNewPage) {
        m_requestedNewPage = false;
        addBlankPage();
        return;
    }
    if (m_session->isConnected() || m_session->isConnecting()) {
        addBlankPage();
    }
}

void MainWindow::closeCurrentTab() { closePage(m_page); }

void MainWindow::duplicateSession()
{
    TerminalPage *source = m_page;
    if (!source->session()->canDuplicate()) {
        return;
    }

    TerminalPage *destination = addBlankPage();
    QString error;
    if (!destination->session()->copySettingsFrom(*source->session(), &error)
        || !destination->plugins()->copySettingsFrom(*source->plugins(), &error)
        || !source->session()->duplicateInto(destination->session(), &error)) {
        closePage(destination, false);
        activatePage(source);
        QMessageBox::critical(this, tr("Duplicate session"), error);
        return;
    }
    if (const auto &connection = source->selectorConnection()) {
        // Duplicate reopens the same target, including the parts of an SSH
        // record that are not visible in its label. Give the new page its own
        // copy so either tab can restore the shared selector later.
        destination->setSelectorConnection(*connection,
                                           source->selectorLabel());
        refreshConnectionSelector(destination);
    }
    updateTabTitle(destination);
    updateStatus();
}

void MainWindow::closePage(TerminalPage *page, bool confirm)
{
    const int index = m_panels->indexOf(page);
    if (index < 0) {
        return;
    }
    if (m_panels->count() == 1) {
        close();
        return;
    }

    if (confirm && page->session()->isConnected()
        && !confirmDisconnect(page)) {
        return;
    }

    // Before the page goes: an SSH record waiting on this page has nothing left
    // to wait for, and a pointer to a freed page could compare equal to a later
    // one allocated in its place.
    m_pendingSsh.remove(page);
    m_panels->removePage(index);
    page->deleteLater();
    updateTabBar();
    // The page still exists until the event loop deletes it, so the port it
    // held is published by the pages that remain rather than by asking this
    // one to give it up.
    publishOpenPorts();
}

/// Tell the other windows which serial ports this one has open.
///
/// Asked of the transports rather than of `m_lastPort`: that member is loaded
/// from the settings at startup and names a port nothing has opened, and a
/// window that claimed it would grey out a free adapter in every other
/// window's dropdown.
void MainWindow::publishOpenPorts()
{
    if (!m_control) {
        return;
    }
    QStringList devices;
    for (int i = 0; i < m_panels->count(); i++) {
        // The container holds nothing else, and `TerminalPage` has no
        // `Q_OBJECT` — the same static cast every other walk of these pages
        // uses.
        auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
        if (!page) {
            continue;
        }
        const QString device = page->session()->serialPath();
        if (!device.isEmpty() && !devices.contains(device)) {
            devices.append(device);
        }
    }
    m_control->claimPorts(devices);
}

void MainWindow::updateTabTitle(TerminalPage *page)
{
    const int index = m_panels->indexOf(page);
    if (index < 0) {
        return;
    }
    Session *session = page->session();
    QString label = session->connectionHost();
    if (label.isEmpty() && session->isConnected()) {
        label = session->describe();
    }
    if (label.isEmpty()) {
        label = session->isConnecting() ? tr("connecting...") : tr("Terminal");
    }
    m_panels->setTabText(index, label);
    m_panels->setTabToolTip(index, session->describe());
    // The same string on the page's own line. In tiled mode the tab bar is not
    // there to carry it, and in tabbed mode a single connection has no tab
    // either — so this is the only place a terminal is named.
    page->status()->setName(label);
}

void MainWindow::updateTabBar()
{
    const bool several = m_panels->count() > 1;
    m_panels->setTabsClosable(several);
    if (m_closeTabAction) {
        m_closeTabAction->setEnabled(true);
    }
    // Opening and closing move the tile count, and the marker's rule is about
    // that count — closing the second of two tiles has to take the highlight
    // off the one that is left, and that path need not go through
    // `activatePage` when the page that closed was not the active one.
    markActiveTile();
}

MainWindow::~MainWindow()
{
    // The updater's code lives in the library, so its QObject must be
    // destroyed before QLibrary unloads that code. QObject's ordinary child
    // order is not a lifetime contract to entrust a function pointer to.
    delete m_updater;
    m_updater = nullptr;
    delete m_updateLibrary;
    m_updateLibrary = nullptr;
    // The mirror of the constructor's last line, and for the same reason: the
    // control socket publishes this window, so it stops answering before
    // anything it can answer about goes away.
    delete m_control;
    m_control = nullptr;
    // `TerminalPage` owns the macro/session ordering. It remains alive until
    // the central widget is destroyed, after this control endpoint is gone.
}

void MainWindow::applySavedPosition()
{
    if (!windowPositionIsMeaningful()) {
        return;
    }
    bool xOk = false;
    bool yOk = false;
    const int x = m_session->setting(QStringLiteral("window.x")).toInt(&xOk);
    const int y = m_session->setting(QStringLiteral("window.y")).toInt(&yOk);

    // Upstream tests X alone (`vtwin.cpp:682`): the pair's default is
    // `CW_USEDEFAULT,CW_USEDEFAULT`, and a real X means both fields came out of
    // the value. A present but short `VTPos=12` has already become `(12,0)` in
    // the settings parser, because GetNthNum makes the omitted field zero.
    if (!xOk || !yOk || x == std::numeric_limits<int>::min()) {
        return;
    }

    QPoint position(x, y);
    if (QScreen *screen = QGuiApplication::primaryScreen()) {
        const QRect desktop = screen->virtualGeometry();
        // Win32's RECT has exclusive right/bottom edges, and upstream uses a
        // strict `>` test against them (`vtdisp.c:1517`). Keep that odd last
        // accepted coordinate rather than quietly making the check tidier.
        const qint64 right = qint64(desktop.x()) + desktop.width();
        const qint64 bottom = qint64(desktop.y()) + desktop.height();
        if (x > right || y > bottom || qint64(x) < qint64(desktop.x()) - 20
            || qint64(y) < qint64(desktop.y()) - 20) {
            return;
        }
        position.setX(qMax(x, desktop.x()));
        position.setY(qMax(y, desktop.y()));
    }
    move(position);
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
    // `TtPtyParams` has no environment array. The endpoint is window-wide and
    // follows the selected tab, just like the menu and status bar.
    qputenv("STERNA_CTL", m_control->path().toUtf8());
}

QString MainWindow::settingsPath()
{
    const QString dir =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return QDir(dir).filePath(QStringLiteral("sterna.ini"));
}

QString MainWindow::pluginsPath()
{
    const QString dir =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return QDir(dir).filePath(QStringLiteral("plugins"));
}

bool MainWindow::event(QEvent *event)
{
    const QEvent::Type type = event->type();
    // `QMainWindow::event` delivers this one to `statusBar()->showMessage`,
    // *if there is a status bar*. There is not, and with none the event falls
    // through `QWidget::event` and is dropped with no warning — every
    // `setStatusTip` in the window would stop working and nothing would say
    // so. Handled here, before the base class, and shown where the rest of
    // this window's remarks go.
    if (type == QEvent::StatusTip) {
        showPageMessage(nullptr, static_cast<QStatusTipEvent *>(event)->tip());
        return true;
    }
    const bool handled = QMainWindow::event(event);
    if (m_session && (type == QEvent::WindowActivate
                      || type == QEvent::WindowDeactivate)) {
        applyWindowOpacity(type == QEvent::WindowActivate);
    }
    // Everything that can move a number `CSI 13`/`14`/`15`/`16`/`19 t` reports.
    // `ScreenChangeInternal` is here because the work area is the *screen's*,
    // so dragging a window to the other monitor changes two of the answers
    // without changing anything about the window.
    if (m_session
        && (type == QEvent::Move || type == QEvent::Resize
            || type == QEvent::WindowStateChange || type == QEvent::Show
            || type == QEvent::ScreenChangeInternal)) {
        queueWindowMetrics();
    }
    return handled;
}

void MainWindow::pushWindowMetrics()
{
    const QRect frame = frameGeometry();
    // The *work* area, not the whole monitor: `GetDesktopRect`
    // (`ttlib_static.c:135`) is `MONITORINFO::rcWork`, so a panel or a dock
    // comes off before `CSI 15 t` and `CSI 19 t` are answered.
    const QScreen *screen = this->screen();
    const QSize work = screen ? screen->availableGeometry().size() : QSize(0, 0);

    for (QWidget *widget : m_panels->visiblePages()) {
        auto *page = static_cast<TerminalPage *>(widget);
        TerminalView *view = page->view();
        const QPoint client = view->mapToGlobal(QPoint(0, 0));
        const QSize cell = view->sizeForCells(1, 1);

        TtWindowMetrics m{};
        m.x = frame.x();
        m.y = frame.y();
        m.client_x = client.x();
        m.client_y = client.y();
        m.width = frame.width();
        m.height = frame.height();
        m.client_width = view->width();
        m.client_height = view->height();
        m.cell_width = cell.width();
        m.cell_height = cell.height();
        m.screen_width = work.width();
        m.screen_height = work.height();
        m.iconified = isMinimized();
        page->session()->setWindowMetrics(m);
    }
}

void MainWindow::queueWindowMetrics()
{
    if (m_metricsQueued) {
        return;
    }
    m_metricsQueued = true;
    QTimer::singleShot(0, this, [this] {
        m_metricsQueued = false;
        pushWindowMetrics();
    });
}

void MainWindow::onWindowOperation(const TtWindowRequest &request)
{
    switch (request.op) {
    case TT_WINDOW_OP_DEICONIFY:
    case TT_WINDOW_OP_UNMAXIMIZE:
        // `SW_RESTORE` un-minimises *and* un-maximises, so upstream's
        // de-iconify and its restore are one call. Qt spells it the same way.
        showNormal();
        break;
    case TT_WINDOW_OP_ICONIFY:
        showMinimized();
        break;
    case TT_WINDOW_OP_MOVE:
        // Wayland has no request to place a surface — placement is the
        // compositor's — so `move()` is silently ignored there. Declined out
        // loud instead, because `CSI 13 t` answers from the metrics pushed
        // above and reporting a position the window never took would put a
        // lie on the wire.
        if (QGuiApplication::platformName().startsWith(QLatin1String("wayland"))) {
            onNotice(tr("The host asked to move the window; Wayland does not allow it"));
        } else {
            move(request.x, request.y);
        }
        break;
    case TT_WINDOW_OP_RESIZE_PIXELS: {
        // Upstream's `SetWindowPos` sizes the *frame*; `QWidget::resize` sizes
        // the client area, so the chrome comes off first. A zero axis means
        // "leave that one alone" (`vtdisp.c:3652`), not "zero pixels".
        const QSize chrome = frameGeometry().size() - size();
        const int w = request.x > 0 ? request.x - chrome.width() : width();
        const int h = request.y > 0 ? request.y - chrome.height() : height();
        resize(qMax(1, w), qMax(1, h));
        break;
    }
    case TT_WINDOW_OP_RAISE:
        // Deliberately without taking focus. Upstream has the
        // `SetForegroundWindow` version in the source behind a `#if` nobody
        // turns on, and flashes the taskbar instead when the raise left the
        // window behind another one.
        raise();
        if (!isActiveWindow()) {
            QApplication::alert(this);
        }
        break;
    case TT_WINDOW_OP_LOWER:
        lower();
        break;
    case TT_WINDOW_OP_REFRESH:
        update();
        m_view->update();
        break;
    case TT_WINDOW_OP_MAXIMIZE:
        showMaximized();
        break;
    case TT_WINDOW_OP_TOGGLE_MAXIMIZE:
        if (isMaximized()) {
            showNormal();
        } else {
            showMaximized();
        }
        break;
    }
}

void MainWindow::applyWindowOpacity(bool active)
{
    const QString name = active ? QStringLiteral("window.opacity_active")
                                : QStringLiteral("window.opacity_inactive");
    const int opacity = m_session->setting(name).toInt();
    setWindowOpacity(static_cast<qreal>(opacity) / 255.0);
}

void MainWindow::onPageSettingsChanged(TerminalPage *page)
{
    const PanelLayout requested = panelLayout(
        page->session()->setting(QStringLiteral("window.panel_layout")));
    if (!m_syncingPanelLayout && requested != m_panels->layoutMode()) {
        // This catches every generic surface, not just the View actions: the
        // settings dialog, TTL/Lua setsetting, and plugin settings all arrive
        // as the same page signal and move the one window-wide value.
        // Loading a page applies the file without rewriting it. A live change
        // through View or any generic settings surface is persisted at once.
        setPanelLayout(requested, !m_loadingPage);
    }

    bool resizing = false;
    if (page == m_page) {
        resizing = onSettingsChanged();
    } else {
        page->view()->applySettings();
        page->applySettings();
    }
    // Not when the window has just been asked to change size: the view still
    // has its old geometry here, so refitting now would put the grid straight
    // back to the size the settings just moved it off — and take the setting
    // with it, since `Session::resize` writes `TerminalSize`. The resize event
    // that answers the request refits at the new size.
    if (!resizing && isVisible() && m_panels->panelOf(page) >= 0) {
        page->view()->refitToViewport();
    }
    queueWindowMetrics();
}

void MainWindow::setPanelLayout(PanelLayout layout, bool persist)
{
    if (m_syncingPanelLayout) {
        return;
    }
    m_syncingPanelLayout = true;
    const QString value = panelLayoutSetting(layout);
    for (int i = 0; i < m_panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
        if (page->session()->setting(QStringLiteral("window.panel_layout"))
            == value) {
            continue;
        }
        QString error;
        if (!page->session()->setSetting(
                QStringLiteral("window.panel_layout"), value, &error)) {
            onNotice(tr("Could not change the panel layout: %1").arg(error));
        }
    }
    m_panels->setLayoutMode(layout);
    updatePanelActions();
    // Switching to Single has to *un*-mark: one strip wearing a permanent
    // highlight looks like a stuck state rather than an answer to a question
    // nobody is asking.
    markActiveTile();
    updateTabBar();
    queueWindowMetrics();
    m_syncingPanelLayout = false;

    if (!persist) {
        return;
    }
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    QString error;
    if (!m_session->rememberSettings(
            {{QStringLiteral("window.panel_layout"), value}}, m_settingsPath,
            &error)) {
        fprintf(stderr, "Sterna: could not save the panel layout: %s\n",
                qPrintable(error));
    }
}

bool MainWindow::onSettingsChanged()
{
    reloadLanguage();
    const QSize oldCell = m_view->sizeForCells(1, 1);
    m_view->applySettings();
    // The page's own settings, which is the line-number gutter — and it has to
    // be before the `haveCols` measurement below, because that measurement is
    // what turns a gutter appearing into a wider window rather than a narrower
    // terminal. `TerminalPage::applySettings` re-lays-out the row synchronously
    // so the view's width is already the post-gutter one by then.
    m_page->applySettings();
    // PanelContainer deliberately supplies one terminal's hint regardless of
    // how many slots are visible. The page is below a stacked pane layout, so
    // carry the child's invalidation to QMainWindow explicitly; otherwise the
    // first `sizeHint()` after loading a 100x30 setup can still be the hint the
    // disconnected 80x24 page had at construction.
    m_panels->updateGeometry();
    if (layout()) {
        layout()->invalidate();
    }
    const bool cellSizeChanged = oldCell != m_view->sizeForCells(1, 1);

    // Tera Term's four accelerators are resources whose handlers consult
    // these switches (`vtwin.cpp:1454`). Qt actions own their shortcuts, so a
    // disabled accelerator is represented by no shortcut while the menu item
    // remains available.
    if (m_connectAction) {
        const bool enabled =
            m_session->setting(QStringLiteral("menu.accelerator_new_connection"))
            == QLatin1String("on");
        m_connectAction->setShortcut(
            enabled ? QKeySequence(Qt::ALT | Qt::Key_N) : QKeySequence());
    }
    if (m_localShellAction) {
        const bool enabled =
            m_session->setting(QStringLiteral("menu.accelerator_local_shell"))
            == QLatin1String("on");
        m_localShellAction->setShortcut(
            enabled ? QKeySequence(Qt::ALT | Qt::Key_G) : QKeySequence());
    }
    if (m_breakAction) {
        const bool disabled =
            m_session->setting(QStringLiteral("menu.disable_accelerator_send_break"))
            == QLatin1String("on");
        m_breakAction->setShortcut(
            disabled ? QKeySequence() : QKeySequence(Qt::ALT | Qt::Key_B));
    }
    if (m_duplicateAction) {
        const bool disabled =
            m_session->setting(
                QStringLiteral("menu.disable_accelerator_duplicate"))
            == QLatin1String("on");
        m_duplicateAction->setShortcut(
            disabled ? QKeySequence() : QKeySequence(Qt::ALT | Qt::Key_D));
    }

    // The rules live in the same file, so anything that reloads settings — the
    // dialog, a `setsetting`, a plugin — can also have changed them; and this
    // is what moves the Highlight matches tick when the switch is flipped from
    // somewhere else.
    reloadHighlights();

    // Before the first show, upstream explicitly applies the active value
    // (`vtwin.cpp:780`). Afterwards the desktop's activation state decides,
    // including while the settings dialog is still the active window.
    applyWindowOpacity(!isVisible() || isActiveWindow());

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
    //
    // **The comparison is against the view, not against the grid.** The core
    // applies a setting before this signal is emitted, so by now the grid is
    // already the configured size and asking it whether anything moved always
    // answers no — which left `TerminalSize` in Setup resizing the grid, the
    // refit above putting it back, and the window never moving at all. What is
    // still true here is how many cells the view has room for, which is what
    // `TerminalView::refit` would give the grid.
    bool resizing = false;
    const int cols = m_session->setting(QStringLiteral("terminal.cols")).toInt();
    const int rows = m_session->setting(QStringLiteral("terminal.rows")).toInt();
    const QSize cell = m_view->sizeForCells(1, 1);
    const int haveCols = cell.width() > 0 ? m_view->width() / cell.width() : cols;
    const int haveRows = cell.height() > 0 ? m_view->height() / cell.height() : rows;
    if (!m_loadingPage && m_panels->count() == 1
        && m_panels->layoutMode() == PanelLayout::Single && isVisible()
        && cols > 0 && rows > 0
        && (cellSizeChanged || cols != haveCols || rows != haveRows)) {
        const QSize want = m_view->sizeForCells(cols, rows);
        resize(size() + (want - m_view->size()));
        resizing = true;
    }

    // The title and the title *bar*, which are both `TERATERM.INI` keys and
    // both reachable from the command line — `/W=` and `/H`. They are applied
    // here rather than in the startup path so that a file which sets them and
    // a line which sets them arrive at the same place.
    //
    // The core combines `terminal.title` with whatever the host set, the way
    // `window.title_change` says (`ttwinman.c:95`). The shell adds the
    // connection-dependent `TitleFormat` pieces because it owns the window.
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

    // `PopupMenu` is named for what replaces the bar, not for the gesture
    // itself: it hides the bar, while `EnablePopupMenu` independently gates
    // Ctrl+left-click. `HideTitle` removes the bar too (`vtwin.cpp:3461`), so
    // it participates in the same replacement without changing PopupMenu.
    const bool popupMenu =
        m_session->setting(QStringLiteral("window.popup_menu")) == QLatin1String("on");
    const bool menuHidden = hideTitle || popupMenu;
    menuBar()->setVisible(!menuHidden);

    // Its own switch, and deliberately not tied to the two above: those hide
    // the *menu*, and a person who wants the port and the connect button within
    // reach is not asking for a menu bar.
    const bool toolbar =
        m_session->setting(QStringLiteral("window.toolbar")) == QLatin1String("on");
    if (m_connectBar) {
        m_connectBar->setVisible(toolbar);
    }
    if (m_toolbarAction) {
        m_toolbarAction->setChecked(toolbar);
    }
    // The same, for the gutter — so flipping it in the settings dialog, or from
    // a script, moves the menu's tick with it.
    if (m_lineNumbersAction) {
        const QSignalBlocker block(m_lineNumbersAction);
        m_lineNumbersAction->setChecked(
            m_session->setting(QStringLiteral("terminal.line_numbers"))
            == QLatin1String("on"));
    }
    updatePanelActions();
    // The buttons themselves are not settings, so this rereads the list as
    // well: the settings dialog is one of the places `[Sterna Buttons]` can
    // have changed under the window, the other being a hand edit.
    reloadQuickButtons();
    m_view->setPopupMenuEnabled(
        menuHidden && m_session->setting(QStringLiteral("window.popup_menu_enabled"))
                          == QLatin1String("on"));
    updateStatus();
    return resizing;
}

void MainWindow::showPopupMenu(const QPoint &globalPos)
{
    // The menu bar's own actions carry the real submenus, enabled states and
    // shortcuts. Associating those same actions with this temporary menu gives
    // the popup exactly the ordinary tree instead of building a second copy
    // that will drift as commands are added.
    auto *popup = new QMenu(this);
    popup->setObjectName(QStringLiteral("terminalPopupMenu"));
    for (QAction *action : menuBar()->actions()) {
        popup->addAction(action);
    }

    // Upstream adds this to the Win32 system menu (`vtwin.cpp:3509`). A Qt
    // application cannot add an action to the compositor-owned menu, so it is
    // kept reachable in the replacement popup. Like upstream's command it
    // clears PopupMenu only; HideTitle still keeps the bar hidden.
    if (m_session->setting(QStringLiteral("window.show_menu_enabled"))
        == QLatin1String("on")) {
        popup->addSeparator();
        popup->addAction(tr("Show menu bar"), this, [this] {
            QString error;
            if (!m_session->setSetting(QStringLiteral("window.popup_menu"),
                                       QStringLiteral("off"), &error)) {
                onNotice(tr("Could not show the menu bar: %1").arg(error));
            }
        });
    }

    connect(popup, &QMenu::aboutToHide, m_view, &TerminalView::popupMenuClosed);
    connect(popup, &QMenu::aboutToHide, popup, &QObject::deleteLater);
    popup->popup(globalPos);
}

void MainWindow::showPasteMenu(const QPoint &globalPos, bool pasteEnabled)
{
    // Upstream's `IDR_PASTEMENU` supplies Paste and Paste<CR>
    // (`vtwin.cpp:1317`); Copy is this port's ordinary context-menu addition.
    // Borrow the Edit menu's actions rather than making copies, so their
    // translated text and `KEYBOARD.CNF` shortcuts cannot drift apart.
    auto *menu = new QMenu(this);
    menu->setObjectName(QStringLiteral("pasteMenu"));
    m_copyAction->setEnabled(m_view->hasSelection());
    m_pasteAction->setEnabled(pasteEnabled);
    m_pasteCrAction->setEnabled(pasteEnabled);
    menu->addAction(m_copyAction);
    menu->addSeparator();
    menu->addAction(m_pasteAction);
    menu->addAction(m_pasteCrAction);

    connect(menu, &QMenu::aboutToHide, m_view, &TerminalView::pasteMenuClosed);
    // These actions also own the keyboard shortcuts and appear in Edit. The
    // temporary menu's enabled state must not outlive it: a selection or a
    // connection can become available before either menu is opened again.
    connect(menu, &QMenu::aboutToHide, this, [this] {
        m_copyAction->setEnabled(true);
        m_pasteAction->setEnabled(true);
        m_pasteCrAction->setEnabled(true);
    });
    connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
    menu->popup(globalPos);
}

void MainWindow::showSettingsDialog(int initialPage)
{
    ensureAutoSaveChoice();
    SettingsDialog dialog(m_session, m_plugins, m_i18n, this, initialPage);
    if (dialog.exec() == QDialog::Accepted) {
        persistDialogChanges(dialog);
    }
}

void MainWindow::ensureAutoSaveChoice()
{
    if (m_autoSaveChoiceChecked) {
        return;
    }

    const QString setting = QStringLiteral("settings.auto_save_changes");
    bool present = false;
    QString error;
    // Not latched yet, deliberately: a read that failed has not answered the
    // question. Latching here would let one unreadable file — an NFS mount
    // that blipped — suppress the prompt for the window's whole life, and the
    // only trace would be a status-bar line behind the modal dialog that
    // follows. The next Setup gets to ask again.
    if (!Session::settingPresent(m_settingsPath, setting, &present, &error)) {
        onNotice(tr("Could not check whether settings changes should be saved: %1")
                     .arg(error));
        return;
    }
    m_autoSaveChoiceChecked = true;
    if (present) {
        return;
    }

    QMessageBox prompt(QMessageBox::Question, tr("Save settings changes?"),
                       tr("Should changes accepted in the Settings dialog be "
                          "saved to this setup file automatically? You can "
                          "change this later on the Settings page."),
                       QMessageBox::NoButton, this);
    prompt.setObjectName(QStringLiteral("autoSaveSettingsPrompt"));
    QPushButton *automatic = prompt.addButton(tr("Save automatically"),
                                               QMessageBox::AcceptRole);
    automatic->setObjectName(QStringLiteral("autoSaveSettingsEnableButton"));
    QPushButton *manual = prompt.addButton(tr("Keep manual saving"),
                                            QMessageBox::RejectRole);
    manual->setObjectName(QStringLiteral("autoSaveSettingsManualButton"));
    prompt.setDefaultButton(manual);
    prompt.setEscapeButton(manual);
    prompt.exec();

    const QString value = prompt.clickedButton() == automatic
                              ? QStringLiteral("on")
                              : QStringLiteral("off");
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    if (!m_session->rememberSettings({{setting, value}}, m_settingsPath, &error)) {
        QMessageBox::warning(
            this, tr("Setup"),
            tr("Could not save the automatic-save choice: %1\n\nSterna will "
               "ask again after it is restarted.")
                .arg(error));
    }
}

void MainWindow::persistDialogChanges(const SettingsDialog &dialog)
{
    const QString autoSave = QStringLiteral("settings.auto_save_changes");
    const bool enabled = m_session->setting(autoSave) == QLatin1String("on");
    QVector<QPair<QString, QString>> core;
    for (const auto &change : dialog.appliedCoreChanges()) {
        if (enabled || change.first == autoSave) {
            core.append(change);
        }
    }

    QStringList failures;
    if (!core.isEmpty()) {
        QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
        QString error;
        if (!m_session->rememberSettings(core, m_settingsPath, &error)) {
            failures.append(tr("Could not save the settings: %1").arg(error));
        }
    }
    if (enabled && !dialog.appliedPluginChanges().isEmpty()) {
        QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
        QString error;
        if (!m_plugins->saveSelectedSettings(
                m_settingsPath, dialog.appliedPluginChanges(), &error)) {
            failures.append(
                tr("Could not save the plugin settings: %1").arg(error));
        }
    }
    if (!failures.isEmpty()) {
        QMessageBox::warning(this, tr("Setup"), failures.join(QLatin1Char('\n')));
    }
}

void MainWindow::reloadLanguage()
{
    const QString configured =
        m_session->setting(QStringLiteral("settings.language_file"));
    if (configured == m_languageSetting) {
        return;
    }
    m_languageSetting = configured;
    QString error;
    if (!m_i18n->load(configured, m_settingsPath, &error)) {
        onNotice(tr("Could not read the language file: %1").arg(error));
    }
    translateMenus();
}

void MainWindow::translateMenus()
{
    for (QAction *action : findChildren<QAction *>()) {
        const QByteArray key = action->property("sternaLanguageKey").toByteArray();
        if (key.isEmpty()) {
            continue;
        }
        const QByteArray section =
            action->property("sternaLanguageSection").toByteArray();
        const QString fallback =
            action->property("sternaLanguageFallback").toString();
        action->setText(
            m_i18n->plainText(key.constData(), fallback, section.constData()));
    }
    // Its key depends on live state, so it cannot be a fixed action property.
    updateStatus();
}

void MainWindow::installPluginActions()
{
    QHash<QString, QMenu *> menus = {
        {QStringLiteral("File"), findChild<QMenu *>(QStringLiteral("fileMenu"))},
        {QStringLiteral("Edit"), findChild<QMenu *>(QStringLiteral("editMenu"))},
        {QStringLiteral("View"), findChild<QMenu *>(QStringLiteral("viewMenu"))},
        {QStringLiteral("Control"),
         findChild<QMenu *>(QStringLiteral("controlMenu"))},
        {QStringLiteral("Setup"), findChild<QMenu *>(QStringLiteral("setupMenu"))},
    };

    for (const PluginActionInfo &info : m_plugins->actions()) {
        const QString shortcutText = info.shortcut.trimmed();
        const QKeySequence shortcut = QKeySequence::fromString(
            shortcutText, QKeySequence::PortableText);
        const bool validShortcut = shortcutText.isEmpty() || !shortcut.isEmpty();
        if (!validShortcut) {
            onNotice(tr("Lua plugin %1 has an invalid shortcut: %2")
                         .arg(info.plugin, info.shortcut));
        }

        auto *action = new QAction(this);
        action->setObjectName(
            QStringLiteral("luaPluginAction%1").arg(info.id));
        action->setShortcutContext(Qt::WindowShortcut);

        if (info.kind == TT_PLUGIN_ACTION_KEY) {
            if (!validShortcut || shortcutText.isEmpty()) {
                delete action;
                continue;
            }
            action->setText(tr("%1 plugin shortcut").arg(info.plugin));
            action->setShortcut(shortcut);
            addAction(action);
        } else {
            QStringList parts;
            for (const QString &part : info.menu.split(QLatin1Char('/'),
                                                       Qt::SkipEmptyParts)) {
                if (!part.trimmed().isEmpty()) {
                    parts.append(part.trimmed());
                }
            }
            if (parts.isEmpty()) {
                onNotice(tr("Lua plugin %1 has an empty menu path.")
                             .arg(info.plugin));
                delete action;
                continue;
            }

            QMenu *menu = nullptr;
            QString path;
            for (const QString &part : parts) {
                path = path.isEmpty() ? part
                                      : path + QLatin1Char('/') + part;
                QMenu *next = menus.value(path, nullptr);
                if (!next) {
                    next = menu ? menu->addMenu(part) : menuBar()->addMenu(part);
                    next->setObjectName(
                        QStringLiteral("luaPluginMenu%1").arg(menus.size()));
                    menus.insert(path, next);
                }
                menu = next;
            }

            action->setText(info.label);
            if (validShortcut && !shortcutText.isEmpty()) {
                action->setShortcut(shortcut);
            }
            menu->addAction(action);
        }

        connect(action, &QAction::triggered, this, [this, info] {
            Plugins *plugins = m_page->plugins();
            const QVector<PluginActionInfo> &actions = plugins->actions();
            if (info.id >= static_cast<size_t>(actions.size())
                || !(actions.at(static_cast<qsizetype>(info.id)) == info)) {
                onNotice(tr("Lua plugins changed on disk; restart Sterna before "
                            "using this action."));
                return;
            }

            QString error;
            if (!plugins->invoke(info.id, &error)) {
                onNotice(tr("Lua plugin %1: %2").arg(info.plugin, error));
            }
        });
        m_pluginActions.append(action);
    }
}

void MainWindow::saveSettings()
{
    // Back to the file it came from, which a `/F=` may have chosen. Writing to
    // the default one instead would move somebody's settings without saying so.
    const QString path = m_settingsPath;
    QDir().mkpath(QFileInfo(path).absolutePath());

    // Upstream makes this best-effort: a collision in the same second or an
    // unwritable directory does not turn Save setup itself into a failure.
    // QFile::copy also refuses to overwrite, so the first pre-save copy in a
    // second wins just as it does through CopyFileW(..., TRUE).
    if (m_session->setting(QStringLiteral("settings.auto_backup"))
            == QLatin1String("on")
        && QFile::exists(path)) {
        QFile::copy(path, settingsBackupPath(path));
    }

    QString error;
    const QPoint p = pos();
    if (!m_session->saveSettingsForWindow(path, p.x(), p.y(),
                                          windowPositionIsMeaningful(), &error)) {
        QMessageBox::warning(this, tr("Setup"),
                             tr("Could not save the settings: %1").arg(error));
        return;
    }
    if (!m_plugins->saveSettings(path, &error)) {
        QMessageBox::warning(this, tr("Setup"),
                             tr("Could not save the plugin settings: %1").arg(error));
        return;
    }
    onNotice(tr("Settings saved to %1").arg(path));
}

void MainWindow::restoreRememberedConnection()
{
    // The line settings first, and from the core: `FlowCtrlRTS`/`FlowCtrlDTR`
    // ship as the sentinel -1, meaning "derive from the flow control", and a
    // frontend that read the two keys itself would hold both lines low.
    m_lastParams = m_session->serialParams();

    const auto text = [this](const char *name) {
        return m_session->setting(QLatin1String(name));
    };
    const QString port = text("recent.serial_port");
    if (!port.isEmpty()) {
        m_lastPort = port;
    }
    // The bar opens on the last connection, whatever kind it was.
    loadRecents();

    // An empty host is how "nothing was remembered" is spelled, and it is not
    // the same as an empty value: blanking the members from it would replace a
    // dialog's own default with a blank field.
    const QString sshHost = text("recent.ssh_host");
    if (!sshHost.isEmpty()) {
        m_lastSshHost = sshHost;
        m_lastSshUser = text("recent.ssh_user");
        m_lastSshPort = text("recent.ssh_port").toInt();
        m_lastSshIdentity = text("recent.ssh_identity");
        m_lastSshLegacy = text("recent.ssh_legacy") == QLatin1String("on");
    }

    const QString telnetHost = text("recent.telnet_host");
    if (!telnetHost.isEmpty()) {
        m_lastTelnetHost = telnetHost;
        m_lastTelnetPort = static_cast<quint16>(text("recent.telnet_port").toUInt());
        const QString mode = text("recent.telnet_mode");
        if (mode == QLatin1String("negotiate")) {
            m_lastTelnetMode = TT_TELNET_NEGOTIATE;
        } else if (mode == QLatin1String("framed")) {
            m_lastTelnetMode = TT_TELNET_FRAMED;
        } else if (mode == QLatin1String("raw")) {
            m_lastTelnetMode = TT_TELNET_RAW;
        } else {
            m_lastTelnetMode = TT_TELNET_AUTO;
        }
    }
}

void MainWindow::rememberSettings(const QVector<QPair<QString, QString>> &values)
{
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    QString error;
    if (!m_session->rememberSettings(values, m_settingsPath, &error)) {
        // Not a box. Whatever this is bookkeeping for — the connection the user
        // asked for, or an update check nobody asked for at all — has more
        // right to the screen than the bookkeeping does. `fprintf` rather than
        // `qWarning`, which Fedora routes to the journal when stderr is not a
        // terminal.
        fprintf(stderr, "Sterna: could not write the settings: %s\n",
                qPrintable(error));
    }
}

void MainWindow::rememberSerial(const QString &path, const TtSerialParams &params)
{
    // The line settings go into `[Tera Term]`'s own keys, which is where the
    // serial dialog and a macro's `setbaud` already read and write them; only
    // the device path has no upstream key. Their spellings are the schema's, not
    // this dialog's — `enum(none=None,x=XonXoff,hard/rtscts=Hardware,…)`.
    const auto parity = [&] {
        switch (params.parity) {
        case TT_PARITY_ODD: return QStringLiteral("odd");
        case TT_PARITY_EVEN: return QStringLiteral("even");
        case TT_PARITY_MARK: return QStringLiteral("mark");
        case TT_PARITY_SPACE: return QStringLiteral("space");
        default: return QStringLiteral("none");
        }
    }();
    const auto flow = [&] {
        switch (params.flow) {
        case TT_FLOW_CONTROL_XON_XOFF: return QStringLiteral("x");
        case TT_FLOW_CONTROL_RTS_CTS: return QStringLiteral("hard");
        case TT_FLOW_CONTROL_DSR_DTR: return QStringLiteral("dsrdtr");
        default: return QStringLiteral("none");
        }
    }();
    rememberSettings({
        {QStringLiteral("recent.serial_port"), path},
        {QStringLiteral("serial.baud"), QString::number(params.baud)},
        {QStringLiteral("serial.data_bits"), QString::number(params.data_bits)},
        {QStringLiteral("serial.parity"), parity},
        {QStringLiteral("serial.stop_bits"), QString::number(params.stop_bits)},
        {QStringLiteral("serial.flow"), flow},
    });
}

void MainWindow::rememberSsh(const QString &host, const QString &user, int port,
                             const QString &identity, bool legacy)
{
    rememberSettings({
        {QStringLiteral("recent.ssh_host"), host},
        {QStringLiteral("recent.ssh_user"), user},
        {QStringLiteral("recent.ssh_port"), QString::number(port)},
        {QStringLiteral("recent.ssh_identity"), identity},
        {QStringLiteral("recent.ssh_legacy"),
         legacy ? QStringLiteral("on") : QStringLiteral("off")},
    });
}

void MainWindow::rememberTelnet(const QString &host, quint16 port, TtTelnetMode mode)
{
    const auto spelling = [mode] {
        switch (mode) {
        case TT_TELNET_NEGOTIATE: return QStringLiteral("negotiate");
        case TT_TELNET_FRAMED: return QStringLiteral("framed");
        case TT_TELNET_RAW: return QStringLiteral("raw");
        default: return QStringLiteral("auto");
        }
    }();
    rememberSettings({
        {QStringLiteral("recent.telnet_host"), host},
        {QStringLiteral("recent.telnet_port"), QString::number(port)},
        {QStringLiteral("recent.telnet_mode"), spelling},
    });
}

void MainWindow::rememberRecent(const RecentConnection &recent)
{
    // The list is offered whatever this says; the switch is about *adding* to
    // it. Somebody who turns recording off still owns what is already there
    // until the bar's Forget item removes it.
    if (m_session->setting(QStringLiteral("recent.remember"))
        != QLatin1String("on")) {
        return;
    }
    recent::remember(m_recents, recent);
    rememberSettings({{QStringLiteral("recent.connections"),
                       recent::encode(m_recents)}});
    if (m_connectBar) {
        m_connectBar->setRecents(m_recents);
    }
}

void MainWindow::setPageConnection(TerminalPage *page,
                                   const RecentConnection &connection)
{
    if (!page) {
        return;
    }
    if (page == m_page && m_connectBar) {
        m_connectBar->showConnection(connection);
        page->setSelectorConnection(connection, m_connectBar->destination());
        return;
    }
    // All ordinary opens activate their page first. This fallback is for a
    // connection finishing in the background; only serial labels need the
    // device enumerator's friendlier spelling, and that work waits until the
    // page is actually selected.
    page->setSelectorConnection(
        connection, connection.kind == RecentConnection::Kind::Serial
                        ? QString()
                        : connection.label());
}

void MainWindow::refreshConnectionSelector(TerminalPage *page)
{
    if (!m_connectBar || !page) {
        return;
    }
    if (const auto &connection = page->selectorConnection()) {
        const QString label = page->selectorLabel();
        if (label.isEmpty()) {
            m_connectBar->showConnection(*connection);
            page->setSelectorConnection(*connection,
                                        m_connectBar->destination());
        } else {
            m_connectBar->showConnection(*connection, label);
        }
        return;
    }
    // Connections made directly by a macro do not carry a RecentConnection,
    // but their transport still has an honest short description.
    if (page->session()->isConnected()) {
        m_connectBar->setDestination(page->session()->describe());
        return;
    }
    // A page connected to nothing has nothing to say about the destination, so
    // the field keeps what it had. Clearing it here reads as tidier and takes
    // away the two things that field is holding at exactly this moment: the
    // last connection, which `loadRecents` puts there so that going back is one
    // click, and a destination somebody is part way through typing. Both are
    // reachable — every open makes a page (`ensureIdlePage`), so File > New tab
    // greyed out its own Connect button, and a second connection that failed
    // arrived on a fresh page having thrown away the host that was mistyped.
}

void MainWindow::loadRecents()
{
    m_recents =
        recent::decode(m_session->setting(QStringLiteral("recent.connections")));
    if (m_connectBar) {
        m_connectBar->setRecents(m_recents);
        // The field opens on the last connection rather than empty: the
        // commonest thing anyone does with this bar is go back where they
        // were, and that should be one click and not two.
        if (!m_recents.isEmpty()) {
            m_connectBar->showConnection(m_recents.constFirst());
        }
    }
}

void MainWindow::forgetRecents()
{
    m_recents.clear();
    rememberSettings({{QStringLiteral("recent.connections"), QString()}});
    if (m_connectBar) {
        m_connectBar->setRecents(m_recents);
        // Emptied because the field may be holding the entry `loadRecents`
        // took out of the list just forgotten; then put back, because this
        // page's own connection is not one of the things being forgotten and
        // is still open.
        m_connectBar->setDestination(QString());
        refreshConnectionSelector(m_page);
    }
}

void MainWindow::openRecent(const RecentConnection &recent)
{
    switch (recent.kind) {
    case RecentConnection::Kind::Serial:
        // The record's five fields over the settings' parameters, which is
        // where everything it does not hold comes from.
        connectSerial(recent.path, recent.appliedTo(m_lastParams));
        return;
    case RecentConnection::Kind::Ssh: {
        TtSshParams params;
        tt_ssh_params_default(&params);
        const QByteArray host = recent.host.toUtf8();
        const QByteArray user = recent.user.toUtf8();
        const QByteArray identity = recent.identity.toUtf8();
        const char *identities[] = {identity.constData(), nullptr};
        params.host = host.constData();
        // Null, not "": empty means whatever `~/.ssh/config` says.
        params.user = recent.user.isEmpty() ? nullptr : user.constData();
        params.port = recent.port;
        params.identities = recent.identity.isEmpty() ? nullptr : identities;
        params.legacy = recent.legacy;
        startSsh(params, recent.host);
        return;
    }
    case RecentConnection::Kind::Telnet: {
        TtTelnetParams params;
        tt_telnet_params_default(&params, recent.port);
        params.mode = recent.mode;
        connectTelnet(recent.host, recent.port, &params);
        return;
    }
    case RecentConnection::Kind::Shell:
        connectPty();
        return;
    }
}

void MainWindow::splitTarget(const QString &text, QString *host, QString *user,
                             int *port)
{
    QString rest = text;
    *user = QString();
    *port = 0;
    const int at = rest.indexOf(QLatin1Char('@'));
    if (at >= 0) {
        *user = rest.left(at);
        rest = rest.mid(at + 1);
    }
    // Split on the *last* colon so a bracketed IPv6 literal survives; a bare
    // IPv6 address without brackets is ambiguous here exactly as it is for
    // `ssh`, and is spelled with -p there and in ~/.ssh/config.
    const int colon = rest.lastIndexOf(QLatin1Char(':'));
    if (colon > rest.lastIndexOf(QLatin1Char(']'))) {
        bool ok = false;
        const uint value = QStringView(rest).mid(colon + 1).toUInt(&ok);
        if (!ok || value == 0 || value > 65535) {
            *port = -1;
            *host = rest.left(colon);
            return;
        }
        *port = static_cast<int>(value);
        rest = rest.left(colon);
    }
    *host = rest;
}

namespace {

/// Does this name a serial port rather than a host?
///
/// A path, a Windows device name, or something the enumerator answered with.
/// The last of those is what makes a pasted `/dev/serial/by-path/...` work
/// without the first rule having to know every spelling a platform uses for a
/// device node.
bool looksLikeSerialPort(const QString &text)
{
    if (text.startsWith(QLatin1Char('/'))
        || text.startsWith(QLatin1String("\\\\.\\"))) {
        return true;
    }
    static const QRegularExpression com(QStringLiteral("^COM[0-9]+$"),
                                        QRegularExpression::CaseInsensitiveOption);
    if (com.match(text).hasMatch()) {
        return true;
    }
    bool found = false;
    if (TtPortList *list = tt_serial_enumerate()) {
        for (size_t i = 0; !found && i < tt_port_list_len(list); i++) {
            const TtPortInfo *info = tt_port_list_at(list, i);
            found = info
                && (text == QString::fromUtf8(info->open_path)
                    || text == QString::fromUtf8(info->device));
        }
        tt_port_list_free(list);
    }
    return found;
}

} // namespace

MainWindow::Destination MainWindow::parseDestination(const QString &text)
{
    Destination out;
    const QString target = text.trimmed();
    if (target.isEmpty()) {
        return out;
    }

    // Whitespace is what switches vocabularies, and it switches to the other
    // parser entire — the same choice `main.cpp` makes when it sees a
    // `/OPTION`, for the same reason: a bare host name means SSH on this
    // command line and telnet on Tera Term's, so the two are read one way or
    // the other and never half of each. A destination is one word; anything
    // with a space in it is a Tera Term command line, which is how
    // `/ssh /auth=publickey myrouter` reaches this field.
    if (target.contains(QLatin1Char(' ')) || target.contains(QLatin1Char('\t'))) {
        out.kind = Destination::Kind::CommandLine;
        out.text = target;
        return out;
    }

    if (target.compare(QLatin1String("shell"), Qt::CaseInsensitive) == 0) {
        out.kind = Destination::Kind::Shell;
        return out;
    }
    if (target.startsWith(QLatin1String("ssh://"), Qt::CaseInsensitive)) {
        out.kind = Destination::Kind::Ssh;
        splitTarget(target.mid(6), &out.host, &out.user, &out.port);
        if (out.port < 0) {
            out.kind = Destination::Kind::Invalid;
            out.text = target;
        }
        return out;
    }
    if (target.startsWith(QLatin1String("telnet://"), Qt::CaseInsensitive)) {
        out.kind = Destination::Kind::Telnet;
        splitTarget(target.mid(9), &out.host, &out.user, &out.port);
        if (out.port < 0) {
            out.kind = Destination::Kind::Invalid;
            out.text = target;
            return out;
        }
        if (out.port == 0) {
            out.port = 23;
        }
        return out;
    }
    if (looksLikeSerialPort(target)) {
        out.kind = Destination::Kind::Serial;
        out.path = target;
        return out;
    }

    // A bare word is an SSH destination, which is what the shell's own
    // positional argument means and what somebody who types a host name into
    // a terminal expects. Tera Term reads the same token as telnet; that
    // divergence is `docs/deviations.md`'s, and it is why a line with a space
    // in it goes to the other parser above rather than being merged with this.
    out.kind = Destination::Kind::Ssh;
    splitTarget(target, &out.host, &out.user, &out.port);
    if (out.port < 0) {
        out.kind = Destination::Kind::Invalid;
        out.text = target;
    }
    return out;
}

void MainWindow::connectDestination(const QString &text)
{
    const Destination where = parseDestination(text);
    switch (where.kind) {
    case Destination::Kind::Empty:
        return;
    case Destination::Kind::Invalid:
        note(tr("Connect"),
             tr("The port in %1 must be a number from 1 to 65535.")
                 .arg(where.text));
        return;
    case Destination::Kind::CommandLine: {
        TtCmdLine *cmd =
            tt_cmdline_parse_line(where.text.toUtf8().constData(), 0);
        if (!cmd) {
            note(tr("Connect"), tr("Could not read %1.").arg(where.text));
            return;
        }
        // The settings and the target are one command line, so they belong to
        // one page. `openTarget` also calls this, but that is too late when the
        // active page is live: applying `/W=` or `/BAUD=` here would change
        // the old terminal, then the target would open in a fresh page loaded
        // from the file and never see it.
        ensureIdlePage();
        QString error;
        // Applied first, so `/BAUD=` and its family are in the settings the
        // startup target is then built from. Upstream's order, and
        // `startFrom`'s.
        if (!m_session->applyCommandLine(cmd, &error)) {
            onNotice(tr("Could not apply the command line: %1").arg(error));
        }
        TtStartup startup;
        if (m_session->startup(cmd, &startup) == TT_STARTUP_OPEN) {
            openTarget(startup);
        } else {
            note(tr("Connect"), tr("Nothing to open in %1.").arg(where.text));
        }
        // After `openTarget`: every pointer in the startup is borrowed from
        // the command line.
        tt_cmdline_free(cmd);
        return;
    }
    case Destination::Kind::Shell:
        connectPty();
        return;
    case Destination::Kind::Serial:
        connectSerial(where.path, m_lastParams);
        return;
    case Destination::Kind::Ssh:
        if (where.host.isEmpty()) {
            note(tr("SSH"), tr("Enter a host to connect to."));
            return;
        }
        connectSsh(where.host, where.user, where.port);
        return;
    case Destination::Kind::Telnet:
        if (where.host.isEmpty()) {
            note(tr("Telnet"), tr("Enter a host to connect to."));
            return;
        }
        connectTelnet(where.host, static_cast<quint16>(where.port));
        return;
    }
}

void MainWindow::buildMenus()
{
    // No `&` mnemonics anywhere in this menu bar, and that is deliberate: Qt
    // opens a menu on Alt+letter when one matches, and Alt+letter is how a
    // Linux line editor receives Meta. A menu that stole Alt+B from readline
    // would be a menu people disable the whole menu bar to escape.
    //
    // The compatible menus keep Tera Term's order. View is Sterna's one
    // addition, between Edit and Setup where desktop applications put it, and
    // so is each menu, for every item the two programs share: the log and the
    // transfers under File, Send break under Control, Load key map after Save
    // setup. There is deliberately no Terminal menu, because upstream has
    // none and a hand reaching for Control > Send break should find it there.
    // upstream's Window menu remains absent: it arranges several top-level
    // windows, while View chooses whether this one shows its connections one
    // at a time or all at once.
    QMenu *file = menuBar()->addMenu(tr("File"));
    file->setObjectName(QStringLiteral("fileMenu"));
    languageAction(file->menuAction(), "MENU_FILE", tr("File"));
    m_newTabAction = file->addAction(
        tr("New tab"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_T), this,
        &MainWindow::newTab);
    m_newTabAction->setObjectName(QStringLiteral("newTabAction"));
    m_closeTabAction = file->addAction(
        tr("Close tab"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_W), this,
        &MainWindow::closeCurrentTab);
    m_closeTabAction->setObjectName(QStringLiteral("closeTabAction"));
    file->addSeparator();
    // One item, as upstream has: the screen behind it covers every transport.
    m_connectAction = file->addAction(tr("New connection..."), this,
                                      [this] { showConnectDialog(); });
    m_connectAction->setObjectName(QStringLiteral("connectAction"));
    languageAction(m_connectAction, "MENU_FILE_NEW", tr("New connection..."));
    m_duplicateAction = file->addAction(tr("Duplicate session"), this,
                                         &MainWindow::duplicateSession);
    m_duplicateAction->setObjectName(QStringLiteral("duplicateSessionAction"));
    languageAction(m_duplicateAction, "MENU_FILE_DUPLICATE",
                   tr("Duplicate session"));
    // No dialog: there is nothing to ask. The shell, the size and the
    // environment are all already known, and a dialog whose only button is OK
    // is a dialog nobody wants twice. Upstream's Cygwin connection is in this
    // place in the menu and is the item this replaces.
    m_localShellAction =
        file->addAction(tr("Local shell"), this, [this] { connectPty(); });
    languageAction(m_localShellAction, "MENU_FILE_GYGWIN", tr("Local shell"));
    file->addSeparator();
    // Upstream's three log items, in its order and with its enabling rules
    // (`vtwin.cpp:1176`): Log opens the dialog and is greyed while one is
    // running, and Pause and Stop are the other way round. One item that
    // flipped its own caption was fewer widgets and it cost the pause a home —
    // and `KEYBOARD.CNF` and a menu-command quick button can both name 50124
    // and 50125, which need somewhere to arrive.
    m_logAction = file->addAction(tr("Log..."), this, &MainWindow::showLogDialog);
    m_logAction->setObjectName(QStringLiteral("logAction"));
    languageAction(m_logAction, "MENU_FILE_LOG", tr("Log..."));
    m_pauseLogAction =
        file->addAction(tr("Pause logging"), this, &MainWindow::togglePauseLogging);
    m_pauseLogAction->setObjectName(QStringLiteral("pauseLogAction"));
    m_pauseLogAction->setCheckable(true);
    languageAction(m_pauseLogAction, "MENU_FILE_PAUSELOG", tr("Pause logging"));
    m_pauseLogAction->setStatusTip(
        tr("Stop writing to the log without closing it. What arrives while it "
           "is paused is not written later — it is not kept."));
    m_stopLogAction = file->addAction(tr("Stop logging"), this, &MainWindow::stopLogging);
    m_stopLogAction->setObjectName(QStringLiteral("stopLogAction"));
    languageAction(m_stopLogAction, "MENU_FILE_STOPLOG", tr("Stop logging"));
    file->addSeparator();
    // Under File, next to the connection, because that is where upstream puts
    // it and because a transfer is a thing you do *to* a connection.
    m_sendAction = file->addAction(tr("Send file..."), this,
                                   &MainWindow::sendFile);
    languageAction(m_sendAction, "MENU_FILE_SENDFILE", tr("Send file..."));
    m_receiveAction = file->addAction(tr("Receive file..."), this,
                                      &MainWindow::receiveFile);
    languageAction(m_receiveAction, "MENU_FILE_RECVFILE", tr("Receive file..."));
    file->addSeparator();
    // Upstream's File > Print, which is the same `BuffPrint` call `CSI 0 i`
    // makes — the menu asks for the screen and the sequence can ask for the
    // scroll region instead.
    QAction *print = file->addAction(tr("Print..."), QKeySequence::Print, this,
                                     [this] { m_printer->printScreen(false); });
    languageAction(print, "MENU_FILE_PRINT", tr("Print..."));
    m_disconnectAction = file->addAction(tr("Disconnect"), this,
                                         &MainWindow::disconnectPort);
    languageAction(m_disconnectAction, "MENU_FILE_DISCONNECT", tr("Disconnect"));
    file->addSeparator();
    QAction *quit = file->addAction(tr("Quit"), QKeySequence::Quit, this,
                                    &QWidget::close);
    languageAction(quit, "MENU_FILE_EXIT", tr("Quit"));

    QMenu *edit = menuBar()->addMenu(tr("Edit"));
    edit->setObjectName(QStringLiteral("editMenu"));
    languageAction(edit->menuAction(), "MENU_EDIT", tr("Edit"));
    m_copyAction = edit->addAction(
        tr("Copy"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_C), this,
        [this] { m_view->copySelection(); });
    m_copyAction->setObjectName(QStringLiteral("copyAction"));
    languageAction(m_copyAction, "MENU_EDIT_COPY", tr("Copy"));
    m_pasteAction = edit->addAction(
        tr("Paste"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_V), this,
        [this] { m_view->pasteClipboard(); });
    m_pasteAction->setObjectName(QStringLiteral("pasteAction"));
    languageAction(m_pasteAction, "MENU_EDIT_PASTE", tr("Paste"));
    // `ID_EDIT_PASTECR`, the other half of upstream's Edit menu and of the
    // right button's. Upstream gives it Alt+R; that is a Meta key here, so it
    // is left to `KEYBOARD.CNF`'s `EditPasteCR` rather than taking a keystroke
    // away from the host — see the shortcut trap in AGENTS.md.
    m_pasteCrAction = edit->addAction(tr("Paste<CR>"), this,
                                      [this] { m_view->pasteClipboard(true); });
    m_pasteCrAction->setObjectName(QStringLiteral("pasteCrAction"));
    m_pasteCrAction->setStatusTip(
        tr("Pastes the clipboard and adds the Return that runs it."));
    languageAction(m_pasteCrAction, "MENU_EDIT_PASTECR", tr("Paste<CR>"));
    edit->addSeparator();
    QAction *clearScreen = edit->addAction(
        tr("Clear screen"), this, [this] { m_view->clearScreen(); });
    clearScreen->setObjectName(QStringLiteral("clearScreenAction"));
    clearScreen->setStatusTip(
        tr("Clears the visible page and keeps it in scrollback."));
    languageAction(clearScreen, "MENU_EDIT_CLSCREEN", tr("Clear screen"));
    QAction *clearBuffer = edit->addAction(
        tr("Clear buffer"), this, [this] { m_view->clearBuffer(); });
    clearBuffer->setObjectName(QStringLiteral("clearBufferAction"));
    clearBuffer->setStatusTip(
        tr("Clears the visible page and permanently removes all scrollback."));
    languageAction(clearBuffer, "MENU_EDIT_CLBUFFER", tr("Clear buffer"));
    // Upstream's own pair, in upstream's order (`ID_EDIT_SELECTSCREEN`,
    // `ID_EDIT_SELECTALL`). Neither takes a shortcut: it has none there, there
    // is no `KEYBOARD.CNF` command to bind one to either, and a `QAction`
    // shortcut silently outranks `TerminalView::keyPressEvent` — Ctrl+Shift+A
    // would be a key the host stops receiving, for a command reached from a
    // menu once in a while. See the shortcut trap in `AGENTS.md`.
    edit->addSeparator();
    QAction *selectScreen = edit->addAction(
        tr("Select screen"), this, [this] { m_view->selectScreen(); });
    selectScreen->setObjectName(QStringLiteral("selectScreenAction"));
    selectScreen->setStatusTip(
        tr("Selects the lines on screen, wherever the view is scrolled to."));
    languageAction(selectScreen, "MENU_EDIT_SELECTSCREEN", tr("Select screen"));
    QAction *selectAll = edit->addAction(
        tr("Select all"), this, [this] { m_view->selectAll(); });
    selectAll->setObjectName(QStringLiteral("selectAllAction"));
    selectAll->setStatusTip(tr("Selects the whole buffer, scrollback and all."));
    languageAction(selectAll, "MENU_EDIT_SELECTALL", tr("Select all"));
    // In Edit because it is the same gesture as Copy with a different
    // destination: select the command that worked, keep it. No upstream key.
    edit->addSeparator();
    m_quickButtonFromSelectionAction =
        edit->addAction(tr("New quick button from selection..."), this,
                        &MainWindow::quickButtonFromSelection);
    m_quickButtonFromSelectionAction->setObjectName(
        QStringLiteral("quickButtonFromSelectionAction"));
    connect(edit, &QMenu::aboutToShow, this, [this] {
        m_quickButtonFromSelectionAction->setEnabled(m_view->hasSelection());
    });

    QMenu *view = menuBar()->addMenu(tr("View"));
    view->setObjectName(QStringLiteral("viewMenu"));
    m_tiledAction = view->addAction(tr("Tiled"));
    m_tiledAction->setObjectName(QStringLiteral("tiledAction"));
    m_tiledAction->setCheckable(true);
    // Deliberately no shortcut. A terminal must not lose a key combination to
    // window furniture, especially when KEYBOARD.CNF can map every physical
    // combination independently — and a QAction shortcut silently outranks
    // `TerminalView::keyPressEvent`, so it is a key the host stops receiving.
    connect(m_tiledAction, &QAction::triggered, this, [this](bool on) {
        setPanelLayout(on ? PanelLayout::Tiled : PanelLayout::Single, true);
    });
    updatePanelActions();

    // The switches that decide what the window *shows*. Their editors stay in
    // Setup, which is the line between the two menus: this one answers "is it
    // on screen", that one answers "what is on it". Upstream has none of them
    // — no toolbar, no quick buttons, no line numbers, no pattern highlighting
    // — so there is no `.lng` key to hang on any of them and no upstream order
    // to keep. Each writes its setting rather than hiding its widget directly,
    // so that this menu, the settings dialog and Save setup all mean the same
    // thing.
    view->addSeparator();
    m_toolbarAction = view->addAction(tr("Show toolbar"));
    m_toolbarAction->setObjectName(QStringLiteral("showToolbarAction"));
    m_toolbarAction->setCheckable(true);
    connect(m_toolbarAction, &QAction::triggered, this, [this](bool on) {
        QString error;
        if (!m_session->setSetting(QStringLiteral("window.toolbar"),
                                   on ? QStringLiteral("on")
                                      : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change the toolbar: %1").arg(error));
        }
    });
    m_quickButtonsAction = view->addAction(tr("Show quick buttons"));
    m_quickButtonsAction->setObjectName(QStringLiteral("showQuickButtonsAction"));
    m_quickButtonsAction->setCheckable(true);
    connect(m_quickButtonsAction, &QAction::triggered, this, [this](bool on) {
        QString error;
        if (!m_session->setSetting(QStringLiteral("window.quick_buttons"),
                                   on ? QStringLiteral("on")
                                      : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change the quick buttons: %1").arg(error));
        }
    });
    m_lineNumbersAction = view->addAction(tr("Show line numbers"));
    m_lineNumbersAction->setObjectName(QStringLiteral("showLineNumbersAction"));
    m_lineNumbersAction->setCheckable(true);
    connect(m_lineNumbersAction, &QAction::triggered, this, [this](bool on) {
        QString error;
        if (!m_session->setSetting(QStringLiteral("terminal.line_numbers"),
                                   on ? QStringLiteral("on")
                                      : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change the line numbers: %1").arg(error));
        }
    });
    m_highlightingAction = view->addAction(tr("Highlight matches"));
    m_highlightingAction->setObjectName(QStringLiteral("highlightMatchesAction"));
    m_highlightingAction->setCheckable(true);
    connect(m_highlightingAction, &QAction::triggered, this, [this](bool on) {
        QString error;
        if (!m_session->setSetting(QStringLiteral("color.highlighting"),
                                   on ? QStringLiteral("on") : QStringLiteral("off"),
                                   &error)) {
            onNotice(tr("Could not change highlighting: %1").arg(error));
        }
    });

    // "Setup", which is Tera Term's own name for this menu, so that someone
    // arriving from it looks in the right place — and before Control, which is
    // where upstream's bar has it.
    QMenu *setup = menuBar()->addMenu(tr("Setup"));
    setup->setObjectName(QStringLiteral("setupMenu"));
    languageAction(setup->menuAction(), "MENU_SETUP", tr("Setup"));
    // One item, not one per page. The schema has 26 pages; a menu that long is
    // a wall to read and a scrolling list to click through, and every one of
    // them opens the same dialog on a different tab — which the dialog's own
    // tab rows and its search box already do better. `MENU_SETUP_ADDITION` is
    // upstream's key for the item that opens *its* tabbed everything-else
    // dialog, which is what this dialog is; the five per-page keys upstream had
    // (`MENU_SETUP_TERMINAL` and friends) name dialogs that no longer have a
    // menu entry of their own.
    QAction *preferences = setup->addAction(tr("Preferences..."), this,
                                            [this] { showSettingsDialog(); });
    preferences->setObjectName(QStringLiteral("preferencesAction"));
    languageAction(preferences, "MENU_SETUP_ADDITION", tr("Preferences..."));
    QAction *font =
        setup->addAction(tr("Choose font…"), this, &MainWindow::chooseFont);
    font->setObjectName(QStringLiteral("chooseFontAction"));
    languageAction(font, "MENU_SETUP_FONT", tr("Choose font…"));
    setup->addSeparator();
    QAction *save = setup->addAction(tr("Save setup"), this,
                                     &MainWindow::saveSettings);
    languageAction(save, "MENU_SETUP_SAVE", tr("Save setup"));
    // After Save setup, which is upstream's order: the key map is the last
    // item of its Setup menu.
    QAction *keyMap =
        setup->addAction(tr("Load key map..."), this, &MainWindow::chooseKeyMap);
    languageAction(keyMap, "MENU_SETUP_LOADKEYMAP", tr("Load key map..."));
    // The two editors, which are settings the schema cannot describe: a list of
    // highlight rules and a list of buttons. Both live here rather than on a
    // page of the settings dialog because that dialog is generated from the
    // schema, and a list is exactly what a schema row cannot be. Their two
    // switches are in View — this menu is what the things *are*, that one is
    // whether they are on screen.
    setup->addSeparator();
    QAction *highlighting =
        setup->addAction(tr("Highlighting..."), this, &MainWindow::editHighlights);
    highlighting->setObjectName(QStringLiteral("highlightingAction"));
    // Upstream's nearest thing to a quick button is a KEYBOARD.CNF user key,
    // which is the same four actions with no face on them. This item is how
    // somebody finds the feature: the bar is not there until a button exists.
    QAction *quickButtons =
        setup->addAction(tr("Quick buttons..."), this,
                         &MainWindow::showQuickButtonsDialog);
    quickButtons->setObjectName(QStringLiteral("quickButtonsAction"));

    // Upstream's Control menu, which is where the break is sent and a macro is
    // started and stopped. Stop is upstream's End button, which lives on
    // `ttpmacro.exe`'s own control window — there is no second window here, so
    // it belongs on the one there is.
    QMenu *control = menuBar()->addMenu(tr("Control"));
    control->setObjectName(QStringLiteral("controlMenu"));
    languageAction(control->menuAction(), "MENU_CONTROL", tr("Control"));
    m_breakAction = control->addAction(tr("Send break"), this,
                                       &MainWindow::sendBreak);
    languageAction(m_breakAction, "MENU_CONTROL_SENDBREAK", tr("Send break"));
    control->addSeparator();
    QAction *runMacroAction =
        control->addAction(tr("Run macro..."), this, &MainWindow::runMacro);
    languageAction(runMacroAction, "MENU_CONTROL_MACRO", tr("Run macro..."));
    m_stopMacroAction = control->addAction(tr("Stop macro"), this,
                                           &MainWindow::stopMacro);
    languageAction(m_stopMacroAction, "BTN_STOP", tr("Stop macro"));
    m_stopMacroAction->setEnabled(false);

    QMenu *help = menuBar()->addMenu(tr("Help"));
    help->setObjectName(QStringLiteral("helpMenu"));
    QAction *about = help->addAction(tr("About Sterna"), this, [this] {
        QMessageBox box(
            QMessageBox::Information, tr("About Sterna"),
            tr("<h3>Sterna %1</h3>"
               "<p>A serial, SSH and telnet terminal for Linux and Windows.</p>"
               "<p>Copyright &copy; the Sterna authors.</p>")
                .arg(QCoreApplication::applicationVersion().toHtmlEscaped()),
            QMessageBox::Ok, this);
        box.setIconPixmap(sternaIcon().pixmap(QSize(128, 128)));
        // The same check the once-a-day startup one makes, minus the silence:
        // this button says what it found, including "current" and including a
        // server it could not reach. It works whether or not
        // `updates.check_on_startup` is on — that setting is a schedule, not a
        // switch on the feature.
        QPushButton *update =
            box.addButton(tr("Check for Updates..."), QMessageBox::ActionRole);
        update->setObjectName(QStringLiteral("aboutUpdateButton"));
        // Beside the version it is about, rather than an item of its own in
        // Help: the two questions a release page answers — what is the newest
        // version, and what changed — are both questions about the number in
        // the line above it.
        QPushButton *releases =
            box.addButton(tr("Release Page"), QMessageBox::ActionRole);
        releases->setObjectName(QStringLiteral("aboutReleasesButton"));
        box.exec();
        if (box.clickedButton() == update) {
            checkForUpdates();
        } else if (box.clickedButton() == releases) {
            QDesktopServices::openUrl(
                QUrl(QStringLiteral("https://github.com/nataloko/Sterna/releases")));
        }
    });
    about->setObjectName(QStringLiteral("aboutAction"));

    installPluginActions();
    translateMenus();
}

QObject *MainWindow::loadUpdater(QString *outError)
{
    if (m_updater) {
        return m_updater;
    }
    // Installed/AppImage layout first, then the build tree. The updater is
    // a local library rather than a process so its dialogs remain modal to
    // this window; not linking it keeps Qt Network and its TLS backends out
    // of startup and idle RSS until an update is actually being looked for.
    const QDir bin(QCoreApplication::applicationDirPath());
    QStringList candidates;
#ifdef Q_OS_WIN
    candidates << bin.filePath(QStringLiteral("sterna_updater.dll"));
#else
    candidates
        << QDir(bin.filePath(QStringLiteral("../lib")))
               .filePath(QStringLiteral("libsterna_updater.so"))
        << bin.filePath(QStringLiteral("libsterna_updater.so"));
#endif
    QString error;
    for (const QString &path : candidates) {
        auto *library = new QLibrary(path, this);
        using Factory = QObject *(*)(QWidget *);
        const auto factory =
            reinterpret_cast<Factory>(library->resolve("sterna_updater_new"));
        if (factory) {
            m_updater = factory(this);
            m_updateLibrary = library;
            return m_updater;
        }
        error = library->errorString();
        delete library;
    }
    if (outError) {
        *outError = error;
    }
    return nullptr;
}

void MainWindow::checkForUpdates()
{
    QString error;
    if (!loadUpdater(&error)) {
        QMessageBox::warning(this, tr("Sterna update"),
                             tr("The updater could not be loaded: %1").arg(error));
        return;
    }
    QMetaObject::invokeMethod(m_updater, "check", Qt::QueuedConnection);
}

void MainWindow::checkForUpdatesOnStartup()
{
    if (m_session->setting(QStringLiteral("updates.check_on_startup"))
        != QLatin1String("on")) {
        return;
    }
    if (!updateCheckDue(m_session->setting(QStringLiteral("updates.last_check")),
                        QDateTime::currentDateTimeUtc())) {
        return;
    }

    QTimer::singleShot(UpdateCheckDelayMs, this, [this] {
        // `/V` deliberately runs with no window. It is used for unattended
        // command lines and macros, where an update offer would be an invisible
        // modal dialog that stalls the process rather than useful notice.
        if (!isVisible()) {
            return;
        }
        // A modal dialog here is an SSH password or an unknown host key: the
        // session the user opened Sterna for, mid-question. An update offer
        // landing on top of it would take the keystrokes meant for that answer.
        // Skipping is free — this costs a day, and the check is due for as long
        // as nothing writes the stamp.
        if (QApplication::activeModalWidget()) {
            return;
        }
        // Silently: a missing updater library is a loose build or a partial
        // install, and nobody asked this question. Help > Check for Updates
        // reports it in full. Do this before writing the stamp: no library
        // means no request, so there was no check to remember.
        if (!loadUpdater(nullptr)) {
            return;
        }
        // Written *before* the request, so an unreachable release server costs
        // one attempt a day rather than one per launch — the failure a quiet
        // check deliberately does not report is also the one that would
        // otherwise retry forever.
        rememberSettings({{QStringLiteral("updates.last_check"),
                           updateCheckStamp(QDateTime::currentDateTimeUtc())}});
        QMetaObject::invokeMethod(m_updater, "checkQuietly", Qt::QueuedConnection);
    });
}

void MainWindow::showConnectDialog(ConnectDialog::Kind kind)
{
    ConnectDialog dialog(this, m_i18n);
    // Select first because SSH and Telnet share the destination fields. Their
    // detail panels are both seeded below, but only the selected service may
    // put its remembered host and port into the shared controls.
    dialog.selectKind(kind);
    // Every half is seeded, not just the one being opened on: the point of one
    // screen is that switching halves inside it costs nothing.
    dialog.setInitialSerial(m_lastPort, m_lastParams);
    dialog.setInitialSsh(m_lastSshHost, m_lastSshUser, m_lastSshPort,
                         m_lastSshIdentity, m_lastSshLegacy);
    dialog.setInitialTelnet(m_lastTelnetHost, m_lastTelnetPort, m_lastTelnetMode);
    const bool history =
        m_session->setting(QStringLiteral("connection.history_list"))
        == QLatin1String("on");
    dialog.setRemembersHistory(history);
    if (history) {
        dialog.setHistory(
            m_session->setting(QStringLiteral("recent.host_history"))
                .split(QLatin1Char(';'), Qt::SkipEmptyParts));
    }
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    // The box is a setting of its own, so unticking it is remembered even
    // though nothing was connected to.
    if (dialog.remembersHistory() != history) {
        rememberSettings({
            {QStringLiteral("connection.history_list"),
             dialog.remembersHistory() ? QStringLiteral("on")
                                       : QStringLiteral("off")},
        });
    }

    switch (dialog.kind()) {
    case ConnectDialog::Kind::Serial:
        if (dialog.portPath().isEmpty()) {
            QMessageBox::warning(this, tr("Connect"),
                                 tr("No serial ports were found."));
            return;
        }
        connectSerial(dialog.portPath(), dialog.serialParams());
        return;
    case ConnectDialog::Kind::Ssh: {
        if (dialog.host().isEmpty()) {
            QMessageBox::warning(this, tr("SSH"), tr("Enter a host to connect to."));
            return;
        }
        TtSshParams params;
        dialog.fillSsh(&params);
        rememberHost(dialog.host(), dialog.remembersHistory());
        startSsh(params, dialog.host());
        return;
    }
    case ConnectDialog::Kind::Telnet: {
        if (dialog.host().isEmpty()) {
            QMessageBox::warning(this, tr("Telnet"), tr("Enter a host to connect to."));
            return;
        }
        TtTelnetParams params;
        dialog.fillTelnet(&params);
        // The dialog's own params, not the ones `connectTelnet` would derive
        // from `m_lastTelnetMode`: the mode is the reason the panel exists, and
        // going through that member applied the chosen mode to the *next*
        // connection rather than to this one.
        rememberHost(dialog.host(), dialog.remembersHistory());
        connectTelnet(dialog.host(), dialog.port(), &params);
        return;
    }
    }
}

void MainWindow::rememberHost(const QString &host, bool remember)
{
    if (!remember || host.isEmpty() || host.contains(QLatin1Char(';'))) {
        return;
    }
    // Newest first, deduplicated, and bounded — an unbounded list would grow
    // into the settings file for ever and the dialog only shows the top of it.
    QStringList hosts = m_session->setting(QStringLiteral("recent.host_history"))
                            .split(QLatin1Char(';'), Qt::SkipEmptyParts);
    hosts.removeAll(host);
    hosts.prepend(host);
    while (hosts.size() > 16) {
        hosts.removeLast();
    }
    rememberSettings({{QStringLiteral("recent.host_history"),
                       hosts.join(QLatin1Char(';'))}});
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

    if (info.key_cnf_file && *info.key_cnf_file) {
        loadKeyMap(keyboardFile(QString::fromUtf8(info.key_cnf_file),
                                m_settingsPath));
    }

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

    // Last, and after starting the connection: a startup macro's first line is
    // usually a `wait` for the prompt of the session the same command line
    // opened. Upstream launches TTPMACRO first with `/S`, then its DDE init
    // starts the connection (`ttdde.c:657`). The link is in-process here, so
    // starting the attempt first and the macro immediately afterwards gives
    // the same ordering without a second process to synchronise.
    switch (info.macro_kind) {
    case TT_MACRO_UNSET: {
        const QString configured =
            m_session->setting(QStringLiteral("macro.startup_file"));
        if (!configured.isEmpty()) {
            startNamedMacro(configured);
        }
        break;
    }
    case TT_MACRO_CLEARED:
        // A `/D=` topic cancels the settings file's `StartupMacro`, which is
        // the whole of what `TT_MACRO_CLEARED` means: a terminal launched by
        // a macro must not recursively launch another one.
        break;
    case TT_MACRO_PROMPT:
        // `/M` on its own, or `/M=*`: upstream puts its file dialog up.
        runMacro();
        break;
    default:
        if (info.macro_file) {
            startNamedMacro(QString::fromUtf8(info.macro_file));
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

#ifdef Q_OS_WIN
namespace {

/// Is there anywhere for a windowless message to be *printed*?
///
/// This is `QCommandLineParser`'s own test (`qcommandlineparser.cpp`),
/// reproduced rather than approximated, because the two want the same answer
/// on the same line: an inherited console means a `cmd` prompt or a `.bat`
/// file is watching, and standard handles named in the startup information
/// mean whoever launched this redirected them and is reading.
bool hasSomewhereToPrint()
{
    if (GetConsoleWindow()) {
        return true;
    }
    STARTUPINFOW startup;
    startup.cb = sizeof startup;
    GetStartupInfoW(&startup);
    return (startup.dwFlags & STARTF_USESTDHANDLES) != 0;
}

} // namespace
#endif

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
///
/// **On Windows the fallback needs a fallback of its own.** `sterna.exe` is a
/// GUI-subsystem binary, like `ttermpro.exe`, so unless the launcher supplied
/// a console or redirected the handles there is no stderr for the line to
/// reach — and a windowless session started from Explorer or a shortcut is
/// exactly that case. A parentless box is then the only thing the user can
/// see, which is again what `QCommandLineParser` does with its own errors.
void MainWindow::note(const QString &title, const QString &text)
{
    if (isVisible()) {
        QMessageBox::information(this, title, text);
        return;
    }
#ifdef Q_OS_WIN
    if (!hasSomewhereToPrint()) {
        QMessageBox::information(nullptr, title, text);
        return;
    }
#endif
    fprintf(stderr, "%s: %s\n", qUtf8Printable(title), qUtf8Printable(text));
}

void MainWindow::connectSerial(const QString &path, const TtSerialParams &params)
{
    ensureIdlePage();
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
    rememberSerial(path, params);
    // Whatever opened the port — the dialog, `--port`, a macro — the bar shows
    // the one that is open and the list has it at the top.
    const RecentConnection recent = RecentConnection::serial(path, params);
    setPageConnection(m_page, recent);
    rememberRecent(recent);
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

    // `startSsh` records the user and port out of the params, so there is
    // nothing to assign here afterwards.
    startSsh(params, host);
}

void MainWindow::startSsh(const TtSshParams &params, const QString &host)
{
    ensureIdlePage();

    // Arm this before `Session::startSsh`: that call polls once and is allowed
    // to finish immediately. Its first `connectionChanged` only says
    // "connecting" and deliberately leaves the entry in place; the terminal
    // success/failure edge consumes it.
    const RecentConnection recent = RecentConnection::ssh(
        host, params.user ? QString::fromUtf8(params.user) : QString(),
        params.port,
        params.identities && params.identities[0]
            ? QString::fromUtf8(params.identities[0])
            : QString(),
        params.legacy);
    m_pendingSsh.insert(m_page, recent);
    // Put the indefinite message in place before starting. `startSsh` polls
    // once and may reach its terminal edge synchronously; in that case the
    // connectionChanged handler above must see and dismiss the message rather
    // than having this function put it back afterwards.
    const QString connecting = tr("Connecting to %1...").arg(host);
    showPageMessage(m_page, connecting, 0);
    QString error;
    if (!m_session->startSsh(params, &error)) {
        m_pendingSsh.remove(m_page);
        m_page->status()->clearMessage(connecting);
        QMessageBox::critical(this, tr("SSH"),
                              tr("Could not start the connection.\n\n%1").arg(error));
        return;
    }
    // Out of the params rather than out of the dialog, because this is the one
    // place an attempt starts and the command line and the control socket come
    // through it too. A null string is not an empty one: it means "whatever
    // ~/.ssh/config says", and the record keeps that distinction.
    m_lastSshHost = host;
    m_lastSshUser = recent.user;
    m_lastSshPort = recent.port;
    m_lastSshIdentity = recent.identity;
    m_lastSshLegacy = recent.legacy;
    setPageConnection(m_page, recent);

    updateStatus();
}

void MainWindow::onSshHostKeyWanted(const HostKeyRequest &request)
{
    HostKeyDialog dialog(request, this, m_i18n);
    dialog.exec();
    m_session->answerHostKey(dialog.decision());
}

void MainWindow::onSshAuthWanted(const AuthRequest &request)
{
    AuthDialog dialog(request, this, m_i18n);
    if (dialog.exec() != QDialog::Accepted) {
        // Cancelling has to end the attempt rather than send empty strings:
        // a device that counts failures should not be walked toward a lockout
        // by someone who changed their mind.
        m_session->cancelSsh();
        m_pendingSsh.remove(m_page);
        // `wirePage` activates the page before raising this dialog, so the
        // attempt's page is the active one.
        showPageMessage(m_page, tr("Connection cancelled"));
        updateStatus();
        return;
    }
    m_session->answerAuth(dialog.answers());
}

void MainWindow::connectTelnet(const QString &host, quint16 port,
                               const TtTelnetParams *given)
{
    ensureIdlePage();
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
    m_lastTelnetMode = params.mode;
    rememberTelnet(host, port, params.mode);
    const RecentConnection recent =
        RecentConnection::telnet(host, port, params.mode);
    setPageConnection(m_page, recent);
    rememberRecent(recent);
    updateStatus();
}

void MainWindow::connectPty(const QStringList &argv)
{
    ensureIdlePage();
    QString error;
    if (!m_session->connectPty(argv, &error)) {
        QMessageBox::critical(this, tr("Local shell"),
                              tr("Could not start a local shell.\n\n%1").arg(error));
        return;
    }
    // Only the login shell is remembered: `--shell -- journalctl -f` is a
    // command, and a list that offered to re-run one would be offering
    // something this record cannot describe.
    if (argv.isEmpty()) {
        const RecentConnection recent = RecentConnection::shell();
        setPageConnection(m_page, recent);
        rememberRecent(recent);
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

bool MainWindow::confirmDisconnect(TerminalPage *page)
{
    Session *session = page ? page->session() : nullptr;
    if (!session || session->linkKind() != TT_LINK_NETWORK) {
        return true;
    }
    if (session->setting(QStringLiteral("connection.confirm_disconnect"))
        != QLatin1String("on")) {
        return true;
    }
    // Cancel is the default button, as it is upstream (`MB_DEFBUTTON2`): the
    // question is asked at the moment somebody may have hit the wrong thing,
    // so Return must not be the answer that loses the session.
    QMessageBox box(QMessageBox::Warning, tr("Sterna"),
                    m_i18n->text("MSG_DISCONNECT_CONF", tr("Disconnect?")),
                    QMessageBox::NoButton, this);
    QPushButton *disconnect =
        box.addButton(m_i18n->text("BTN_OK", tr("OK")),
                      QMessageBox::AcceptRole);
    QPushButton *cancel =
        box.addButton(m_i18n->text("BTN_CANCEL", tr("Cancel")),
                      QMessageBox::RejectRole);
    box.setDefaultButton(cancel);
    box.setEscapeButton(cancel);
    box.exec();
    return box.clickedButton() == disconnect;
}

void MainWindow::closeEvent(QCloseEvent *event)
{
    // `CloseTT` is upstream's third condition here (`vtwin.cpp:1670`) — a
    // window closing because the *application* is quitting does not ask. There
    // is nothing equivalent yet: this process is one window.
    for (int i = 0; i < m_panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
        if (!page->session()->isConnected()) {
            continue;
        }
        if (!confirmDisconnect(page)) {
            event->ignore();
            return;
        }
    }
    QMainWindow::closeEvent(event);
    if (!event->isAccepted()) {
        return;
    }

    // Where the quick button dock was left. On close rather than on the drag,
    // because this is exactly what `SaveVTPos` does with the window's own
    // position one line further down.
    // Unlike that one it has no switch: the bar is this program's own and
    // somebody who moved it meant it.
    if (m_quickDock) {
        const QString area = quickButtonAreaName(dockWidgetArea(m_quickDock));
        if (area != m_session->setting(QStringLiteral("window.quick_buttons_area"))) {
            rememberSettings({{QStringLiteral("window.quick_buttons_area"), area}});
        }
    }

    if (m_session->setting(QStringLiteral("window.save_position"))
        != QLatin1String("on")) {
        return;
    }

    // `SaveVTPos` runs on close upstream and is deliberately *not* Save
    // setup: only the live geometry is written. Pinning every schema default
    // merely because the user remembers a position would make their shared
    // file stop following future upstream defaults.
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    const QPoint p = pos();
    QString error;
    if (!m_session->saveWindowGeometry(m_settingsPath, p.x(), p.y(),
                                       windowPositionIsMeaningful(), &error)) {
        // The window is already closing, so a modal box would make quitting
        // contingent on dismissing a failure from a convenience setting. Use
        // stderr rather than qWarning: Fedora routes the latter to journald
        // when the process did not start in a terminal.
        fprintf(stderr, "Sterna: could not save the window position: %s\n",
                qPrintable(error));
    }
}

void MainWindow::disconnectPort()
{
    if (!confirmDisconnect(m_page)) {
        return;
    }
    m_session->disconnectPort();
    updateStatus();
}

void MainWindow::sendBreak()
{
    m_session->sendBreak();
}

void MainWindow::sendFile()
{
    XferOptionsDialog options(true, m_session, this, m_i18n);
    if (options.exec() != QDialog::Accepted) {
        return;
    }
    // The protocol first and the files second, because the protocol decides
    // whether more than one is allowed: X/YMODEM send a batch happily, and
    // Kermit's `Send` does too, but a user who picked XMODEM and three files
    // would be surprised by which one arrived.
    const bool batch = options.job().protocol != TT_XFER_PROTOCOL_X_MODEM;
    const QString dir = transferDirectory(*m_session);
    const QString filter = transferNameFilter(
        m_session->setting(QStringLiteral("transfer.send_filter")));
    const QString title = options.transferTitle();
    const QStringList paths =
        batch ? QFileDialog::getOpenFileNames(this, title, dir, filter)
              : QStringList{QFileDialog::getOpenFileName(this, title, dir,
                                                         filter)};
    if (paths.isEmpty() || paths.first().isEmpty()) {
        return;
    }

    QString error;
    if (!m_session->sendFiles(options.job(), paths, &error)) {
        QMessageBox::warning(this, options.windowTitle(), error);
        return;
    }

    auto *dialog = new XferProgressDialog(title, this, m_i18n);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    m_page->setTransferDialog(dialog);
    connect(dialog, &XferProgressDialog::cancelled, m_session,
            &Session::cancelTransfer);
    dialog->show();
    updateStatus();
}

void MainWindow::receiveFile()
{
    XferOptionsDialog options(false, m_session, this, m_i18n);
    if (options.exec() != QDialog::Accepted) {
        return;
    }
    const QString title = options.transferTitle();
    const QString dir = QFileDialog::getExistingDirectory(
        this, title, transferDirectory(*m_session));
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
        QMessageBox::warning(this, options.windowTitle(), error);
        return;
    }

    auto *dialog = new XferProgressDialog(title, this, m_i18n);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    m_page->setTransferDialog(dialog);
    connect(dialog, &XferProgressDialog::cancelled, m_session,
            &Session::cancelTransfer);
    dialog->show();
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
        this,
        m_i18n->plainText("FILEDLG_OPEN_MACRO_TITLE", tr("Run macro")),
        m_lastMacroDir,
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

void MainWindow::startNamedMacro(const QString &name)
{
    // TTPMACRO checks the first character rather than the whole name
    // (`ttmmain.cpp:285`), so `StartupMacro=*anything` is a prompt too.
    if (name.startsWith(QLatin1Char('*'))) {
        runMacro();
        return;
    }

    // Tera Term sets its current directory to `HomeDirW` at process startup,
    // and `/M=` also resolves against that directory explicitly. Sterna does
    // not change the process-wide working directory: it may have more than one
    // settings path, and a macro can launch a child which should inherit the
    // directory the user launched from. Resolve only this name instead. The
    // active INI's directory is the useful cross-platform analogue and keeps a
    // copied `TERATERM.INI` and its relative startup macro together.
    QString path = name;
    if (QFileInfo(path).isRelative()) {
        path = QDir(QFileInfo(m_settingsPath).absolutePath()).filePath(path);
    }
    startMacro({path});
}

void MainWindow::loadKeyMap(const QString &path)
{
    QVector<quint16> duplicates;
    QString error;
    if (!m_session->loadKeyMap(path, &duplicates, &error)) {
        onNotice(tr("Could not read the key map: %1").arg(error));
        return;
    }
    m_keyMapPath = path;
    if (!duplicates.isEmpty()) {
        QStringList codes;
        codes.reserve(duplicates.size());
        for (quint16 code : duplicates) {
            codes.append(QString::number(code));
        }
        onNotice(tr("Key codes used more than once: %1").arg(codes.join(", ")));
    }
}

// --- highlight rules --------------------------------------------------------

void MainWindow::reloadHighlights()
{
    m_highlights = loadHighlights(m_settingsPath);
    for (int i = 0; i < m_panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
        page->session()->setHighlights(m_highlights);
    }
    if (m_highlightingAction && m_session) {
        QSignalBlocker block(m_highlightingAction);
        m_highlightingAction->setChecked(
            m_session->setting(QStringLiteral("color.highlighting"))
            == QLatin1String("on"));
    }
    // Only a hand-edited file can produce one — the editor will not save a
    // pattern the engine refuses — and a rule that is in the file and does
    // nothing is worth one line rather than silence.
    if (m_session) {
        const QString problems = m_session->highlightProblems();
        if (!problems.isEmpty()) {
            onNotice(tr("Highlight rules that did not compile: %1")
                         .arg(problems.split(QLatin1Char('\n')).join(QStringLiteral("; "))));
        }
    }
    if (m_view) {
        m_view->update();
    }
}

void MainWindow::storeHighlights(const QVector<QuickHighlight> &rules)
{
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    QString error;
    if (!saveHighlights(m_settingsPath, rules, &error)) {
        QMessageBox::warning(this, tr("Highlighting"),
                             tr("Could not save the highlight rules: %1").arg(error));
        return;
    }
    // Through the file rather than straight into the sessions, so what is in
    // force is always what a person reading the file would expect.
    reloadHighlights();
}

void MainWindow::editHighlights()
{
    HighlightsDialog dialog(m_highlights, this);
    if (dialog.exec() == QDialog::Accepted) {
        storeHighlights(dialog.rules());
    }
}

void MainWindow::invokeMenuCommand(quint16 command)
{
    // `tt_res.h`'s ids, limited to actions this window actually has. Unknown
    // and deferred commands do nothing, which is what sending an unhandled
    // WM_COMMAND to upstream's window does too.
    switch (command) {
    case 50110: showConnectDialog(); break;
    case 50111: duplicateSession(); break;
    case 50112: connectPty(); break;
    case 50120: showLogDialog(); break;
    case 50124: togglePauseLogging(); break;
    case 50125: stopLogging(); break;
    case 50130: sendFile(); break;
    case 50131: receiveFile(); break;
    case 50190: disconnectPort(); break;
    case 50199: close(); break;
    case 50210: m_view->copySelection(); break;
    case 50230: m_view->pasteClipboard(); break;
    case 50240:
        m_view->pasteText(QApplication::clipboard()->text(QClipboard::Clipboard)
                          + QLatin1Char('\r'));
        break;
    case 50310: showSettingsDialog(); break;
    case 50330: chooseFont(); break;
    case 50380: saveSettings(); break;
    case 50395: chooseKeyMap(); break;
    case 50430: sendBreak(); break;
    case 50470: runMacro(); break;
    default: break;
    }
}

void MainWindow::runKeyAction(const KeyCodeAction &action)
{
    switch (action.kind) {
    case TT_KEY_CODE_MACRO:
        startNamedMacro(action.text);
        break;
    case TT_KEY_CODE_COMMAND:
        invokeMenuCommand(static_cast<quint16>(action.value));
        break;
    default:
        // Sent, or a kind with nothing for the window to do. The core has
        // already put whatever there was on the wire.
        break;
    }
}

// --- quick buttons ---------------------------------------------------------

void MainWindow::reloadQuickButtons()
{
    if (!m_quickBar) {
        return;
    }
    // Every run is an index into the list that is about to be replaced, and a
    // button at index 3 after an edit need not be the button that was at index
    // 3 before it. Following one would mean guessing which; stopping is the
    // answer that cannot be wrong, and the press to start it again is one
    // click.
    if (m_quickRepeat) {
        m_quickRepeat->stopAll();
    }
    const QVector<QuickButton> buttons = loadQuickButtons(m_settingsPath);
    m_quickBar->setButtons(buttons);

    // Shortcuts live on the bar's own actions, so hiding the bar hands the
    // keys back to the terminal. That is the honest behaviour: a shortcut is a
    // key the host stops receiving, and somebody who has put the bar away has
    // not asked to keep paying for it.
    for (int i = 0; i < buttons.size(); i++) {
        if (buttons[i].shortcut.isEmpty()) {
            continue;
        }
        const QKeySequence sequence = QKeySequence::fromString(
            buttons[i].shortcut, QKeySequence::PortableText);
        QAction *action = m_quickBar->findChild<QAction *>(
            QStringLiteral("quickButton%1").arg(i));
        if (!action) {
            continue;
        }
        if (sequence.isEmpty()) {
            onNotice(tr("Quick button \"%1\" has an invalid shortcut: %2")
                         .arg(buttons[i].caption(), buttons[i].shortcut));
            continue;
        }
        action->setShortcutContext(Qt::WindowShortcut);
        action->setShortcut(sequence);
    }

    // **Only when the setting itself has moved**, not whenever the dock is not
    // where the file says. A drag is the user placing it, and this runs on
    // every edit of the list — comparing against the live area would put the
    // bar back at the file's edge the moment somebody added a button, and the
    // file is not written until the window closes.
    const QString setting =
        m_session->setting(QStringLiteral("window.quick_buttons_area"));
    if (setting != m_quickDockArea) {
        m_quickDockArea = setting;
        addDockWidget(quickButtonArea(setting), m_quickDock);
    }
    // The setting alone owns visibility. An empty list still has a useful +
    // button, which is the shortest route to defining the first command.
    const bool wanted =
        m_session->setting(QStringLiteral("window.quick_buttons")) == QLatin1String("on");
    m_quickDock->setVisible(wanted);
    if (m_quickButtonsAction) {
        m_quickButtonsAction->setChecked(wanted);
    }
    m_quickBar->refresh(m_session);
}

void MainWindow::runQuickButton(int index, bool withoutEnter)
{
    if (!m_quickBar || index < 0 || index >= m_quickBar->buttons().size()) {
        return;
    }
    // A second press stops the run rather than starting a rival one — and
    // before the confirmation, because "are you sure?" is not a question to
    // ask somebody who is trying to make something stop.
    if (m_quickRepeat->isRunning(index)) {
        // On the page the run was sending to, which need not be the active
        // one — that page is where its effect was visible, so that is where
        // "it has stopped" belongs.
        TerminalPage *ran = m_quickRepeatPage.value(index).data();
        m_quickRepeat->stop(index);
        showPageMessage(ran, tr("Stopped repeating"), 3000);
        return;
    }

    const QuickButton button = withoutEnter
        ? m_quickBar->buttons()[index].withoutEnter()
        : m_quickBar->buttons()[index];

    if (button.confirm
        && QMessageBox::question(this, tr("Quick button"),
                                 tr("%1\n\n%2")
                                     .arg(button.caption(), button.describe()))
            != QMessageBox::Yes) {
        return;
    }

    // Recorded before the first send, so that every send in the run — this one
    // included — goes to the session it was started on.
    if (button.repeats()) {
        m_quickRepeatPage.insert(index, m_page);
    }
    sendQuickButton(index, withoutEnter);
    if (button.repeats()) {
        m_quickRepeat->start(index, withoutEnter, button.repeat,
                             static_cast<int>(button.intervalMs));
        if (m_quickRepeat->isRunning(index)) {
            showPageMessage(m_page, button.repeatSummary());
        } else {
            m_quickRepeatPage.remove(index);
        }
    }
    // Typing goes to the live screen, and so does pressing a button that types.
    m_view->setViewOffset(0);
    m_view->setFocus();
}

void MainWindow::sendQuickButton(int index, bool withoutEnter)
{
    if (!m_quickBar || index < 0 || index >= m_quickBar->buttons().size()) {
        return;
    }
    // A repeat sends where it was started, not to whichever tab happens to be
    // in front: switching tabs to watch something else must never redirect a
    // poll onto a different console.
    Session *session = m_session;
    if (m_quickRepeatPage.contains(index)) {
        TerminalPage *page = m_quickRepeatPage.value(index);
        if (!page) {
            // Its tab has been closed. Nothing to send to and nothing to say:
            // the window that would have shown a complaint has gone.
            m_quickRepeat->stop(index);
            return;
        }
        session = page->session();
    }
    if (!session) {
        return;
    }
    const QuickButton button = withoutEnter
        ? m_quickBar->buttons()[index].withoutEnter()
        : m_quickBar->buttons()[index];
    // A run whose line has gone ends here, and not only in
    // `stopRepeatsWithNoLink`: that one runs off `updateStatus`, which a
    // background page's disconnect does not reach. This is the tick itself, so
    // it is the one check no run can be going without.
    const bool needsLink = button.kind == TT_QUICK_BUTTON_TEXT
        || button.kind == TT_QUICK_BUTTON_BYTES;
    if (needsLink && !session->isConnected() && m_quickRepeat->isRunning(index)) {
        m_quickRepeat->stop(index);
        return;
    }
    // The core does the sending and hands back what is left for the window,
    // which is the same answer a pressed key gives. Only the sending half is
    // bound to a page: what `runKeyAction` is left holding is a macro or a
    // menu command, and both of those are the window's, not a session's.
    runKeyAction(session->runQuickButton(button.kind, button.value));
}

void MainWindow::quickRepeatChanged(int index, int remaining)
{
    if (remaining == 0) {
        m_quickRepeatPage.remove(index);
    }
    if (m_quickBar) {
        m_quickBar->setRepeating(index, remaining);
    }
    // The stop key is the current view's, and `activatePage` re-arms whichever
    // one comes forward — a background view left armed would otherwise keep
    // swallowing an Escape the host should have had.
    if (m_view) {
        m_view->setStopKeyArmed(!m_quickRepeat->isIdle());
    }
}

void MainWindow::stopRepeatsWithNoLink()
{
    if (!m_quickBar || !m_quickRepeat || m_quickRepeat->isIdle()) {
        return;
    }
    const QVector<QuickButton> &buttons = m_quickBar->buttons();
    for (int i = 0; i < buttons.size(); i++) {
        if (!m_quickRepeat->isRunning(i)) {
            continue;
        }
        const bool needsLink = buttons[i].kind == TT_QUICK_BUTTON_TEXT
            || buttons[i].kind == TT_QUICK_BUTTON_BYTES;
        if (!needsLink) {
            continue;
        }
        // The session the run belongs to, which is not necessarily the one in
        // front. A page whose tab has gone counts as no link at all.
        Session *session = m_session;
        if (m_quickRepeatPage.contains(i)) {
            TerminalPage *page = m_quickRepeatPage.value(i);
            session = page ? page->session() : nullptr;
        }
        if (!session || !session->isConnected()) {
            m_quickRepeat->stop(i);
        }
    }
}

bool MainWindow::storeQuickButtons(const QVector<QuickButton> &buttons)
{
    QDir().mkpath(QFileInfo(m_settingsPath).absolutePath());
    QString error;
    if (!saveQuickButtons(m_settingsPath, buttons, &error)) {
        QMessageBox::warning(this, tr("Quick buttons"),
                             tr("Could not save the quick buttons: %1").arg(error));
        return false;
    }
    reloadQuickButtons();
    return true;
}

void MainWindow::showQuickButtonsDialog() { editQuickButtons(-1); }

void MainWindow::editQuickButtons(int index, const QuickButton *seed)
{
    if (!m_quickBar) {
        return;
    }
    QuickButtonsDialog dialog(m_quickBar->buttons(), m_session, this, this);
    if (seed) {
        // The `+`, Add from the context menu, and New from selection: all
        // three asked for a *new* button, so the editor opens on a new row
        // with the cursor in its label rather than on somebody else's.
        dialog.appendButton(*seed);
    } else if (index >= 0) {
        dialog.selectRow(index);
    } else if (m_quickBar->buttons().isEmpty()) {
        // Opened from the menu with nothing defined: the first thing anybody
        // wants here is a button, not an empty list looking at them.
        dialog.appendButton(QuickButton());
    }
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    storeQuickButtons(dialog.buttons());
}

void MainWindow::quickButtonFromSelection()
{
    const QString selected = m_view->selectedText();
    if (selected.trimmed().isEmpty()) {
        return;
    }
    QuickButton seed;
    seed.kind = TT_QUICK_BUTTON_TEXT;
    // Everything after the first line break is dropped rather than kept: a
    // selection spanning lines is usually output that happens to include the
    // command, and a button that sends four lines by accident is worse than
    // one somebody has to finish typing.
    seed.text = selected.section(QLatin1Char('\n'), 0, 0).trimmed()
        + QLatin1Char('\r');
    editQuickButtons(-1, &seed);
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

void MainWindow::showLogDialog()
{
    // A second log would replace the first silently. Upstream greys its Log
    // item instead of guarding here; both are true, and the guard is what
    // makes `KEYBOARD.CNF`'s 50120 safe, since a key binding does not consult
    // a menu's enabled state.
    if (m_session->isLogging()) {
        return;
    }

    LogOptionsDialog dialog(m_session, this, m_i18n);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const QString path = dialog.path();
    if (path.isEmpty()) {
        return;
    }

    // Settings first, because two of them decide what the log *is* rather than
    // how it is opened: `log.plain_text` reaches the parser's tap, and the
    // rest are what the next dialog will open on. Then the options, which
    // carry the two questions no key can be asked.
    dialog.applySettings();
    const TtLogOptions options = dialog.options();

    QString error;
    if (!m_session->startLog(path, &error, &options)) {
        QMessageBox::critical(this, tr("Logging"),
                              tr("Could not write %1.\n\n%2").arg(path, error));
        return;
    }

    // Remember only a file that was actually opened. `applySettings` cannot
    // do this: it runs before `startLog`, and a directory from a failed open
    // is not where the last log was written. This one setting is bookkeeping,
    // like the recent connection, so it survives a restart without turning
    // all the dialog's deliberately live-only choices into saved settings.
    const QString directory = QFileInfo(path).absolutePath();
    if (!directory.isEmpty()) {
        rememberSettings({{QStringLiteral("recent.log_dir"), directory}});
        // Every existing tab owns its own settings snapshot. Keep the memory
        // window-wide so the next File > Log follows the last one even after
        // the user changes tabs; a new tab copies the active snapshot already.
        for (int i = 0; i < m_panels->count(); i++) {
            auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
            if (page->session() == m_session) {
                continue;
            }
            QString ignored;
            page->session()->setSetting(QStringLiteral("recent.log_dir"), directory,
                                        &ignored);
        }
    }
    updateStatus();
}

void MainWindow::togglePauseLogging()
{
    if (!m_session->isLogging()) {
        return;
    }
    const bool paused = !m_session->logPaused();
    m_session->pauseLog(paused);
    showPageMessage(m_page, paused ? tr("Logging paused") : tr("Logging resumed"), 3000);
    updateStatus();
}

void MainWindow::stopLogging()
{
    if (!m_session->isLogging()) {
        return;
    }
    m_session->stopLog();
    showPageMessage(m_page, tr("Logging stopped"), 3000);
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

void MainWindow::chooseKeyMap()
{
    const QString path = QFileDialog::getOpenFileName(
        this, m_i18n->plainText("MENU_SETUP_LOADKEYMAP", tr("Load key map")),
        m_keyMapPath,
        tr("Tera Term key maps (*.cnf *.CNF);;All files (*)"));
    if (!path.isEmpty()) {
        loadKeyMap(path);
    }
}

void MainWindow::onTitleChanged(const QString &title) { showTitle(title); }

void MainWindow::showTitle(const QString &title)
{
    WindowTitleState state;
    state.title = title;
    state.configuredTitle = m_session->setting(QStringLiteral("terminal.title"));
    state.upstreamDefaultTitle = settingDefault("terminal.title");
    state.productTitle = tr("Sterna");
    state.titleChange = m_session->setting(QStringLiteral("window.title_change"));
    state.endpoint = m_session->connectionHost();
    state.tcpPort = m_session->connectionPort();
    state.serialBaud = m_session->serialBaud();
    state.linkKind = m_session->linkKind();
    state.connected = m_session->isConnected();
    state.connecting = m_session->isConnecting();
    state.format = m_session->setting(QStringLiteral("window.title_format")).toInt();

    // A local pty has no upstream `PortType`: CygTerm appears there as TCP.
    // Its transport description is the useful equivalent of a host name.
    if (state.endpoint.isEmpty() && state.linkKind == TT_LINK_LOCAL_PTY) {
        state.endpoint = m_session->describe();
    }

    setWindowTitle(formatWindowTitle(
        state,
        m_i18n->text("DLG_MAIN_TITLE_CONNECTING", tr("[connecting...]")),
        m_i18n->text("DLG_MAIN_TITLE_DISCONNECTED", tr("[disconnected]"))));
}

void MainWindow::onNotice(const QString &text)
{
    // The window's own remarks — an unreadable settings file, a plugin that
    // would not load, a macro that finished — land on whichever terminal is in
    // front. A *session's* remarks go to that session's own strip instead; see
    // the `notice` wiring in `wirePage`.
    showPageMessage(nullptr, text);
}

void MainWindow::onConnectionChanged()
{
    showTitle(m_session->title());
    updateStatus();
}

void MainWindow::updateLogStatus(TerminalPage *page)
{
    Session *session = page->session();
    const bool logging = session->isLogging();
    // Reached from `Session::damaged`, which fires on every read on **every**
    // open session — not just the visible one and not just the active one, as
    // it was when this fed a single window-wide label. So the byte count is
    // not even asked for unless this page is recording, and `setLogging`
    // compares before it assigns: a quiet page costs a pointer test, and a
    // busy one costs a relayout only when the formatted size actually moves.
    page->status()->setLogging(logging, logging ? session->logBytes() : 0,
                               logging && session->logPaused());
}

void MainWindow::updatePanelActions()
{
    if (m_tiledAction) {
        m_tiledAction->setChecked(m_panels->layoutMode() == PanelLayout::Tiled);
    }
}

void MainWindow::markActiveTile()
{
    // Only when there is more than one tile on screen. The marker answers "which
    // of these is the menus' target"; with a single terminal — tabbed, or tiled
    // with one connection — nothing is asking, and a permanently highlighted
    // strip reads as a stuck state rather than an answer.
    const bool several = m_panels->layoutMode() == PanelLayout::Tiled
                         && m_panels->tileCount() > 1;
    for (int i = 0; i < m_panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(m_panels->widget(i));
        page->status()->setActive(several && page == m_page);
    }
}

void MainWindow::updatePageStatus(TerminalPage *page)
{
    Session *session = page->session();
    page->status()->setConnection(session->isConnected(),
                                  session->isConnecting(),
                                  session->describe());
    updateLogStatus(page);
}

void MainWindow::showPageMessage(TerminalPage *page, const QString &text,
                                 int ms)
{
    TerminalPage *target = page ? page : m_page;
    if (target) {
        target->status()->showMessage(text, ms);
    }
}

void MainWindow::updateStatus()
{
    // `vtwin.cpp:1176`'s table: which of the three is reachable depends only
    // on whether a log is open, and Pause carries the tick.
    const bool logging = m_session->isLogging();
    if (m_logAction) {
        m_logAction->setEnabled(!logging);
    }
    if (m_pauseLogAction) {
        m_pauseLogAction->setEnabled(logging);
        m_pauseLogAction->setChecked(logging && m_session->logPaused());
    }
    if (m_stopLogAction) {
        m_stopLogAction->setEnabled(logging);
    }
    updatePageStatus(m_page);
    if (m_stopMacroAction) {
        m_stopMacroAction->setEnabled(m_macro->running());
    }

    const bool connected = m_session->isConnected();
    const bool connecting = m_session->isConnecting();
    if (m_connectBar) {
        m_connectBar->refresh(m_session);
    }
    if (m_quickBar) {
        // Before the refresh, so a run that has just lost its line is already
        // over by the time the bar decides what to grey out.
        stopRepeatsWithNoLink();
        m_quickBar->refresh(m_session);
    }
    if (m_disconnectAction) {
        // Enabled while connecting too: stopping an attempt that is waiting on
        // a slow key exchange is a thing people need to be able to do.
        m_disconnectAction->setEnabled(connected || connecting);
    }
    if (m_duplicateAction) {
        const bool disabled =
            m_session->setting(QStringLiteral("menu.disable_duplicate"))
            == QLatin1String("on");
        m_duplicateAction->setEnabled(!disabled && m_session->canDuplicate());
    }
    if (m_breakAction) {
        // Asked of the core rather than inferred from the transport: SSH has
        // no break — RFC 4335 defines one and russh does not implement it —
        // and offering the item anyway offers an error message at the moment
        // a console has stopped answering.
        const bool menuDisabled =
            m_session->setting(QStringLiteral("menu.disable_send_break"))
            == QLatin1String("on");
        m_breakAction->setEnabled(m_session->supportsBreak() && !menuDisabled);
    }
    // `DisableMenuNewConnection` is consulted only while a connection is
    // already open (`vtwin.cpp:1133`); the item is always grey while a new one
    // is still connecting. One action, as upstream has — the gate covers every
    // transport because the screen behind it does. Local shell is Cygwin
    // connection's counterpart and remains independent upstream too.
    const bool disableNew =
        m_session->setting(QStringLiteral("menu.disable_new_connection"))
        == QLatin1String("on");
    const bool canOpenNew = !connecting && (!connected || !disableNew);
    if (m_connectAction) {
        m_connectAction->setEnabled(canOpenNew);
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
