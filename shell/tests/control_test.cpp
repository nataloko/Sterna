// The control socket, driven by the window's own event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/control_test
//
// The core's own tests drive the socket from a pump loop; this drives it from
// a `QSocketNotifier` against a real `MainWindow`, which is the arrangement
// that ships. What only shows up here: a notifier registered on the socket's
// descriptor and never serviced, a request that reaches the window's own
// `runMacroFile` and `openCommandLine`, and a `close` arriving from inside
// `tt_ctl_service` — which is a request asking to delete the object whose
// stack frame it is running on.
//
// It needs no server and no hardware: it binds an endpoint of its own and
// talks to itself — a Unix socket in a scratch runtime directory, or a
// byte-mode named pipe under `\\.\pipe\`.

#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QPointer>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>
#include <cstring>

// The platform's own calls rather than `QLocalSocket`, which lives in
// Qt6::Network — the shell links Widgets and nothing else, and a test is the
// wrong reason to add a module to what ships. It is also what a client
// actually looks like: the whole claim is that this endpoint needs no library.
#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
// Winsock first: `windows.h` pulls in the 1.1 header otherwise and the two
// disagree about every name in them.
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <poll.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#endif

#include "Control.h"
#include "MainWindow.h"
#include "Session.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

template <typename F>
bool spin(F done, int ms)
{
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(5, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

// --- the one thing that is two platforms -------------------------------------
//
// A client end, opened, written and drained. Everything above these four calls
// is the same on both, and deliberately: the interesting half of this test is
// that the wait has to be the *window's* event loop, and that is a fact about
// the window rather than about the address.

#ifdef Q_OS_WIN
using Endpoint = HANDLE;
// `const` rather than `constexpr`: `INVALID_HANDLE_VALUE` is a cast, which is
// not something a constant expression may contain.
const Endpoint kNoEndpoint = INVALID_HANDLE_VALUE;

Endpoint openEnd(const QString &path)
{
    // `tt-ctl`'s own client retries on ERROR_PIPE_BUSY, because a window can
    // be answering somebody else. Here nobody else is connected, so one open
    // is the whole of it.
    return CreateFileW(reinterpret_cast<const wchar_t *>(path.utf16()),
                       GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING,
                       0, nullptr);
}

void closeEnd(Endpoint end) { CloseHandle(end); }

bool writeAll(Endpoint end, const QByteArray &bytes)
{
    DWORD wrote = 0;
    return WriteFile(end, bytes.constData(), DWORD(bytes.size()), &wrote,
                     nullptr)
           && wrote == DWORD(bytes.size());
}

/// Bytes taken, 0 for nothing waiting, -1 for a hang-up.
///
/// `PeekNamedPipe` first, and not a read with a timeout: the handle is a
/// synchronous one, so a `ReadFile` with an empty pipe behind it blocks this
/// thread — which is the thread that was going to produce the answer.
int readAvailable(Endpoint end, char *buf, int cap)
{
    DWORD waiting = 0;
    if (!PeekNamedPipe(end, nullptr, 0, nullptr, &waiting, nullptr)) {
        return -1;
    }
    if (waiting == 0) {
        return 0;
    }
    DWORD got = 0;
    const DWORD want = waiting < DWORD(cap) ? waiting : DWORD(cap);
    if (!ReadFile(end, buf, want, &got, nullptr)) {
        return -1;
    }
    return got ? int(got) : -1;
}
#else
using Endpoint = int;
const Endpoint kNoEndpoint = -1;

Endpoint openEnd(const QString &path)
{
    const Endpoint fd = ::socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        return kNoEndpoint;
    }
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof addr);
    addr.sun_family = AF_UNIX;
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s",
             path.toUtf8().constData());
    if (::connect(fd, reinterpret_cast<struct sockaddr *>(&addr), sizeof addr)
        != 0) {
        ::close(fd);
        return kNoEndpoint;
    }
    return fd;
}

void closeEnd(Endpoint end) { ::close(end); }

bool writeAll(Endpoint end, const QByteArray &bytes)
{
    return ::write(end, bytes.constData(), size_t(bytes.size()))
           == bytes.size();
}

int readAvailable(Endpoint end, char *buf, int cap)
{
    struct pollfd pfd = {end, POLLIN, 0};
    if (::poll(&pfd, 1, 0) <= 0) {
        return 0;
    }
    const ssize_t n = ::read(end, buf, size_t(cap));
    return n > 0 ? int(n) : -1;
}
#endif

/// Whether the window's endpoint is still there.
bool endpointExists(const QString &path)
{
#ifdef Q_OS_WIN
    // A named pipe leaves no file behind, so there is nothing to look for on
    // disk: the question is whether the name still resolves. Every instance
    // busy is not the same answer as gone, and only the second one is what
    // a closed window produces.
    if (WaitNamedPipeW(reinterpret_cast<const wchar_t *>(path.utf16()), 1)) {
        return true;
    }
    return GetLastError() != ERROR_FILE_NOT_FOUND;
#else
    return QFile::exists(path);
#endif
}

/// A path inside a JSON string.
///
/// Qt spells even a Windows temporary directory with forward slashes, so in
/// practice nothing here needs escaping — but a `\` in a JSON string opens an
/// escape, and a hand-built request that met one would come back rejected as
/// malformed rather than as the thing the case was about.
QByteArray jsonPath(const QString &path)
{
    return path.toUtf8().replace('\\', "\\\\").replace('"', "\\\"");
}

/// A client on this thread, which is also the window's.
///
/// It cannot block waiting for an answer — the window would never get round to
/// producing one — so every call writes and then spins the event loop until
/// the reply arrives. That is exactly the shape a GUI client would have, and
/// it is why the endpoint's own threads are in the core rather than out here.
class Client {
public:
    ~Client()
    {
        if (m_end != kNoEndpoint) {
            closeEnd(m_end);
        }
    }

    bool open(const QString &path)
    {
        m_end = openEnd(path);
        return m_end != kNoEndpoint;
    }

    /// One request, one line back. Empty on timeout.
    ///
    /// It cannot simply block on the read: the thing that will answer is this
    /// thread's own event loop, so the wait has to *be* the event loop.
    QByteArray call(const QByteArray &request)
    {
        if (!writeAll(m_end, request + "\n")) {
            return {};
        }
        QByteArray answer;
        spin(
            [&] {
                char buf[4096];
                const int n = readAvailable(m_end, buf, int(sizeof buf));
                if (n > 0) {
                    answer.append(buf, n);
                } else if (n < 0) {
                    m_hungUp = true;
                }
                return answer.contains('\n') || m_hungUp;
            },
            5000);
        return answer;
    }

    /// Whether the far end is still there — the hang-up a closed window
    /// produces, which is a read of zero on Unix and a broken pipe on Windows.
    bool connected()
    {
        if (m_hungUp) {
            return false;
        }
        char buf[64];
        if (readAvailable(m_end, buf, int(sizeof buf)) < 0) {
            m_hungUp = true;
        }
        return !m_hungUp;
    }

private:
    Endpoint m_end = kNoEndpoint;
    bool m_hungUp = false;
};

/// A runtime directory of its own, so a run inside a live session cannot find
/// — or prune — the developer's own windows.
///
/// Windows has neither half of that problem, so this is a no-op there rather
/// than a shim over something that is not present: the pipe namespace has no
/// directory to redirect, a pipe leaves nothing behind to go stale, and every
/// name here already carries this process's own id.
class Scratch {
public:
#ifdef Q_OS_WIN
    // Written out rather than `= default`, which would make this trivial —
    // and a trivial type with no members is an *unused variable* at every one
    // of the eight places that declares one, which is eight warnings in a
    // build that treats them as worth reading.
    Scratch() {}
    ~Scratch() {}
#else
    Scratch()
    {
        m_prev = qgetenv("XDG_RUNTIME_DIR");
        qputenv("XDG_RUNTIME_DIR", m_dir.path().toUtf8());
    }
    ~Scratch() { qputenv("XDG_RUNTIME_DIR", m_prev); }

private:
    QTemporaryDir m_dir;
    QByteArray m_prev;
#endif
};

// The two spellings of the same handle. A Winsock `SOCKET` is *unsigned*, so
// the `< 0` that means "no socket" on POSIX is a comparison that can never be
// true there — see `cmdline_test`, where the same listener lives.
#ifdef Q_OS_WIN
using Socket = SOCKET;
const Socket kNoSocket = INVALID_SOCKET;
inline void closeSocket(Socket s) { ::closesocket(s); }
inline int waitReadable(Socket s, int ms)
{
    WSAPOLLFD pfd = {s, POLLRDNORM, 0};
    return ::WSAPoll(&pfd, 1, ms);
}
#else
using Socket = int;
const Socket kNoSocket = -1;
inline void closeSocket(Socket s) { ::close(s); }
inline int waitReadable(Socket s, int ms)
{
    struct pollfd pfd = {s, POLLIN, 0};
    return ::poll(&pfd, 1, ms);
}
#endif

/// A socket listening on localhost, so the case that connects needs no server.
///
/// The platform's own sockets rather than `QTcpServer`, for the reason
/// `cmdline_test`'s copy gives: the shell links Qt Widgets and nothing else,
/// and a test is not a reason to put Qt Network into what the AppImage
/// carries.
class Listener {
public:
    Listener()
    {
#ifdef Q_OS_WIN
        // Winsock has to be started before the first socket call, and nothing
        // else in this binary does it: the core is a DLL with its own copy of
        // the initialisation, which does not count for this process's calls.
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
            || ::listen(m_fd, 1) != 0
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
        if (m_client != kNoSocket) {
            closeSocket(m_client);
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
    Listener(const Listener &) = delete;
    Listener &operator=(const Listener &) = delete;

    quint16 port() const { return m_port; }

    /// Take the waiting connection and say something down it.
    ///
    /// Bounded: a blocking `accept` with nothing to take hangs the whole test
    /// rather than failing the check that was about to notice.
    bool accept(const char *text, int ms)
    {
        if (waitReadable(m_fd, ms) <= 0) {
            return false;
        }
        m_client = ::accept(m_fd, nullptr, nullptr);
        if (m_client == kNoSocket) {
            return false;
        }
        // `send` rather than `write`, which on Windows is a *file* call that a
        // socket handle is not valid for.
        return ::send(m_client, text, int(strlen(text)), 0) > 0;
    }

private:
    Socket m_fd = kNoSocket;
    Socket m_client = kNoSocket;
    quint16 m_port = 0;
#ifdef Q_OS_WIN
    bool m_started = false;
#endif
};

QByteArray request(const char *method, const QByteArray &params = "{}")
{
    return QByteArray("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"") + method
           + "\",\"params\":" + params + "}";
}

// --- the cases ---------------------------------------------------------------

/// The window binds one on construction, and it answers.
void test_a_window_is_reachable()
{
    Scratch scratch;
    MainWindow window;
    window.show();
    CHECK(window.control() != nullptr);
    if (!window.control()) {
        return;
    }
    // The name is the pid, which is what a window with no `/D=` uses.
    CHECK(window.control()->path().contains(QString::number(QCoreApplication::applicationPid())));
    // And it is published, so a shell started inside the terminal finds it.
    CHECK(qgetenv("STERNA_CTL") == window.control()->path().toUtf8());

    Client client;
    CHECK(client.open(window.control()->path()));

    const QByteArray reply = client.call(request("status"));
    // Not a fixed size: `show()` lets the view fit the terminal to whatever
    // the platform gave the window, and under `offscreen` that is neither the
    // settings' size nor the desktop's — see the `adjustedSize` trap.
    CHECK(reply.contains("\"cols\":"));
    CHECK(reply.contains("\"connected\":false"));

    // The *window's* title, not the terminal's — which is the one thing the
    // `title` callback exists for.
    CHECK(reply.contains(window.windowTitle().toUtf8()));
}

/// The terminal, read back as text through the socket.
void test_the_screen_comes_back()
{
    Scratch scratch;
    MainWindow window;
    window.show();
    if (!window.control()) {
        CHECK(false);
        return;
    }
    window.session()->feed(QByteArray("hello\r\nworld"));

    Client client;
    CHECK(client.open(window.control()->path()));
    const QByteArray reply = client.call(request("screen"));
    CHECK(reply.contains("\"hello\""));
    CHECK(reply.contains("\"world\""));
}

/// A macro started over the socket, through the same `runMacroFile` the menu
/// uses — and the busy code for a second one, which is the only refusal a
/// client would retry.
void test_a_macro_runs_and_a_second_is_busy()
{
    Scratch scratch;
    QTemporaryDir dir;
    // A `.ttl`, because `FitTTLFileName` appends one to a name with no dot at
    // all and the file it then opens is not the file we wrote.
    const QString path = QDir(dir.path()).filePath(QStringLiteral("m.ttl"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    // Long enough to still be running when the second request arrives, and
    // short enough not to hold the test up.
    file.write("pause 2\nsetexitcode 4\n");
    file.close();

    MainWindow window;
    window.show();
    if (!window.control()) {
        CHECK(false);
        return;
    }

    Client client;
    CHECK(client.open(window.control()->path()));

    const QByteArray start =
        client.call(request("macro.run",
                            QByteArray("{\"path\":\"") + jsonPath(path) + "\"}"));
    CHECK(start.contains("\"started\":true"));
    CHECK(window.macroRunning());

    const QByteArray second =
        client.call(request("macro.run",
                            QByteArray("{\"path\":\"") + jsonPath(path) + "\"}"));
    CHECK(second.contains("-32002"));

    // And the End button, which is the same request from the other side.
    const QByteArray stopped = client.call(request("macro.stop"));
    CHECK(stopped.contains("\"stopped\":true"));
    CHECK(spin([&] { return !window.macroRunning(); }, 5000));
}

/// A macro that does not exist is refused with a message rather than with a
/// message box — the window is not what asked.
void test_a_macro_that_is_not_there_says_so()
{
    Scratch scratch;
    MainWindow window;
    window.show();
    if (!window.control()) {
        CHECK(false);
        return;
    }
    Client client;
    CHECK(client.open(window.control()->path()));
    const QByteArray reply =
        client.call(request("macro.run", "{\"path\":\"/nonexistent/x.ttl\"}"));
    CHECK(reply.contains("\"error\""));
    CHECK(reply.contains("-32004"));
}

/// `connect` goes through the window's own command-line path and opens a real
/// connection — to a socket this test is listening on, so it needs no server.
void test_connect_takes_a_command_line()
{
    Scratch scratch;
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }

    MainWindow window;
    window.show();
    if (!window.control()) {
        CHECK(false);
        return;
    }
    Client client;
    CHECK(client.open(window.control()->path()));

    const QByteArray line = QByteArray("127.0.0.1:") + QByteArray::number(listener.port());
    const QByteArray reply =
        client.call(request("connect", QByteArray("{\"line\":\"") + line + "\"}"));
    CHECK(reply.contains("\"started\":true"));
    if (!reply.contains("\"started\":true")) {
        fprintf(stderr, "  reply: %s\n", reply.constData());
        return;
    }

    // The open is queued, so it happens on a turn of the loop this spin
    // provides — which is also what a real client's window would give it.
    //
    // The result is latched because `spin` evaluates its predicate once more
    // to produce its return value, and taking the connection is a thing that
    // only works the first time.
    bool taken = false;
    CHECK(spin([&] { return taken = taken || listener.accept("far end\r\n", 0); }, 5000));
    CHECK(spin([&] { return window.session()->isConnected(); }, 5000));
}

/// A line with nothing openable in it is refused rather than answered with a
/// dialog. Upstream's `connect` opens the New Connection box here; a request
/// off a socket must not be able to make the window wait on a person.
void test_connect_with_nothing_to_open_is_refused()
{
    Scratch scratch;
    MainWindow window;
    window.show();
    if (!window.control()) {
        CHECK(false);
        return;
    }
    Client client;
    CHECK(client.open(window.control()->path()));

    // An empty line: no host name and a TCP port type, which is
    // `Startup::of`'s "not named" arm — and with `HostDialogOnStartup` on,
    // which is its default, that is exactly where upstream opens the New
    // Connection dialog.
    const QByteArray reply = client.call(request("connect", "{\"line\":\"\"}"));
    CHECK(reply.contains("\"error\""));
    if (!reply.contains("\"error\"")) {
        fprintf(stderr, "  reply: %s\n", reply.constData());
    }
    CHECK(reply.contains("nothing to connect to"));
    // ...and the window is still answering, which is the thing a dialog would
    // have cost.
    CHECK(client.call(request("ping")).contains("\"pid\""));
}

/// A `close` arrives from inside `tt_ctl_service`, which is inside a call the
/// window's own child object is making. It has to survive the return.
void test_close_is_queued_rather_than_immediate()
{
    Scratch scratch;
    auto *window = new MainWindow;
    window->show();
    if (!window->control()) {
        CHECK(false);
        delete window;
        return;
    }
    QPointer<MainWindow> alive(window);

    Client client;
    CHECK(client.open(window->control()->path()));
    const QByteArray reply = client.call(request("close"));
    CHECK(reply.contains("\"closed\":true"));
    // Still there when the request returned — the close is queued.
    CHECK(!alive.isNull());
    CHECK(spin([&] { return !window->isVisible(); }, 5000));
    delete window;
}

/// The socket goes when the window does, and a client waiting on it is told
/// rather than left hanging.
void test_the_socket_dies_with_the_window()
{
    Scratch scratch;
    QString path;
    Client client;
    {
        MainWindow window;
        window.show();
        if (!window.control()) {
            CHECK(false);
            return;
        }
        path = window.control()->path();
        CHECK(client.open(path));
        CHECK(client.connected());
    }
    CHECK(!endpointExists(path));
    // The hang-up reaches the client rather than leaving it blocked.
    CHECK(spin([&] { return !client.connected(); }, 5000));
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    // A `MainWindow` reads the developer's own `sterna.ini`, and the
    // terminal's size and title are both in it — so without this the checks
    // below measure somebody's settings file rather than the code, silently
    // and consistently, for a reason nobody would think to look for.
    QStandardPaths::setTestModeEnabled(true);

    test_a_window_is_reachable();
    test_the_screen_comes_back();
    test_a_macro_runs_and_a_second_is_busy();
    test_a_macro_that_is_not_there_says_so();
    test_connect_takes_a_command_line();
    test_connect_with_nothing_to_open_is_refused();
    test_close_is_queued_rather_than_immediate();
    test_the_socket_dies_with_the_window();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("control ok\n");
    return 0;
}
