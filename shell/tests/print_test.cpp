// The printer, driven from the window's event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/print_test
//
// Needs no printer. `PassThruPort` is upstream's escape hatch for a device on
// a port rather than a Windows print job, and a file is a device as far as
// `PrintFileDirect` is concerned — so this points it at a temporary file and
// reads back exactly what a printer would have received.
//
// What it catches that `tt-vt` and the C ABI cannot: those stop at the event
// queue. This is the half where a job is accumulated across events, held for
// `PassThruDelay`, and written; and `Printer` is a `QObject` with a `QTimer`,
// so none of it exists outside an event loop.

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "Printer.h"
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
        QTimer::singleShot(10, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

QString slurp(const QString &path)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) {
        return QString();
    }
    return QString::fromLocal8Bit(f.readAll());
}

/// A session and a printer wired together the way `MainWindow` wires them,
/// pointed at `path` and told not to wait.
struct Rig {
    Session session{24, 4};
    Printer printer{&session};

    explicit Rig(const QString &path)
    {
        QObject::connect(&session, &Session::printerEvent, &printer, &Printer::handle);
        // These fixtures exercise a bare CR as an overwrite. Keep that
        // byte-level premise explicit now that the application ships Auto.
        CHECK(session.setSetting(QStringLiteral("terminal.cr_receive"),
                                 QStringLiteral("CR"), nullptr));
        CHECK(session.setSetting(QStringLiteral("printer.control_sequences"),
                                 QStringLiteral("on"), nullptr));
        CHECK(session.setSetting(QStringLiteral("printer.passthrough_port"), path,
                                 nullptr));
        CHECK(session.setSetting(QStringLiteral("printer.passthrough_delay"),
                                 QStringLiteral("0"), nullptr));
    }
};

/// A whole controller-mode job, from the sequence that opens it to the bytes
/// on the device.
void testControllerJob(const QString &path)
{
    Rig rig(path);
    rig.session.feed(QByteArray("\x1b[5iHELLO\r\n\x1b[2J\x1b[4i"));
    // The write happens on the timer, so nothing is on the device yet.
    CHECK(spin([&] { return !slurp(path).isEmpty(); }, 2000));
    // The text arrived through the tap and the two controls arrived raw — the
    // erase among them, which is exactly what controller mode is for.
    CHECK(slurp(path) == QStringLiteral("HELLO\r\n\x1b[2J"));
    // Applying one setting applies all of them, and the terminal size is one:
    // the 24x4 this was constructed with is gone the moment `Rig` sets the
    // gate. Named here because two of the checks below depend on it.
    CHECK(rig.session.rows() == 24);
}

/// Auto print, which dumps the *grid* rather than the stream — so an
/// overwritten line reaches the printer as what was displayed.
void testAutoPrint(const QString &path)
{
    Rig rig(path);
    rig.session.feed(QByteArray("\x1b[?5ihello\rH\r\nsecond\n\x1b[?4i"));
    CHECK(spin([&] { return !slurp(path).isEmpty(); }, 2000));
    CHECK(slurp(path) == QStringLiteral("Hello\r\nsecond\r\n"));
}

/// `CSI 0 i` and File > Print are the same call, and neither goes through the
/// spool: the page is the screen as it stands.
void testPrintScreen(const QString &path)
{
    Rig rig(path);
    rig.session.feed(QByteArray("one\r\ntwo\x1b[0i"));
    CHECK(spin([&] { return !slurp(path).isEmpty(); }, 2000));
    QString expected = QStringLiteral("one\r\ntwo\r\n");
    for (int y = 2; y < rig.session.rows(); y++) {
        expected += QStringLiteral("\r\n");
    }
    CHECK(slurp(path) == expected);

    // DECPEX reset asks for the scroll region instead, which is the one thing
    // the flag decides and the reason the ABI hands out the margins.
    QFile::remove(path);
    rig.session.feed(QByteArray("\x1b[2;3r\x1b[?19l\x1b[0i"));
    CHECK(spin([&] { return !slurp(path).isEmpty(); }, 2000));
    CHECK(slurp(path) == QStringLiteral("two\r\n\r\n"));
}

/// The gate is the file's, and it ships off. With it off the same stream is an
/// ordinary one and nothing is printed at all.
void testGate(const QString &path)
{
    Session session(24, 4);
    Printer printer(&session);
    QObject::connect(&session, &Session::printerEvent, &printer, &Printer::handle);
    CHECK(session.setSetting(QStringLiteral("printer.passthrough_port"), path, nullptr));
    session.feed(QByteArray("\x1b[5iX\r\nY\x1b[4i"));
    spin([] { return false; }, 200);
    CHECK(!QFile::exists(path));
    CHECK(!printer.busy());
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QTemporaryDir dir;
    if (!dir.isValid()) {
        fprintf(stderr, "print_test: no temporary directory\n");
        return 1;
    }

    int n = 0;
    auto path = [&] { return dir.filePath(QStringLiteral("prn%1.txt").arg(++n)); };
    testControllerJob(path());
    testAutoPrint(path());
    testPrintScreen(path());
    testGate(path());

    if (failures != 0) {
        fprintf(stderr, "print_test: %d check(s) failed\n", failures);
        return 1;
    }
    printf("print ok\n");
    return 0;
}
