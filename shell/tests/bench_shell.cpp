// The shell half of the performance gate: what the window costs.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   ./build/bench_shell            # a table
//   ./build/bench_shell --json     # the same, for bench/bench.py
//   ./build/bench_shell --runs 3
//
// **Build this in `sterna-fedora`, in Release, and run it on the desktop.**
// Qt 6.4.2 in the Ubuntu container is seven releases behind what the desktop
// runs and has already produced one false finding and one set of numbers
// flattering by 2x — see `AGENTS.md`. A Debug build measures `-O0` Qt calls.
// Neither refuses to run, because both are useful for checking that the
// harness works; both are wrong to quote.
//
// Four numbers, and the reason each one needs a *process* rather than a
// function call:
//
//   start        exec → the first frame is painted. Only a launcher can time
//                this, because most of it happens before `main` — the dynamic
//                loader resolving Qt is a real part of what a user waits for.
//   idle         RSS and PSS with a shell attached and nothing arriving. The
//                claim this project makes is "light"; this is the number that
//                either supports it or does not.
//   throughput   10 MB out of a pty, painted, until the far end hangs up. The
//                same corpus `tt-bench` feeds the engine directly, so the two
//                numbers subtract: the difference is what the window costs.
//   latency      a keystroke → the frame that shows it, over a pty whose line
//                discipline echoes. The app's own contribution, which is all a
//                process can see — the compositor's half needs a camera.
//
// Each probe is a child process (`--probe NAME`) printing one line of
// `key=value`; this process launches them, takes the **minimum** of K runs
// after no warm-up (there is nothing to warm — each child is cold by
// construction), and reports. The minimum is the run least disturbed by
// everything else on the machine.

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QProcess>
#include <QTimer>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <limits>

#include <QStandardPaths>

#include "MainWindow.h"
#include "Session.h"

namespace {

/// Nanoseconds on a clock two processes on this machine share, which is what
/// lets the launcher subtract its own timestamp from the child's.
long long monotonicNs()
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return static_cast<long long>(ts.tv_sec) * 1000000000LL + ts.tv_nsec;
}

template <typename F>
bool spin(F done, int ms)
{
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(1, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

/// Counts painted frames, and stamps the first one.
///
/// Installed on the application rather than on the view, because `MainWindow`
/// owns its `TerminalView` privately and reaching in to subclass it would mean
/// changing the thing being measured.
///
/// The stamp is taken on the *next* turn of the event loop rather than inside
/// the filter: a filter runs before the widget's own `paintEvent`, so stamping
/// there would time everything up to the point where painting begins. One
/// queued call costs microseconds and moves the mark to the far side of the
/// frame.
class PaintProbe : public QObject {
public:
    long long firstPaintNs = 0;
    int paints = 0;

    bool eventFilter(QObject *obj, QEvent *event) override
    {
        if (event->type() == QEvent::Paint) {
            paints++;
            if (firstPaintNs == 0) {
                QTimer::singleShot(0, this, [this] {
                    if (firstPaintNs == 0) {
                        firstPaintNs = monotonicNs();
                    }
                });
            }
        }
        return QObject::eventFilter(obj, event);
    }
};

QStringList sh(const QString &script)
{
    return {QStringLiteral("/bin/sh"), QStringLiteral("-c"), script};
}

/// A field out of a `/proc` file whose format is `Name:  <number> kB`, in KiB.
/// Zero when the file or the field is absent — `smaps_rollup` needs a 4.14
/// kernel and is not in every container.
///
/// **`QFile` cannot read this and does not say so.** `QFileDevice::atEnd()`
/// answers from `size()`, and every file under `/proc` reports a size of zero
/// because its contents are generated on read — so the loop body never runs,
/// the field is never found, and the measurement comes out as a confident
/// `0.0 MB`. Which is exactly what a window using no memory would look like.
/// stdio has no such opinion.
long long procKb(const char *path, const char *field)
{
    FILE *f = fopen(path, "re");
    if (!f) {
        return 0;
    }
    const size_t len = strlen(field);
    long long kb = 0;
    char line[256];
    while (fgets(line, sizeof(line), f)) {
        if (strncmp(line, field, len) == 0 && line[len] == ':') {
            kb = strtoll(line + len + 1, nullptr, 10);
            break;
        }
    }
    fclose(f);
    return kb;
}

// --- the probes -------------------------------------------------------------

/// exec → the first frame. The launcher's timestamp is the other half.
int probeStart()
{
    PaintProbe probe;
    qApp->installEventFilter(&probe);

    MainWindow window;
    window.show();

    if (!spin([&] { return probe.firstPaintNs != 0; }, 10000)) {
        fprintf(stderr, "bench_shell: no frame was ever painted\n");
        return 1;
    }
    printf("first_paint_ns=%lld\n", probe.firstPaintNs);
    return 0;
}

/// What the window weighs with a shell attached and nothing arriving.
///
/// The child prints a screenful and then sleeps, rather than being a login
/// shell: a real one runs the user's `rc` files, and the measurement would
/// move with somebody's prompt.
int probeIdle()
{
    MainWindow window;
    window.show();

    QString error;
    if (!window.session()->connectPty(
            sh(QStringLiteral("i=1; while [ $i -le 24 ]; do echo \"line $i of "
                              "the idle measurement\"; i=$((i+1)); done; sleep 30")),
            &error)) {
        fprintf(stderr, "bench_shell: %s\n", qPrintable(error));
        return 1;
    }

    // Long enough for the output to arrive and be painted, and for Qt's own
    // lazily-created machinery — font caches, the backing store — to exist.
    // Measuring before that reports a window that has not finished being one.
    spin([] { return false; }, 2000);

    printf("rss_kb=%lld pss_kb=%lld\n", procKb("/proc/self/status", "VmRSS"),
           procKb("/proc/self/smaps_rollup", "Pss"));
    return 0;
}

/// 10 MB out of a pty, painted as it arrives, until the far end hangs up.
int probeThroughput(const QString &emitter, int mb)
{
    PaintProbe probe;
    qApp->installEventFilter(&probe);

    MainWindow window;
    window.show();
    spin([&] { return probe.firstPaintNs != 0; }, 10000);

    Session *session = window.session();

    // The clock starts at the *first byte*, not at the fork. Spawning the
    // emitter and letting it generate ten megabytes is tens of milliseconds
    // that have nothing to do with the window, and including them made a 2 MB
    // run look four times slower than a 10 MB one — the fixed cost, amortised.
    long long t0 = 0;
    QObject::connect(session, &Session::damaged, [&t0] {
        if (t0 == 0) {
            t0 = monotonicNs();
        }
    });

    QString error;
    if (!session->connectPty({emitter, QStringLiteral("--emit"), QStringLiteral("plain"),
                              QStringLiteral("--mb"), QString::number(mb)},
                             &error)) {
        fprintf(stderr, "bench_shell: %s\n", qPrintable(error));
        return 1;
    }
    // The far end exiting is what ends this: the pty gives us the bytes it
    // still holds and *then* the hangup, so the last frame is painted before
    // the disconnect is noticed.
    if (!spin([&] { return !session->isConnected(); }, 120000)) {
        fprintf(stderr, "bench_shell: the emitter never finished\n");
        return 1;
    }
    const long long t1 = monotonicNs();
    if (t0 == 0) {
        fprintf(stderr, "bench_shell: the emitter sent nothing\n");
        return 1;
    }

    printf("bytes=%lld ns=%lld paints=%d\n",
           static_cast<long long>(mb) * 1024 * 1024, t1 - t0, probe.paints);
    return 0;
}

/// A keystroke to the frame that shows it.
///
/// Nothing is running in the shell: the pty's *line discipline* echoes, which
/// it does in the kernel and within microseconds, so what is left in the
/// measurement is the notifier waking, the session pumping, and the frame.
int probeLatency()
{
    PaintProbe probe;
    qApp->installEventFilter(&probe);

    MainWindow window;
    window.show();

    QString error;
    if (!window.session()->connectPty(sh(QStringLiteral("sleep 30")), &error)) {
        fprintf(stderr, "bench_shell: %s\n", qPrintable(error));
        return 1;
    }
    spin([&] { return probe.firstPaintNs != 0; }, 10000);
    spin([] { return false; }, 300);

    long long best = std::numeric_limits<long long>::max();
    int painted = 0;
    for (int i = 0; i < 24; i++) {
        const int before = probe.paints;
        const long long t0 = monotonicNs();
        window.session()->sendText(QStringLiteral("x"));
        if (!spin([&] { return probe.paints > before; }, 2000)) {
            continue;
        }
        best = qMin(best, monotonicNs() - t0);
        painted++;
        // Let the line settle, so the next sample starts from an idle window
        // rather than from the tail of this one.
        spin([] { return false; }, 20);
    }

    if (painted < 4) {
        fprintf(stderr, "bench_shell: only %d of 24 keystrokes were painted\n", painted);
        return 1;
    }
    printf("ns=%lld samples=%d\n", best, painted);
    return 0;
}

// --- the launcher -----------------------------------------------------------

struct Child {
    bool ok = false;
    long long launchedNs = 0;
    QString line;

    /// One `key=value` field, or -1.
    long long field(const char *name) const
    {
        for (const QString &pair : line.split(QLatin1Char(' '), Qt::SkipEmptyParts)) {
            const int eq = pair.indexOf(QLatin1Char('='));
            if (eq > 0 && pair.left(eq) == QLatin1String(name)) {
                return pair.mid(eq + 1).toLongLong();
            }
        }
        return -1;
    }
};

Child runProbe(const QString &self, const QStringList &args)
{
    Child child;
    QProcess process;
    process.setProgram(self);
    process.setArguments(args);
    process.setProcessChannelMode(QProcess::ForwardedErrorChannel);

    // Immediately before, because everything after it — fork, exec, the loader
    // resolving Qt — is part of what `start` is measuring.
    child.launchedNs = monotonicNs();
    process.start();
    if (!process.waitForFinished(180000) || process.exitCode() != 0) {
        return child;
    }
    child.line = QString::fromUtf8(process.readAllStandardOutput()).trimmed();
    child.ok = !child.line.isEmpty();
    return child;
}

struct Results {
    double startMs = 0;
    double rssMb = 0;
    double pssMb = 0;
    double throughputMbS = 0;
    int paints = 0;
    double latencyMs = 0;
};

} // namespace

int main(int argc, char **argv)
{
    // Before anything constructs a window, because a `MainWindow` reads the
    // settings and the settings decide the terminal's size. A developer with a
    // 132x50 in their own `sterna.ini` would otherwise be benchmarking a
    // different window from the baseline's, and nothing downstream could tell:
    // the numbers would simply be worse, consistently, for a reason nobody
    // would think to look for.
    QStandardPaths::setTestModeEnabled(true);

    QApplication app(argc, argv);
    const QStringList args = QApplication::arguments();

    // --- a child, doing one thing -------------------------------------------
    const int probeAt = args.indexOf(QStringLiteral("--probe"));
    if (probeAt > 0 && probeAt + 1 < args.size()) {
        const QString which = args.at(probeAt + 1);
        if (which == QLatin1String("start")) {
            return probeStart();
        }
        if (which == QLatin1String("idle")) {
            return probeIdle();
        }
        if (which == QLatin1String("latency")) {
            return probeLatency();
        }
        if (which == QLatin1String("throughput")) {
            const int emitterAt = args.indexOf(QStringLiteral("--emitter"));
            const int mbAt = args.indexOf(QStringLiteral("--mb"));
            return probeThroughput(emitterAt > 0 ? args.at(emitterAt + 1) : QString(),
                                   mbAt > 0 ? args.at(mbAt + 1).toInt() : 10);
        }
        fprintf(stderr, "bench_shell: unknown probe '%s'\n", qPrintable(which));
        return 2;
    }

    // --- the launcher --------------------------------------------------------
    bool json = args.contains(QStringLiteral("--json"));
    int runs = 5;
    const int runsAt = args.indexOf(QStringLiteral("--runs"));
    if (runsAt > 0 && runsAt + 1 < args.size()) {
        runs = qMax(1, args.at(runsAt + 1).toInt());
    }
    int mb = 10;
    const int mbAt = args.indexOf(QStringLiteral("--mb"));
    if (mbAt > 0 && mbAt + 1 < args.size()) {
        mb = qMax(1, args.at(mbAt + 1).toInt());
    }

    // Generated by `tt-bench --emit`, so the bytes that go through the window
    // are the same bytes that go through the engine on its own and the two
    // measurements can be subtracted. The path is compiled in because CMake
    // knows where cargo put it and nothing else does.
    QString emitter = qEnvironmentVariable("TT_BENCH_EXE");
#ifdef TT_BENCH_EXE
    if (emitter.isEmpty()) {
        emitter = QStringLiteral(TT_BENCH_EXE);
    }
#endif

    const QString self = QApplication::applicationFilePath();
    Results best;
    int failures = 0;

    for (int i = 0; i < runs; i++) {
        const Child start = runProbe(self, {QStringLiteral("--probe"), QStringLiteral("start")});
        if (start.ok) {
            const double ms = double(start.field("first_paint_ns") - start.launchedNs) / 1e6;
            if (best.startMs == 0 || ms < best.startMs) {
                best.startMs = ms;
            }
        } else {
            failures++;
        }

        const Child idle = runProbe(self, {QStringLiteral("--probe"), QStringLiteral("idle")});
        if (idle.ok) {
            const double rss = double(idle.field("rss_kb")) / 1024.0;
            const double pss = double(idle.field("pss_kb")) / 1024.0;
            if (best.rssMb == 0 || rss < best.rssMb) {
                best.rssMb = rss;
                best.pssMb = pss;
            }
        } else {
            failures++;
        }

        const Child lat = runProbe(self, {QStringLiteral("--probe"), QStringLiteral("latency")});
        if (lat.ok) {
            const double ms = double(lat.field("ns")) / 1e6;
            if (best.latencyMs == 0 || ms < best.latencyMs) {
                best.latencyMs = ms;
            }
        } else {
            failures++;
        }

        if (!emitter.isEmpty() && QFile::exists(emitter)) {
            const Child put = runProbe(
                self, {QStringLiteral("--probe"), QStringLiteral("throughput"),
                       QStringLiteral("--emitter"), emitter, QStringLiteral("--mb"),
                       QString::number(mb)});
            if (put.ok) {
                const double mbs = (double(put.field("bytes")) / (1024.0 * 1024.0))
                        / (double(put.field("ns")) / 1e9);
                if (mbs > best.throughputMbS) {
                    best.throughputMbS = mbs;
                    best.paints = int(put.field("paints"));
                }
            } else {
                failures++;
            }
        } else if (i == 0) {
            fprintf(stderr,
                    "bench_shell: no tt-bench at '%s' — skipping throughput.\n"
                    "             build it with `cargo build --release -p tt-bench`.\n",
                    qPrintable(emitter));
        }
    }

    if (json) {
        printf("{\n");
        printf("  \"start_ms\": %.1f,\n", best.startMs);
        printf("  \"idle_rss_mb\": %.1f,\n", best.rssMb);
        printf("  \"idle_pss_mb\": %.1f,\n", best.pssMb);
        printf("  \"latency_ms\": %.2f,\n", best.latencyMs);
        printf("  \"throughput_mb_s\": %.2f,\n", best.throughputMbS);
        printf("  \"throughput_paints\": %d,\n", best.paints);
        printf("  \"platform\": \"%s\",\n", qPrintable(QApplication::platformName()));
        // Part of the machine's identity for `bench/bench.py`, not decoration.
        // Qt 6.4.2 and 6.11.1 both call themselves "wayland" and do not
        // measure alike — that gap has produced a false finding here before.
        printf("  \"qt\": \"%s\",\n", qVersion());
        printf("  \"failed_probes\": %d\n", failures);
        printf("}\n");
    } else {
        printf("platform     %s, Qt %s, %d runs\n", qPrintable(QApplication::platformName()),
               qVersion(), runs);
        printf("\n");
        printf("start        %8.1f ms   exec to the first frame\n", best.startMs);
        printf("idle RSS     %8.1f MB   with a shell attached\n", best.rssMb);
        printf("idle PSS     %8.1f MB   ...shared pages counted once\n", best.pssMb);
        printf("latency      %8.2f ms   keystroke to the frame that shows it\n", best.latencyMs);
        if (best.throughputMbS > 0) {
            printf("throughput   %8.2f MB/s %d MB out of a pty, in %d frames\n",
                   best.throughputMbS, mb, best.paints);
        }
        if (failures) {
            printf("\n%d probe run(s) failed\n", failures);
        }
    }

    return failures ? 1 : 0;
}
