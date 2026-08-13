// Multiple sessions in one window.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QAction>
#include <QApplication>
#include <QDialog>
#include <QFile>
#include <QLabel>
#include <QMouseEvent>
#include <QPushButton>
#include <QScrollBar>
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
#include "PanelContainer.h"
#include "Session.h"
#include "TelnetDialog.h"
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

void test_panel_assignment_and_geometry()
{
    PanelContainer panels;
    panels.resize(800, 600);

    QWidget *a = mockPage(QStringLiteral("a"));
    QWidget *b = mockPage(QStringLiteral("b"));
    QWidget *c = mockPage(QStringLiteral("c"));
    QWidget *d = mockPage(QStringLiteral("d"));
    QWidget *e = mockPage(QStringLiteral("e"));
    panels.addPage(a, QStringLiteral("A"));
    panels.addPage(b, QStringLiteral("B"));
    panels.addPage(c, QStringLiteral("C"));
    panels.addPage(d, QStringLiteral("D"));
    panels.addPage(e, QStringLiteral("E"));
    CHECK(panels.count() == 5);
    CHECK(panels.pageAtPanel(0) == e);
    CHECK(panels.currentWidget() == e);

    // A layout change starts with the active connection, then the connections
    // which were visible, then the remaining tabs in tab order.
    panels.setLayoutMode(PanelLayout::Four);
    CHECK(panels.pageAtPanel(0) == e);
    CHECK(panels.pageAtPanel(1) == a);
    CHECK(panels.pageAtPanel(2) == b);
    CHECK(panels.pageAtPanel(3) == c);
    CHECK(panels.panelOf(d) == -1);

    panels.show();
    QApplication::processEvents();
    auto *f0 = panels.findChild<QWidget *>(QStringLiteral("panelFrame0"));
    auto *f1 = panels.findChild<QWidget *>(QStringLiteral("panelFrame1"));
    auto *f2 = panels.findChild<QWidget *>(QStringLiteral("panelFrame2"));
    auto *f3 = panels.findChild<QWidget *>(QStringLiteral("panelFrame3"));
    CHECK(f0 && f1 && f2 && f3);
    if (f0 && f1 && f2 && f3) {
        CHECK(qAbs(f0->width() - f1->width()) <= 1);
        CHECK(qAbs(f0->height() - f2->height()) <= 1);
        CHECK(qAbs(f2->width() - f3->width()) <= 1);
        CHECK(qAbs(f1->height() - f3->height()) <= 1);
    }

    // A header or a page child (the terminal's scrollbar in production)
    // routes the shared window actions to that pane.
    press(panels.findChild<QWidget *>(QStringLiteral("panelHeader1")));
    CHECK(panels.currentWidget() == a);
    press(b->findChild<QScrollBar *>(QStringLiteral("bScroll")));
    CHECK(panels.currentWidget() == b);

    // An off-screen tab replaces the active pane and leaves the displaced
    // connection open and in tab order.
    panels.setCurrentWidget(d);
    CHECK(panels.currentWidget() == d);
    CHECK(panels.pageAtPanel(2) == d);
    CHECK(panels.indexOf(b) >= 0);
    CHECK(panels.panelOf(b) == -1);

    panels.setLayoutMode(PanelLayout::Two);
    CHECK(panels.pageAtPanel(0) == d);
    CHECK(panels.pageAtPanel(1) == e);

    // Moving a tab changes which hidden page is the next refill without
    // disturbing the connections already on screen.
    panels.tabBar()->moveTab(panels.indexOf(c), 0);
    CHECK(panels.widget(0) == c);
    QWidget *removed = panels.removePage(panels.indexOf(e));
    CHECK(removed == e);
    CHECK(panels.pageAtPanel(1) == c);
    CHECK(!e->isVisible());
    delete e;

    // New pages use an empty slot first; a requested empty-tile slot is exact.
    panels.setLayoutMode(PanelLayout::Four);
    removed = panels.removePage(panels.indexOf(a));
    CHECK(removed == a);
    const int empty = panels.firstEmptyPanel();
    CHECK(empty >= 0);
    QWidget *f = mockPage(QStringLiteral("f"));
    panels.addPage(f, QStringLiteral("F"));
    CHECK(panels.panelOf(f) == empty);
    delete a;

    removed = panels.removePage(panels.indexOf(b));
    CHECK(removed == b);
    const int exact = panels.firstEmptyPanel();
    CHECK(exact >= 0);
    QWidget *g = mockPage(QStringLiteral("g"));
    panels.addPage(g, QStringLiteral("G"), exact);
    CHECK(panels.pageAtPanel(exact) == g);
    delete b;
}

void test_empty_panels_request_connections_without_creating_pages()
{
    PanelContainer panels;
    panels.setLayoutMode(PanelLayout::Four);
    int requestedPanel = -1;
    PanelContainer::ConnectionKind requestedKind =
        PanelContainer::ConnectionKind::Serial;
    QObject::connect(
        &panels, &PanelContainer::emptyConnectionRequested, &panels,
        [&](int panel, PanelContainer::ConnectionKind kind) {
            requestedPanel = panel;
            requestedKind = kind;
        });
    auto *ssh = panels.findChild<QPushButton *>(QStringLiteral("panelSsh2"));
    CHECK(ssh != nullptr);
    if (ssh) {
        ssh->click();
    }
    CHECK(requestedPanel == 2);
    CHECK(requestedKind == PanelContainer::ConnectionKind::Ssh);
    CHECK(panels.count() == 0);
    CHECK(panels.pageAtPanel(2) == nullptr);
}

void test_tabs_are_independent_and_actions_follow_the_active_one()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\r\nTerminalSize=40,10\r\n"
               "[Sterna]\r\nPanelLayout=two\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    auto *close = window.findChild<QAction *>(QStringLiteral("closeTabAction"));
    CHECK(panels != nullptr);
    CHECK(add != nullptr);
    CHECK(close != nullptr);
    CHECK(panels->count() == 1);
    CHECK(panels->layoutMode() == PanelLayout::Two);
    CHECK(panels->tabBar()->autoHide());

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

    panels->setCurrentIndex(0);
    CHECK(window.session() == first->session());
    panels->setCurrentIndex(1);
    CHECK(window.session() == second->session());

    press(window.findChild<QWidget *>(QStringLiteral("panelHeader0")));
    CHECK(window.session() == first->session());
    press(second->findChild<QScrollBar *>(QStringLiteral("terminalScrollBar")));
    CHECK(window.session() == second->session());
    auto *activeHeader =
        window.findChild<QLabel *>(QStringLiteral("panelHeader1"));
    CHECK(activeHeader != nullptr);
    CHECK(activeHeader
          && activeHeader->palette().color(QPalette::Window)
                 == activeHeader->palette().color(QPalette::Highlight));

    close->trigger();
    CHECK(panels->count() == 1);
    CHECK(window.session() == first->session());
    CHECK(!panels->tabsClosable());
}

void test_layout_actions_persist_only_the_window_setting()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("; keep this byte-for-byte\n[Sterna]\nOther = untouched\n"
               "PanelLayout = four\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *single = window.findChild<QAction *>(QStringLiteral("singlePanelAction"));
    auto *two = window.findChild<QAction *>(QStringLiteral("twoPanelAction"));
    auto *four = window.findChild<QAction *>(QStringLiteral("fourPanelAction"));
    CHECK(panels && single && two && four);
    CHECK(panels->layoutMode() == PanelLayout::Four);
    CHECK(four->isChecked() && !two->isChecked() && !single->isChecked());
    CHECK(single->shortcut().isEmpty());
    CHECK(two->shortcut().isEmpty());
    CHECK(four->shortcut().isEmpty());

    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll()
          == QByteArray("; keep this byte-for-byte\n[Sterna]\n"
                        "Other = untouched\nPanelLayout = four\n"));
    file.close();

    window.findChild<QAction *>(QStringLiteral("newTabAction"))->trigger();
    CHECK(panels->count() == 2);
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.panel_layout"),
                                       QStringLiteral("single"), &error));
    CHECK(panels->layoutMode() == PanelLayout::Single);
    CHECK(single->isChecked());
    for (int i = 0; i < panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(panels->widget(i));
        CHECK(page->session()->setting(QStringLiteral("window.panel_layout"))
              == QStringLiteral("single"));
    }

    two->trigger();
    CHECK(panels->layoutMode() == PanelLayout::Two);
    CHECK(two->isChecked() && !four->isChecked() && !single->isChecked());
    CHECK(window.session()->setting(QStringLiteral("window.panel_layout"))
          == QStringLiteral("two"));
    for (int i = 0; i < panels->count(); i++) {
        auto *page = static_cast<TerminalPage *>(panels->widget(i));
        CHECK(page->session()->setting(QStringLiteral("window.panel_layout"))
              == QStringLiteral("two"));
    }

    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll()
          == QByteArray("; keep this byte-for-byte\n[Sterna]\n"
                        "Other = untouched\nPanelLayout=two\n"));
    file.close();

    const QString malformed = dir.filePath(QStringLiteral("malformed.ini"));
    file.setFileName(malformed);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=diagonal\r\n");
    file.close();
    MainWindow fallback(malformed);
    auto *fallbackPanels = fallback.findChild<PanelContainer *>();
    auto *fallbackSingle =
        fallback.findChild<QAction *>(QStringLiteral("singlePanelAction"));
    CHECK(fallbackPanels->layoutMode() == PanelLayout::Single);
    CHECK(fallbackSingle->isChecked());
}

void test_visible_panels_refit_and_receive_their_own_metrics()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\r\nPanelLayout=two\r\n");
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
    window.findChild<QAction *>(QStringLiteral("fourPanelAction"))->trigger();
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
    file.write("[Sterna]\r\nPanelLayout=four\r\n");
    file.close();

    MainWindow window(ini);
    auto *panels = window.findChild<PanelContainer *>();
    auto *serial =
        window.findChild<QPushButton *>(QStringLiteral("panelSerial1"));
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
    CHECK(panels->count() == 1);
    CHECK(panels->pageAtPanel(1) == nullptr);

    auto *telnet =
        window.findChild<QPushButton *>(QStringLiteral("panelTelnet2"));
    CHECK(telnet != nullptr);
    if (!telnet) {
        return;
    }
    QTimer::singleShot(0, [&] {
        if (auto *dialog = qobject_cast<TelnetDialog *>(
                QApplication::activeModalWidget())) {
            dialog->setInitial(QStringLiteral("127.0.0.1"), listener.port(),
                               TT_TELNET_NEGOTIATE);
            dialog->accept();
        }
    });
    telnet->click();
    listener.acceptOne();
    CHECK(panels->count() == 2);
    auto *connected =
        static_cast<TerminalPage *>(panels->pageAtPanel(2));
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
    file.write("[Sterna]\r\nPanelLayout=two\r\n");
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
    test_panel_assignment_and_geometry();
    test_empty_panels_request_connections_without_creating_pages();
    test_tabs_are_independent_and_actions_follow_the_active_one();
    test_layout_actions_persist_only_the_window_setting();
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
