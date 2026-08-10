// A file transfer, driven by the window's own event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   ./build/xfer_test
//   ./build/xfer_test --write /tmp    # ...and the two dialogs, as PNGs
//
// The core's suites prove the protocols move a file and that the session hands
// the byte stream over. What only this can prove is that the *window* keeps
// the transfer running: the core's tests pump in a loop, and here the pump
// happens when a `QSocketNotifier` fires or when a timer does — and the timer
// is the half a descriptor cannot cover, because a protocol waiting to retry
// is a line with nothing on it.
//
// Needs `lrzsz`. Without it the transfer cases skip loudly and the dialogs are
// still rendered, since those need no peer.

#include <QApplication>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QLabel>
#include <QPushButton>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "MainWindow.h"
#include "I18n.h"
#include "Session.h"
#include "XferDialog.h"

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
        QTimer::singleShot(10, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

bool have(const char *tool)
{
    return system(qPrintable(QStringLiteral("command -v %1 >/dev/null 2>&1")
                                 .arg(QLatin1String(tool))))
           == 0;
}

/// A payload with the bytes that break naive implementations in it.
bool writePayload(const QString &path, int size)
{
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly)) {
        return false;
    }
    QByteArray body(size, Qt::Uninitialized);
    for (int i = 0; i < size; i++) {
        body[i] = static_cast<char>((i * 31 + i / 251) & 0xff);
    }
    body.append("\x11\x13\x18\x0d\x0a\x1a\xff\xff\xff", 9);
    return f.write(body) == body.size();
}

QStringList sh(const QString &script)
{
    return {QStringLiteral("/bin/sh"), QStringLiteral("-c"), script};
}

TtXferJob zmodemSend()
{
    TtXferJob job = {};
    job.protocol = TT_XFER_PROTOCOL_Z_MODEM;
    job.sending = true;
    job.binary = true;
    job.kermit_mode = 3;
    return job;
}

/// A real ZMODEM send to `rz`, with nothing pumping but Qt.
void test_a_file_goes_out_through_the_event_loop()
{
    if (!have("rz")) {
        printf("  skipped: lrzsz is not installed\n");
        return;
    }
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString src = dir.filePath(QStringLiteral("payload.bin"));
    const QString out = dir.filePath(QStringLiteral("out"));
    CHECK(QDir().mkpath(out));
    CHECK(writePayload(src, 128 * 1024));

    Session session(80, 24);
    QString error;
    // `cd` rather than a cwd parameter: `rz` writes to its working directory,
    // and 2>/dev/null because the peer shares the pty — a warning it prints
    // lands in the protocol stream, and `ymodem.c` meets an unexpected byte
    // with assert(0).
    CHECK(session.connectPty(
        sh(QStringLiteral("cd %1 && rz -b 2>/dev/null").arg(out)), &error));
    CHECK(error.isEmpty());

    TransferResult result;
    bool finished = false;
    int progressCount = 0;
    QObject::connect(&session, &Session::transferFinished,
                     [&](const TransferResult &r) { result = r; finished = true; });
    QObject::connect(&session, &Session::transferProgressed,
                     [&](const TransferProgress &) { progressCount++; });

    CHECK(session.sendFiles(zmodemSend(), {src}, &error));
    CHECK(error.isEmpty());
    CHECK(session.isTransferring());

    CHECK(spin([&] { return finished; }, 60000));
    CHECK(result.success);
    CHECK(!result.cancelled);
    CHECK(progressCount > 0);
    CHECK(!session.isTransferring());

    QFile a(src), b(out + QStringLiteral("/payload.bin"));
    CHECK(a.open(QIODevice::ReadOnly));
    CHECK(b.open(QIODevice::ReadOnly));
    CHECK(a.readAll() == b.readAll());
}

/// Cancelling with no peer at all.
///
/// The case the timer exists for, and the one a descriptor cannot carry:
/// ZMODEM's cancel arms 500 ms and finishes on it, and this line will never
/// become readable again. A window without `m_xferTimer` hangs here with a
/// dialog the user has already dismissed.
void test_cancelling_a_silent_transfer_ends_it()
{
    QTemporaryDir dir;
    const QString src = dir.filePath(QStringLiteral("payload.bin"));
    CHECK(writePayload(src, 4096));

    Session session(80, 24);
    QString error;
    // A peer that reads nothing and says nothing: `sleep` holds the pty open.
    CHECK(session.connectPty(sh(QStringLiteral("sleep 30")), &error));
    CHECK(session.sendFiles(zmodemSend(), {src}, &error));

    bool finished = false;
    TransferResult result;
    QObject::connect(&session, &Session::transferFinished,
                     [&](const TransferResult &r) { result = r; finished = true; });

    session.cancelTransfer();
    QElapsedTimer clock;
    clock.start();
    CHECK(spin([&] { return finished; }, 5000));
    CHECK(result.cancelled);
    CHECK(!result.success);
    // Generous, but it is the *order of magnitude* that matters: half a second
    // is the protocol's own timer and ten seconds would be a read timeout
    // nobody displaced.
    CHECK(clock.elapsed() < 3000);
    session.disconnectPort();
}

/// The window's menu items follow the state, so a second transfer cannot be
/// started on top of the first and neither can be started with no connection.
void test_the_menu_follows_the_state()
{
    MainWindow window;
    window.resize(800, 500);
    window.show();
    QApplication::processEvents();

    const auto action = [&](const QString &text) -> QAction * {
        for (QAction *a : window.findChildren<QAction *>()) {
            if (a->text() == text) {
                return a;
            }
        }
        return nullptr;
    };
    QAction *send = action(QStringLiteral("Send file..."));
    QAction *receive = action(QStringLiteral("Receive file..."));
    CHECK(send != nullptr);
    CHECK(receive != nullptr);
    if (!send || !receive) {
        return;
    }
    CHECK(!send->isEnabled());
    CHECK(!receive->isEnabled());

    QString error;
    CHECK(window.session()->connectPty(sh(QStringLiteral("sleep 5")), &error));
    QApplication::processEvents();
    CHECK(send->isEnabled());
    CHECK(receive->isEnabled());

    window.session()->disconnectPort();
    QApplication::processEvents();
    CHECK(!send->isEnabled());
}

void test_the_dialogs_use_the_language_catalog()
{
    QString error;
    I18n i18n;
    CHECK(i18n.load(QStringLiteral("lang\\ja_JP.lng"), QString(), &error));

    XferOptionsDialog options(true, nullptr, nullptr, &i18n);
    CHECK(options.windowTitle() == QStringLiteral("ファイル送信"));
    CHECK(options.transferTitle() == QStringLiteral("ZMODEM送信"));
    bool protocolLabel = false;
    bool optionLabel = false;
    for (const QLabel *label : options.findChildren<QLabel *>()) {
        protocolLabel |= label->text() == QStringLiteral("プロトコル:");
        optionLabel |= label->text() == QStringLiteral("オプション");
    }
    CHECK(protocolLabel);
    CHECK(optionLabel);

    options.setProtocol(TT_XFER_PROTOCOL_X_MODEM);
    CHECK(options.transferTitle() == QStringLiteral("XMODEM送信"));
    bool checksum = false;
    for (const QComboBox *combo : options.findChildren<QComboBox *>()) {
        checksum |= combo->findText(QStringLiteral("128 bytes, チェックサム"))
                    >= 0;
    }
    CHECK(checksum);

    XferProgressDialog progress(options.transferTitle(), nullptr, &i18n);
    auto *buttons = progress.findChild<QDialogButtonBox *>();
    CHECK(buttons != nullptr);
    if (buttons) {
        CHECK(buttons->button(QDialogButtonBox::Cancel)->text()
              == QStringLiteral("キャンセル"));
        TransferResult complete;
        complete.success = true;
        progress.finish(complete);
        CHECK(buttons->button(QDialogButtonBox::Close)->text()
              == QStringLiteral("閉じる(&C)"));
    }
}

/// The dialogs, rendered. `QWidget::grab()` re-renders offscreen, which is the
/// only screenshot that works here and exactly what is wanted for checking our
/// own layout.
void render_dialogs()
{
    if (g_writeTo.isEmpty()) {
        return;
    }
    XferOptionsDialog options(true);
    options.setProtocol(TT_XFER_PROTOCOL_X_MODEM);
    // Without this the dialog is grabbed before layout and wrapped labels
    // overlap in the image and nowhere else.
    options.adjustSize();
    options.grab().save(g_writeTo + QStringLiteral("/xfer-options.png"));

    XferProgressDialog progress(QStringLiteral("Sending — ZMODEM"));
    TransferProgress p;
    p.protocol = QStringLiteral("ZMODEM");
    p.file = QStringLiteral("/home/user/firmware-2.4.1.bin");
    p.bytes = 812345;
    p.done = 812345;
    p.total = 2097152;
    p.percent = 38;
    p.elapsedMs = 4200;
    progress.update(p);
    progress.adjustSize();
    progress.grab().save(g_writeTo + QStringLiteral("/xfer-progress.png"));

    TransferResult failed;
    failed.message = QStringLiteral("Cannot create file");
    progress.finish(failed);
    progress.adjustSize();
    progress.grab().save(g_writeTo + QStringLiteral("/xfer-failed.png"));

    printf("  wrote xfer-options.png, xfer-progress.png, xfer-failed.png to %s\n",
           qPrintable(g_writeTo));
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    for (int i = 1; i < argc; i++) {
        if (QLatin1String(argv[i]) == QLatin1String("--write") && i + 1 < argc) {
            g_writeTo = QString::fromLocal8Bit(argv[++i]);
        }
    }

    test_a_file_goes_out_through_the_event_loop();
    test_cancelling_a_silent_transfer_ends_it();
    test_the_menu_follows_the_state();
    test_the_dialogs_use_the_language_catalog();
    render_dialogs();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("xfer ok\n");
    return 0;
}
