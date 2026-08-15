// The log dialog, and the two ways to pause what it started.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/log_test [--write <dir>]
//
// Needs nothing: no host, no hardware, no printer. Everything here is either a
// dialog measured before it is shown or a log written to a temporary file by a
// session that was fed its bytes directly.
//
// What it catches that `tt-session` and the C ABI cannot: those stop at
// `start_log`. This is the half where a dialog decides what to pass, a menu
// decides which of three items is reachable, and a click on a status label
// reaches a core call.

#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QLabel>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPushButton>
#include <QRadioButton>
#include <QSpinBox>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "LogDialog.h"
#include "MainWindow.h"
#include "PageStatusBar.h"
#include "PanelContainer.h"
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
        QTimer::singleShot(10, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

QByteArray slurp(const QString &path)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) {
        return QByteArray();
    }
    return f.readAll();
}

/// An INI in a scratch directory, so a test never reads the developer's own.
QString makeIni(const QTemporaryDir &dir, const QByteArray &body = QByteArray("[Sterna]\r\n"))
{
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(path);
    if (file.open(QIODevice::WriteOnly)) {
        file.write(body);
        file.close();
    }
    return path;
}

/// Every control the dialog offers is answered from the settings it edits, so
/// opening it twice shows what was chosen the first time.
void test_the_dialog_is_seeded_from_the_settings_and_writes_them_back()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    Session session(24, 4);
    CHECK(session.setSetting(QStringLiteral("log.binary"), QStringLiteral("on"), nullptr));
    CHECK(session.setSetting(QStringLiteral("log.timestamp"), QStringLiteral("on"), nullptr));
    CHECK(session.setSetting(QStringLiteral("log.timestamp_type"), QStringLiteral("UTC"),
                             nullptr));
    CHECK(session.setSetting(QStringLiteral("log.rotate"), QStringLiteral("1"), nullptr));
    CHECK(session.setSetting(QStringLiteral("log.rotate_size_type"), QStringLiteral("2"),
                             nullptr));
    CHECK(session.setSetting(QStringLiteral("log.rotate_size"),
                             QString::number(3 * 1024 * 1024), nullptr));

    LogOptionsDialog dialog(&session);
    auto *binary = dialog.findChild<QRadioButton *>(QStringLiteral("logBinary"));
    auto *timestamp = dialog.findChild<QCheckBox *>(QStringLiteral("logTimestamp"));
    auto *type = dialog.findChild<QComboBox *>(QStringLiteral("logTimestampType"));
    auto *size = dialog.findChild<QSpinBox *>(QStringLiteral("logRotateSize"));
    auto *unit = dialog.findChild<QComboBox *>(QStringLiteral("logRotateUnit"));
    CHECK(binary && timestamp && type && size && unit);
    if (!binary || !timestamp || !type || !size || !unit) {
        return;
    }

    CHECK(binary->isChecked());
    CHECK(timestamp->isChecked());
    CHECK(type->currentIndex() == 1);
    // The stored number is bytes and the unit is only how it is shown — a
    // reader that scaled the stored value by the unit would offer 3 GB here
    // and rotate at a terabyte.
    CHECK(unit->currentIndex() == 2);
    CHECK(size->value() == 3);

    // ...and back the other way. The type is written as a name, not as the
    // `GetCurSel() - 1` upstream writes against the plain index it reads.
    auto *text = dialog.findChild<QRadioButton *>(QStringLiteral("logText"));
    CHECK(text != nullptr);
    if (text) {
        text->setChecked(true);
    }
    type->setCurrentIndex(3);
    size->setValue(5);
    unit->setCurrentIndex(1);
    dialog.applySettings();
    CHECK(session.setting(QStringLiteral("log.binary")) == QLatin1String("off"));
    CHECK(session.setting(QStringLiteral("log.timestamp_type"))
          == QLatin1String("ConnectionElapsed"));
    CHECK(session.setting(QStringLiteral("log.rotate_size")) == QString::number(5 * 1024));
}

/// `ArrangeControls` (`logdlg.cpp:167`), which is the dialog's only real logic:
/// a choice that makes another meaningless greys it rather than ignoring it.
void test_a_choice_disables_what_it_makes_meaningless()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    Session session(24, 4);
    LogOptionsDialog dialog(&session);

    auto *file = dialog.findChild<QLineEdit *>(QStringLiteral("logFile"));
    auto *overwrite = dialog.findChild<QRadioButton *>(QStringLiteral("logOverwrite"));
    auto *append = dialog.findChild<QRadioButton *>(QStringLiteral("logAppend"));
    auto *text = dialog.findChild<QRadioButton *>(QStringLiteral("logText"));
    auto *binary = dialog.findChild<QRadioButton *>(QStringLiteral("logBinary"));
    auto *bom = dialog.findChild<QCheckBox *>(QStringLiteral("logBom"));
    auto *plain = dialog.findChild<QCheckBox *>(QStringLiteral("logPlainText"));
    auto *timestamp = dialog.findChild<QCheckBox *>(QStringLiteral("logTimestamp"));
    auto *type = dialog.findChild<QComboBox *>(QStringLiteral("logTimestampType"));
    auto *rotate = dialog.findChild<QCheckBox *>(QStringLiteral("logRotate"));
    auto *size = dialog.findChild<QSpinBox *>(QStringLiteral("logRotateSize"));
    CHECK(file && overwrite && append && text && binary && bom && plain && timestamp && type
          && rotate && size);
    if (!file || !overwrite || !append || !text || !binary || !bom || !plain || !timestamp
        || !type || !rotate || !size) {
        return;
    }

    // A binary log is bytes as they arrived: nothing to strip, nowhere to put
    // a stamp, and no screen to prepend.
    binary->setChecked(true);
    CHECK(!plain->isEnabled());
    CHECK(!timestamp->isEnabled());
    CHECK(!type->isEnabled());
    CHECK(!bom->isEnabled());

    text->setChecked(true);
    CHECK(plain->isEnabled());
    CHECK(timestamp->isEnabled());
    CHECK(bom->isEnabled());
    // The type is only a question once there is a timestamp to type.
    timestamp->setChecked(false);
    CHECK(!type->isEnabled());
    timestamp->setChecked(true);
    CHECK(type->isEnabled());

    CHECK(!rotate->isChecked());
    CHECK(!size->isEnabled());
    rotate->setChecked(true);
    CHECK(size->isEnabled());

    // Appending to a file that does not exist is overwriting it with extra
    // steps, so the choice is not offered until the name names something.
    const QString missing = dir.filePath(QStringLiteral("not-here.log"));
    file->setText(missing);
    emit file->textEdited(missing);
    CHECK(!append->isEnabled());
    CHECK(overwrite->isChecked());

    const QString here = dir.filePath(QStringLiteral("here.log"));
    QFile f(here);
    CHECK(f.open(QIODevice::WriteOnly));
    f.write("old\n");
    f.close();
    file->setText(here);
    emit file->textEdited(here);
    CHECK(append->isEnabled());
    // ...and a mark belongs at the head of a file, so an append cannot ask for
    // one however the tick was left.
    append->setChecked(true);
    CHECK(!bom->isEnabled());
}

/// The field opens on the directory the last log went to, and on the settings'
/// own when nothing has been written yet. Whichever it is, the *name* is the
/// template expanded now — which is what stops a second log landing on the
/// first.
void test_the_field_offers_the_last_directory_and_a_dated_name()
{
    QTemporaryDir dir;
    QTemporaryDir elsewhere;
    CHECK(dir.isValid() && elsewhere.isValid());
    Session session(24, 4);
    CHECK(session.setSetting(QStringLiteral("log.default_path"), dir.path(), nullptr));

    {
        LogOptionsDialog dialog(&session);
        auto *file = dialog.findChild<QLineEdit *>(QStringLiteral("logFile"));
        CHECK(file != nullptr);
        if (!file) {
            return;
        }
        CHECK(file->text().startsWith(dir.path()));
        // The shipped `LogDefaultName` is a template, so the offered name
        // carries the date rather than being one constant for every session.
        CHECK(!file->text().endsWith(QStringLiteral("teraterm.log")));
        CHECK(file->text().endsWith(QStringLiteral(".log")));
    }

    // A directory somebody chose is a more recent answer than the one the file
    // names, so it wins next time.
    CHECK(session.setSetting(QStringLiteral("recent.log_dir"), elsewhere.path(), nullptr));
    {
        LogOptionsDialog dialog(&session);
        auto *file = dialog.findChild<QLineEdit *>(QStringLiteral("logFile"));
        CHECK(file != nullptr);
        if (file) {
            CHECK(file->text().startsWith(elsewhere.path()));
        }
    }
}

/// File > Log opens the dialog, what it says reaches the file, and the
/// directory it landed in is remembered.
void test_the_menu_item_starts_the_log_the_dialog_configured()
{
    QTemporaryDir dir;
    QTemporaryDir logs;
    CHECK(dir.isValid() && logs.isValid());
    const QString ini = makeIni(dir);
    MainWindow window(ini);
    window.session()->feed(QByteArray("already here\r\n"));

    auto *action = window.findChild<QAction *>(QStringLiteral("logAction"));
    auto *pause = window.findChild<QAction *>(QStringLiteral("pauseLogAction"));
    auto *stop = window.findChild<QAction *>(QStringLiteral("stopLogAction"));
    CHECK(action && pause && stop);
    if (!action || !pause || !stop) {
        return;
    }
    // Nothing is logging, so only the item that can start one is reachable.
    CHECK(action->isEnabled());
    CHECK(!pause->isEnabled());
    CHECK(!stop->isEnabled());

    const QString path = logs.filePath(QStringLiteral("session.log"));
    QTimer::singleShot(0, [&path] {
        auto *dialog = qobject_cast<LogOptionsDialog *>(QApplication::activeModalWidget());
        CHECK(dialog != nullptr);
        if (!dialog) {
            return;
        }
        auto *file = dialog->findChild<QLineEdit *>(QStringLiteral("logFile"));
        auto *screen = dialog->findChild<QCheckBox *>(QStringLiteral("logIncludeScreen"));
        auto *buttons = dialog->findChild<QDialogButtonBox *>();
        CHECK(file && screen && buttons);
        if (!file || !screen || !buttons) {
            dialog->reject();
            return;
        }
        file->setText(path);
        emit file->textEdited(path);
        screen->setChecked(true);
        buttons->button(QDialogButtonBox::Ok)->click();
    });
    action->trigger();

    CHECK(window.session()->isLogging());
    CHECK(window.session()->logPath() == path);
    // ...and the three items have swapped over.
    CHECK(!action->isEnabled());
    CHECK(pause->isEnabled());
    CHECK(stop->isEnabled());

    window.session()->feed(QByteArray("live bytes\r\n"));
    CHECK(spin([&] { return window.session()->logBytes() > 0; }, 2000));
    stop->trigger();
    CHECK(!window.session()->isLogging());

    const QByteArray written = slurp(path);
    // What was on the screen when the log opened is in the file, ahead of what
    // arrived afterwards.
    CHECK(written.contains("already here"));
    CHECK(written.contains("live bytes"));
    CHECK(written.indexOf("already here") < written.indexOf("live bytes"));

    // Where it went is remembered, for the next dialog to open on.
    CHECK(window.session()->setting(QStringLiteral("recent.log_dir")) == logs.path());
}

/// Pausing means what arrives meanwhile is dropped rather than held, which is
/// the whole point of it: a pause that buffered would write the gap into the
/// file the moment it ended. Reachable from the menu and from the indicator
/// counting the bytes, which is where Tera Term's Pause button would be if
/// this program had the logging window it lives on.
void test_pausing_stops_the_bytes_from_either_place()
{
    QTemporaryDir dir;
    QTemporaryDir logs;
    CHECK(dir.isValid() && logs.isValid());
    const QString ini = makeIni(dir);
    MainWindow window(ini);
    Session *session = window.session();

    const QString path = logs.filePath(QStringLiteral("paused.log"));
    QString error;
    CHECK(session->startLog(path, &error));
    session->feed(QByteArray("kept\r\n"));
    const quint64 atPause = session->logBytes();
    CHECK(atPause > 0);

    auto *pause = window.findChild<QAction *>(QStringLiteral("pauseLogAction"));
    CHECK(pause != nullptr);
    if (!pause) {
        return;
    }
    CHECK(pause->isEnabled());
    CHECK(!pause->isChecked());

    pause->trigger();
    CHECK(session->logPaused());
    CHECK(pause->isChecked());
    session->feed(QByteArray("lost\r\n"));
    CHECK(session->logBytes() == atPause);

    auto *page = static_cast<TerminalPage *>(
        window.findChild<PanelContainer *>()->widget(0));
    auto *label = page->status()->findChild<QLabel *>(QStringLiteral("statusLog"));
    CHECK(label != nullptr);
    if (!label) {
        return;
    }
    // Paused it says so and stops blinking — a steady number is the honest
    // shape for a counter that has stopped, and the blink is what says a
    // recording is running.
    auto *blink = page->status()->findChild<QTimer *>(QStringLiteral("statusLogBlinkTimer"));
    CHECK(blink != nullptr);
    CHECK(label->text().startsWith(QStringLiteral("PAUSED ")));
    CHECK(label->styleSheet().contains(QStringLiteral("#f9a825")));
    CHECK(blink && !blink->isActive());

    const auto click = [label] {
        QMouseEvent press(QEvent::MouseButtonPress, QPointF(2, 2), QPointF(2, 2),
                          Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(label, &press);
    };
    click();
    CHECK(!session->logPaused());
    CHECK(!pause->isChecked());
    CHECK(label->text().startsWith(QStringLiteral("REC ")));
    CHECK(blink && blink->isActive());

    // ...and back the other way, from the same place.
    click();
    CHECK(session->logPaused());
    click();
    CHECK(!session->logPaused());

    session->feed(QByteArray("kept2\r\n"));
    CHECK(session->logBytes() > atPause);
    session->stopLog();
    CHECK(slurp(path) == QByteArray("kept\nkept2\n"));
    // A closed log is not a paused one, and the item is gone with it.
    CHECK(!session->logPaused());
    CHECK(!pause->isEnabled());
}

void render_dialogs()
{
    if (g_writeTo.isEmpty()) {
        return;
    }
    Session session(24, 4);
    LogOptionsDialog dialog(&session);
    // Without this the dialog is grabbed before its layout has run and the
    // group boxes overlap in the image and nowhere else.
    dialog.adjustSize();
    dialog.resize(dialog.sizeHint());
    dialog.grab().save(g_writeTo + QStringLiteral("/log-options.png"));
}

} // namespace

int main(int argc, char **argv)
{
    // Before `QApplication`: a `MainWindow` reads the settings the moment it is
    // constructed, and the developer's own `sterna.ini` would decide this
    // test's terminal size and title.
    QStandardPaths::setTestModeEnabled(true);
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("log_test"));
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--write") == 0 && i + 1 < argc) {
            g_writeTo = QString::fromLocal8Bit(argv[++i]);
        }
    }

    test_the_dialog_is_seeded_from_the_settings_and_writes_them_back();
    test_a_choice_disables_what_it_makes_meaningless();
    test_the_field_offers_the_last_directory_and_a_dated_name();
    test_the_menu_item_starts_the_log_the_dialog_configured();
    test_pausing_stops_the_bytes_from_either_place();
    render_dialogs();

    if (failures != 0) {
        fprintf(stderr, "log_test: %d check(s) failed\n", failures);
        return 1;
    }
    printf("log ok\n");
    return 0;
}
