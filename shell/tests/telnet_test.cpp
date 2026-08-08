// The window, connecting over telnet, against a real telnetd.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   cd telnet-audit && ./servers.sh start
//   TT_TELNET_HOST=127.0.0.1 TT_TELNET_PORT=2323
//   QT_QPA_PLATFORM=offscreen ./build/telnet_test
//   cd telnet-audit && ./servers.sh stop
//
// Skips loudly without the server, like every other test here that needs one.
//
// The core's own telnet tests poll in a busy loop; this one waits on a
// QSocketNotifier, so it is what would catch the socket being registered on
// the wrong descriptor or a negotiation reply that never gets flushed because
// nothing wrote after it.

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QTimer>

#include <cstdio>
#include <cstdlib>

#include "Session.h"
#include "TelnetDialog.h"

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
            if (row[x].width_class == TT_WIDTH_PAD) {
                continue;
            }
            const uint32_t c = row[x].text[0];
            out.append(c ? QChar(static_cast<char16_t>(c)) : QLatin1Char(' '));
        }
        out.append(QLatin1Char('\n'));
    }
    return out;
}

/// The dialog's own rule, which is the core's: the protocol follows the port.
/// Worth a test because it is the one setting a user cannot be expected to
/// know, and the failure — protocol bytes in a serial console — is silent.
void test_the_dialog_follows_the_port()
{
    TelnetDialog dialog;
    TtTelnetParams params;

    dialog.setInitial(QStringLiteral("host"), 23, TT_TELNET_NEGOTIATE);
    dialog.fill(&params);
    CHECK(params.mode == TT_TELNET_NEGOTIATE);

    dialog.setInitial(QStringLiteral("host"), 2001, TT_TELNET_AUTO);
    dialog.fill(&params);
    CHECK(params.mode == TT_TELNET_AUTO);
    CHECK(dialog.port() == 2001);
    CHECK(dialog.host() == QStringLiteral("host"));
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);

    test_the_dialog_follows_the_port();

    const QByteArray host = qgetenv("TT_TELNET_HOST");
    if (host.isEmpty()) {
        printf("SKIPPED: set TT_TELNET_HOST (see the file header)\n");
        return failures ? 1 : 0;
    }
    const QByteArray portEnv = qgetenv("TT_TELNET_PORT");
    const quint16 port = portEnv.isEmpty() ? 23 : portEnv.toUShort();

    Session session(80, 24);

    TtTelnetParams params;
    tt_telnet_params_default(&params, port);
    // The server is on 2323 rather than 23, so the burst has to be asked for.
    params.mode = TT_TELNET_NEGOTIATE;

    QString error;
    CHECK(session.connectTelnet(QString::fromUtf8(host), port, params, &error));
    if (!error.isEmpty()) {
        fprintf(stderr, "connect: %s\n", qPrintable(error));
        return 1;
    }
    CHECK(session.isConnected());
    // Telnet has a break where SSH does not, so the menu item is live.
    CHECK(session.supportsBreak());
    printf("  telnet: %s\n", qPrintable(session.describe()));

    int quiet = 0;
    QObject::connect(&session, &Session::damaged, [&] { quiet = 0; });
    spin([&] { return ++quiet > 25; }, 8000);

    session.sendText(QStringLiteral("shell-telnet-ok\r\n"));
    const bool seen = spin(
        [&] { return screenText(session).contains(QStringLiteral("shell-telnet-ok")); },
        10000);
    CHECK(seen);
    if (!seen) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }

    // A resize goes out as a NAWS subnegotiation while the session is live. A
    // malformed one desynchronises the far end and nothing comes back after
    // it, which is what the next marker detects.
    session.resize(100, 30);
    session.sendText(QStringLiteral("after-resize\r\n"));
    CHECK(spin([&] { return screenText(session).contains(QStringLiteral("after-resize")); },
               10000));

    session.disconnectPort();
    CHECK(!session.isConnected());

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("telnet ok\n");
    return 0;
}
