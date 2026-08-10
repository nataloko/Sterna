// The window, opened by a Tera Term command line.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/cmdline_test
//
// Needs nothing: the one case that actually connects does so to a listening
// socket this file opens, so `argv` to a live connection is checked end to end
// without a server, a serial port or an environment variable.
//
// What it covers that `crates/tt-ffi/tests/abi.c` cannot is the half that is
// a *window*: `/W=` and `/H` arriving through the settings rather than through
// the startup, `/V` meaning no window at all, `/I`, `/X=`, and `/F=` choosing
// which settings file was read in the first place. Which transport a line
// resolves to is checked there and in `tt-session`'s own tests, and is not
// repeated here.

#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>

#include <cstdio>
#include <cstring>

#include "MainWindow.h"
#include "Session.h"
#include "WindowTitle.h"

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
        QTimer::singleShot(20, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

QString screenText(const Session &session)
{
    QString out;
    for (int y = 0; y < session.rows(); y++) {
        size_t len = 0;
        const TtCell *row = session.row(y, &len);
        if (!row) {
            continue;
        }
        for (size_t x = 0; x < len; x++) {
            const uint32_t c = row[x].text[0];
            out.append(c ? QChar(static_cast<char16_t>(c)) : QLatin1Char(' '));
        }
        out.append(QLatin1Char('\n'));
    }
    return out;
}

/// A socket listening on localhost, so the one case that connects needs no
/// server.
///
/// POSIX rather than `QTcpServer`: the shell links Qt Widgets and nothing
/// else, and a test is not a reason to put Qt Network into what the AppImage
/// carries. Nothing has to accept before the client connects — the kernel
/// completes the handshake off the backlog — so this stays single-threaded.
class Listener {
public:
    Listener()
    {
        m_fd = socket(AF_INET, SOCK_STREAM, 0);
        if (m_fd < 0) {
            return;
        }
        int on = 1;
        setsockopt(m_fd, SOL_SOCKET, SO_REUSEADDR, &on, sizeof on);
        sockaddr_in addr = {};
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        addr.sin_port = 0;
        socklen_t len = sizeof addr;
        if (bind(m_fd, reinterpret_cast<sockaddr *>(&addr), len) < 0
            || listen(m_fd, 1) < 0
            || getsockname(m_fd, reinterpret_cast<sockaddr *>(&addr), &len) < 0) {
            close(m_fd);
            m_fd = -1;
            return;
        }
        m_port = ntohs(addr.sin_port);
    }
    ~Listener()
    {
        if (m_client >= 0) {
            close(m_client);
        }
        if (m_fd >= 0) {
            close(m_fd);
        }
    }
    Listener(const Listener &) = delete;
    Listener &operator=(const Listener &) = delete;

    quint16 port() const { return m_port; }

    /// Take the waiting connection and say something down it.
    void accept(const char *text)
    {
        m_client = ::accept(m_fd, nullptr, nullptr);
        if (m_client >= 0) {
            const ssize_t n = write(m_client, text, strlen(text));
            (void)n;
        }
    }

private:
    int m_fd = -1;
    int m_client = -1;
    quint16 m_port = 0;
};

/// Parse a command line the way `main` does, minus the program name.
TtCmdLine *parse(const QStringList &args, uint16_t maxComPort = 0)
{
    QList<QByteArray> owned;
    QList<const char *> argv;
    for (const QString &arg : args) {
        owned.append(arg.toUtf8());
    }
    for (const QByteArray &arg : owned) {
        argv.append(arg.constData());
    }
    return tt_cmdline_parse(argv.constData(), static_cast<size_t>(argv.size()),
                            maxComPort);
}

/// `/DS` on everything below that must not connect: without it a line naming
/// nothing puts the New Connection dialog up, which is modal and would hang a
/// test rather than fail it.
void test_a_window_that_is_told_to_open_nothing()
{
    MainWindow window;
    TtCmdLine *cmd = parse({QStringLiteral("/DS")});
    CHECK(cmd != nullptr);
    window.startFrom(cmd);
    tt_cmdline_free(cmd);

    CHECK(window.isVisible());
    CHECK(!window.session()->isConnected());
}

/// `/W=` and `/H` are settings, so they arrive through the same path a
/// `TERATERM.INI` uses. The title is the interesting one: the schema's default
/// is upstream's product name, and taking it literally would put "Tera Term"
/// in this program's title bar.
void test_the_title_and_the_title_bar()
{
    {
        MainWindow window;
        CHECK(window.windowTitle()
              == QStringLiteral("Sterna - [disconnected] VT"));
        CHECK(!window.windowFlags().testFlag(Qt::FramelessWindowHint));

        TtCmdLine *cmd = parse({QStringLiteral("/W=My Session"),
                                QStringLiteral("/H"), QStringLiteral("/DS")});
        window.startFrom(cmd);
        tt_cmdline_free(cmd);
        CHECK(window.windowTitle()
              == QStringLiteral("My Session - [disconnected] VT"));
        CHECK(window.windowFlags().testFlag(Qt::FramelessWindowHint));
        CHECK(window.isVisible());
    }

    // A host's OSC title owns the title bar from then on: a later settings
    // change must not take it back. Attach a line first because upstream
    // ignores remote titles while disconnected.
    MainWindow window;
    TtCmdLine *cmd = parse({QStringLiteral("/DS")});
    window.startFrom(cmd);
    tt_cmdline_free(cmd);
    QString error;
    CHECK(window.session()->connectPty(
        {QStringLiteral("/bin/sh"), QStringLiteral("-c"),
         QStringLiteral("sleep 30")},
        &error));
    window.session()->feed(QByteArray("\033]0;from the host\007"));
    CHECK(spin([&] { return window.windowTitle()
                            == QStringLiteral("sh -c sleep 30 - from the host VT"); },
               1000));
    CHECK(window.session()->setSetting(QStringLiteral("terminal.title"),
                                       QStringLiteral("late"), &error));
    CHECK(window.windowTitle()
          == QStringLiteral("sh -c sleep 30 - from the host VT"));
}

void test_title_format_bits()
{
    WindowTitleState state;
    state.title = QStringLiteral("Tera Term");
    state.configuredTitle = QStringLiteral("Tera Term");
    state.upstreamDefaultTitle = QStringLiteral("Tera Term");
    state.productTitle = QStringLiteral("Sterna");
    state.titleChange = QStringLiteral("overwrite");
    state.endpoint = QStringLiteral("router");
    state.tcpPort = 2222;
    state.linkKind = TT_LINK_NETWORK;
    state.connected = true;

    state.format = 13;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("router - Sterna VT"));
    state.format = 1;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("Sterna - router"));
    state.format = 17;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("Sterna - router:2222"));

    state.linkKind = TT_LINK_SERIAL;
    state.endpoint = QStringLiteral("ttyUSB0");
    state.serialBaud = 115200;
    state.format = 33;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("Sterna - ttyUSB0:115200bps"));

    state.connected = false;
    state.connecting = true;
    state.format = 15;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("Sterna - [connecting...] (1) VT"));
    state.connecting = false;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("Sterna - [disconnected] (1) VT"));

    // Replace only the configured-title component of a combined OSC title.
    state.connected = true;
    state.titleChange = QStringLiteral("ahead");
    state.title = QStringLiteral("remote Tera Term");
    state.format = 0;
    CHECK(formatWindowTitle(state, QStringLiteral("[connecting...]"),
                            QStringLiteral("[disconnected]"))
          == QStringLiteral("remote Sterna"));
}

/// `/V` is a session with no window at all, for one driven entirely by a
/// macro — so nothing here may assume `show()`.
void test_a_window_that_is_never_shown()
{
    MainWindow window;
    TtCmdLine *cmd = parse({QStringLiteral("/V"), QStringLiteral("/DS")});
    window.startFrom(cmd);
    tt_cmdline_free(cmd);
    CHECK(!window.isVisible());
}

/// `/X=` alone. Upstream pairs the two coordinates, because a real position in
/// one axis and "wherever you like" in the other is not a position a window
/// manager can honour.
void test_a_window_position()
{
    MainWindow window;
    TtCmdLine *cmd = parse({QStringLiteral("/X=120"), QStringLiteral("/DS")});
    window.startFrom(cmd);
    tt_cmdline_free(cmd);
    CHECK(window.pos().x() == 120);
    CHECK(window.pos().y() == 0);
}

/// `/F=` chooses the settings file, which is why it has to be read before the
/// window exists — and why upstream parses the line twice.
void test_a_settings_file_named_on_the_line()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("other.ini"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\nTerminalSize=100,40\nMaxComPort=1024\n");
    file.close();

    MainWindow window(path);
    CHECK(window.session()->cols() == 100);
    CHECK(window.session()->rows() == 40);

    // And the second parse it makes possible: `MaxComPort=` bounds `/C=`, and
    // out of range is dropped rather than clamped, so a `/C=300` this file
    // allows is invisible to a parse that took the default of 256.
    //
    // Applied rather than started: a resolved serial target would try to open
    // the port, and a machine where that succeeds and one where it fails are
    // both the wrong thing to be testing here.
    QString error;
    TtCmdLine *cmd = parse({QStringLiteral("/C=300")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(window.session()->setting(QStringLiteral("serial.com_port"))
          == QStringLiteral("1"));

    cmd = parse({QStringLiteral("/C=300")}, 1024);
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(window.session()->setting(QStringLiteral("serial.com_port"))
          == QStringLiteral("300"));
}

/// `/OSC52=` is an override of the file's permission, not a second setting.
/// The unrecognised-value arm matters because it clears both bits upstream.
void test_osc52_overrides_the_file_for_this_launch()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("clipboard.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\nClipboardAccessFromRemote=write\n");
    file.close();

    MainWindow window(ini);
    CHECK(window.session()->setting(QStringLiteral("clipboard.remote_access"))
          == QStringLiteral("write"));

    QString error;
    TtCmdLine *cmd = parse({QStringLiteral("/OSC52=read")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(window.session()->setting(QStringLiteral("clipboard.remote_access"))
          == QStringLiteral("read"));

    cmd = parse({QStringLiteral("/OSC52=nonsense")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(window.session()->setting(QStringLiteral("clipboard.remote_access"))
          == QStringLiteral("off"));
}

/// `StartupMacro` is a file setting with two command-line overrides: `/M`
/// replaces it and a `/D=` topic cancels it. Relative names live beside the
/// active INI here instead of depending on a desktop launcher's working
/// directory.
void test_the_startup_macro_setting_and_its_overrides()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    if (!dir.isValid()) {
        return;
    }

    const auto write = [](const QString &path, const QByteArray &body) {
        QFile file(path);
        return file.open(QIODevice::WriteOnly) && file.write(body) == body.size();
    };
    CHECK(write(dir.filePath(QStringLiteral("startup.ttl")),
                QByteArray("dispstr 'from the startup setting'\n")));
    // `tt_macro_start` reproduces `FitTTLFileName`, including the upper-case
    // extension upstream adds. The filesystem here is case-sensitive.
    CHECK(write(dir.filePath(QStringLiteral("override.TTL")),
                QByteArray("dispstr 'from the command line'\n")));
    const QString ini = dir.filePath(QStringLiteral("settings.ini"));
    CHECK(write(ini, QByteArray("[Tera Term]\nStartupMacro=startup.ttl\n")));

    {
        MainWindow window(ini);
        TtCmdLine *cmd = parse({QStringLiteral("/DS")});
        CHECK(cmd != nullptr);
        window.startFrom(cmd);
        tt_cmdline_free(cmd);
        CHECK(spin([&] {
            return screenText(*window.session())
                .contains(QStringLiteral("from the startup setting"));
        }, 5000));
    }

    {
        MainWindow window(ini);
        TtCmdLine *cmd =
            parse({QStringLiteral("/M=override"), QStringLiteral("/DS")});
        CHECK(cmd != nullptr);
        window.startFrom(cmd);
        tt_cmdline_free(cmd);
        CHECK(spin([&] {
            return screenText(*window.session())
                .contains(QStringLiteral("from the command line"));
        }, 5000));
        CHECK(!screenText(*window.session())
                   .contains(QStringLiteral("from the startup setting")));
    }

    {
        MainWindow window(ini);
        TtCmdLine *cmd =
            parse({QStringLiteral("/D=startup-test"), QStringLiteral("/DS")});
        CHECK(cmd != nullptr);
        window.startFrom(cmd);
        tt_cmdline_free(cmd);
        CHECK(!spin([&] {
            return screenText(*window.session())
                .contains(QStringLiteral("from the startup setting"));
        }, 250));
    }
}

/// The whole path, argv to a live connection — and the log the same line asked
/// for, which starts before the connection so a console's opening banner is in
/// the file.
void test_a_host_name_connects_and_logs()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }

    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString log = dir.filePath(QStringLiteral("out.log"));

    MainWindow window;
    TtCmdLine *cmd = parse({QStringLiteral("/L=") + log,
                            QStringLiteral("127.0.0.1:")
                                + QString::number(listener.port())});
    CHECK(cmd != nullptr);
    window.startFrom(cmd);
    tt_cmdline_free(cmd);

    // A bare host name is telnet, and a port that is not 23 auto-detects
    // rather than negotiating — so nothing has been sent yet and the far end
    // sees a plain TCP connection.
    CHECK(window.session()->isConnected());
    // Started before the connection, so a console's opening banner is in the
    // file rather than just after it.
    CHECK(window.session()->isLogging());
    CHECK(window.windowTitle() == QStringLiteral("127.0.0.1 - Sterna VT"));

    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.title_format"),
                                       QStringLiteral("29"), &error));
    CHECK(window.windowTitle()
          == QStringLiteral("127.0.0.1:%1 - Sterna VT")
                 .arg(listener.port()));

    listener.accept("hello from the far end\r\n");
    CHECK(spin([&] { return window.session()->logBytes() > 0; }, 2000));
    CHECK(screenText(*window.session()).contains(
        QStringLiteral("hello from the far end")));
}

} // namespace

int main(int argc, char **argv)
{
    // Before `QApplication`, and load-bearing: a `MainWindow` reads
    // `sterna.ini` from the user's own config directory, and the terminal's
    // *size* and *title* are in it. Without this, a developer with a 132x50 or
    // a `Title=` in their file fails these assertions consistently, for a
    // reason nobody would think to look for. `bench_shell` does the same.
    QStandardPaths::setTestModeEnabled(true);
    QApplication app(argc, argv);

    test_a_window_that_is_told_to_open_nothing();
    test_the_title_and_the_title_bar();
    test_title_format_bits();
    test_a_window_that_is_never_shown();
    test_a_window_position();
    test_a_settings_file_named_on_the_line();
    test_osc52_overrides_the_file_for_this_launch();
    test_the_startup_macro_setting_and_its_overrides();
    test_a_host_name_connects_and_logs();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("cmdline ok\n");
    return 0;
}
