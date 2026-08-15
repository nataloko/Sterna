// Multiple sessions in one window.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QDialog>
#include <QElapsedTimer>
#include <QFile>
#include <QLabel>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QMenu>
#include <QPushButton>
#include <QScrollBar>
#include <QStatusBar>
#include <QStatusTipEvent>
#include <QTabBar>
#include <QTemporaryDir>
#include <QTimer>
#include <QVBoxLayout>
#include <QVector>

#include <cstdio>
#include <cstring>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

#include "MainWindow.h"
#include "ConnectBar.h"
#include "PageStatusBar.h"
#include "PanelContainer.h"
#include "Session.h"
#include "ConnectDialog.h"
#include "TerminalPage.h"
#include "TerminalView.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

#ifdef Q_OS_WIN
using Socket = SOCKET;
constexpr Socket kNoSocket = INVALID_SOCKET;
inline void closeSocket(Socket s) { ::closesocket(s); }
#else
using Socket = int;
constexpr Socket kNoSocket = -1;
inline void closeSocket(Socket s) { ::close(s); }
#endif

/// One endpoint which can accept both the original telnet session and its
/// duplicate. The kernel completes each connect from the backlog, so the GUI
/// thread need not accept concurrently with the action it is testing.
class Listener {
public:
    Listener()
    {
#ifdef Q_OS_WIN
        WSADATA wsa;
        if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
            return;
        }
        m_started = true;
#endif
        m_fd = ::socket(AF_INET, SOCK_STREAM, 0);
        if (m_fd == kNoSocket) {
            return;
        }
        int on = 1;
        ::setsockopt(m_fd, SOL_SOCKET, SO_REUSEADDR,
                     reinterpret_cast<const char *>(&on), sizeof on);
        sockaddr_in addr = {};
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        addr.sin_port = 0;
        socklen_t len = sizeof addr;
        if (::bind(m_fd, reinterpret_cast<sockaddr *>(&addr), len) != 0
            || ::listen(m_fd, 2) != 0
            || ::getsockname(m_fd, reinterpret_cast<sockaddr *>(&addr), &len)
                   != 0) {
            closeSocket(m_fd);
            m_fd = kNoSocket;
            return;
        }
        m_port = ntohs(addr.sin_port);
    }

    ~Listener()
    {
        for (Socket client : m_clients) {
            closeSocket(client);
        }
        if (m_fd != kNoSocket) {
            closeSocket(m_fd);
        }
#ifdef Q_OS_WIN
        if (m_started) {
            WSACleanup();
        }
#endif
    }

    quint16 port() const { return m_port; }

    void acceptOne()
    {
        const Socket client = ::accept(m_fd, nullptr, nullptr);
        CHECK(client != kNoSocket);
        if (client != kNoSocket) {
            m_clients.append(client);
        }
    }

private:
    Socket m_fd = kNoSocket;
    QVector<Socket> m_clients;
    quint16 m_port = 0;
#ifdef Q_OS_WIN
    bool m_started = false;
#endif
};

QString screenText(const Session &session)
{
    QString out;
    for (int y = 0; y < session.rows(); y++) {
        size_t len = 0;
        const TtCell *row = session.row(y, &len);
        for (size_t x = 0; row && x < len; x++) {
            if (row[x].width_class != TT_WIDTH_PAD && row[x].text[0] != 0) {
                out += QChar(static_cast<char16_t>(row[x].text[0]));
            }
        }
        out += QLatin1Char('\n');
    }
    return out;
}

QWidget *mockPage(const QString &name)
{
    auto *page = new QWidget;
    page->setObjectName(name);
    auto *layout = new QVBoxLayout(page);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(new QLabel(name, page));
    auto *scroll = new QScrollBar(Qt::Vertical, page);
    scroll->setObjectName(name + QStringLiteral("Scroll"));
    layout->addWidget(scroll);
    return page;
}

void press(QWidget *widget)
{
    CHECK(widget != nullptr);
    if (!widget) {
        return;
    }
    const QPointF point(2, 2);
    QMouseEvent event(QEvent::MouseButtonPress, point, point,
                      widget->mapToGlobal(point.toPoint()), Qt::LeftButton,
                      Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(widget, &event);
}

void type(TerminalView *view, int code, const QString &text)
{
    CHECK(view != nullptr);
    if (!view) {
        return;
    }
    QKeyEvent event(QEvent::KeyPress, code, Qt::NoModifier, text);
    QApplication::sendEvent(view, &event);
}

/// The grid is a function of how many connections there are.
///
/// `cols = ceil(sqrt(n))`, rows to suit: 1, 1x2, 2x2, 2x2, 2x3, 2x3, 3x3...
/// Tiles are tab order, exactly, and every connection has one — there is no
/// hidden page in tiled mode, which is the half of the old design that made
/// "where are my connections" have two answers.
void test_tiled_panels_fit_the_session_count()
{
    PanelContainer panels;
    panels.resize(900, 700);
    panels.setLayoutMode(PanelLayout::Tiled);
    CHECK(panels.tileCount() == 1);
    // With nothing open the sole tile offers a connection rather than being a
    // grey rectangle.
    CHECK(panels.firstEmptyPanel() == 0);

    QVector<QWidget *> pages;
    const auto add = [&](const char *name) {
        QWidget *page = mockPage(QString::fromLatin1(name));
        pages.append(page);
        panels.addPage(page, QString::fromLatin1(name).toUpper());
    };

    add("a");
    CHECK(panels.tileCount() == 1);   // 1x1, exactly full
    CHECK(panels.tileColumns() == 1);
    CHECK(panels.firstEmptyPanel() == -1);
    add("b");
    CHECK(panels.tileCount() == 2);   // 1x2, exactly full
    CHECK(panels.tileColumns() == 2);
    CHECK(panels.firstEmptyPanel() == -1);
    add("c");
    CHECK(panels.tileCount() == 4);   // 2x2 with one spare
    CHECK(panels.tileColumns() == 2);
    CHECK(panels.firstEmptyPanel() == 3);
    add("d");
    CHECK(panels.tileCount() == 4);   // 2x2, exactly full again
    CHECK(panels.firstEmptyPanel() == -1);
    add("e");
    CHECK(panels.tileCount() == 6);   // 2x3 with one spare
    CHECK(panels.tileColumns() == 3);
    CHECK(panels.firstEmptyPanel() == 5);

    // Tiles are tab order and every page has one.
    for (int i = 0; i < pages.size(); i++) {
        CHECK(panels.pageAtPanel(i) == pages[i]);
        CHECK(panels.panelOf(pages[i]) == i);
    }
    CHECK(panels.visiblePages().size() == pages.size());
    CHECK(panels.tabBar()->isHidden());

    panels.show();
    QApplication::processEvents();
    auto *f0 = panels.findChild<QWidget *>(QStringLiteral("panelFrame0"));
    auto *f1 = panels.findChild<QWidget *>(QStringLiteral("panelFrame1"));
    auto *f3 = panels.findChild<QWidget *>(QStringLiteral("panelFrame3"));
    CHECK(f0 && f1 && f3);
    if (f0 && f1 && f3) {
        // Same row: equal height, different x. Next row down: same x as the
        // first of its row, greater y.
        CHECK(qAbs(f0->height() - f1->height()) <= 1);
        CHECK(qAbs(f0->width() - f1->width()) <= 1);
        CHECK(f1->x() > f0->x());
        CHECK(f3->y() > f0->y());
        CHECK(qAbs(f3->x() - f0->x()) <= 1);
    }

    // Clicking a page's child — the terminal's scrollbar in production —
    // routes the shared window actions to that tile. There is no pane header
    // to click any more; the page's own status strip is the marker.
    press(pages[1]->findChild<QScrollBar *>(QStringLiteral("bScroll")));
    CHECK(panels.currentWidget() == pages[1]);

    // At seven the rectangle is 3x3 with *two* cells over, so the spare takes
    // both rather than leaving a hole beside itself.
    add("f");
    add("g");
    CHECK(panels.count() == 7);
    CHECK(panels.tileColumns() == 3);
    CHECK(panels.tileCount() == 8);
    CHECK(panels.firstEmptyPanel() == 7);
    QApplication::processEvents();
    auto *spare = panels.findChild<QWidget *>(QStringLiteral("panelFrame7"));
    CHECK(spare != nullptr);
    if (spare && f0) {
        CHECK(spare->width() > f0->width() * 3 / 2);
    }

    // Closing one re-tiles rather than leaving a gap, and nothing else moves
    // out of tab order.
    QWidget *removed = panels.removePage(panels.indexOf(pages[0]));
    CHECK(removed == pages[0]);
    CHECK(panels.count() == 6);
    CHECK(panels.pageAtPanel(0) == pages[1]);
    CHECK(panels.firstEmptyPanel() == -1);
    delete pages[0];

    // Back to tabs: the same six connections, one visible, and the bar returns.
    panels.setLayoutMode(PanelLayout::Single);
    CHECK(panels.count() == 6);
    CHECK(panels.tileCount() == 1);
    CHECK(panels.visiblePages().size() == 1);
    CHECK(panels.visiblePages().first() == panels.currentWidget());
    QApplication::processEvents();
    CHECK(!panels.tabBar()->isHidden());
    // A page that is not the current one is hidden, not closed. `pages[6]` is
    // the last one added and so is the current one; `pages[1]` is not.
    CHECK(panels.currentWidget() == pages[6]);
    CHECK(panels.indexOf(pages[1]) >= 0);
    CHECK(panels.panelOf(pages[1]) == -1);
}

void test_empty_panels_request_connections_without_creating_pages()
{
    PanelContainer panels;
    panels.setLayoutMode(PanelLayout::Tiled);
    // Three connections make a 2x2 whose fourth cell is the spare one.
    QWidget *a = mockPage(QStringLiteral("a"));
    QWidget *b = mockPage(QStringLiteral("b"));
    QWidget *c = mockPage(QStringLiteral("c"));
    panels.addPage(a, QStringLiteral("A"));
    panels.addPage(b, QStringLiteral("B"));
    panels.addPage(c, QStringLiteral("C"));
    CHECK(panels.firstEmptyPanel() == 3);

    int requestedPanel = -1;
    PanelContainer::ConnectionKind requestedKind =
        PanelContainer::ConnectionKind::Serial;
    QObject::connect(
        &panels, &PanelContainer::emptyConnectionRequested, &panels,
        [&](int panel, PanelContainer::ConnectionKind kind) {
            requestedPanel = panel;
            requestedKind = kind;
        });
    auto *ssh = panels.findChild<QPushButton *>(QStringLiteral("panelSsh3"));
    CHECK(ssh != nullptr);
    if (ssh) {
        ssh->click();
    }
    CHECK(requestedPanel == 3);
    CHECK(requestedKind == PanelContainer::ConnectionKind::Ssh);
    CHECK(panels.count() == 3);
    CHECK(panels.pageAtPanel(3) == nullptr);
}

void test_tabs_are_independent_and_actions_follow_the_active_one()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\r\nTerminalSize=40,10\r\n"
               "[Sterna]\r\nPanelLayout=tiled\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    auto *close = window.findChild<QAction *>(QStringLiteral("closeTabAction"));
    CHECK(panels != nullptr);
    CHECK(add != nullptr);
    CHECK(close != nullptr);
    CHECK(panels->count() == 1);
    CHECK(panels->layoutMode() == PanelLayout::Tiled);
    // Tiles and tabs are exclusive: the bar is not merely auto-hidden for one
    // connection, it is gone for as long as tiles are on.
    CHECK(panels->tabBar()->isHidden());

    auto *first = static_cast<TerminalPage *>(panels->widget(0));
    first->session()->feed(QByteArrayLiteral("first"));

    add->trigger();
    CHECK(panels->count() == 2);
    CHECK(panels->tabsClosable());
    auto *second = static_cast<TerminalPage *>(panels->widget(1));
    CHECK(first != second);
    CHECK(window.session() == second->session());
    CHECK(second->session()->cols() == 40);
    CHECK(second->session()->rows() == 10);

    second->session()->feed(QByteArrayLiteral("second"));
    CHECK(screenText(*first->session()).contains(QStringLiteral("first")));
    CHECK(!screenText(*first->session()).contains(QStringLiteral("second")));
    CHECK(screenText(*second->session()).contains(QStringLiteral("second")));
    CHECK(!screenText(*second->session()).contains(QStringLiteral("first")));

    // Window actions resolve the active page when they are triggered. Holding
    // the first view from `buildMenus` would silently clear the wrong tab.
    auto *clearScreen =
        window.findChild<QAction *>(QStringLiteral("clearScreenAction"));
    auto *clearBuffer =
        window.findChild<QAction *>(QStringLiteral("clearBufferAction"));
    CHECK(clearScreen != nullptr);
    CHECK(clearBuffer != nullptr);
    clearScreen->trigger();
    CHECK(screenText(*first->session()).contains(QStringLiteral("first")));
    CHECK(screenText(*second->session()).trimmed().isEmpty());
    CHECK(second->session()->scrollbackLen() == second->session()->rows());
    panels->setCurrentIndex(0);
    clearBuffer->trigger();
    CHECK(screenText(*first->session()).trimmed().isEmpty());
    CHECK(first->session()->scrollbackLen() == 0);
    CHECK(second->session()->scrollbackLen() == second->session()->rows());
    panels->setCurrentIndex(1);

    // The shared local-echo checkbox follows the active page. These pages are
    // deliberately offline, so the now-grey control cannot change them; a
    // script or host assignment still refreshes the displayed state.
    auto *echo =
        window.findChild<QCheckBox *>(QStringLiteral("connectBarLocalEcho"));
    CHECK(echo != nullptr);
    QString error;
    CHECK(second->session()->setSetting(QStringLiteral("terminal.local_echo"),
                                        QStringLiteral("on"), &error));
    CHECK(echo && echo->isChecked());

    panels->setCurrentIndex(0);
    CHECK(window.session() == first->session());
    CHECK(echo && !echo->isChecked());
    CHECK(echo && !echo->isEnabled());
    CHECK(first->session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), &error));
    CHECK(first->session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("on"));
    panels->setCurrentIndex(1);
    CHECK(window.session() == second->session());
    CHECK(echo && echo->isChecked());
    CHECK(echo && !echo->isEnabled());
    CHECK(second->session()->setSetting(QStringLiteral("terminal.local_echo"),
                                        QStringLiteral("off"), &error));
    CHECK(second->session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("off"));
    CHECK(first->session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("on"));

    // Line editing is per page too. The shared checkbox follows the active
    // page, and a draft stays with the page while another panel is active.
    auto *line =
        window.findChild<QCheckBox *>(QStringLiteral("connectBarLineEdit"));
    CHECK(line != nullptr);
    CHECK(second->session()->setSetting(QStringLiteral("terminal.line_edit"),
                                        QStringLiteral("on"), &error));
    CHECK(line && line->isChecked());
    panels->setCurrentIndex(0);
    CHECK(line && !line->isChecked());
    CHECK(line && !line->isEnabled());
    CHECK(first->session()->setSetting(QStringLiteral("terminal.line_edit"),
                                       QStringLiteral("on"), &error));
    CHECK(first->session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("on"));
    type(first->view(), Qt::Key_D, QStringLiteral("d"));
    type(first->view(), Qt::Key_R, QStringLiteral("r"));
    CHECK(first->view()->lineEditText() == QStringLiteral("dr"));
    panels->setCurrentIndex(1);
    CHECK(line && line->isChecked());
    CHECK(first->view()->lineEditText() == QStringLiteral("dr"));
    panels->setCurrentIndex(0);
    CHECK(first->view()->lineEditText() == QStringLiteral("dr"));

    // Clicking inside a tile routes the shared window actions to it. The pane
    // header this used to click is gone; the page's own status strip is both
    // the name and the marker now.
    press(first->status());
    CHECK(window.session() == first->session());
    CHECK(echo && echo->isChecked());
    press(second->findChild<QScrollBar *>(QStringLiteral("terminalScrollBar")));
    CHECK(window.session() == second->session());
    CHECK(second->status()->palette().color(QPalette::Window)
          == second->status()->palette().color(QPalette::Highlight));
    // ...and the one that is no longer active gives the highlight back.
    CHECK(first->status()->palette().color(QPalette::Window)
          != first->status()->palette().color(QPalette::Highlight));

    close->trigger();
    CHECK(panels->count() == 1);
    CHECK(window.session() == first->session());
    CHECK(!panels->tabsClosable());
}

void test_connection_selector_follows_the_active_page()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=tiled\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *bar = window.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(panels != nullptr);
    CHECK(bar != nullptr);
    if (!panels || !bar) {
        return;
    }

    window.connectTelnet(QStringLiteral("127.0.0.1"), listener.port());
    listener.acceptOne();
    auto *first = static_cast<TerminalPage *>(panels->currentWidget());
    const QString firstLabel =
        QStringLiteral("telnet 127.0.0.1:%1").arg(listener.port());
    CHECK(bar->destination() == firstLabel);

    window.connectTelnet(QStringLiteral("localhost"), listener.port());
    listener.acceptOne();
    auto *second = static_cast<TerminalPage *>(panels->currentWidget());
    const QString secondLabel =
        QStringLiteral("telnet localhost:%1").arg(listener.port());
    CHECK(second != first);
    CHECK(bar->destination() == secondLabel);

    // Tiled pages share the same toolbar, but selecting one makes all of its
    // per-session controls authoritative, including the destination record.
    panels->setCurrentWidget(first);
    CHECK(window.session() == first->session());
    CHECK(bar->destination() == firstLabel);
    panels->setCurrentWidget(second);
    CHECK(window.session() == second->session());
    CHECK(bar->destination() == secondLabel);

    // The same currentChanged path serves ordinary tabs.
    panels->setLayoutMode(PanelLayout::Single);
    panels->setCurrentWidget(first);
    CHECK(bar->destination() == firstLabel);
    panels->setCurrentWidget(second);
    CHECK(bar->destination() == secondLabel);

    // A page connected to nothing has nothing to say, and saying it anyway
    // empties the field somebody is about to connect from — including the one
    // `loadRecents` fills in so that going back where you were is one click.
    // Every open makes a page, so this is also the field a second connection
    // that failed comes back to.
    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    auto *blank = static_cast<TerminalPage *>(panels->currentWidget());
    CHECK(blank != second);
    CHECK(!blank->session()->isConnected());
    CHECK(bar->destination() == secondLabel);
    panels->setCurrentWidget(second);
    CHECK(bar->destination() == secondLabel);
}

void test_new_tabs_load_the_saved_line_edit_default()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nLineEdit=on\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    CHECK(panels != nullptr);
    auto *first = static_cast<TerminalPage *>(panels->widget(0));
    CHECK(first->view()->lineEditEnabled());

    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    CHECK(panels->count() == 2);
    auto *second = static_cast<TerminalPage *>(panels->widget(1));
    CHECK(second->view()->lineEditEnabled());
    CHECK(second->session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("on"));
}

/// View > Tiled switches the layout, and writes only the one key back.
void test_the_view_menu_switches_tiling_and_persists_only_that_key()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    // `four` is the 0.2.x spelling of "several panels" and has to keep opening
    // a tiled window — the file may have been written by that version.
    file.write("; keep this byte-for-byte\n[Sterna]\nOther = untouched\n"
               "PanelLayout = four\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    CHECK(panels != nullptr);
    CHECK(window.findChild<QMenu *>(QStringLiteral("viewMenu")) != nullptr);
    auto *tiled = window.findChild<QAction *>(QStringLiteral("tiledAction"));
    CHECK(tiled != nullptr);
    if (!tiled) {
        return;
    }
    CHECK(tiled->isCheckable());
    // No shortcut: a QAction shortcut silently outranks the terminal's own key
    // handling, so every one installed here is a key the host stops receiving.
    CHECK(tiled->shortcut().isEmpty());
    CHECK(panels->layoutMode() == PanelLayout::Tiled);
    CHECK(tiled->isChecked());

    // Reading the file changes nothing in it.
    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll()
          == QByteArray("; keep this byte-for-byte\n[Sterna]\n"
                        "Other = untouched\nPanelLayout = four\n"));
    file.close();

    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    CHECK(panels->count() == 2);

    // The menu item, not `setSetting` — this is the route a person takes.
    // `trigger()` toggles a checkable action itself, so do not pre-set it.
    tiled->trigger();
    CHECK(panels->layoutMode() == PanelLayout::Single);
    for (int i = 0; i < panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(panels->widget(i));
        CHECK(page->session()->setting(QStringLiteral("window.panel_layout"))
              == QStringLiteral("single"));
    }

    tiled->trigger();
    CHECK(panels->layoutMode() == PanelLayout::Tiled);
    for (int i = 0; i < panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(panels->widget(i));
        CHECK(page->session()->setting(QStringLiteral("window.panel_layout"))
              == QStringLiteral("tiled"));
    }

    // One key rewritten, in this version's spelling, and the rest of the file
    // byte-for-byte — comment, spacing and unrelated key included.
    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll()
          == QByteArray("; keep this byte-for-byte\n[Sterna]\n"
                        "Other = untouched\nPanelLayout=tiled\n"));
    file.close();

    // ...and a generic settings surface — the dialog, a macro's `setsetting`,
    // a plugin — moves the tick as well as the layout.
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.panel_layout"),
                                       QStringLiteral("single"), &error));
    CHECK(panels->layoutMode() == PanelLayout::Single);
    CHECK(!tiled->isChecked());

    const QString malformed = dir.filePath(QStringLiteral("malformed.ini"));
    file.setFileName(malformed);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=diagonal\r\n");
    file.close();
    MainWindow fallback(malformed);
    auto *fallbackPanels = fallback.findChild<PanelContainer *>();
    CHECK(fallbackPanels->layoutMode() == PanelLayout::Single);
}

/// A window the user has resized keeps that size when a setting is applied.
///
/// `onSettingsChanged` resizes the window to `TerminalSize` when it differs
/// from the live grid, which is what makes a configured 132x50 arrive from the
/// file. The guard only holds because a resize moves `TerminalSize` with it
/// upstream (`buffer.c:5022`, transcribed in `Session::resize`) — without that
/// the setting stays at whatever the file said, and clicking Local echo, Line
/// edit, or anything in the settings dialog snapped the window back to 80x24.
void test_a_resized_window_survives_a_settings_change()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\r\nTerminalSize=80,24\r\n");
    file.close();

    MainWindow window(ini);
    window.show();
    QApplication::processEvents();
    auto *session = window.session();
    // Not 80 by assertion: offscreen calls the screen 800x800 and Qt caps a
    // window's initial size at two thirds of it, so what the window opened at
    // is the platform's business. What matters is that growing it moves the
    // terminal, and that the setting goes with it.
    const int startCols = session->cols();

    auto *view = window.findChild<TerminalView *>();
    const QSize cell = view->sizeForCells(1, 1);
    window.resize(window.size() + QSize(cell.width() * 10, cell.height() * 4));
    QApplication::processEvents();
    QApplication::processEvents();
    const QSize resized = window.size();
    const int cols = session->cols();
    const int rows = session->rows();
    CHECK(cols > startCols);
    // The setting followed the window, so nothing below has a stale size to
    // snap back to.
    CHECK(session->setting(QStringLiteral("terminal.cols")).toInt() == cols);
    CHECK(session->setting(QStringLiteral("terminal.rows")).toInt() == rows);

    for (const QString &name : {QStringLiteral("terminal.local_echo"),
                                QStringLiteral("terminal.line_edit"),
                                QStringLiteral("keyboard.disable_app_keypad")}) {
        QString error;
        CHECK(session->setSetting(name, QStringLiteral("on"), &error));
        QApplication::processEvents();
        QApplication::processEvents();
        CHECK(window.size() == resized);
        CHECK(session->cols() == cols);
        CHECK(session->rows() == rows);
    }

    // ...while a size that really did change still moves the window, which is
    // the behaviour the guard exists for. The window resize is a request to the
    // compositor, so the grid does not reach the new width until the resize
    // event has come back and the view refitted to it.
    QString error;
    CHECK(session->setSetting(QStringLiteral("terminal.cols"),
                              QString::number(cols + 10), &error));
    QElapsedTimer clock;
    clock.start();
    while (session->cols() != cols + 10 && clock.elapsed() < 2000) {
        QApplication::processEvents(QEventLoop::AllEvents, 20);
    }
    CHECK(session->cols() == cols + 10);
    CHECK(window.size() != resized);
}

/// Each terminal states its own business on its own line.
///
/// The whole reason the window's single `QStatusBar` was retired: with four
/// tiles it said "connected to router1, REC 4.2 MB" and there was nothing on
/// screen to say which of the four that was about. So a message raised by a
/// background session must land on *that* session's strip and must not touch
/// the one in front.
void test_the_status_strip_belongs_to_its_own_page()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\n");
    file.close();

    MainWindow window(ini);
    // Never `window.statusBar()` — that call *creates* one, so asking whether
    // the window has a status bar with it is a question that changes its own
    // answer. Ask Qt for the child instead.
    CHECK(window.findChild<QStatusBar *>() == nullptr);

    auto *panels = window.findChild<PanelContainer *>();
    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    CHECK(panels->count() == 2);
    auto *first = static_cast<TerminalPage *>(panels->widget(0));
    auto *second = static_cast<TerminalPage *>(panels->widget(1));
    CHECK(first->status() != nullptr);
    CHECK(second->status() != nullptr);
    CHECK(first->status() != second->status());

    // Neither is connected, so both wear the chip — and there are two of them,
    // which is the point.
    const auto chips = window.findChildren<QLabel *>(
        QStringLiteral("connectionStatus"));
    CHECK(chips.size() == 2);
    for (QLabel *chip : chips) {
        CHECK(chip->text() == QStringLiteral("not connected"));
        CHECK(chip->styleSheet().contains(QStringLiteral("#b71c1c")));
    }

    // `second` is the active page after New tab. A notice from the *other*
    // session goes to the other strip.
    first->session()->feed(QByteArray());
    emit first->session()->notice(QStringLiteral("first says so"));
    CHECK(first->status()->currentMessage() == QStringLiteral("first says so"));
    CHECK(second->status()->currentMessage().isEmpty());

    // ...and a window-level remark goes to whichever terminal is in front.
    QStatusTipEvent tip(QStringLiteral("a menu explains itself"));
    QApplication::sendEvent(&window, &tip);
    CHECK(second->status()->currentMessage()
          == QStringLiteral("a menu explains itself"));
    CHECK(first->status()->currentMessage() == QStringLiteral("first says so"));
}

/// Logging must be hard to overlook without making the rest of the strip move:
/// the REC label alternates red and transparent at a fixed width, and stopping
/// the log stops its timer and restores the ordinary palette.
void test_the_logging_indicator_blinks_red()
{
    PageStatusBar status;
    auto *log = status.findChild<QLabel *>(QStringLiteral("statusLog"));
    auto *blink =
        status.findChild<QTimer *>(QStringLiteral("statusLogBlinkTimer"));
    CHECK(log != nullptr);
    CHECK(blink != nullptr);
    if (!log || !blink) {
        return;
    }

    status.setLogging(false, 0);
    CHECK(log->text().isEmpty());
    CHECK(log->styleSheet().isEmpty());
    CHECK(!blink->isActive());

    status.setLogging(true, 44);
    CHECK(log->text().startsWith(QStringLiteral("REC ")));
    CHECK(log->styleSheet().contains(QStringLiteral("#d32f2f")));
    CHECK(blink->isActive());

    QMetaObject::invokeMethod(blink, "timeout", Qt::DirectConnection);
    CHECK(log->styleSheet().contains(QStringLiteral("transparent")));
    // Updating the byte count must not restart the blink on every receive.
    status.setLogging(true, 45);
    CHECK(log->styleSheet().contains(QStringLiteral("transparent")));

    // Paused is a third state and it has to reach the label: the early return
    // that keeps a per-read call cheap compares the whole state, not just
    // whether something is logging.
    status.setLogging(true, 45, true);
    CHECK(log->text().startsWith(QStringLiteral("PAUSED ")));
    CHECK(log->styleSheet().contains(QStringLiteral("#f9a825")));
    CHECK(!blink->isActive());
    status.setLogging(true, 46, false);
    CHECK(log->text().startsWith(QStringLiteral("REC ")));
    CHECK(blink->isActive());

    status.setLogging(false, 45);
    CHECK(log->text().isEmpty());
    CHECK(log->styleSheet().isEmpty());
    CHECK(!blink->isActive());
}

/// The left side says which host an SSH handshake is waiting for. It has no
/// timeout because a prompt may take as long as the person answering it; the
/// terminal edge therefore has to dismiss it explicitly.
void test_an_ssh_attempt_dismisses_its_connecting_message()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\n");
    file.close();

    Listener listener;
    CHECK(listener.port() != 0);
    MainWindow window(ini);
    auto *page = static_cast<TerminalPage *>(
        window.findChild<PanelContainer *>()->widget(0));

    window.connectSsh(QStringLiteral("127.0.0.1"), QString(), listener.port());
    CHECK(window.session()->isConnecting());
    CHECK(page->status()->currentMessage()
          == QStringLiteral("Connecting to 127.0.0.1..."));

    window.session()->disconnectPort();
    CHECK(!window.session()->isConnecting());
    CHECK(page->status()->currentMessage().isEmpty());

    // A message that superseded the connection progress belongs to its own
    // timer and must survive the same lifecycle edge.
    window.connectSsh(QStringLiteral("127.0.0.1"), QString(), listener.port());
    page->status()->showMessage(QStringLiteral("newer notice"), 0);
    window.session()->disconnectPort();
    CHECK(page->status()->currentMessage() == QStringLiteral("newer notice"));
}

/// A status line must not decide how wide a terminal is.
///
/// `describe()` can be a whole serial device path and a title is host-supplied,
/// so a strip whose labels quoted their own text would grow the page's size
/// hint — and the window would resize at the moment a session connected, which
/// looks like the `terminal.cols` guard misfiring and gets hunted nowhere near
/// the status line.
void test_the_status_strip_never_widens_its_page()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\n");
    file.close();

    MainWindow window(ini);
    window.show();
    QApplication::processEvents();
    auto *panels = window.findChild<PanelContainer *>();
    auto *page = static_cast<TerminalPage *>(panels->widget(0));

    const QSize before = page->sizeHint();
    const QSize windowBefore = window.size();
    page->status()->setName(QStringLiteral(
        "/dev/serial/by-path/pci-0000:c8:00.3-usb-0:1.3.2:1.0-port0"));
    page->status()->setConnection(
        true, false,
        QStringLiteral("/dev/serial/by-path/"
                       "pci-0000:c8:00.3-usb-0:1.3.2:1.0-port0 115200"));
    QApplication::processEvents();
    CHECK(page->sizeHint().width() == before.width());
    CHECK(page->minimumSizeHint().width() <= before.width());
    CHECK(window.size() == windowBefore);
}

void test_visible_panels_refit_and_receive_their_own_metrics()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=tiled\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    add->trigger();
    auto *first = static_cast<TerminalPage *>(panels->widget(0));
    auto *second = static_cast<TerminalPage *>(panels->widget(1));
    window.resize(900, 620);
    window.show();
    QApplication::processEvents();
    QApplication::processEvents();

    CHECK(qAbs(first->view()->width() - second->view()->width()) <= 1);
    CHECK(first->session()->cols() > 1 && second->session()->cols() > 1);
    CHECK(qAbs(first->session()->cols() - second->session()->cols()) <= 1);
    const TtWindowMetrics firstMetrics = first->session()->windowMetrics();
    const TtWindowMetrics secondMetrics = second->session()->windowMetrics();
    CHECK(firstMetrics.client_width == first->view()->width());
    CHECK(secondMetrics.client_width == second->view()->width());
    CHECK(firstMetrics.client_height == first->view()->height());
    CHECK(secondMetrics.client_height == second->view()->height());
    CHECK(firstMetrics.client_x != secondMetrics.client_x);
    CHECK(firstMetrics.width == secondMetrics.width);
    CHECK(firstMetrics.height == secondMetrics.height);

    // A visible background page does not resize the window when its cell size
    // changes; it refits its own grid and refreshes its metrics in place.
    const int secondCols = second->session()->cols();
    QString error;
    CHECK(first->session()->setSetting(QStringLiteral("font.space_right"),
                                       QStringLiteral("8"), &error));
    QApplication::processEvents();
    const int firstCellWidth = first->view()->sizeForCells(1, 1).width();
    CHECK(first->session()->cols() == first->view()->width() / firstCellWidth);
    CHECK(second->session()->cols() == secondCols);
    CHECK(first->session()->windowMetrics().cell_width == firstCellWidth);

    const QSize topLevel = window.size();
    // Re-tiling never resizes the top-level window — not on a layout change and
    // not on the open that turns a 1x2 into a 2x2. The client area is divided,
    // never multiplied.
    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    QApplication::processEvents();
    CHECK(panels->count() == 3);
    CHECK(window.size() == topLevel);
    CHECK(window.session()->setSetting(QStringLiteral("window.panel_layout"),
                                       QStringLiteral("single"), &error));
    QApplication::processEvents();
    CHECK(window.size() == topLevel);
}

void test_empty_panel_dialogs_cancel_or_connect_in_place()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=tiled\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    // One connection is an exactly-full 1x1 grid, so there is no spare tile to
    // click. Two make a 1x2 which is also full; three make a 2x2 whose fourth
    // cell is the spare one. That is where the connect buttons live.
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    add->trigger();
    add->trigger();
    CHECK(panels->count() == 3);
    CHECK(panels->firstEmptyPanel() == 3);

    auto *serial =
        window.findChild<QPushButton *>(QStringLiteral("panelSerial3"));
    CHECK(serial != nullptr);
    if (!serial) {
        return;
    }
    QTimer::singleShot(0, [] {
        if (auto *dialog = qobject_cast<QDialog *>(
                QApplication::activeModalWidget())) {
            dialog->reject();
        }
    });
    serial->click();
    // Cancelling allocates nothing: no page, no session, and the tile is still
    // the spare one.
    CHECK(panels->count() == 3);
    CHECK(panels->pageAtPanel(3) == nullptr);

    auto *telnet =
        window.findChild<QPushButton *>(QStringLiteral("panelTelnet3"));
    CHECK(telnet != nullptr);
    if (!telnet) {
        return;
    }
    QTimer::singleShot(0, [&] {
        if (auto *dialog = qobject_cast<ConnectDialog *>(
                QApplication::activeModalWidget())) {
            dialog->selectKind(ConnectDialog::Kind::Telnet);
            dialog->setInitialTelnet(QStringLiteral("127.0.0.1"), listener.port(),
                                     TT_TELNET_NEGOTIATE);
            dialog->accept();
        }
    });
    telnet->click();
    listener.acceptOne();
    // Accepting fills the tile it was started from and re-tiles to 2x3, so the
    // spare moves along rather than disappearing.
    CHECK(panels->count() == 4);
    CHECK(panels->firstEmptyPanel() == -1);
    auto *connected =
        static_cast<TerminalPage *>(panels->pageAtPanel(3));
    CHECK(connected != nullptr);
    CHECK(connected && connected->session()->isConnected());
}

void test_duplicate_reopens_telnet_with_the_live_settings()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=tiled\r\n");
    file.close();
    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *duplicate =
        window.findChild<QAction *>(QStringLiteral("duplicateSessionAction"));
    CHECK(panels != nullptr);
    CHECK(duplicate != nullptr);
    CHECK(!duplicate->isEnabled());

    window.connectTelnet(QStringLiteral("127.0.0.1"), listener.port());
    listener.acceptOne();
    CHECK(window.session()->canDuplicate());
    CHECK(duplicate->isEnabled());

    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("terminal.title"),
                                       QStringLiteral("copied live"), &error));
    CHECK(window.session()->setSetting(QStringLiteral("terminal.line_edit"),
                                       QStringLiteral("on"), &error));
    auto *source =
        static_cast<TerminalPage *>(panels->currentWidget());
    duplicate->trigger();
    listener.acceptOne();

    CHECK(panels->count() == 2);
    auto *copy = static_cast<TerminalPage *>(panels->currentWidget());
    CHECK(copy != source);
    CHECK(panels->panelOf(source) == 0);
    CHECK(panels->panelOf(copy) == 1);
    CHECK(copy->session()->isConnected());
    CHECK(copy->session()->canDuplicate());
    CHECK(copy->session()->setting(QStringLiteral("terminal.title"))
          == QStringLiteral("copied live"));
    CHECK(copy->session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("on"));
    CHECK(copy->view()->lineEditEnabled());
    CHECK(source->session()->isConnected());

    // Closing a background target consults that page's settings without
    // displaying or activating it along the way.
    QString closeError;
    CHECK(copy->session()->setSetting(
        QStringLiteral("connection.confirm_disconnect"), QStringLiteral("off"),
        &closeError));
    panels->setCurrentWidget(source);
    int activations = 0;
    QObject::connect(panels, &PanelContainer::currentChanged, panels,
                     [&](QWidget *) { activations++; });
    panels->closeRequested(copy);
    CHECK(panels->count() == 1);
    CHECK(panels->currentWidget() == source);
    CHECK(activations == 0);
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("tabs_test"));
    test_tiled_panels_fit_the_session_count();
    test_empty_panels_request_connections_without_creating_pages();
    test_tabs_are_independent_and_actions_follow_the_active_one();
    test_connection_selector_follows_the_active_page();
    test_new_tabs_load_the_saved_line_edit_default();
    test_the_view_menu_switches_tiling_and_persists_only_that_key();
    test_a_resized_window_survives_a_settings_change();
    test_the_status_strip_belongs_to_its_own_page();
    test_the_logging_indicator_blinks_red();
    test_an_ssh_attempt_dismisses_its_connecting_message();
    test_the_status_strip_never_widens_its_page();
    test_visible_panels_refit_and_receive_their_own_metrics();
    test_empty_panel_dialogs_cancel_or_connect_in_place();
    test_duplicate_reopens_telnet_with_the_live_settings();
    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("tabs ok");
    return 0;
}
