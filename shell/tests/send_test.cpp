// Sending a file a piece at a time, from the menu to the wire.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/send_test
//
// Needs nothing: every connected case forks `/bin/sh` onto a pty, the way
// `buttons_test` and `pty_test` do.
//
// What it covers that the core's own tests cannot: the clock. `send.rs` is
// handed a fake `Instant` and `tests/send.rs` drives the deadline in a loop of
// its own, so neither of them can tell whether the window arms a timer at all
// — and a paced send whose timer is never armed looks exactly like a send that
// finished instantly with nothing on the wire. Everything here goes through the
// real event loop.

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QLabel>
#include <QLineEdit>
#include <QProgressBar>
#include <QPushButton>
#include <QSpinBox>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "MainWindow.h"
#include "PanelContainer.h"
#include "QuickButtons.h"
#include "SendFileDialog.h"
#include "Session.h"
#include "TerminalPage.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

QString g_writeTo;

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

QString writeIni(const QTemporaryDir &dir)
{
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        return {};
    }
    file.write("[Tera Term]\r\nTerminalSize=60,12\r\n");
    file.close();
    return path;
}

QString writeFile(const QTemporaryDir &dir, const QString &name, const QByteArray &body)
{
    const QString path = dir.filePath(name);
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        return {};
    }
    file.write(body);
    file.close();
    return path;
}

TtSendOptions perLine(int ms)
{
    TtSendOptions o {};
    o.pace = TT_SEND_PACE_PER_LINE;
    o.tick_ms = static_cast<uint32_t>(ms);
    o.chunk = 4096;
    return o;
}

// --- the cases ------------------------------------------------------------

/// The whole path: a file on disk, through the window's timer, onto the wire.
///
/// `cat` sends every line back, so the screen is the receipt.
void a_file_goes_out_a_line_at_a_time()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file =
        writeFile(dir, QStringLiteral("config.txt"),
                  "echo send-alpha\necho send-bravo\necho send-charlie\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    QString error;
    CHECK(session->sendFile(file, perLine(20), &error));
    CHECK(error.isEmpty());
    CHECK(session->isSending());

    // Nothing here services the queue: the window's own timer does, and if it
    // is not armed this waits three seconds and fails.
    CHECK(spin([session] { return !session->isSending(); }, 3000));
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("send-charlie"));
        },
        3000));
    const QString screen = screenText(*session);
    CHECK(screen.contains(QLatin1String("send-alpha")));
    CHECK(screen.contains(QLatin1String("send-bravo")));
    // In order. A queue that lost its place would still have all three.
    CHECK(screen.indexOf(QLatin1String("send-alpha"))
          < screen.indexOf(QLatin1String("send-bravo")));
    CHECK(screen.indexOf(QLatin1String("send-bravo"))
          < screen.indexOf(QLatin1String("send-charlie")));
}

/// The pace is a real wait and not a number the core rounds away.
void a_pace_costs_the_time_it_says()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file = writeFile(dir, QStringLiteral("slow.txt"), "a\nb\nc\nd\n");

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    QElapsedTimer clock;
    clock.start();
    QString error;
    CHECK(session->sendFile(file, perLine(60), &error));
    CHECK(spin([session] { return !session->isSending(); }, 5000));
    // Four lines, and the wait comes after a line only when one is left: three
    // intervals of 60 ms. Asserted as a floor with slack of one interval,
    // because a loaded machine can only make it longer.
    CHECK(clock.elapsed() >= 150);
}

/// Typing is dropped while a send owns the wire — upstream's `TalkStatus`.
void typing_is_dropped_while_a_send_is_running()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file =
        writeFile(dir, QStringLiteral("busy.txt"), "echo one\necho two\n");

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    QString error;
    CHECK(session->sendFile(file, perLine(40), &error));
    session->sendText(QStringLiteral("typed-while-busy\r"));
    CHECK(spin([session] { return !session->isSending(); }, 4000));
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("echo two"));
        },
        3000));
    CHECK(!screenText(*session).contains(QLatin1String("typed-while-busy")));

    // ...and the keyboard comes back.
    session->sendText(QStringLiteral("typed-after\r"));
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("typed-after"));
        },
        3000));
}

/// Hold, let go, and stop — the three buttons on the progress panel, and the
/// property that a held send arms nothing.
void a_send_can_be_held_and_stopped()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    QByteArray body;
    for (int i = 0; i < 40; i++) {
        body += "a line of configuration\n";
    }
    const QString file = writeFile(dir, QStringLiteral("long.txt"), body);

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    QString error;
    CHECK(session->sendFile(file, perLine(30), &error));
    CHECK(spin([session] { return session->sendProgress().sent > 0; }, 2000));

    session->pauseSend(true);
    CHECK(session->sendProgress().paused);
    const qint64 at = session->sendProgress().sent;
    // Half a second of a real event loop, and it has not moved: a held send
    // arms no deadline, so nothing wakes up to move it.
    spin([] { return false; }, 500);
    CHECK(session->sendProgress().sent == at);
    CHECK(session->isSending());

    session->pauseSend(false);
    CHECK(spin([session, at] { return session->sendProgress().sent > at; }, 2000));

    SendResult ended;
    bool sawEnd = false;
    QObject::connect(session, &Session::sendFinished,
                     [&ended, &sawEnd](const SendResult &r) {
                         ended = r;
                         sawEnd = true;
                     });
    session->cancelSend();
    CHECK(spin([&sawEnd] { return sawEnd; }, 2000));
    CHECK(ended.end == TT_SEND_END_CANCELLED);
    CHECK(ended.sent > 0);
    CHECK(ended.sent < ended.total);
    CHECK(!session->isSending());
}

/// A send that outlives its connection ends, and says which.
void a_send_ends_with_the_connection()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    QByteArray body;
    for (int i = 0; i < 40; i++) {
        body += "another line\n";
    }
    const QString file = writeFile(dir, QStringLiteral("long.txt"), body);

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null; exit 0")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    SendResult ended;
    bool sawEnd = false;
    QObject::connect(session, &Session::sendFinished,
                     [&ended, &sawEnd](const SendResult &r) {
                         ended = r;
                         sawEnd = true;
                     });
    QString error;
    CHECK(session->sendFile(file, perLine(40), &error));
    CHECK(spin([session] { return session->sendProgress().sent > 0; }, 2000));
    session->disconnectPort();
    CHECK(spin([&sawEnd] { return sawEnd; }, 3000));
    CHECK(!session->isSending());
    CHECK(session->sendProgress().total == 0);
}

/// Nothing connected, and a file that is not there: both refused with a reason.
void a_send_that_cannot_start_says_why()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file = writeFile(dir, QStringLiteral("f.txt"), "hello\n");

    MainWindow window(ini);
    Session *session = window.session();
    QString error;
    CHECK(!session->sendFile(file, perLine(10), &error));
    CHECK(!error.isEmpty());
    CHECK(!session->isSending());

    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    error.clear();
    CHECK(!session->sendFile(dir.filePath(QStringLiteral("nope.txt")),
                             perLine(10), &error));
    CHECK(!error.isEmpty());
    CHECK(!session->isSending());
}

/// The whole point of the feature, against a real shell: each line waits for
/// the prompt, and the send takes as long as the far end takes.
///
/// `PS1` is set to something unmistakable so the pattern is about this test and
/// not about whatever the developer's own prompt happens to be.
void a_gated_send_waits_for_the_prompt()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file =
        writeFile(dir, QStringLiteral("gated.txt"),
                  "echo gate-one\necho gate-two\necho gate-three\n");

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("PS1='STERNA> '; export PS1; exec /bin/sh -i")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("STERNA>"));
        },
        5000));

    SendResult ended;
    bool sawEnd = false;
    QObject::connect(session, &Session::sendFinished,
                     [&ended, &sawEnd](const SendResult &r) {
                         ended = r;
                         sawEnd = true;
                     });

    TtSendOptions o {};
    o.pace = TT_SEND_PACE_NONE;
    o.gate = TT_SEND_GATE_PROMPT;
    const QByteArray pattern = "STERNA> ";
    o.gate_pattern = pattern.constData();
    // Long enough that a timeout means the gate is broken rather than that the
    // machine is busy.
    o.gate_timeout_ms = 4000;
    o.quiet_ms = 300;

    QString error;
    CHECK(session->sendFile(file, o, &error));
    CHECK(error.isEmpty());
    CHECK(spin([&sawEnd] { return sawEnd; }, 15000));
    CHECK(ended.end == TT_SEND_END_FINISHED);
    // **Nothing was released by the timeout**: every line waited for a real
    // prompt. This is the assertion that separates a working gate from one that
    // quietly gave up twice and produced an identical screen.
    CHECK(ended.timeouts == 0);

    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("gate-three"));
        },
        5000));
    const QString screen = screenText(*session);
    CHECK(screen.contains(QLatin1String("gate-one")));
    CHECK(screen.contains(QLatin1String("gate-two")));
}

/// A pattern nothing will ever match must not wedge the send: the lines go
/// anyway, and the count of how many says the pattern was wrong.
void a_gate_nobody_answers_still_finishes()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    const QString file =
        writeFile(dir, QStringLiteral("wrong.txt"), "one\ntwo\nthree\n");

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    SendResult ended;
    bool sawEnd = false;
    QObject::connect(session, &Session::sendFinished,
                     [&ended, &sawEnd](const SendResult &r) {
                         ended = r;
                         sawEnd = true;
                     });

    TtSendOptions o {};
    o.gate = TT_SEND_GATE_PROMPT;
    const QByteArray pattern = "this-never-appears";
    o.gate_pattern = pattern.constData();
    o.gate_timeout_ms = 100;
    QString error;
    CHECK(session->sendFile(file, o, &error));
    CHECK(spin([&sawEnd] { return sawEnd; }, 6000));
    CHECK(ended.end == TT_SEND_END_FINISHED);
    CHECK(ended.sent == ended.total);
    // Three lines, two gaps between them: two lines went out unanswered.
    CHECK(ended.timeouts == 2);
}

/// A pattern the engine refuses is refused at the dialog, before anything is
/// sent — and the OK button says so.
void a_bad_pattern_is_refused_where_it_is_typed()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    MainWindow window(ini);

    SendFileDialog dialog(window.session());
    auto *gate = dialog.findChild<QComboBox *>(QStringLiteral("sendGate"));
    auto *pattern = dialog.findChild<QLineEdit *>(QStringLiteral("sendGatePattern"));
    auto *error = dialog.findChild<QLabel *>(QStringLiteral("sendGatePatternError"));
    auto *box = dialog.findChild<QDialogButtonBox *>();
    CHECK(gate && pattern && error && box);
    if (!gate || !pattern || !error || !box) {
        return;
    }
    QPushButton *ok = box->button(QDialogButtonBox::Ok);
    CHECK(ok != nullptr);

    gate->setCurrentIndex(gate->findData(TT_SEND_GATE_PROMPT));
    pattern->setText(QStringLiteral("(unclosed"));
    CHECK(!error->text().isEmpty());
    CHECK(ok && !ok->isEnabled());

    pattern->setText(QStringLiteral("# $"));
    CHECK(error->text().isEmpty());
    CHECK(ok && ok->isEnabled());

    // ...and a gate that needs no pattern is never blocked by one.
    pattern->setText(QStringLiteral("(unclosed"));
    gate->setCurrentIndex(gate->findData(TT_SEND_GATE_ECHO));
    CHECK(ok && ok->isEnabled());
    CHECK(!pattern->isVisibleTo(&dialog));
}

/// The dialog's fields come out of the settings file and go back into it.
void the_dialog_reads_and_writes_the_four_settings()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    MainWindow window(ini);
    Session *session = window.session();

    QString error;
    CHECK(session->setSetting(QStringLiteral("transfer.raw_send_delay_type"),
                              QStringLiteral("PerLine"), &error));
    CHECK(session->setSetting(QStringLiteral("transfer.raw_send_delay_tick"),
                              QStringLiteral("250"), &error));
    CHECK(session->setSetting(QStringLiteral("transfer.raw_send_size"),
                              QStringLiteral("512"), &error));
    CHECK(session->setSetting(QStringLiteral("transfer.binary"),
                              QStringLiteral("on"), &error));

    SendFileDialog dialog(session);
    auto *pace = dialog.findChild<QComboBox *>(QStringLiteral("sendPace"));
    auto *interval = dialog.findChild<QSpinBox *>(QStringLiteral("sendInterval"));
    auto *group = dialog.findChild<QSpinBox *>(QStringLiteral("sendGroup"));
    auto *binary = dialog.findChild<QCheckBox *>(QStringLiteral("sendBinary"));
    CHECK(pace && interval && group && binary);
    if (!pace || !interval || !group || !binary) {
        return;
    }
    CHECK(pace->currentData().toInt() == TT_SEND_PACE_PER_LINE);
    CHECK(interval->value() == 250);
    CHECK(group->value() == 512);
    CHECK(binary->isChecked());

    // The group size shows only for the pace that uses it, so a field that
    // means nothing is not sitting there inviting a number.
    CHECK(!group->isVisibleTo(&dialog));
    pace->setCurrentIndex(pace->findData(TT_SEND_PACE_PER_CHUNK));
    CHECK(group->isVisibleTo(&dialog));
    pace->setCurrentIndex(pace->findData(TT_SEND_PACE_NONE));
    CHECK(!interval->isVisibleTo(&dialog));

    const TtSendOptions out = dialog.options();
    CHECK(out.pace == TT_SEND_PACE_NONE);
    CHECK(out.tick_ms == 250);
    CHECK(out.binary);
}

/// The fifth quick-button kind: one click, one file, the gate the button chose.
void a_file_button_sends_its_file()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString file =
        writeFile(dir, QStringLiteral("boot.txt"), "echo button-one\necho button-two\n");

    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile f(ini);
    CHECK(f.open(QIODevice::WriteOnly));
    f.write("[Tera Term]\r\nTerminalSize=60,12\r\n[Sterna Buttons]\r\n"
            "Button1Label=Boot\r\nButton1Kind=file\r\nButton1Value=");
    f.write(file.toUtf8());
    f.write("\r\nButton1Gate=none\r\n");
    f.close();

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    QMetaObject::invokeMethod(&window, "runQuickButton", Qt::DirectConnection,
                              Q_ARG(int, 0), Q_ARG(bool, false));
    CHECK(spin([session] { return !session->isSending(); }, 5000));
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("button-two"));
        },
        3000));
    CHECK(screenText(*session).contains(QLatin1String("button-one")));
}

/// ...and it survives the settings file: the two keys are written only for this
/// kind, so a file with none of them is the file it always was.
void a_file_buttons_gate_round_trips_through_the_file()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile f(ini);
    CHECK(f.open(QIODevice::WriteOnly));
    f.write("[Tera Term]\r\nTerminalSize=60,12\r\n[Sterna Buttons]\r\n"
            "Button1Label=Boot\r\nButton1Kind=file\r\nButton1Value=/tmp/x.txt\r\n"
            "Button1Gate=prompt\r\nButton1Prompt=# $\r\n"
            "Button2Label=Say\r\nButton2Kind=text\r\nButton2Value=hello\r\n");
    f.close();

    const QuickButtonSet set = loadQuickButtons(ini);
    CHECK(set.buttons.size() == 2);
    if (set.buttons.size() != 2) {
        return;
    }
    CHECK(set.buttons[0].kind == TT_QUICK_BUTTON_FILE);
    CHECK(set.buttons[0].text == QLatin1String("/tmp/x.txt"));
    CHECK(set.buttons[0].gate == static_cast<int>(TT_SEND_GATE_PROMPT));
    CHECK(set.buttons[0].prompt == QLatin1String("# $"));
    // An ordinary button has not grown a gate: absent means the settings
    // decide, which is what every button written before this kind existed says.
    CHECK(set.buttons[1].gate == TT_SEND_GATE_FROM_SETTINGS);
    CHECK(set.buttons[1].prompt.isEmpty());

    const QString out = dir.filePath(QStringLiteral("out.ini"));
    QFile blank(out);
    CHECK(blank.open(QIODevice::WriteOnly));
    blank.close();
    CHECK(saveQuickButtons(out, set));
    const QuickButtonSet again = loadQuickButtons(out);
    CHECK(again.buttons.size() == 2);
    if (again.buttons.size() == 2) {
        CHECK(again.buttons[0] == set.buttons[0]);
        CHECK(again.buttons[1] == set.buttons[1]);
    }
    // ...and the ordinary button's record says nothing about a gate.
    QFile written(out);
    CHECK(written.open(QIODevice::ReadOnly));
    const QString body = QString::fromUtf8(written.readAll());
    CHECK(!body.contains(QLatin1String("Button2Gate")));
    CHECK(!body.contains(QLatin1String("Button2Prompt")));
}

/// The progress panel follows the send without being told, and turns Stop into
/// Close when it is over.
void the_progress_panel_follows_the_send()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir);
    QByteArray body;
    for (int i = 0; i < 30; i++) {
        body += "a line to send\n";
    }
    const QString file = writeFile(dir, QStringLiteral("long.txt"), body);

    MainWindow window(ini);
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    // Through the container rather than `findChild<TerminalPage *>`, which does
    // not compile: `TerminalPage` carries no `Q_OBJECT`.
    auto *panels = window.findChild<PanelContainer *>();
    CHECK(panels != nullptr);
    if (!panels || panels->visiblePages().isEmpty()) {
        return;
    }
    auto *page = static_cast<TerminalPage *>(panels->visiblePages().first());

    auto *dialog = new SendProgressDialog(QStringLiteral("Send"), &window);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    page->setSendDialog(dialog);
    QObject::connect(dialog, &SendProgressDialog::poll, dialog,
                     [dialog, session] { dialog->update(session->sendProgress()); });
    QObject::connect(dialog, &SendProgressDialog::cancelled, session,
                     &Session::cancelSend);
    QObject::connect(session, &Session::sendFinished, dialog,
                     [dialog](const SendResult &r) { dialog->finish(r); });
    dialog->show();

    QString error;
    CHECK(session->sendFile(file, perLine(20), &error));
    auto *bar = dialog->findChild<QProgressBar *>(QStringLiteral("sendProgressBar"));
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    // Nothing calls `update` here. The panel's own timer does — which is the
    // point: a progress event per piece would be one per character on a
    // per-character pace.
    CHECK(spin([bar] { return bar->value() > 0; }, 3000));
    CHECK(spin([session] { return !session->isSending(); }, 6000));

    auto *stop = dialog->findChild<QPushButton *>(QStringLiteral("sendStopButton"));
    auto *hold = dialog->findChild<QPushButton *>(QStringLiteral("sendPauseButton"));
    CHECK(stop && hold);
    if (stop && hold) {
        CHECK(!hold->isEnabled());
        CHECK(bar->value() == 100);
    }

    if (!g_writeTo.isEmpty()) {
        dialog->grab().save(g_writeTo + QStringLiteral("/send-progress.png"));
        SendFileDialog options(session);
        options.adjustSize();
        options.grab().save(g_writeTo + QStringLiteral("/send-options.png"));
    }
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("send_test"));
    for (int i = 1; i < argc; i++) {
        if (QLatin1String(argv[i]) == QLatin1String("--write") && i + 1 < argc) {
            g_writeTo = QString::fromLocal8Bit(argv[++i]);
        }
    }

    a_file_goes_out_a_line_at_a_time();
    a_pace_costs_the_time_it_says();
    typing_is_dropped_while_a_send_is_running();
    a_send_can_be_held_and_stopped();
    a_send_ends_with_the_connection();
    a_send_that_cannot_start_says_why();
    a_gated_send_waits_for_the_prompt();
    a_gate_nobody_answers_still_finishes();
    a_bad_pattern_is_refused_where_it_is_typed();
    a_file_button_sends_its_file();
    a_file_buttons_gate_round_trips_through_the_file();
    the_dialog_reads_and_writes_the_four_settings();
    the_progress_panel_follows_the_send();

    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("send ok");
    return 0;
}
