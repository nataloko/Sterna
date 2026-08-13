// The quick buttons, from the settings file to the wire.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/buttons_test
//
// Needs nothing: the connected cases fork `/bin/sh` onto a pty, which is what
// makes this runnable in CI alongside `pty_test`.
//
// What it covers that the core's own tests cannot: the list is read from a
// file the *window* owns, a press goes through a `QAction` on a toolbar, and
// the shortcut check has to reach both Qt's action table and the core's key
// map. None of those exist below the ABI.

#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPlainTextEdit>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "MainWindow.h"
#include "QuickButtonBar.h"
#include "QuickButtons.h"
#include "QuickButtonsDialog.h"
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

QString g_writeTo;

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

/// A settings file with a `[Sterna Buttons]` section in it.
QString writeIni(const QTemporaryDir &dir, const QByteArray &buttons)
{
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        return {};
    }
    file.write("; a comment\r\n[Tera Term]\r\nTerminalSize=60,12\r\n");
    file.write(buttons);
    file.close();
    return path;
}

QuickButtonBar *barOf(const MainWindow &window)
{
    return window.findChild<QuickButtonBar *>(QStringLiteral("quickButtonBar"));
}

QAction *buttonAction(const MainWindow &window, int index)
{
    return window.findChild<QAction *>(
        QStringLiteral("quickButton%1").arg(index));
}

/// Press a button the way the window's own slot does, so the Shift half can be
/// exercised without a modifier the toolkit reads from the live keyboard.
void press(MainWindow &window, int index, bool withoutEnter = false)
{
    QMetaObject::invokeMethod(&window, "runQuickButton", Qt::DirectConnection,
                              Q_ARG(int, index), Q_ARG(bool, withoutEnter));
}

// --- the cases ------------------------------------------------------------

/// The whole path: a button in the file becomes an action on a bar, and
/// pressing it puts its command on the wire.
void a_button_in_the_file_types_into_the_session()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(
        dir,
        "[Sterna Buttons]\r\nButton1Label=Greet\r\nButton1Kind=text\r\n"
        "Button1Value=echo quick-button-ran$0D\r\n"
        "Button2Label=Bare\r\nButton2Value=echo second-one$0D\r\n");

    MainWindow window(ini);
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    CHECK(bar->buttons().size() == 2);
    CHECK(bar->buttons()[0].label == QLatin1String("Greet"));
    CHECK(bar->buttons()[0].text == QLatin1String("echo quick-button-ran\r"));
    CHECK(bar->buttons()[0].sendsEnter());

    // The bar exists because a button does. `window.quick_buttons` is on by
    // default, but an empty list is what really decides.
    window.show();
    CHECK(spin([bar] { return bar->isVisible(); }, 2000));

    // Nothing connected: a sending button has nowhere to go and says so by
    // being grey. A menu command would still be available.
    CHECK(buttonAction(window, 0) != nullptr);
    CHECK(!buttonAction(window, 0)->isEnabled());

    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    buttonAction(window, 0)->trigger();
    CHECK(spin(
        [session] {
            return screenText(*session).contains(
                QLatin1String("echo quick-button-ran"));
        },
        3000));
    // `cat` echoes what it is given, so the CR really left the process.
    CHECK(screenText(*session).contains(QLatin1String("echo quick-button-ran")));

    // ...and a Shift+click sends the same command with the Return left off, so
    // it can be finished by hand.
    press(window, 1, true);
    CHECK(spin(
        [session] {
            return screenText(*session).contains(QLatin1String("echo second-one"));
        },
        3000));
    // The rows below it are blanks rather than empty strings, so this is the
    // last line with anything on it.
    QStringList lines;
    for (const QString &line :
         screenText(*session).split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
        if (!line.trimmed().isEmpty()) {
            lines.append(line.trimmed());
        }
    }
    // The second command sits unterminated at the end: the shell has not been
    // given a Return, which is the whole point of a Shift+click.
    CHECK(!lines.isEmpty());
    CHECK(lines.last() == QLatin1String("echo second-one"));
}

/// A menu-command button does what the menu item does, and needs no link to be
/// worth offering.
void a_command_button_reaches_the_windows_own_actions()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    // 50190 is Disconnect, one of the seventeen `tt_res.h` ids the window
    // implements.
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Hang up\r\n"
                 "Button1Kind=command\r\nButton1Value=50190\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("sleep 30")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));

    press(window, 0);
    CHECK(spin([session] { return !session->isConnected(); }, 3000));
    // And it is enabled with nothing connected, unlike a sending button: Save
    // setup and the settings dialog work perfectly well offline.
    CHECK(buttonAction(window, 0)->isEnabled());
}

/// A macro is allowed to open its own connection, so it remains runnable when
/// this tab has no link yet.
void a_macro_button_is_available_offline()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Connect\r\n"
                 "Button1Kind=macro\r\nButton1Value=connect.ttl\r\n");

    MainWindow window(ini);
    CHECK(!window.session()->isConnected());
    CHECK(buttonAction(window, 0) != nullptr);
    CHECK(buttonAction(window, 0)->isEnabled());
}

/// A text button uses the keyboard's local echo path. Its damage event must be
/// drained by the press itself rather than waiting for the host's next byte.
void a_text_button_repaints_local_echo_immediately()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Type\r\n"
                 "Button1Value=x\r\n");

    MainWindow window(ini);
    Session *session = window.session();
    CHECK(session->setSetting(QStringLiteral("terminal.local_echo"),
                              QStringLiteral("on"), nullptr));
    int repaints = 0;
    QObject::connect(session, &Session::damaged, [&repaints] { repaints++; });

    const QuickButton button = barOf(window)->buttons()[0];
    const KeyCodeAction action = session->runQuickButton(button.kind, button.value);
    CHECK(action.kind == TT_KEY_CODE_SENT);
    CHECK(repaints == 1);
    CHECK(screenText(*session).startsWith(QLatin1Char('x')));
}

/// `Confirm=on` asks, and a dismissed question sends nothing. The box is a
/// modal loop, so the assertion is what happens when it is closed rather than
/// answered — which is what the close box does to it in real use.
void a_button_that_asks_sends_nothing_when_the_question_is_dismissed()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Reload\r\n"
                 "Button1Value=echo dangerous-thing$0D\r\nButton1Confirm=on\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(barOf(window)->buttons()[0].confirm);

    QTimer::singleShot(0, &window, [] {
        if (QWidget *modal = QApplication::activeModalWidget()) {
            modal->close();
        }
    });
    press(window, 0);
    // Give it as long as the send would have taken to arrive.
    spin([] { return false; }, 500);
    CHECK(!screenText(*session).contains(QLatin1String("dangerous-thing")));
}

/// An empty list has no bar at all, whatever the setting says — and defining
/// one brings it back without a restart.
void an_empty_list_has_no_bar()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    CHECK(bar->buttons().isEmpty());
    spin([] { return false; }, 200);
    CHECK(!bar->isVisible());

    QVector<QuickButton> buttons;
    QuickButton made;
    made.label = QStringLiteral("Uptime");
    made.kind = TT_QUICK_BUTTON_TEXT;
    made.text = QStringLiteral("uptime\r");
    buttons.append(made);
    QString error;
    CHECK(saveQuickButtons(ini, buttons, &error));

    // The window rereads on a settings change, which is what the editor's OK
    // ends in.
    QMetaObject::invokeMethod(window.session(), "settingsChanged");
    CHECK(spin([bar] { return bar->isVisible(); }, 2000));
    CHECK(bar->buttons().size() == 1);
    CHECK(bar->buttons()[0].label == QLatin1String("Uptime"));

    // The file kept everything that was not ours.
    QFile file(ini);
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray text = file.readAll();
    CHECK(text.contains("; a comment"));
    CHECK(text.contains("TerminalSize=60,12"));
    CHECK(text.contains("Button1Value=uptime$0D"));
}

/// The editor: a round trip through the widgets and back to the file.
void the_editor_round_trips_a_button()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");

    MainWindow window(ini);
    QuickButtonsDialog dialog(QVector<QuickButton>(), window.session(), &window);
    QuickButton seed;
    seed.kind = TT_QUICK_BUTTON_TEXT;
    seed.text = QStringLiteral("show version\r");
    dialog.appendButton(seed);

    auto *label = dialog.findChild<QLineEdit *>(QStringLiteral("quickButtonLabel"));
    auto *text = dialog.findChild<QPlainTextEdit *>(QStringLiteral("quickButtonText"));
    auto *enter = dialog.findChild<QCheckBox *>(QStringLiteral("quickButtonEnter"));
    auto *confirm = dialog.findChild<QCheckBox *>(QStringLiteral("quickButtonConfirm"));
    auto *list = dialog.findChild<QListWidget *>(QStringLiteral("quickButtonsList"));
    CHECK(label != nullptr && text != nullptr && enter != nullptr);
    CHECK(confirm != nullptr && list != nullptr);
    CHECK(list->count() == 1);

    // The seed arrives with its Return already ticked and out of the box, so
    // the field shows the command rather than the command plus an invisible
    // character.
    CHECK(enter->isChecked());
    CHECK(text->toPlainText() == QLatin1String("show version"));

    label->setText(QStringLiteral("Version"));
    emit label->textEdited(QStringLiteral("Version"));
    confirm->setChecked(true);
    CHECK(list->item(0)->text() == QLatin1String("Version"));

    QVector<QuickButton> edited = dialog.buttons();
    CHECK(edited.size() == 1);
    CHECK(edited[0].label == QLatin1String("Version"));
    CHECK(edited[0].text == QLatin1String("show version\r"));
    CHECK(edited[0].confirm);

    // Untick it and the Return goes; tick it and it comes back, with no second
    // one when the text already ended in it.
    enter->setChecked(false);
    CHECK(dialog.buttons()[0].text == QLatin1String("show version"));
    enter->setChecked(true);
    CHECK(dialog.buttons()[0].text == QLatin1String("show version\r"));

    QString error;
    CHECK(saveQuickButtons(ini, dialog.buttons(), &error));
    const QVector<QuickButton> back = loadQuickButtons(ini);
    CHECK(back.size() == 1);
    CHECK(back[0].label == QLatin1String("Version"));
    CHECK(back[0].value == QLatin1String("show version$0D"));
    CHECK(back[0].text == QLatin1String("show version\r"));
    CHECK(back[0].confirm);
}

/// A command supplied by a plugin or a newer build is not in this window's
/// picker, but editing another field must not silently turn it into Break.
void the_editor_preserves_an_unknown_command()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");

    MainWindow window(ini);
    QuickButton unknown;
    unknown.kind = TT_QUICK_BUTTON_COMMAND;
    unknown.text = QStringLiteral("60000");
    QuickButtonsDialog dialog({unknown}, window.session(), &window);
    auto *command =
        dialog.findChild<QComboBox *>(QStringLiteral("quickButtonCommand"));
    auto *confirm =
        dialog.findChild<QCheckBox *>(QStringLiteral("quickButtonConfirm"));
    CHECK(command != nullptr && confirm != nullptr);
    CHECK(command->currentData().toString() == QLatin1String("60000"));

    confirm->setChecked(true);
    CHECK(dialog.buttons()[0].text == QLatin1String("60000"));
    CHECK(dialog.buttons()[0].confirm);
}

/// The shortcut field, which is where the care is: every warning it can give,
/// and the fact that it is a warning rather than a refusal.
void the_editor_warns_about_a_key_the_host_wants()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");
    // A key map that binds Shift+F1 — scan 59 with the shift bit — which is
    // exactly the sequence YAT uses and this program cannot have.
    const QString cnf = dir.filePath(QStringLiteral("KEYBOARD.CNF"));
    QFile map(cnf);
    CHECK(map.open(QIODevice::WriteOnly));
    map.write("[User keys]\r\nUser1=571,1,hello\r\n");
    map.close();

    MainWindow window(ini);
    window.session()->loadKeyMap(cnf, nullptr, nullptr);

    QVector<QuickButton> existing;
    QuickButton taken;
    taken.label = QStringLiteral("Taken");
    taken.text = QStringLiteral("x\r");
    taken.shortcut = QStringLiteral("Ctrl+Alt+9");
    existing.append(taken);

    QuickButtonsDialog dialog(existing, window.session(), &window);
    QuickButton seed;
    seed.text = QStringLiteral("y\r");
    dialog.appendButton(seed);

    auto *shortcut =
        dialog.findChild<QKeySequenceEdit *>(QStringLiteral("quickButtonShortcut"));
    auto *warning = dialog.findChild<QLabel *>(QStringLiteral("quickButtonWarning"));
    CHECK(shortcut != nullptr && warning != nullptr);

    // Nothing assigned, nothing to say — and nothing assigned is the shipping
    // state of every button.
    CHECK(warning->text().isEmpty());

    // Free: no menu item, no plugin, no key map entry, and a modifier the
    // terminal does not send.
    shortcut->setKeySequence(QKeySequence(QStringLiteral("Ctrl+Alt+2")));
    CHECK(warning->text().isEmpty());

    // Another button's.
    shortcut->setKeySequence(QKeySequence(QStringLiteral("Ctrl+Alt+9")));
    CHECK(warning->text().contains(QLatin1String("Taken")));

    // The window's own menu — Copy is Ctrl+Shift+C.
    shortcut->setKeySequence(QKeySequence(QStringLiteral("Ctrl+Shift+C")));
    CHECK(!warning->text().isEmpty());

    // The key map's, which nothing in Qt can see.
    shortcut->setKeySequence(QKeySequence(QStringLiteral("Shift+F1")));
    CHECK(!warning->text().isEmpty());

    // ...and a key the host plainly wants, whether or not anything binds it.
    shortcut->setKeySequence(QKeySequence(QStringLiteral("F5")));
    CHECK(!warning->text().isEmpty());

    // A warning is not a refusal: it is still assigned.
    CHECK(dialog.buttons()[1].shortcut == QLatin1String("F5"));
}

/// A shortcut in the file becomes a real key on the window, and hiding the bar
/// hands it back to the terminal.
void a_shortcut_is_installed_and_released_with_the_bar()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n"
                 "Button1Shortcut=Ctrl+Alt+1\r\n");

    MainWindow window(ini);
    window.show();
    CHECK(buttonAction(window, 0)->shortcut()
          == QKeySequence(QStringLiteral("Ctrl+Alt+1")));
    CHECK(barOf(window)->isVisible());

    // Setup > Show quick buttons writes the setting; the bar follows it, and
    // an action on a hidden toolbar no longer answers its shortcut.
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.quick_buttons"),
                                       QStringLiteral("off"), &error));
    CHECK(spin([&window] { return !barOf(window)->isVisible(); }, 2000));
}

/// Where the bar opens: down the right unless the file says otherwise, and
/// wherever it was dragged to for as long as the window is open.
void the_bar_opens_down_the_right_and_stays_where_it_is_put()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString shipped =
        writeIni(dir, "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");

    {
        // Nothing in the file about the bar, so this is the shipped answer: a
        // terminal's rows are the scarce dimension and a vertical bar costs
        // none of them.
        MainWindow window(shipped);
        window.show();
        CHECK(window.toolBarArea(barOf(window)) == Qt::RightToolBarArea);
    }

    QTemporaryDir other;
    CHECK(other.isValid());
    const QString ini =
        writeIni(other,
                 "[Sterna]\r\nQuickButtonsArea=left\r\n"
                 "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");

    MainWindow window(ini);
    window.show();
    CHECK(window.toolBarArea(barOf(window)) == Qt::LeftToolBarArea);

    // A drag is the user placing it. Editing the list rebuilds the bar, and
    // that must not put it back where the file — which is not written until
    // the window closes — still says it was.
    window.addToolBar(Qt::BottomToolBarArea, barOf(window));
    CHECK(window.toolBarArea(barOf(window)) == Qt::BottomToolBarArea);

    QVector<QuickButton> buttons = barOf(window)->buttons();
    QuickButton added;
    added.label = QStringLiteral("Second");
    added.text = QStringLiteral("uptime\r");
    buttons.append(added);
    QString error;
    CHECK(saveQuickButtons(ini, buttons, &error));
    QMetaObject::invokeMethod(window.session(), "settingsChanged");
    CHECK(spin([&window] { return barOf(window)->buttons().size() == 2; }, 2000));
    CHECK(window.toolBarArea(barOf(window)) == Qt::BottomToolBarArea);

    // ...and a setting that really changes still moves it.
    CHECK(window.session()->setSetting(QStringLiteral("window.quick_buttons_area"),
                                       QStringLiteral("top"), &error));
    CHECK(spin(
        [&window] {
            return window.toolBarArea(barOf(window)) == Qt::TopToolBarArea;
        },
        2000));
}

/// Add — the `+` at the end of the bar, and Add in its context menu — opens
/// the editor on a *new* row rather than on whichever button was first.
void adding_starts_on_a_new_row()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=One\r\nButton1Value=a$0D\r\n"
                 "Button2Label=Two\r\nButton2Value=b$0D\r\n");

    MainWindow window(ini);
    QuickButtonsDialog dialog(barOf(window)->buttons(), window.session(), &window);
    auto *list = dialog.findChild<QListWidget *>(QStringLiteral("quickButtonsList"));
    auto *label = dialog.findChild<QLineEdit *>(QStringLiteral("quickButtonLabel"));
    CHECK(list != nullptr && label != nullptr);
    CHECK(list->currentRow() == 0);

    dialog.appendButton(QuickButton());
    CHECK(list->count() == 3);
    CHECK(list->currentRow() == 2);
    CHECK(label->text().isEmpty());
    // The existing two are untouched by the arrival of a third.
    CHECK(dialog.buttons().size() == 3);
    CHECK(dialog.buttons()[0].label == QLatin1String("One"));
    CHECK(dialog.buttons()[1].label == QLatin1String("Two"));
}

/// `--write <dir>`: the bar and its editor as PNGs, for a human to look at.
///
/// `QWidget::grab()` re-renders offscreen, which is the only screenshot that
/// works in this container anyway — and it is what makes a review of a toolbar
/// possible without somebody sitting in front of it.
void render_widgets()
{
    if (g_writeTo.isEmpty()) {
        return;
    }
    QTemporaryDir dir;
    const QString ini = writeIni(
        dir,
        "[Sterna Buttons]\r\n"
        "Button1Label=Show version\r\nButton1Value=show version$0D\r\n"
        "Button1Shortcut=Ctrl+Alt+1\r\n"
        "Button2Label=Interfaces\r\nButton2Value=show ip interface brief$0D\r\n"
        "Button2Shortcut=Ctrl+Alt+2\r\n"
        "Button3Label=Save config\r\nButton3Value=write memory$0D\r\n"
        "Button4Label=Reload\r\nButton4Value=reload$0D\r\nButton4Confirm=on\r\n"
        "Button5Label=Break\r\nButton5Kind=command\r\nButton5Value=50430\r\n");

    MainWindow window(ini);
    window.resize(760, 400);
    window.show();
    spin([] { return false; }, 300);
    window.grab().save(g_writeTo + QStringLiteral("/quick-buttons-window.png"));

    QuickButtonsDialog dialog(loadQuickButtons(ini), window.session(), &window);
    dialog.selectRow(3);
    // Without this the dialog is grabbed before layout and the wrapped warning
    // overlaps the fields in the image and nowhere else.
    dialog.adjustSize();
    dialog.grab().save(g_writeTo + QStringLiteral("/quick-buttons-editor.png"));
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("buttons_test"));
    for (int i = 1; i < argc; i++) {
        if (QLatin1String(argv[i]) == QLatin1String("--write") && i + 1 < argc) {
            g_writeTo = QString::fromLocal8Bit(argv[++i]);
        }
    }

    a_button_in_the_file_types_into_the_session();
    a_command_button_reaches_the_windows_own_actions();
    a_macro_button_is_available_offline();
    a_text_button_repaints_local_echo_immediately();
    a_button_that_asks_sends_nothing_when_the_question_is_dismissed();
    an_empty_list_has_no_bar();
    the_editor_round_trips_a_button();
    the_editor_preserves_an_unknown_command();
    the_editor_warns_about_a_key_the_host_wants();
    a_shortcut_is_installed_and_released_with_the_bar();
    the_bar_opens_down_the_right_and_stays_where_it_is_put();
    adding_starts_on_a_new_row();
    render_widgets();

    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("buttons ok");
    return 0;
}
