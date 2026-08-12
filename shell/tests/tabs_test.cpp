// Multiple sessions in one window.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QAction>
#include <QApplication>
#include <QFile>
#include <QTabWidget>
#include <QTemporaryDir>
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
#include "Session.h"
#include "TerminalPage.h"

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

void test_tabs_are_independent_and_actions_follow_the_active_one()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\r\nTerminalSize=40,10\r\n");
    file.close();

    MainWindow window(ini);
    auto *tabs = window.findChild<QTabWidget *>();
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    auto *close = window.findChild<QAction *>(QStringLiteral("closeTabAction"));
    CHECK(tabs != nullptr);
    CHECK(add != nullptr);
    CHECK(close != nullptr);
    CHECK(tabs->count() == 1);
    CHECK(tabs->tabBarAutoHide());

    auto *first = static_cast<TerminalPage *>(tabs->widget(0));
    first->session()->feed(QByteArrayLiteral("first"));

    add->trigger();
    CHECK(tabs->count() == 2);
    CHECK(tabs->tabsClosable());
    auto *second = static_cast<TerminalPage *>(tabs->widget(1));
    CHECK(first != second);
    CHECK(window.session() == second->session());
    CHECK(second->session()->cols() == 40);
    CHECK(second->session()->rows() == 10);

    second->session()->feed(QByteArrayLiteral("second"));
    CHECK(screenText(*first->session()).contains(QStringLiteral("first")));
    CHECK(!screenText(*first->session()).contains(QStringLiteral("second")));
    CHECK(screenText(*second->session()).contains(QStringLiteral("second")));
    CHECK(!screenText(*second->session()).contains(QStringLiteral("first")));

    tabs->setCurrentIndex(0);
    CHECK(window.session() == first->session());
    tabs->setCurrentIndex(1);
    CHECK(window.session() == second->session());

    close->trigger();
    CHECK(tabs->count() == 1);
    CHECK(window.session() == first->session());
    CHECK(!tabs->tabsClosable());
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
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));
    auto *tabs = window.findChild<QTabWidget *>();
    auto *duplicate =
        window.findChild<QAction *>(QStringLiteral("duplicateSessionAction"));
    CHECK(tabs != nullptr);
    CHECK(duplicate != nullptr);
    CHECK(!duplicate->isEnabled());

    window.connectTelnet(QStringLiteral("127.0.0.1"), listener.port());
    listener.acceptOne();
    CHECK(window.session()->canDuplicate());
    CHECK(duplicate->isEnabled());

    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("terminal.title"),
                                       QStringLiteral("copied live"), &error));
    auto *source = static_cast<TerminalPage *>(tabs->currentWidget());
    duplicate->trigger();
    listener.acceptOne();

    CHECK(tabs->count() == 2);
    auto *copy = static_cast<TerminalPage *>(tabs->currentWidget());
    CHECK(copy != source);
    CHECK(copy->session()->isConnected());
    CHECK(copy->session()->canDuplicate());
    CHECK(copy->session()->setting(QStringLiteral("terminal.title"))
          == QStringLiteral("copied live"));
    CHECK(source->session()->isConnected());
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("tabs_test"));
    test_tabs_are_independent_and_actions_follow_the_active_one();
    test_duplicate_reopens_telnet_with_the_live_settings();
    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("tabs ok");
    return 0;
}
