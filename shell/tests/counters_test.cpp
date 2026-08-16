// The counters, over a real connection and a real event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/counters_test [--write DIR]
//
// `tabs_test` owns the status strip's own properties — the reserved width, the
// height that must not move, the early return. What is proved here is the part
// that needs a session behind it: that a pty's bytes reach the field, that the
// popover reads the same numbers the core has, that a link with no control
// lines has no row for them, and — the one with a cost behind it — that
// nothing asks the port while the popover is shut.
//
// A pty needs no server, no hardware and no environment variables, so like
// `pty_test` this skips nothing.

#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QLabel>
#include <QTimer>

#include <cstdio>

#include "CountersPopover.h"
#include "PageStatusBar.h"
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

QString writeDir;

template <typename F>
bool spin(F done, int ms)
{
    QElapsedTimer timer;
    timer.start();
    // Latched, because the predicate is called once more for the return value
    // and some of them are not free to ask twice.
    bool ok = done();
    while (!ok && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(20, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
        ok = done();
    }
    return ok;
}

QStringList sh(const QString &script)
{
    return {QStringLiteral("/bin/sh"), QStringLiteral("-c"), script};
}

QString valueOf(const CountersPopover &popover, const char *name)
{
    auto *label = popover.findChild<QLabel *>(QString::fromLatin1(name));
    CHECK(label != nullptr);
    return label ? label->text() : QString();
}

/// Bytes from a real child reach the counters, and the popover says the same
/// thing the core does.
void test_a_shells_output_is_counted()
{
    Session session(40, 10);
    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("printf 'one\\r\\ntwo\\r\\n'; sleep 5")),
                             &error));
    CHECK(error.isEmpty());

    CHECK(spin([&] { return session.counters().bytes_in >= 10; }, 5000));
    const TtCounters c = session.counters();
    CHECK(c.lines_in >= 2);
    CHECK(c.live);
    CHECK(c.connected_ms >= 0);

    CountersPopover popover;
    popover.refresh(&session);
    // The popover is a view of the same numbers, not a second count.
    CHECK(valueOf(popover, "countersLines")
          == QLocale().toString(static_cast<qulonglong>(c.lines_in)));
    CHECK(valueOf(popover, "countersConnected") != QStringLiteral("—"));

    // A pty has no control lines, so the whole serial row is absent rather
    // than present and reading false — four captions saying nothing is worse
    // than no captions.
    auto *serial = popover.findChild<QWidget *>(QStringLiteral("countersSerial"));
    CHECK(serial != nullptr);
    CHECK(serial != nullptr && serial->isHidden());

    if (!writeDir.isEmpty()) {
        popover.adjustSize();
        popover.grab().save(QDir(writeDir).filePath(
            QStringLiteral("counters-popover.png")));
    }
}

/// A session that has never connected: every number present, and the two that
/// have no value saying so rather than reading zero.
void test_a_session_that_never_connected_says_so()
{
    Session session(40, 10);
    CountersPopover popover;
    popover.refresh(&session);

    CHECK(valueOf(popover, "countersConnected") == QStringLiteral("—"));
    CHECK(valueOf(popover, "countersLines") == QStringLiteral("0"));
    CHECK(valueOf(popover, "countersBreaks") == QStringLiteral("0"));
    CHECK(popover.findChild<QWidget *>(QStringLiteral("countersSerial"))->isHidden());
}

/// The shell exits and the numbers stay. This is the whole reason a disconnect
/// freezes rather than clears: "how much did that session move before it died"
/// is asked after the line has gone.
void test_the_counters_survive_the_shell_exiting()
{
    Session session(40, 10);
    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("printf 'bye\\r\\n'; exit 0")), &error));
    CHECK(error.isEmpty());

    CHECK(spin([&] { return !session.isConnected(); }, 5000));
    const TtCounters c = session.counters();
    CHECK(!c.live);
    CHECK(c.bytes_in > 0);
    CHECK(c.connected_ms >= 0);
    // Frozen: the clock gives the same answer twice.
    CHECK(session.counters().connected_ms == c.connected_ms);
    CHECK(c.rate_in == 0);

    CountersPopover popover;
    popover.refresh(&session);
    CHECK(valueOf(popover, "countersConnected") != QStringLiteral("—"));
}

/// The popover's timer runs while it is on screen and not otherwise.
///
/// That is the whole cost argument for reading the serial control lines live:
/// with the popover shut, nothing asks the port at all — which matters on
/// Windows, where the reading is four kernel calls rather than one ioctl.
void test_the_port_is_only_read_while_somebody_is_looking()
{
    Session session(40, 10);
    CountersPopover popover;
    auto *poll = popover.findChild<QTimer *>(QStringLiteral("countersPollTimer"));
    CHECK(poll != nullptr);
    if (!poll) {
        return;
    }

    CHECK(!poll->isActive());
    popover.popUp(nullptr, &session);
    CHECK(popover.isVisible());
    CHECK(poll->isActive());
    popover.hide();
    CHECK(!poll->isActive());
}

/// The session behind the popover can die while the popover is on screen.
///
/// A `Qt::Popup` grabs the pointer and the keyboard and spins no event loop of
/// its own, so everything underneath goes on running: a macro's `closett` or a
/// control-socket request closes the page, and a second later the poll would
/// be reading a freed `Session`. `MainWindow::closePage` has carried the same
/// rule for `m_pendingSsh` since before this existed.
void test_the_popover_lets_go_of_a_session_that_dies()
{
    auto *session = new Session(40, 10);
    CountersPopover popover;
    popover.popUp(nullptr, session);
    auto *poll = popover.findChild<QTimer *>(QStringLiteral("countersPollTimer"));
    CHECK(poll != nullptr);
    CHECK(poll != nullptr && poll->isActive());

    delete session;
    CHECK(!popover.isVisible());
    CHECK(poll != nullptr && !poll->isActive());
    // And the tick that would have read it does nothing at all.
    if (poll) {
        QMetaObject::invokeMethod(poll, "timeout", Qt::DirectConnection);
    }
}

/// The serial row, against the loopback rig.
///
/// Skipped without one, loudly, the way the core's own rig tests are:
///
///   TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1
///   QT_QPA_PLATFORM=offscreen ./build/counters_test
///
/// **Both ends are needed, and that is the point of the wiring.** A port's own
/// CTS and DSR are driven by what the *other* end asserts — the rig has A's RTS
/// going to B's CTS and A's DTR to B's DSR, so opening A alone leaves A's two
/// inputs floating. Opening B raises them, because `SerialParams::default`
/// asserts DTR and RTS. That is what makes this the one case which proves the
/// four lamps are reading a port rather than a struct somebody filled in.
void test_the_serial_row_reads_the_port()
{
    const QByteArray near = qgetenv("TT_SERIAL_A");
    const QByteArray far = qgetenv("TT_SERIAL_B");
    if (near.isEmpty() || far.isEmpty()) {
        fprintf(stderr, "  serial: skipped, set TT_SERIAL_A and TT_SERIAL_B\n");
        return;
    }

    TtSerialParams params;
    tt_serial_params_default(&params);
    QString error;

    Session session(40, 10);
    if (!session.connectSerial(QString::fromLocal8Bit(near), params, &error)) {
        fprintf(stderr, "  serial: skipped, cannot open %s: %s\n", near.constData(),
                qPrintable(error));
        return;
    }
    // Opened only to drive this end's inputs. Nothing is read from it.
    Session driver(40, 10);
    if (!driver.connectSerial(QString::fromLocal8Bit(far), params, &error)) {
        fprintf(stderr, "  serial: skipped, cannot open %s: %s\n", far.constData(),
                qPrintable(error));
        return;
    }

    CountersPopover popover;
    // A pin change crosses a USB bus, so it is never instant.
    spin(
        [&] {
            TtModemLines lines;
            return session.modemLines(&lines) && lines.cts && lines.dsr;
        },
        2000);
    popover.refresh(&session);
    auto *serial = popover.findChild<QWidget *>(QStringLiteral("countersSerial"));
    CHECK(serial != nullptr);
    CHECK(serial != nullptr && !serial->isHidden());

    // The far end asserted DTR and RTS on open, and the rig carries them to
    // this end's DSR and CTS.
    auto *cts = popover.findChild<QLabel *>(QStringLiteral("countersCts"));
    auto *dsr = popover.findChild<QLabel *>(QStringLiteral("countersDsr"));
    CHECK(cts != nullptr && dsr != nullptr);
    if (cts && dsr) {
        CHECK(cts->styleSheet().contains(QStringLiteral("#2e7d32")));
        CHECK(dsr->styleSheet().contains(QStringLiteral("#2e7d32")));
        CHECK(cts->toolTip().contains(QStringLiteral("CTS")));
    }

    if (!writeDir.isEmpty()) {
        popover.adjustSize();
        popover.grab().save(
            QDir(writeDir).filePath(QStringLiteral("counters-serial.png")));
    }
}

/// What the field leaves for the host name on a tiled quarter-window.
///
/// The question the design turns on: the only stretching item in that row is
/// the name, so every pixel the field reserves is a pixel the name elides
/// away. A quarter of a 1920-wide window is the narrowest strip anybody
/// realistically reads, and the name has to survive it.
void test_the_field_leaves_the_name_room_on_a_quarter_window()
{
    PageStatusBar status;
    status.resize(460, status.sizeHint().height());
    status.setName(QStringLiteral("router1.example.net"));
    status.setConnection(true, false, QStringLiteral("ssh router1.example.net"));
    status.setLogging(true, 4'200'000);
    status.setCounters(true, 44 * 60 * 1000, 1'200, 8, true);
    status.show();
    QApplication::processEvents();

    auto *name = status.findChild<QLabel *>(QStringLiteral("statusName"));
    auto *field = status.findChild<QLabel *>(QStringLiteral("statusCounters"));
    CHECK(name != nullptr && field != nullptr);
    if (!name || !field) {
        return;
    }
    // Not a pixel budget anybody should tune to — what it pins is that the
    // name is still a name. Elided to nothing would be `…`, and the failure
    // this guards against is a later field being added beside this one.
    CHECK(name->width() > 40);
    CHECK(!name->text().isEmpty());
    fprintf(stderr, "  quarter window: name=%dpx counters=%dpx of %dpx\n",
            name->width(), field->width(), status.width());

    if (!writeDir.isEmpty()) {
        status.grab().save(
            QDir(writeDir).filePath(QStringLiteral("counters-strip.png")));
    }
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    for (int i = 1; i + 1 < argc; i++) {
        if (QString::fromLocal8Bit(argv[i]) == QLatin1String("--write")) {
            writeDir = QString::fromLocal8Bit(argv[i + 1]);
        }
    }

    test_a_shells_output_is_counted();
    test_a_session_that_never_connected_says_so();
    test_the_counters_survive_the_shell_exiting();
    test_the_port_is_only_read_while_somebody_is_looking();
    test_the_popover_lets_go_of_a_session_that_dies();
    test_the_serial_row_reads_the_port();
    test_the_field_leaves_the_name_room_on_a_quarter_window();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("counters ok\n");
    return 0;
}
