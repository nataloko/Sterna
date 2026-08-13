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
#include <QFile>
#include <QKeyEvent>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>
#include <functional>
#include <initializer_list>

#include "Session.h"
#include "TerminalView.h"

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

struct Setting {
    const char *name;
    const char *value;
};

void key(TerminalView &view, QEvent::Type type, int code,
         Qt::KeyboardModifiers modifiers, const QString &text = QString(),
         quint32 scanCode = 0, quint32 virtualKey = 0)
{
    QKeyEvent event(type, code, modifiers, scanCode, virtualKey, 0, text);
    QCoreApplication::sendEvent(&view, &event);
}

/// Put exactly `count` terminal input bytes through a real pty and let `od`
/// make them visible. Waiting for `ready` is load-bearing: otherwise a fast
/// test can type before the child has disabled canonical input and spend five
/// seconds waiting for a newline that was intentionally never sent.
QString captureKeys(int count, std::initializer_list<Setting> settings,
                    const std::function<void(TerminalView &)> &send,
                    const std::function<void(Session &)> &prepare = {})
{
    Session session(40, 10);
    QString error;
    for (const Setting &setting : settings) {
        CHECK(session.setSetting(QString::fromLatin1(setting.name),
                                 QString::fromLatin1(setting.value), &error));
    }
    if (prepare) {
        prepare(session);
    }
    TerminalView view(&session);
    view.applySettings();
    CHECK(session.connectPty(
        sh(QStringLiteral("stty raw -echo; printf 'ready\\r\\nbytes:'; "
                          "od -An -t x1 -N %1")
               .arg(count)),
        &error));
    CHECK(error.isEmpty());
    CHECK(spin([&] { return screenText(session).contains(QStringLiteral("ready")); },
               5000));

    send(view);
    const bool finished = spin([&] { return !session.isConnected(); }, 5000);
    CHECK(finished);
    const QString screen = screenText(session);
    if (!finished) {
        fprintf(stderr, "key capture screen was:\n%s\n", qPrintable(screen));
        session.disconnectPort();
    }
    return screen;
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

void test_meta_key_modes()
{
    // MetaKey ships off: Alt belongs to Qt's menu handling, while the next
    // ordinary character still reaches the terminal.
    QString screen = captureKeys(1, {}, [](TerminalView &view) {
        key(view, QEvent::KeyPress, Qt::Key_A, Qt::AltModifier,
            QStringLiteral("a"));
        key(view, QEvent::KeyPress, Qt::Key_Z, Qt::NoModifier,
            QStringLiteral("z"));
    });
    CHECK(screen.contains(QStringLiteral("7a")));

    // Meta8Bit=off means ESC-prefix, despite that setting's misleading name.
    screen = captureKeys(2, {{"keyboard.meta", "on"}}, [](TerminalView &view) {
        key(view, QEvent::KeyPress, Qt::Key_A, Qt::AltModifier,
            QStringLiteral("a"));
    });
    CHECK(screen.contains(QStringLiteral("1b 61")));

    screen = captureKeys(1,
                         {{"keyboard.meta", "on"},
                          {"keyboard.meta_8bit", "raw"}},
                         [](TerminalView &view) {
                             key(view, QEvent::KeyPress, Qt::Key_A,
                                 Qt::AltModifier, QStringLiteral("a"));
                         });
    CHECK(screen.contains(QStringLiteral("e1")));

    // Text mode sets U+0080 before the session encodes the character, making
    // U+00E1 and therefore the two UTF-8 bytes C3 A1.
    screen = captureKeys(2,
                         {{"keyboard.meta", "on"},
                          {"keyboard.meta_8bit", "text"}},
                         [](TerminalView &view) {
                             key(view, QEvent::KeyPress, Qt::Key_A,
                                 Qt::AltModifier, QStringLiteral("a"));
                         });
    CHECK(screen.contains(QStringLiteral("c3 a1")));

    // The character event carries no side. A right-Alt press must not satisfy
    // `left`, while the XKB left-Alt event immediately after it must.
    screen = captureKeys(2, {{"keyboard.meta", "left"}}, [](TerminalView &view) {
        key(view, QEvent::KeyPress, Qt::Key_Alt, Qt::AltModifier, {}, 108,
            0xFFEA);
        key(view, QEvent::KeyPress, Qt::Key_A, Qt::AltModifier,
            QStringLiteral("a"));
        key(view, QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier, {}, 108,
            0xFFEA);
        key(view, QEvent::KeyPress, Qt::Key_Alt, Qt::AltModifier, {}, 64,
            0xFFE9);
        key(view, QEvent::KeyPress, Qt::Key_B, Qt::AltModifier,
            QStringLiteral("b"));
        key(view, QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier, {}, 64,
            0xFFE9);
    });
    CHECK(screen.contains(QStringLiteral("1b 62")));
}

void test_strict_mapping_and_delete()
{
    const QString screen =
        captureKeys(2,
                    {{"keyboard.strict_mapping", "on"},
                     {"keyboard.delete_sends_del", "on"}},
                    [](TerminalView &view) {
                        // No KEYBOARD.CNF entry exists, so strict mode drops
                        // the built-in Up fallback. DeleteKey is upstream's
                        // explicit exception and still produces DEL.
                        key(view, QEvent::KeyPress, Qt::Key_Up, Qt::NoModifier);
                        key(view, QEvent::KeyPress, Qt::Key_Delete,
                            Qt::NoModifier);
                        key(view, QEvent::KeyPress, Qt::Key_Z, Qt::NoModifier,
                            QStringLiteral("z"));
                    });
    CHECK(screen.contains(QStringLiteral("7f 7a")));
}

void test_keyboard_cnf_overrides_the_builtin_key_and_maps_modifiers()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("KEYBOARD.CNF"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[VT editor keypad]\nUp=59\n"
               "[User keys]\nUser1=1054,0,$5A\n");
    file.close();

    const QString screen = captureKeys(
        4, {{"keyboard.strict_mapping", "on"}},
        [](TerminalView &view) {
            // F1's physical code is assigned to the terminal's Up key. Ctrl+A
            // is 30 | 0x400 and invokes a binary user key.
            key(view, QEvent::KeyPress, Qt::Key_F1, Qt::NoModifier);
            key(view, QEvent::KeyPress, Qt::Key_A, Qt::ControlModifier);
        },
        [&](Session &session) {
            QString error;
            QVector<quint16> duplicates;
            CHECK(session.loadKeyMap(path, &duplicates, &error));
            CHECK(error.isEmpty());
            CHECK(duplicates.isEmpty());
        });
    CHECK(screen.contains(QStringLiteral("1b 5b 41 5a")));
}

void test_line_edit_sends_the_edited_line_on_return()
{
    const QString screen = captureKeys(
        4, {{"terminal.line_edit", "on"}}, [](TerminalView &view) {
            key(view, QEvent::KeyPress, Qt::Key_A, Qt::NoModifier,
                QStringLiteral("a"));
            key(view, QEvent::KeyPress, Qt::Key_C, Qt::NoModifier,
                QStringLiteral("c"));
            key(view, QEvent::KeyPress, Qt::Key_Left, Qt::NoModifier);
            key(view, QEvent::KeyPress, Qt::Key_B, Qt::NoModifier,
                QStringLiteral("b"));
            key(view, QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier,
                QStringLiteral("\r"));
        });
    CHECK(screen.contains(QStringLiteral("61 62 63 0d")));
}

void test_line_edit_leaves_function_and_control_keys_immediate()
{
    const QString screen = captureKeys(
        6, {{"terminal.line_edit", "on"}}, [](TerminalView &view) {
            // This printable byte remains in the draft. F1 and Ctrl+C go
            // straight to the child: ESC [ 1 1 ~ followed by ETX.
            key(view, QEvent::KeyPress, Qt::Key_X, Qt::NoModifier,
                QStringLiteral("x"));
            key(view, QEvent::KeyPress, Qt::Key_F1, Qt::NoModifier);
            key(view, QEvent::KeyPress, Qt::Key_C, Qt::ControlModifier,
                QString(QChar(0x03)));
        });
    CHECK(screen.contains(QStringLiteral("1b 5b 31 31 7e 03")));
}

void test_line_edit_queues_pasted_lines_until_each_return()
{
    const QString screen = captureKeys(
        8,
        {{"terminal.line_edit", "on"},
         {"clipboard.confirm_paste", "off"}},
        [](TerminalView &view) {
            view.pasteText(QStringLiteral("one\r\ntwo"));
            CHECK(view.lineEditText() == QStringLiteral("one"));
            CHECK(view.queuedLineCount() == 1);
            key(view, QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier,
                QStringLiteral("\r"));
            CHECK(view.lineEditText() == QStringLiteral("two"));
            key(view, QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier,
                QStringLiteral("\r"));
        });
    CHECK(screen.contains(
        QStringLiteral("6f 6e 65 0d 74 77 6f 0d")));
}

void test_shift_escape_cycles_the_configured_debug_modes()
{
    Session session(40, 10);
    QString error;
    CHECK(session.setSetting(QStringLiteral("debug.enabled"),
                             QStringLiteral("on"), &error));
    CHECK(session.setSetting(QStringLiteral("debug.modes"),
                             QStringLiteral("hex"), &error));
    TerminalView view(&session);
    view.applySettings();

    key(view, QEvent::KeyPress, Qt::Key_Escape, Qt::ShiftModifier);
    session.feed(QByteArray("\033[A", 3));
    CHECK(screenText(session).contains(QStringLiteral("1B 5B 41")));
}

/// XTWINOPS' report half, all the way out to a program that asked.
///
/// The child sends the query and reads the answer off its own stdin, so this
/// covers the whole seam — the frontend's snapshot, the engine's reply, and
/// the write back down the pty — rather than any one layer's idea of it.
void test_the_window_answers_what_a_program_asks_about_it()
{
    Session session(80, 24);

    TtWindowMetrics m{};
    m.x = 300; m.y = 120;
    m.client_x = 308; m.client_y = 156;
    m.width = 1288; m.height = 800;
    m.client_width = 1280; m.client_height = 768;
    m.cell_width = 16; m.cell_height = 32;
    m.screen_width = 2560; m.screen_height = 1440;
    session.setWindowMetrics(m);

    QString error;
    // Raw mode and a byte count, not `read`: the answer to `CSI 14 t` ends in
    // `t` and carries no newline, so a line-oriented read waits for one that
    // is never coming. `-echo` keeps the reply off the screen, and the leading
    // ESC is stripped before it is printed back — otherwise echoing the answer
    // to `CSI 14 t` would be read as `CSI 4 t`, a resize.
    CHECK(session.connectPty(
        sh(QStringLiteral("stty raw -echo; "
                          "printf '\\033[14t'; a=$(head -c 13); "
                          "printf '\\033[16t'; b=$(head -c 10); "
                          "printf 'A=%s B=%s\\r\\n' \"${a#?}\" \"${b#?}\"")),
        &error));

    // Height then width for both, and the cell is the one the frontend pushed
    // rather than the font's.
    const bool seen = spin(
        [&] { return screenText(session).contains(QStringLiteral("A=[4;768;1280t")); },
        5000);
    CHECK(seen);
    CHECK(screenText(session).contains(QStringLiteral("B=[6;32;16t")));
    if (!seen) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }
    session.disconnectPort();
}

/// ...and the action half, which is a signal rather than a report because the
/// core has no window to iconify.
void test_the_window_operations_reach_the_frontend()
{
    Session session(40, 10);
    QVector<TtWindowRequest> seen;
    QObject::connect(&session, &Session::windowOperationRequested,
                     [&](const TtWindowRequest &r) { seen.append(r); });

    QString error;
    CHECK(session.connectPty(sh(QStringLiteral("printf '\\033[5t\\033[3;40;50t'; sleep 0.2")),
                             &error));
    CHECK(spin([&] { return seen.size() >= 2; }, 5000));
    if (seen.size() >= 2) {
        CHECK(seen[0].op == TT_WINDOW_OP_RAISE);
        CHECK(seen[1].op == TT_WINDOW_OP_MOVE);
        CHECK(seen[1].x == 40 && seen[1].y == 50);
    }
    session.disconnectPort();
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
    test_meta_key_modes();
    test_strict_mapping_and_delete();
    test_keyboard_cnf_overrides_the_builtin_key_and_maps_modifiers();
    test_line_edit_sends_the_edited_line_on_return();
    test_line_edit_leaves_function_and_control_keys_immediate();
    test_line_edit_queues_pasted_lines_until_each_return();
    test_shift_escape_cycles_the_configured_debug_modes();
    test_the_window_answers_what_a_program_asks_about_it();
    test_the_window_operations_reach_the_frontend();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("pty ok\n");
    return 0;
}
