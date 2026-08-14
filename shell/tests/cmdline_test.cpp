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
#include <QAction>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

#include <cstdio>
#include <cstring>

#include "Environment.h"
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

// The two spellings of the same handle. A Winsock `SOCKET` is *unsigned*, so
// the `< 0` that means "no socket" on POSIX is a comparison that can never be
// true there — the failure would be a listener that reports success and has
// no descriptor, which is worse than not compiling.
#ifdef Q_OS_WIN
using Socket = SOCKET;
constexpr Socket kNoSocket = INVALID_SOCKET;
inline void closeSocket(Socket s) { ::closesocket(s); }
#else
using Socket = int;
constexpr Socket kNoSocket = -1;
inline void closeSocket(Socket s) { ::close(s); }
#endif

/// A socket listening on localhost, so the one case that connects needs no
/// server.
///
/// The platform's own sockets rather than `QTcpServer`: the shell links Qt
/// Widgets and nothing else, and a test is not a reason to put Qt Network into
/// what the AppImage carries. Nothing has to accept before the client connects
/// — the kernel completes the handshake off the backlog — so this stays
/// single-threaded.
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
    void accept(const char *text)
    {
        m_client = ::accept(m_fd, nullptr, nullptr);
        if (m_client != kNoSocket) {
            // `send` rather than `write`, which on Windows is a *file* call
            // that a socket handle is not valid for.
            const int n = ::send(m_client, text, int(strlen(text)), 0);
            (void)n;
        }
    }

private:
    Socket m_fd = kNoSocket;
    Socket m_client = kNoSocket;
    quint16 m_port = 0;
#ifdef Q_OS_WIN
    bool m_started = false;
#endif
};

/// A local shell that stays open long enough to be titled, and what the title
/// bar calls it — `describe_argv` is the program's basename plus its
/// arguments, so the two halves have to be written together or the assertion
/// is guessing.
///
/// Windows gets `cmd.exe` rather than `/bin/sh` for the reason every other
/// fixture here now does: Wine's `Z:` drive makes the Unix spelling work in
/// the emulator and nowhere else.
struct IdleShell {
    QStringList argv;
    QString title;
};

IdleShell idleShell()
{
#ifdef Q_OS_WIN
    return {{QStringLiteral("cmd.exe"), QStringLiteral("/c"),
             QStringLiteral("pause")},
            QStringLiteral("cmd.exe /c pause")};
#else
    return {{QStringLiteral("/bin/sh"), QStringLiteral("-c"),
             QStringLiteral("sleep 30")},
            QStringLiteral("sh -c sleep 30")};
#endif
}

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

QAction *findAction(MainWindow &window, const QString &text)
{
    for (QAction *action : window.findChildren<QAction *>()) {
        if (action->text() == text) {
            return action;
        }
    }
    return nullptr;
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
    const IdleShell shell = idleShell();
    if (!window.session()->connectPty(shell.argv, &error)) {
        // The three checks below all hang off this one, so the reason it
        // failed is the only useful thing to print — otherwise a local shell
        // that would not open reads as three separate title bugs.
        fprintf(stderr, "could not open a local shell: %s\n",
                qPrintable(error));
        failures++;
    }
    const QString expected =
        QStringLiteral("%1 - from the host VT").arg(shell.title);
    window.session()->feed(QByteArray("\033]0;from the host\007"));
    CHECK(spin([&] { return window.windowTitle() == expected; }, 1000));
    if (!window.session()->setSetting(QStringLiteral("terminal.title"),
                                      QStringLiteral("late"), &error)) {
        // Applying settings resizes the connection, so this can fail for a
        // reason that has nothing to do with the title. Say which.
        fprintf(stderr, "could not set the title: %s\n", qPrintable(error));
        failures++;
    }
    CHECK(window.windowTitle() == expected);
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

/// What the last connection was opened with seeds the next launch — Sterna's
/// behaviour and not Tera Term's, whose host dialog forgets everything on exit
/// unless Setup > Save was used. See `docs/deviations.md`.
///
/// The connect dialogs are modal, so what is checked here is the seam they and
/// `--port` share: the line settings the file describes, the record read back
/// beside them, and that writing a record touches nothing else in the file. The
/// connect edges themselves are `ssh_test`'s and `telnet_test`'s, and the core's
/// half is `crates/tt-ffi/tests/abi.c`.
void test_the_remembered_connection_seeds_the_next_launch()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("remembered.ini"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("; a comment\n[Tera Term]\nBaudRate=57600\nFlowCtrl=hard\n"
               "[Sterna]\nSshHost=router\nSshUser=admin\nSshPort=2222\n"
               "SshLegacy=on\n");
    file.close();

    MainWindow window(path);

    // `--port` with no `--baud` opens at the file's speed rather than at the
    // shipped one, which is the same rule upstream's `/C=1` follows.
    TtSerialParams shipped;
    tt_serial_params_default(&shipped);
    CHECK(shipped.baud == 115200);
    CHECK(window.serialParams().baud == 57600);
    CHECK(window.serialParams().flow == TT_FLOW_CONTROL_RTS_CTS);

    // The endpoints upstream has no key for come out of `[Sterna]`.
    CHECK(window.session()->setting(QStringLiteral("recent.ssh_host"))
          == QStringLiteral("router"));
    CHECK(window.session()->setting(QStringLiteral("recent.ssh_user"))
          == QStringLiteral("admin"));
    CHECK(window.session()->setting(QStringLiteral("recent.ssh_port"))
          == QStringLiteral("2222"));
    CHECK(window.session()->setting(QStringLiteral("recent.ssh_legacy"))
          == QStringLiteral("on"));

    // A connection writes its own record and nothing else: the comment survives
    // and no other schema value is pinned into a file the user may be sharing
    // with a real Tera Term.
    QString error;
    CHECK(window.session()->rememberSettings(
        {{QStringLiteral("recent.serial_port"), QStringLiteral("/dev/ttyS3")},
         {QStringLiteral("serial.baud"), QStringLiteral("9600")}},
        path, &error));
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray written = file.readAll();
    file.close();
    CHECK(written.contains("; a comment"));
    CHECK(written.contains("SerialPort=/dev/ttyS3"));
    CHECK(written.contains("BaudRate=9600"));
    CHECK(written.contains("SshHost=router"));
    CHECK(!written.contains("TerminalSize"));

    // And the next launch opens there.
    MainWindow next(path);
    CHECK(next.serialParams().baud == 9600);
    CHECK(next.serialParams().flow == TT_FLOW_CONTROL_RTS_CTS);
    CHECK(next.session()->setting(QStringLiteral("recent.serial_port"))
          == QStringLiteral("/dev/ttyS3"));
}

/// `/K=` follows `/F=` into that setup file's directory and supplies the
/// extension upstream supplies when the argument has none.
void test_a_keyboard_file_named_on_the_line()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString setup = dir.filePath(QStringLiteral("other.ini"));
    QFile file(dir.filePath(QStringLiteral("custom.CNF")));
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[User keys]\nUser1=59,2,from-keymap.ttl\n");
    file.close();

    MainWindow window(setup);
    TtCmdLine *cmd = parse({QStringLiteral("/K=custom"), QStringLiteral("/DS")});
    CHECK(cmd != nullptr);
    window.startFrom(cmd);
    tt_cmdline_free(cmd);

    const KeyCodeAction action = window.session()->sendKeyCode(59);
    CHECK(action.kind == TT_KEY_CODE_MACRO);
    CHECK(action.text == QStringLiteral("from-keymap.ttl"));
}

void test_menu_and_accelerator_settings()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }

    MainWindow window;
    // One item, as upstream has: the screen behind it covers every transport,
    // so the accelerator and both menu gates land on it rather than on three.
    QAction *newConnection = findAction(window, QStringLiteral("New connection..."));
    QAction *local = findAction(window, QStringLiteral("Local shell"));
    QAction *sendBreak = findAction(window, QStringLiteral("Send break"));
    CHECK(newConnection != nullptr);
    CHECK(local != nullptr);
    CHECK(sendBreak != nullptr);
    if (!newConnection || !local || !sendBreak) {
        return;
    }

    // All three upstream accelerators ship enabled. The duplicate-session
    // fourth has no action until Stage 3.
    CHECK(newConnection->shortcut() == QKeySequence(Qt::ALT | Qt::Key_N));
    CHECK(local->shortcut() == QKeySequence(Qt::ALT | Qt::Key_G));
    CHECK(sendBreak->shortcut() == QKeySequence(Qt::ALT | Qt::Key_B));

    QString error;
    CHECK(window.session()->setSetting(
        QStringLiteral("menu.accelerator_new_connection"), QStringLiteral("off"),
        &error));
    CHECK(window.session()->setSetting(
        QStringLiteral("menu.accelerator_local_shell"), QStringLiteral("off"),
        &error));
    CHECK(window.session()->setSetting(
        QStringLiteral("menu.disable_accelerator_send_break"),
        QStringLiteral("on"), &error));
    CHECK(newConnection->shortcut().isEmpty());
    CHECK(local->shortcut().isEmpty());
    CHECK(sendBreak->shortcut().isEmpty());

    // The menu gates are separate from the shortcuts. New connection stays
    // available on an open line by default, while Local shell remains Cygwin's
    // separate command. Telnet supports break, making that independent gate
    // observable.
    window.connectTelnet(QStringLiteral("127.0.0.1"), listener.port());
    CHECK(window.session()->isConnected());
    CHECK(newConnection->isEnabled());
    CHECK(local->isEnabled());
    CHECK(sendBreak->isEnabled());

    CHECK(window.session()->setSetting(QStringLiteral("menu.disable_new_connection"),
                                       QStringLiteral("on"), &error));
    CHECK(!newConnection->isEnabled());
    CHECK(local->isEnabled());

    CHECK(window.session()->setSetting(QStringLiteral("menu.disable_send_break"),
                                       QStringLiteral("on"), &error));
    CHECK(!sendBreak->isEnabled());
}

void test_the_language_file_translates_menus_without_stealing_alt()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("japanese.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\nUILanguageFile=lang\\ja_JP.lng\n");
    file.close();

    MainWindow window(ini);
    QAction *fileMenu = findAction(window, QStringLiteral("ファイル"));
    QAction *send = findAction(window, QStringLiteral("ファイル送信..."));
    QAction *setup = findAction(window, QStringLiteral("設定"));
    QAction *clearScreen = findAction(window, QStringLiteral("画面クリア"));
    QAction *clearBuffer = findAction(window, QStringLiteral("バッファのクリア"));
    CHECK(fileMenu != nullptr);
    CHECK(send != nullptr);
    CHECK(setup != nullptr);
    CHECK(clearScreen != nullptr);
    CHECK(clearBuffer != nullptr);
    CHECK(window.windowTitle().contains(QStringLiteral("[未接続]")));

    // The catalog advertises Win32 mnemonics and accelerator captions. Sterna
    // keeps Alt for the terminal and puts shortcuts on QAction itself, so
    // neither marker belongs in the displayed translated text.
    for (QAction *action : window.findChildren<QAction *>()) {
        CHECK(!action->text().contains(QLatin1Char('&')));
        CHECK(!action->text().contains(QLatin1Char('\t')));
    }

    // A live change retranslates the existing actions rather than requiring a
    // second menu tree or a restart.
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("settings.language_file"),
                                       QStringLiteral("lang\\de_DE.lng"),
                                       &error));
    CHECK(findAction(window, QStringLiteral("Datei")) != nullptr);
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

/// `-proxy=` is a third parser's option, and everything it carries is a
/// setting — so this is the only place the window can be asked whether it
/// arrived. It is also the one option here whose plugin replaces the record
/// entire, which is why the file's user name does not survive it.
void test_the_proxy_option_reaches_the_settings()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("proxy.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[TTProxy]\nProxyType=\"http\"\nProxyHost=\"from.the.file\"\n"
               "ProxyUser=\"bob\"\n");
    file.close();

    MainWindow window(ini);
    const auto get = [&](const char *key) {
        return window.session()->setting(QString::fromLatin1(key));
    };
    CHECK(get("proxy.type") == QStringLiteral("http"));
    CHECK(get("proxy.user") == QStringLiteral("bob"));

    QString error;
    TtCmdLine *cmd = parse({QStringLiteral("-proxy=socks5://p.example:1080")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    // `socks` and `socks5` are one type under two spellings, and the one that
    // comes back is upstream's writer's — the first of the pair.
    CHECK(get("proxy.type") == QStringLiteral("socks"));
    CHECK(get("proxy.host") == QStringLiteral("p.example"));
    CHECK(get("proxy.port") == QStringLiteral("1080"));
    // The whole record was replaced, so the file's user went with it.
    CHECK(get("proxy.user").isEmpty());

    // `-noproxy` is a proxy of type `none`, which is no proxy.
    cmd = parse({QStringLiteral("-noproxy")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(get("proxy.type") == QStringLiteral("none"));

    // ...and a line that says nothing about it leaves it where it was.
    cmd = parse({QStringLiteral("/DS")});
    CHECK(window.session()->applyCommandLine(cmd, &error));
    tt_cmdline_free(cmd);
    CHECK(get("proxy.type") == QStringLiteral("none"));
}

/// `StartupMacro` is a file setting with two command-line overrides: `/M`
/// replaces it and a `/D=` topic cancels it. Relative names live beside the
/// active INI here instead of depending on a desktop launcher's working
/// directory.
/// A window closing while its macro is still running.
///
/// This asserts almost nothing by itself — that the macro really was running,
/// so the case is the one intended — and the whole of its value is what it
/// does *after* the brace. `QObjectPrivate::deleteChildren` deletes in the
/// order the children were created, the session is created first, and
/// `~Macro` calls `Session::unlinkMacro` to take the terminal's tap off: so a
/// window torn down with a live macro read a freed session. Nothing here
/// notices unless something else has claimed that memory, which is why CI saw
/// it as an intermittent `malloc_consolidate(): unaligned fastbin chunk
/// detected` and this file passed ten times out of ten locally.
///
/// **Run it under AddressSanitizer or it proves very little** — configure a
/// build tree with `-DCMAKE_CXX_FLAGS="-fsanitize=address
/// -fno-omit-frame-pointer -g"` and `-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address`,
/// which is what named the free site here in one run.
///
/// `pause 30` rather than a script that ends, because the bug needs the macro
/// to still be there; a macro that finishes first takes the `if (m_macro)`
/// that guards the use-after-free out of the picture entirely.
void test_a_window_that_closes_with_a_macro_still_running()
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
                QByteArray("pause 30\n")));
    const QString ini = dir.filePath(QStringLiteral("settings.ini"));
    CHECK(write(ini, QByteArray("[Tera Term]\nStartupMacro=startup.ttl\n")));

    MainWindow window(ini);
    TtCmdLine *cmd = parse({QStringLiteral("/DS")});
    CHECK(cmd != nullptr);
    window.startFrom(cmd);
    tt_cmdline_free(cmd);
    // Long enough for the thread to be in `pause`, short enough not to matter.
    spin([] { return false; }, 200);
    CHECK(window.macroRunning());
}

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

/// The AppImage's library path must not reach the shell the terminal opens.
///
/// It is a launch property rather than a transport one, which is why it is
/// here: the same call runs before `QApplication` for every way of starting,
/// and every child — the login shell, whatever its rc files run, the browser a
/// URL opens — inherits whatever it left behind.
void test_the_bundle_does_not_follow_a_child_process()
{
    const QByteArray hadAppDir = qgetenv("APPDIR");
    const QByteArray hadPath = qgetenv("LD_LIBRARY_PATH");

    // Outside an AppImage there is nothing to undo, and a developer's own
    // LD_LIBRARY_PATH is not ours to edit.
    qunsetenv("APPDIR");
    qputenv("LD_LIBRARY_PATH", "/opt/mine/lib");
    environment::unshadowBundledLibraries();
    CHECK(qgetenv("LD_LIBRARY_PATH") == QByteArray("/opt/mine/lib"));

    // The shipped case: the bundle is the only entry, so the variable goes
    // rather than being left empty — an empty LD_LIBRARY_PATH means the
    // working directory to `ld.so`, which is not what was there before.
    qputenv("APPDIR", "/tmp/.mount_sterna42");
    qputenv("LD_LIBRARY_PATH", "/tmp/.mount_sterna42/usr/lib");
    environment::unshadowBundledLibraries();
    CHECK(!qEnvironmentVariableIsSet("LD_LIBRARY_PATH"));

    // ...and a user who had one of their own keeps exactly it, in order.
    qputenv("LD_LIBRARY_PATH",
            "/tmp/.mount_sterna42/usr/lib:/opt/mine/lib:/tmp/.mount_sterna42");
    environment::unshadowBundledLibraries();
    CHECK(qgetenv("LD_LIBRARY_PATH") == QByteArray("/opt/mine/lib"));

    // A directory whose name merely starts the same way is somebody else's.
    qputenv("LD_LIBRARY_PATH", "/tmp/.mount_sterna42-other/lib");
    environment::unshadowBundledLibraries();
    CHECK(qgetenv("LD_LIBRARY_PATH")
          == QByteArray("/tmp/.mount_sterna42-other/lib"));

    if (hadAppDir.isEmpty()) {
        qunsetenv("APPDIR");
    } else {
        qputenv("APPDIR", hadAppDir);
    }
    if (hadPath.isEmpty()) {
        qunsetenv("LD_LIBRARY_PATH");
    } else {
        qputenv("LD_LIBRARY_PATH", hadPath);
    }
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
    test_the_remembered_connection_seeds_the_next_launch();
    test_a_keyboard_file_named_on_the_line();
    test_menu_and_accelerator_settings();
    test_the_language_file_translates_menus_without_stealing_alt();
    test_osc52_overrides_the_file_for_this_launch();
    test_the_proxy_option_reaches_the_settings();
    test_a_window_that_closes_with_a_macro_still_running();
    test_the_startup_macro_setting_and_its_overrides();
    test_a_host_name_connects_and_logs();
    test_the_bundle_does_not_follow_a_child_process();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("cmdline ok\n");
    return 0;
}
