// The window, running a local shell.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/pty_test
//
// The one connection test here that skips nothing: a pty needs no server, no
// hardware and no environment variables, so this is the only end-to-end check
// of the window's event loop that always runs — on a developer's machine and
// in CI alike.
//
// What it can catch that the core's own pty tests cannot: the core polls in a
// busy loop, and this waits on a QSocketNotifier. A descriptor registered
// after it was already hung up, or a notifier left alive on a closed fd, only
// shows up here.

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QTimer>

#include <cstdio>

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

QStringList sh(const QString &script)
{
    return {QStringLiteral("/bin/sh"), QStringLiteral("-c"), script};
}

/// Output arrives through the notifier, the size the window has is the size
/// the child gets, and typing reaches it.
void test_the_window_runs_a_shell()
{
    Session session(40, 10);
    QString error;
    CHECK(session.connectPty(
        sh(QStringLiteral("stty size; read line; printf 'got %s\\r\\n' \"$line\"")),
        &error));
    CHECK(error.isEmpty());
    CHECK(session.isConnected());
    // A pty has no line to break, so the menu item must be dead.
    CHECK(!session.supportsBreak());
    CHECK(session.describe().startsWith(QStringLiteral("sh -c")));

    CHECK(spin([&] { return screenText(session).contains(QStringLiteral("10 40")); },
               5000));

    session.sendText(QStringLiteral("hello\r"));
    const bool seen = spin(
        [&] { return screenText(session).contains(QStringLiteral("got hello")); }, 5000);
    CHECK(seen);
    if (!seen) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }

    session.disconnectPort();
    CHECK(!session.isConnected());
}

/// The child exits by itself. The window has to notice — through the notifier,
/// with nothing polling — and say why.
void test_the_shell_exiting_is_noticed_and_explained()
{
    Session session(40, 10);
    QString notice;
    QObject::connect(&session, &Session::notice,
                     [&](const QString &text) { notice = text; });

    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("printf bye; exit 4")), &error));
    CHECK(session.isConnected());

    CHECK(spin([&] { return !session.isConnected(); }, 5000));
    CHECK(screenText(session).contains(QStringLiteral("bye")));
    // Not "Disconnected": a local shell knows what happened to it, and this is
    // the difference between a window that explains itself and one that just
    // goes quiet.
    CHECK(notice.contains(QStringLiteral("exited with status 4")));
    CHECK(session.closeNote().contains(QStringLiteral("exited with status 4")));
}

/// Resizing the window has to reach the child's `winsize`, or every
/// full-screen program in the shell keeps drawing at the old width.
void test_resizing_the_window_resizes_the_shell()
{
    Session session(40, 10);
    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("sleep 0.3; stty size")), &error));
    session.resize(96, 28);
    const bool seen =
        spin([&] { return screenText(session).contains(QStringLiteral("28 96")); }, 5000);
    CHECK(seen);
    if (!seen) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }
}

/// A shell that exits and is then replaced. The notifier has to move to the
/// new descriptor: the old one is closed, and a notifier left on a closed fd
/// either never fires again or fires forever.
void test_a_second_shell_after_the_first_ended()
{
    Session session(40, 10);
    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("exit 0")), &error));
    CHECK(spin([&] { return !session.isConnected(); }, 5000));
    CHECK(!session.closeNote().isEmpty());

    CHECK(session.connectPty(sh(QStringLiteral("printf second")), &error));
    // The note belongs to the previous connection and must not survive into
    // this one.
    CHECK(session.closeNote().isEmpty());
    CHECK(spin([&] { return screenText(session).contains(QStringLiteral("second")); },
               5000));
}

void test_a_program_that_does_not_exist_reports_rather_than_connects()
{
    Session session(40, 10);
    QString error;
    CHECK(!session.connectPty({QStringLiteral("sterna-no-such-program")}, &error));
    CHECK(!error.isEmpty());
    CHECK(!session.isConnected());
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);

    test_the_window_runs_a_shell();
    test_the_shell_exiting_is_noticed_and_explained();
    test_resizing_the_window_resizes_the_shell();
    test_a_second_shell_after_the_first_ended();
    test_a_program_that_does_not_exist_reports_rather_than_connects();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("pty ok\n");
    return 0;
}
