// A TTL macro, driven by the window's event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/macro_test
//   ./build/macro_test --write /tmp        # ...and the dialogs, as PNGs
//
// The core's own tests run a macro against a `Session` from a pump loop; this
// runs one against a `QSocketNotifier`, which is the arrangement that ships.
// What only shows up here: a notifier registered on the macro's descriptor and
// never serviced, a macro that ends without anyone noticing, and a modal dialog
// re-entered by the notifier that fires inside its own nested event loop.
//
// It needs no server and no hardware — the one connected case forks `/bin/sh`.

#include <QApplication>
#include <QDialog>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "Macro.h"
#include "Session.h"
#include "TerminalView.h"

static int failures = 0;
static QString writeDir;

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

/// A macro on disk. The name matters: upstream fits `.TTL` onto a filename
/// with no extension at all, so a temporary name without one would open a file
/// that is not the one just written.
class Script {
public:
    explicit Script(const QString &body)
    {
        path = QDir(dir.path()).filePath(QStringLiteral("m.ttl"));
        QFile f(path);
        if (f.open(QIODevice::WriteOnly)) {
            f.write(body.toUtf8());
        }
    }
    QString path;

private:
    QTemporaryDir dir;
};

/// Answer whatever modal dialog is up, and photograph it on the way past.
///
/// A repeating timer rather than a `singleShot`, because the dialog runs its
/// own nested event loop and this has to fire *inside* it — which is exactly
/// the situation the notifier's re-entrancy guard exists for.
class Answerer : public QObject {
public:
    explicit Answerer(const QString &tag)
        : m_tag(tag)
    {
        m_timer.setInterval(20);
        connect(&m_timer, &QTimer::timeout, this, &Answerer::answer);
        m_timer.start();
    }
    int answered() const { return m_count; }

private:
    void answer()
    {
        auto *dialog = qobject_cast<QDialog *>(QApplication::activeModalWidget());
        if (!dialog) {
            return;
        }
        if (!writeDir.isEmpty()) {
            // `adjustSize` first for the reason every other dialog test here
            // does it: `grab()` on a dialog that has not been laid out
            // overlaps its own wrapped labels.
            dialog->adjustSize();
            const QString file =
                QDir(writeDir).filePath(QStringLiteral("macro-%1-%2.png")
                                            .arg(m_tag)
                                            .arg(m_count));
            dialog->grab().save(file);
            printf("  wrote %s\n", qPrintable(file));
        }
        m_count++;
        dialog->accept();
    }

    QTimer m_timer;
    QString m_tag;
    int m_count = 0;
};

/// The plainest possible run: a macro that only prints, and a window that
/// notices it started and stopped.
void test_a_macro_prints_and_ends()
{
    Session session(40, 10);
    Macro macro(&session, nullptr);

    int finished = 0;
    int code = -1;
    QObject::connect(&macro, &Macro::finished, [&](int c) {
        finished++;
        code = c;
    });

    Script script(QStringLiteral("dispstr 'hello from a macro'\nsetexitcode 3\n"));
    QString error;
    CHECK(macro.start({script.path}, &error));
    CHECK(error.isEmpty());

    CHECK(spin([&] { return finished > 0; }, 5000));
    CHECK(finished == 1);
    CHECK(code == 3);
    CHECK(!macro.running());
    CHECK(screenText(session).contains(QStringLiteral("hello from a macro")));

    // And a second one runs in the same window, which is the case that finds a
    // notifier or a link left behind by the first.
    Script again(QStringLiteral("dispstr 'and again'\n"));
    CHECK(macro.start({again.path}, &error));
    CHECK(spin([&] { return finished > 1; }, 5000));
    CHECK(screenText(session).contains(QStringLiteral("and again")));
}

/// A macro that will not start says so rather than reporting a run that never
/// happened.
void test_a_macro_that_is_not_there()
{
    Session session(40, 10);
    Macro macro(&session, nullptr);
    QString error;
    CHECK(!macro.start({QStringLiteral("/tmp/sterna-no-such-macro.ttl")}, &error));
    CHECK(!error.isEmpty());
    CHECK(!macro.running());
}

/// The whole path: a macro typing at a shell and waiting for what comes back.
///
/// `wait` reads the *terminal's* output rather than the transport's, which is
/// the surprise `tt-vt`'s macro tap exists for — so a pass here is the tap, the
/// ring, the macro's thread and the window's notifier all in one.
void test_a_macro_drives_a_shell()
{
    Session session(60, 12);
    QString error;
    CHECK(session.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                              QStringLiteral("PS1='$ ' exec /bin/sh -i")},
                             &error));
    CHECK(session.isConnected());

    Macro macro(&session, nullptr);
    int finished = 0;
    QObject::connect(&macro, &Macro::finished, [&](int) { finished++; });

    Script script(QStringLiteral("sendln 'echo macro-was-here'\n"
                                 "wait 'macro-was-here'\n"
                                 "if result = 1 then\n"
                                 "  dispstr 'the macro saw it'\n"
                                 "endif\n"
                                 "sendln 'exit'\n"));
    CHECK(macro.start({script.path}, &error));
    CHECK(spin([&] { return finished > 0; }, 15000));
    const QString screen = screenText(session);
    const bool saw = screen.contains(QStringLiteral("the macro saw it"));
    CHECK(saw);
    if (!saw) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screen));
    }
}

/// The three dialogs a script uses most, answered from the event loop that the
/// macro is blocked against.
void test_the_dialogs()
{
    Session session(60, 12);
    Macro macro(&session, nullptr);
    int finished = 0;
    QObject::connect(&macro, &Macro::finished, [&](int) { finished++; });

    Script script(QStringLiteral("messagebox 'ready?' 'Macro test'\n"
                                 "inputbox 'name' 'Macro test' 'preset'\n"
                                 "strdim items 2\n"
                                 "items[0] = 'first'\n"
                                 "items[1] = 'second'\n"
                                 "listbox 'pick' 'Macro test' items 1\n"
                                 "dispstr inputstr ' and ' items[result]\n"));
    Answerer answerer(QStringLiteral("dialog"));
    QString error;
    CHECK(macro.start({script.path}, &error));
    CHECK(spin([&] { return finished > 0; }, 15000));
    CHECK(answerer.answered() == 3);
    // `inputbox` accepted with its default in it, and the list box on the item
    // the macro asked to start on — so both answers travelled back through the
    // callbacks and into the interpreter's variables.
    const QString screen = screenText(session);
    const bool ok = screen.contains(QStringLiteral("preset and second"));
    CHECK(ok);
    if (!ok) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screen));
    }
}

/// `enablekeyb`, which is the one thing a macro does to the *window* that a
/// user can feel.
void test_enablekeyb_locks_the_terminal()
{
    Session session(40, 10);
    TerminalView view(&session);
    Macro macro(&session, nullptr);
    QObject::connect(&macro, &Macro::keyboardEnabled, &view,
                     &TerminalView::setKeyboardEnabled);

    int finished = 0;
    QObject::connect(&macro, &Macro::finished, [&](int) { finished++; });

    CHECK(view.keyboardEnabled());
    Script script(QStringLiteral("enablekeyb 0\ndispstr 'locked'\npause 1\n"));
    QString error;
    CHECK(macro.start({script.path}, &error));
    CHECK(spin([&] { return !view.keyboardEnabled(); }, 5000));

    // And released when the macro ends, which upstream leaves to Control >
    // Reset terminal — a menu item this port does not have, so a keyboard
    // locked by a macro that died would be locked for good.
    CHECK(spin([&] { return finished > 0; }, 5000));
    CHECK(view.keyboardEnabled());
}

/// Stopping one, which is the End button. A `pause` is broken into polls
/// precisely so this takes milliseconds rather than the hour it asked for.
void test_stopping_a_macro()
{
    Session session(40, 10);
    Macro macro(&session, nullptr);
    int finished = 0;
    QObject::connect(&macro, &Macro::finished, [&](int) { finished++; });

    Script script(QStringLiteral(":top\npause 3600\ngoto top\n"));
    QString error;
    CHECK(macro.start({script.path}, &error));
    CHECK(macro.running());
    macro.cancel();
    CHECK(spin([&] { return finished > 0; }, 5000));
    CHECK(!macro.running());
}

/// And closing the window on one: the destructor has to end the thread rather
/// than deadlock against the join.
void test_destroying_a_running_macro()
{
    Session session(40, 10);
    QElapsedTimer timer;
    timer.start();
    {
        Macro macro(&session, nullptr);
        Script script(QStringLiteral(":top\npause 3600\ngoto top\n"));
        QString error;
        CHECK(macro.start({script.path}, &error));
        CHECK(macro.running());
    }
    CHECK(timer.elapsed() < 2000);
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--write") == 0 && i + 1 < argc) {
            writeDir = QString::fromUtf8(argv[++i]);
        }
    }

    test_a_macro_prints_and_ends();
    test_a_macro_that_is_not_there();
    test_a_macro_drives_a_shell();
    test_the_dialogs();
    test_enablekeyb_locks_the_terminal();
    test_stopping_a_macro();
    test_destroying_a_running_macro();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("macro ok\n");
    return 0;
}
