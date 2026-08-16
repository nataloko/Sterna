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
#include <QDoubleSpinBox>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QKeyEvent>
#include <QPlainTextEdit>
#include <QSpinBox>
#include <QTemporaryDir>
#include <QTimer>
#include <QToolButton>

#include <cstdio>

#include <QScreen>

#include "MainWindow.h"
#include "QuickButtonBar.h"
#include "QuickButtons.h"
#include <QMenu>

#include "QuickButtonsDialog.h"
#include "SettingsDialog.h"
#include "Session.h"
#include "TerminalPage.h"
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

/// How many times `marker` is on the screen.
///
/// The repeating cases run `cat > /dev/null`, so the only copy of a send that
/// comes back is the line discipline's echo — one per send, which makes this
/// a count of what actually left.
int markerCount(const Session &session, const char *marker)
{
    return static_cast<int>(screenText(session).count(QLatin1String(marker)));
}

QuickButtonBar *barOf(const MainWindow &window)
{
    return window.findChild<QuickButtonBar *>(QStringLiteral("quickButtonBar"));
}

/// Ask for a panel width the way Setup's Window page does, and wait for the
/// panel to take it.
///
/// There is no handle to drag: `window.quick_buttons_width` *is* the gesture,
/// so a test that drove synthetic mouse events would be exercising a widget
/// this program does not have. The property is the same either way — the
/// pixels come out of the window and never out of the terminal.
bool setPanelWidth(MainWindow &window, int px)
{
    QString error;
    if (!window.session()->setSetting(
            QStringLiteral("window.quick_buttons_width"),
            QString::number(px), &error)) {
        return false;
    }
    QuickButtonBar *bar = barOf(window);
    return spin([bar, px] { return bar && bar->width() == px; }, 2000);
}

/// Put the window where a widening panel has at least `room` pixels to grow
/// into, and say whether it landed there.
///
/// **Every number computed from the work area, never a literal.** The
/// offscreen plugin's screen is 800x800 and is not the desktop's, so a window
/// opened at a comfortable-looking 900 is already past the right-hand edge —
/// `windowGrowthRoom` then answers 0, the width is correctly clamped to
/// nothing, and the checks fail for a reason that has nothing to do with what
/// they are testing. A window that genuinely has no room is its own case, in
/// `a_width_stops_at_the_edge_of_the_screen`.
bool placeForGrowth(MainWindow &window, int room)
{
    const QScreen *display = window.screen();
    if (!display) {
        return false;
    }
    const QRect work = display->availableGeometry();
    window.move(work.topLeft());
    window.resize(qMax(360, work.width() - room - 40), qMin(500, work.height()));
    spin([] { return false; }, 100);
    return work.right() - window.frameGeometry().right() >= room;
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

/// An unrelated settings change does not rebuild the panel.
///
/// `reloadQuickButtons` runs on every `settingsChanged`, and a rebuild throws
/// away every button widget and makes new ones — which drops the panel's size
/// hint to its empty width and brings it back, so the dock takes those pixels
/// off the central widget and returns them. The terminal beside it is fitted
/// to that width in whole cells, so a few pixels either way is a column, and
/// with `ClearOnResize` on each such resize scrolls the page into history.
/// Toggling line edit blanked the screen this way; CI caught it because a
/// runner with one font makes the cell wide enough for the window to be short
/// of a column in the first place.
///
/// The `QAction` identity is the assertion because it is what a rebuild
/// destroys, and it is what the shortcut and the repeat state hang off.
void an_unrelated_setting_leaves_the_buttons_alone()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir,
                                 "[Sterna Buttons]\r\nButton1Label=Greet\r\n"
                                 "Button1Value=echo hello$0D\r\n");

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    CHECK(spin([bar] { return bar->isVisible(); }, 2000));

    QAction *before = buttonAction(window, 0);
    CHECK(before != nullptr);
    const int cols = window.session()->cols();
    const int rows = window.session()->rows();

    CHECK(window.session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), nullptr));
    qApp->processEvents();

    CHECK(buttonAction(window, 0) == before);
    CHECK(bar->buttons().size() == 1);
    // And the terminal it shares the window with kept its size.
    CHECK(window.session()->cols() == cols);
    CHECK(window.session()->rows() == rows);

    // A change to the list itself still rebuilds, or nothing would ever
    // appear — the guard is about equality, not about the path being dead.
    QVector<QuickButton> edited = bar->buttons();
    edited[0].label = QStringLiteral("Renamed");
    bar->setButtons(QuickButtonSet {edited, {}});
    CHECK(buttonAction(window, 0) != before);
    CHECK(bar->buttons()[0].label == QLatin1String("Renamed"));
}

/// An empty list keeps the checked panel and its Add button visible, and
/// defining the first command fills that same panel without a restart.
void an_empty_list_keeps_the_add_button()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    CHECK(bar->buttons().isEmpty());
    CHECK(spin([bar] { return bar->isVisible(); }, 2000));
    QAction *add = window.findChild<QAction *>(QStringLiteral("quickButtonAdd"));
    CHECK(add != nullptr);
    CHECK(add && add->isVisible());

    QVector<QuickButton> buttons;
    QuickButton made;
    made.label = QStringLiteral("Uptime");
    made.kind = TT_QUICK_BUTTON_TEXT;
    made.text = QStringLiteral("uptime\r");
    buttons.append(made);
    QString error;
    CHECK(saveQuickButtons(ini, QuickButtonSet {buttons, {}}, &error));

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
    QuickButtonsDialog dialog(QuickButtonSet(), 1, window.session(), &window);
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
    CHECK(saveQuickButtons(ini, dialog.set(), &error));
    const QVector<QuickButton> back = loadQuickButtons(ini).buttons;
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
    QuickButtonsDialog dialog(QuickButtonSet {{unknown}, {}}, 1, window.session(),
                              &window);
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

    QuickButtonsDialog dialog(QuickButtonSet {existing, {}}, 1, window.session(),
                              &window);
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

/// The panel opens down the right, and a configured width widens it — taking
/// the pixels from the window, so the buttons grow and the terminal does not.
void the_panel_opens_down_the_right_and_a_width_widens_it()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    // Two captions of very different lengths, because the property below is
    // that both buttons come out the same width.
    const QString shipped = writeIni(
        dir,
        "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n"
        "Button2Label=Show the running configuration\r\n"
        "Button2Value=show run$0D\r\n");

    MainWindow window(shipped);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    if (!placeForGrowth(window, 200)) {
        return;
    }

    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    CHECK(!bar->isHidden());
    // Down the right: the panel starts after the terminals end. Asked of the
    // geometry rather than of a dock area, because there is no longer an enum
    // to ask.
    auto *panels = window.findChild<PanelContainer *>();
    CHECK(panels != nullptr);
    if (panels) {
        CHECK(bar->x() >= panels->x() + panels->width());
    }

    const int before = bar->width();
    CHECK(setPanelWidth(window, before + 100));
    CHECK(bar->width() == before + 100);

    // ...and the buttons take that width with them. A button is as wide as the
    // panel and not as wide as its own caption, so a short one and a long one
    // measure the same and both follow the setting — the extra room goes into
    // the buttons rather than into the margin beside them.
    QToolButton *first = bar->buttonWidget(0);
    QToolButton *second = bar->buttonWidget(1);
    CHECK(first != nullptr && second != nullptr);
    if (first && second) {
        CHECK(spin([first, bar] { return first->width() > bar->width() / 2; },
                   2000));
        CHECK(first->width() == second->width());
        CHECK(first->width() > bar->width() - 24);
    }
}

/// **The bug this whole arrangement exists for.**
///
/// Widening the panel used to take its pixels out of the terminal beside it,
/// and `Grid::resize` truncates every line it shortens — in the page and in
/// the scrollback, and it does not give them back on the way out. So the check
/// that matters is not that the column count came back: it is that the text
/// did. A shrink that truncated and then re-widened passes a column count and
/// fails this.
void widening_the_panel_grows_the_window_and_not_the_grid()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        // `ClearOnResize` on, which is the amplifier: with it, a resize that
        // moved by one column also scrolls the whole page into history. Off is
        // the shipped value and would hide half of what this is watching for.
        file.write("[Tera Term]\r\nTerminalSize=60,12\r\nClearOnResize=on\r\n"
                   "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");
    }

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    if (!placeForGrowth(window, 260)) {
        return;
    }

    Session *session = window.session();
    // A line that reaches the right-hand edge, so anything that narrows the
    // grid cuts a character off the end of it rather than off trailing blanks.
    const QString line(session->cols(), QLatin1Char('x'));
    session->feed(line.toUtf8());
    QCoreApplication::processEvents();

    const int cols = session->cols();
    const int rows = session->rows();
    const int history = session->scrollbackLen();
    const int windowWidth = window.width();
    const QString before = screenText(*session);
    CHECK(cols > 0 && before.contains(line));

    const int wide = barOf(window)->width() + 140;
    CHECK(setPanelWidth(window, wide));
    CHECK(spin([&window, windowWidth] { return window.width() != windowWidth; },
               2000));
    // The window absorbed it, exactly.
    CHECK(window.width() == windowWidth + 140);
    CHECK(session->cols() == cols);
    CHECK(session->rows() == rows);
    CHECK(session->scrollbackLen() == history);
    CHECK(screenText(*session) == before);

    // ...and back, which is the half a truncating resize cannot survive: the
    // text is gone by now if anything shortened a line on the way out.
    CHECK(setPanelWidth(window, wide - 140));
    CHECK(spin([&window, windowWidth] { return window.width() == windowWidth; },
               2000));
    CHECK(session->cols() == cols);
    CHECK(session->scrollbackLen() == history);
    CHECK(screenText(*session).contains(line));
}

/// A width stops at the edge of the screen rather than taking the columns.
///
/// The window cannot grow past its work area, and the whole rule is that the
/// terminal never pays for the panel — so the honest answer there is a width
/// that is not reached, not one that quietly falls back to the old behaviour.
void a_width_stops_at_the_edge_of_the_screen()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = writeIni(
        dir, "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));

    const QScreen *display = window.screen();
    CHECK(display != nullptr);
    if (!display) {
        return;
    }
    // Computed, never a literal: the offscreen plugin's screen is 800x800 and
    // is not the desktop's.
    const QRect work = display->availableGeometry();
    window.resize(work.width(), qMin(500, work.height()));
    window.move(work.topLeft());
    CHECK(spin([&window, &work] { return window.frameGeometry().right() >= work.right() - 2; },
               2000));
    if (window.frameGeometry().right() < work.right() - 2) {
        // A compositor that would not put the window there — Wayland ignores
        // `move()` outright. Nothing to measure, and saying so beats asserting
        // against a window somewhere else.
        return;
    }

    QuickButtonBar *bar = barOf(window);
    Session *session = window.session();
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    const int width = bar->width();
    const int cols = session->cols();
    QString error;
    CHECK(window.session()->setSetting(
        QStringLiteral("window.quick_buttons_width"),
        QString::number(width + 200), &error));
    // Not `setPanelWidth`, which waits for the panel to reach the number: the
    // whole point here is that it does not, and cannot.
    spin([] { return false; }, 200);
    CHECK(bar->width() == width);
    CHECK(session->cols() == cols);
}

/// Showing the panel is a resize too, and it is the second route to the same
/// lost text — one the old absorb arm never covered, because that one is gated
/// on a single page in the untiled layout.
void showing_the_panel_leaves_every_terminal_alone()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        file.write("[Tera Term]\r\nTerminalSize=60,12\r\nClearOnResize=on\r\n"
                   "[Sterna]\r\nQuickButtons=off\r\nPanelLayout=tiled\r\n"
                   "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");
    }

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    if (!placeForGrowth(window, 260)) {
        return;
    }
    QMetaObject::invokeMethod(&window, "newTab", Qt::DirectConnection);
    // Through the container rather than `findChildren<TerminalPage *>`, which
    // does not compile: `TerminalPage` carries no `Q_OBJECT`.
    auto *panels = window.findChild<PanelContainer *>();
    CHECK(panels != nullptr);
    if (!panels) {
        return;
    }
    CHECK(spin([panels] { return panels->count() > 1; }, 2000));

    // Every tile, not just the front one. The old absorb arm was gated on one
    // page in the untiled layout, so this is exactly the shape it could not
    // see: two terminals side by side, both of them beside the one panel.
    const auto columns = [panels] {
        QVector<int> out;
        for (QWidget *widget : panels->visiblePages()) {
            out.append(static_cast<TerminalPage *>(widget)->session()->cols());
        }
        return out;
    };
    const QVector<int> before = columns();
    CHECK(before.size() > 1);

    // A width arriving while the panel is switched off must move nothing. The
    // window's visibility and the panel's come apart here, and taking the
    // window's would widen it to make room for something nobody can see.
    QString error;
    const int idle = window.width();
    CHECK(window.session()->setSetting(
        QStringLiteral("window.quick_buttons_width"), QStringLiteral("220"),
        &error));
    CHECK(spin([&window, idle] { return window.width() != idle; }, 300) == false);
    CHECK(columns() == before);

    CHECK(window.session()->setSetting(QStringLiteral("window.quick_buttons"),
                                       QStringLiteral("on"), &error));
    CHECK(spin([&window] { return !barOf(window)->isHidden(); }, 2000));
    CHECK(columns() == before);

    // ...and off again, which gives the room back rather than handing it to
    // the terminals and then taking it away on the next show.
    CHECK(window.session()->setSetting(QStringLiteral("window.quick_buttons"),
                                       QStringLiteral("off"), &error));
    CHECK(spin([&window] { return barOf(window)->isHidden(); }, 2000));
    CHECK(columns() == before);
}

/// The width is on the panel's own context menu, ticked to say which mode it
/// is in — which is the route somebody actually finds.
void the_panel_menu_offers_the_width()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = writeIni(
        dir, "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    const int natural = bar->width();

    // -1 is a click on the panel rather than on a button, which is the case
    // that offers nothing else — so the width has to be there, or a panel with
    // one button on it has a menu with only Add in it.
    QMenu *menu = bar->buildContextMenu(-1);
    CHECK(menu != nullptr);
    if (!menu) {
        return;
    }
    CHECK(menu->findChild<QMenu *>(QStringLiteral("quickMenuWidth")) != nullptr);
    QAction *fit = menu->findChild<QAction *>(QStringLiteral("quickMenuFit"));
    QAction *set = menu->findChild<QAction *>(QStringLiteral("quickMenuSetWidth"));
    CHECK(fit != nullptr && set != nullptr);
    // Shipped state is fitting, and the tick says so.
    CHECK(fit && fit->isCheckable() && fit->isChecked());
    delete menu;

    // ...and it stops saying so once a width has been chosen, or the menu is
    // lying about which of its two modes the panel is in.
    QString error;
    CHECK(window.session()->setSetting(
        QStringLiteral("window.quick_buttons_width"), QStringLiteral("150"),
        &error));
    CHECK(spin([bar] { return bar->width() == 150; }, 2000));
    menu = bar->buildContextMenu(-1);
    fit = menu->findChild<QAction *>(QStringLiteral("quickMenuFit"));
    CHECK(fit != nullptr && !fit->isChecked());

    // **And the item is wired, not merely present.** Everything above asks
    // what the menu says; nothing asked whether pressing it does anything, and
    // this menu is the route somebody finds — the settings page is the other
    // end of the same key and would go on working with the connection cut. So
    // trigger the action itself rather than the signal behind it, which is
    // what `a_narrow_panel_shortens_its_captions` emits by hand because it is
    // testing the width and not the wiring.
    if (fit) {
        fit->trigger();
        CHECK(spin([bar, natural] { return bar->width() == natural; }, 2000));
        CHECK(window.session()->setting(
                  QStringLiteral("window.quick_buttons_width"))
              == QLatin1String("0"));
    }
    delete menu;
}

/// A panel can be made narrower than its own captions: the buttons shorten
/// their text rather than holding the panel open.
///
/// And there is still a floor, because a panel two pixels wide is a panel
/// nobody can hit. The floor is a fixed number rather than the widest caption
/// — which is the whole point, since the widest caption is exactly what used
/// to decide this.
void a_narrow_panel_shortens_its_captions()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    // One caption far too long for the panel it is about to be given.
    const QString path = writeIni(
        dir,
        "[Sterna Buttons]\r\nButton1Label=Show the running configuration\r\n"
        "Button1Value=show run$0D\r\n");

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    if (!placeForGrowth(window, 200)) {
        return;
    }

    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    const int natural = bar->width();
    QToolButton *button = bar->buttonWidget(0);
    CHECK(button != nullptr);
    if (!button) {
        return;
    }
    const QString caption = button->text();
    CHECK(caption.startsWith(QLatin1String("Show the running")));
    // The caption really is wider than the panel is about to be, or this
    // proves nothing.
    CHECK(button->fontMetrics().horizontalAdvance(caption) > 90);

    CHECK(setPanelWidth(window, 90));
    CHECK(bar->width() == 90);
    // The button went with it rather than overflowing the panel it is in.
    CHECK(button->width() <= 90);
    CHECK(button->width() > 0);
    // **Elision is paint-only.** The full caption is still what the button
    // says, so the tooltip, the editor and this test get the real answer —
    // shortening it in `text()` would put an ellipsis into the settings file
    // the moment somebody opened the editor on it.
    CHECK(button->text() == caption);

    // ...and the floor holds. 10 is below it, so the panel stops at 48 rather
    // than becoming something nobody can hit.
    QString error;
    CHECK(window.session()->setSetting(
        QStringLiteral("window.quick_buttons_width"), QStringLiteral("10"),
        &error));
    CHECK(spin([bar] { return bar->width() == 48; }, 2000));

    // Back to fitting, through the signal the context menu's Fit to buttons
    // emits rather than through the setting it ends at — the menu item is the
    // reachable route and the wiring behind it is what could rot. Emitting the
    // signal because `showContextMenu` execs a modal menu, which a test cannot
    // click.
    CHECK(QMetaObject::invokeMethod(bar, "fitWidthRequested"));
    CHECK(spin([bar, natural] { return bar->width() == natural; }, 2000));
    // ...and it wrote the sentinel rather than the number it measured, or the
    // panel would stop following its captions from here on.
    CHECK(window.session()->setting(
              QStringLiteral("window.quick_buttons_width")) == QLatin1String("0"));
}

/// The width is reachable from Setup, which is the **only** way to set one now
/// that there is no handle.
///
/// The control is generated from the schema rather than written by hand, so
/// this is really asking that `int_clamp` still crosses the ABI as something
/// the dialog knows how to draw. A schema kind it did not recognise would
/// leave the row out of the page and the width unreachable — with nothing
/// failing anywhere else, because every other test sets it through the session.
void the_width_has_a_control_in_setup()
{
    Session session(80, 24);
    SettingsDialog dialog(&session);
    dialog.adjustSize();
    QSpinBox *box = dialog.findChild<QSpinBox *>(
        QStringLiteral("settingEditor:window.quick_buttons_width"));
    CHECK(box != nullptr);
    if (!box) {
        return;
    }
    CHECK(box->minimum() == 0);
    CHECK(box->maximum() == 2000);
    // Zero is the shipped sentinel and has to survive the round trip through a
    // spin box, or a visit to the Window page pins every panel to a number.
    CHECK(box->value() == 0);
}

/// A configured width is opened at, and changing it live costs the terminal
/// nothing — which is the whole of the feature now that there is no handle.
///
/// The *writing* half is the settings dialog's own save path and is not this
/// feature's to test. What is this feature's: reading the number, applying it
/// through the helper that takes the pixels from the window, and leaving the
/// shipped zero alone so a window nobody has sized goes on measuring its
/// buttons.
void the_configured_panel_width_is_opened_at()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        file.write("[Tera Term]\r\nTerminalSize=60,12\r\n"
                   "[Sterna]\r\nQuickButtonsWidth=240\r\n"
                   "[Sterna Buttons]\r\nButton1Label=Hi\r\nButton1Value=hi$0D\r\n");
    }

    MainWindow window(path);
    window.show();
    CHECK(spin([&window] { return window.isVisible(); }, 2000));
    CHECK(spin([&window] { return barOf(window)->width() == 240; }, 2000));

    // ...and a live change goes through the same helper, so it costs the
    // terminal nothing either. This is the Setup route, which is the only
    // route — and the one that still works on a maximised window.
    if (!placeForGrowth(window, 200)) {
        return;
    }
    const int cols = window.session()->cols();
    const int windowWidth = window.width();
    CHECK(setPanelWidth(window, 300));
    CHECK(window.session()->cols() == cols);
    CHECK(spin([&window, windowWidth] { return window.width() == windowWidth + 60; },
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
    QuickButtonsDialog dialog(barOf(window)->set(), 1, window.session(), &window);
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

// --- pages -----------------------------------------------------------------

/// The two-page fixture the cases below share: one command per page, a name on
/// the second, and a shortcut on the button that is *not* on the first.
QByteArray twoPages()
{
    return "[Sterna Buttons]\r\nPage2Name=BMCs\r\n"
           "Button1Label=Version\r\nButton1Value=echo page-one$0D\r\n"
           "Button2Label=Power\r\nButton2Page=2\r\n"
           "Button2Value=echo page-two$0D\r\nButton2Shortcut=Ctrl+Alt+2\r\n";
}

QComboBox *pageBoxOf(const MainWindow &window)
{
    return window.findChild<QComboBox *>(QStringLiteral("quickButtonPageBox"));
}

/// The selector is chrome, so it arrives with the second page and not before.
void a_second_page_puts_a_selector_on_the_panel()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString one = writeIni(
        dir, "[Sterna Buttons]\r\nButton1Label=Only\r\nButton1Value=a$0D\r\n");
    {
        MainWindow window(one);
        window.show();
        QuickButtonBar *bar = barOf(window);
        CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
        CHECK(bar->pageCount() == 1);
        // One page: exactly the panel this was before pages existed.
        CHECK(pageBoxOf(window) == nullptr);
    }

    QTemporaryDir second;
    CHECK(second.isValid());
    const QString two = writeIni(second, twoPages());
    MainWindow window(two);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    CHECK(bar->pageCount() == 2);
    QComboBox *box = pageBoxOf(window);
    CHECK(box != nullptr);
    CHECK(box->count() == 2);
    // A page with no name is called by its number; a named one by its name.
    CHECK(box->itemText(0) == QLatin1String("Page 1"));
    CHECK(box->itemText(1) == QLatin1String("BMCs"));
    CHECK(box->currentIndex() == 0);
}

/// A page filters the widgets and nothing else. The list, the indices and the
/// object names every other part of this window speaks are the whole list's.
void the_panel_shows_only_the_current_page()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));

    CHECK(bar->buttons().size() == 2);
    CHECK(bar->page() == 1);
    CHECK(bar->buttonWidget(0) != nullptr);
    CHECK(bar->buttonWidget(1) == nullptr);

    // The actions are the flat list's, so the one for page 2's button exists
    // while page 1 is showing — which is what keeps its shortcut installed.
    QAction *offPage = buttonAction(window, 1);
    CHECK(offPage != nullptr);
    CHECK(offPage->text() == QLatin1String("Power"));

    bar->setPage(2);
    CHECK(bar->page() == 2);
    CHECK(bar->buttonWidget(0) == nullptr);
    CHECK(bar->buttonWidget(1) != nullptr);
    // ...and the actions were not destroyed and rebuilt on the way.
    CHECK(buttonAction(window, 1) == offPage);
    CHECK(bar->buttons().size() == 2);

    // **One `+`, however many pages have been through.** Its widget is not in
    // the per-button vector, so a rebuild that deleted only its action left the
    // old button in the layout still reading `+` — a panel that grew one more
    // every time somebody changed page. Nothing but the picture showed it.
    const auto plusCount = [bar] {
        int found = 0;
        for (QToolButton *widget : bar->findChildren<QToolButton *>()) {
            if (widget->text() == QLatin1String("+")) {
                found++;
            }
        }
        return found;
    };
    CHECK(plusCount() == 1);
    bar->setPage(1);
    bar->setPage(2);
    bar->setPage(1);
    CHECK(plusCount() == 1);
}

/// A shortcut is a key the host stops receiving. It must not come and go with
/// a drop-down nobody looked at, so it fires from any page.
void a_shortcut_fires_from_another_page()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    CHECK(bar->page() == 1);

    QAction *offPage = buttonAction(window, 1);
    CHECK(offPage != nullptr);
    // Installed, while its button is not drawn.
    CHECK(offPage->shortcut()
          == QKeySequence::fromString(QStringLiteral("Ctrl+Alt+2"),
                                      QKeySequence::PortableText));
    CHECK(offPage->shortcutContext() == Qt::WindowShortcut);

    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([offPage] { return offPage->isEnabled(); }, 2000));

    offPage->trigger();
    CHECK(spin([session]
               { return screenText(*session).contains(QLatin1String("page-two")); },
               3000));

    // And hiding the whole panel still hands every key back — an action on a
    // hidden widget answers no shortcut, and page 2's hangs off the bar rather
    // than off a button, so it goes with the rest.
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.quick_buttons"),
                                       QStringLiteral("off"), &error));
    CHECK(spin([bar] { return !bar->isVisible(); }, 2000));
    CHECK(!bar->isVisible());
}

/// The index invariant, and the case that fails loudly if a page is ever
/// allowed to renumber: a run belongs to a button, not to what is on screen.
void switching_pages_leaves_a_repeat_running()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nPage2Name=BMCs\r\n"
                 "Button1Label=Keepalive\r\nButton1Value=keepalive$0D\r\n"
                 "Button1Repeat=forever\r\nButton1IntervalMs=100\r\n"
                 "Button2Label=Power\r\nButton2Page=2\r\nButton2Value=power$0D\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr);
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    press(window, 0);
    CHECK(spin([session] { return markerCount(*session, "keepalive") >= 2; },
               3000));

    bar->setPage(2);
    CHECK(bar->page() == 2);
    // Still going, and still going in a way that reaches the wire.
    CHECK(buttonAction(window, 0)->isChecked());
    const int settled = markerCount(*session, "keepalive");
    CHECK(spin([session, settled]
               { return markerCount(*session, "keepalive") > settled; },
               3000));

    // Back, and the button is where it was, still marked as running.
    bar->setPage(1);
    CHECK(bar->buttonWidget(0) != nullptr);
    CHECK(buttonAction(window, 0)->isChecked());
    CHECK(buttonAction(window, 0)->text()
          == QString::fromUtf8("Keepalive ⟳"));
    press(window, 0);
    CHECK(!buttonAction(window, 0)->isChecked());
}

/// The page the panel was left on is where it opens.
void the_remembered_page_is_opened_at()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    {
        MainWindow window(ini);
        window.show();
        QuickButtonBar *bar = barOf(window);
        CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
        QComboBox *box = pageBoxOf(window);
        CHECK(box != nullptr);
        // Through the drop-down, which is the gesture — `setPage` alone is the
        // programmatic half and deliberately writes nothing down.
        box->setCurrentIndex(1);
        CHECK(spin([bar] { return bar->page() == 2; }, 2000));
    }

    QFile file(ini);
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray saved = file.readAll();
    CHECK(saved.contains("QuickButtonsPage=2"));

    MainWindow again(ini);
    again.show();
    QuickButtonBar *bar = barOf(again);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    CHECK(bar->page() == 2);
    CHECK(bar->buttonWidget(1) != nullptr);
    CHECK(bar->buttonWidget(0) == nullptr);
}

/// A page that has stopped existing is not one to open on, and the answer is
/// written back rather than quietly differing from the file.
void a_page_that_stops_existing_moves_the_setting_down()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(
        dir,
        "[Sterna]\r\nQuickButtonsPage=2\r\n"
        "[Sterna Buttons]\r\nButton1Label=Version\r\nButton1Value=a$0D\r\n"
        "Button2Label=Power\r\nButton2Page=2\r\nButton2Value=b$0D\r\n");

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    CHECK(bar->page() == 2);

    // Delete page 2's only button — and its name with it, since it had none.
    // Written to the file and then let in through a settings change, which is
    // the path the editor's OK takes.
    QuickButtonSet set = bar->set();
    set.buttons.remove(1);
    QString error;
    CHECK(saveQuickButtons(ini, set, &error));
    CHECK(window.session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), &error));
    CHECK(spin([bar] { return bar->pageCount() == 1; }, 2000));
    CHECK(bar->page() == 1);

    QFile file(ini);
    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll().contains("QuickButtonsPage=1"));
}

/// The panel's own menu: which page, and where a button goes.
void the_panel_menu_offers_the_pages()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));

    QMenu *menu = bar->buildContextMenu(0);
    CHECK(menu != nullptr);
    QMenu *pages = menu->findChild<QMenu *>(QStringLiteral("quickMenuPage"));
    CHECK(pages != nullptr);
    auto *first = pages->findChild<QAction *>(QStringLiteral("quickMenuPage1"));
    auto *second = pages->findChild<QAction *>(QStringLiteral("quickMenuPage2"));
    CHECK(first != nullptr && second != nullptr);
    CHECK(first->isChecked() && !second->isChecked());
    CHECK(second->text() == QLatin1String("BMCs"));

    // Move to page offers every page but the one the button is already on.
    QMenu *move = menu->findChild<QMenu *>(QStringLiteral("quickMenuMoveToPage"));
    CHECK(move != nullptr);
    CHECK(move->findChild<QAction *>(QStringLiteral("quickMenuMoveToPage1"))
          == nullptr);
    auto *to2 = move->findChild<QAction *>(QStringLiteral("quickMenuMoveToPage2"));
    CHECK(to2 != nullptr);

    second->trigger();
    CHECK(spin([bar] { return bar->page() == 2; }, 2000));
    delete menu;

    // ...and the move writes the file and rebuilds the panel around it.
    menu = bar->buildContextMenu(1);
    move = menu->findChild<QMenu *>(QStringLiteral("quickMenuMoveToPage"));
    CHECK(move != nullptr);
    auto *back = move->findChild<QAction *>(QStringLiteral("quickMenuMoveToPage1"));
    CHECK(back != nullptr);
    back->trigger();
    CHECK(spin([bar] { return bar->buttons()[1].page == 1; }, 2000));
    delete menu;
}

/// The editor moves a button between pages, and its list is one page's.
void the_editor_moves_a_button_between_pages()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    MainWindow window(ini);
    QuickButtonsDialog dialog(barOf(window)->set(), 1, window.session(), &window);
    auto *list = dialog.findChild<QListWidget *>(QStringLiteral("quickButtonsList"));
    auto *pageOf = dialog.findChild<QComboBox *>(QStringLiteral("quickButtonPage"));
    auto *pageList =
        dialog.findChild<QComboBox *>(QStringLiteral("quickButtonsPageList"));
    CHECK(list != nullptr && pageOf != nullptr && pageList != nullptr);

    // One page's buttons, not the whole list.
    CHECK(list->count() == 1);
    CHECK(list->item(0)->text() == QLatin1String("Version"));
    CHECK(pageList->count() == 2);
    // The field shows the page the button is really on, which is a lookup that
    // fails silently if the stored data and the button's number are compared
    // as different types.
    CHECK(pageOf->currentData().toInt() == 1);

    // Move the visible one to page 2. The editor follows it there rather than
    // leaving the fields showing a button that is not in the list beside them.
    pageOf->setCurrentIndex(1);
    CHECK(dialog.buttons()[0].page == 2);
    CHECK(list->count() == 2);

    pageList->setCurrentIndex(0);
    CHECK(list->count() == 0);
}

/// The editor opens on the page it was handed, which is the one the panel is
/// showing.
///
/// It did not: `rebuildPages` fills the page box, the first `addItem` emits
/// `currentIndexChanged(0)`, and an unguarded handler turned that into
/// `setPage(1)` — inside the constructor, after the argument had been taken.
/// Every page operation then worked on page 1 whatever was on screen, so
/// Remove page removed the wrong one.
void the_editor_opens_on_the_page_it_was_given()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nPage2Name=BMCs\r\nPage3Name=Switches\r\n"
                 "Button1Label=One\r\nButton1Value=a$0D\r\n"
                 "Button2Label=Two\r\nButton2Page=2\r\nButton2Value=b$0D\r\n"
                 "Button3Label=Three\r\nButton3Page=3\r\nButton3Value=c$0D\r\n");

    MainWindow window(ini);
    QuickButtonsDialog dialog(barOf(window)->set(), 3, window.session(), &window);
    auto *pageList =
        dialog.findChild<QComboBox *>(QStringLiteral("quickButtonsPageList"));
    auto *list = dialog.findChild<QListWidget *>(QStringLiteral("quickButtonsList"));
    CHECK(pageList != nullptr && list != nullptr);
    CHECK(pageList->currentIndex() == 2);
    CHECK(pageList->currentText() == QLatin1String("Switches"));
    CHECK(list->count() == 1);
    CHECK(list->item(0)->text() == QLatin1String("Three"));
}

/// Removing a page keeps every command on it, and removes the page the editor
/// is actually showing. Only Remove deletes a command, and only Remove asks.
void removing_a_page_keeps_its_buttons()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nPage2Name=BMCs\r\nPage3Name=Switches\r\n"
                 "Button1Label=One\r\nButton1Value=a$0D\r\n"
                 "Button2Label=Two\r\nButton2Page=2\r\nButton2Value=b$0D\r\n"
                 "Button3Label=Three\r\nButton3Page=3\r\nButton3Value=c$0D\r\n");

    MainWindow window(ini);
    QuickButtonsDialog dialog(barOf(window)->set(), 3, window.session(), &window);
    auto *remove =
        dialog.findChild<QAction *>(QStringLiteral("quickButtonsPageRemove"));
    CHECK(remove != nullptr && remove->isEnabled());

    // Three pages, showing the third: its command joins the second, and no page
    // below it moves. A dialog that had quietly gone back to page 1 would merge
    // pages 1 and 2 instead, which these three labels can tell apart.
    remove->trigger();
    CHECK(dialog.buttons().size() == 3);
    CHECK(dialog.set().pageCount() == 2);
    CHECK(dialog.buttons()[0].page == 1);
    CHECK(dialog.buttons()[1].page == 2);
    CHECK(dialog.buttons()[2].page == 2);
    CHECK(dialog.set().pageLabel(2) == QLatin1String("BMCs"));
    // **And nothing landed on page 0.** `rebuildPages` clears both combos, and
    // a leaked loading flag turned the empty one's `currentData()` into a page
    // number no button may hold — which took the button off every page in the
    // editor and out of an export, silently.
    for (const QuickButton &button : dialog.buttons()) {
        CHECK(button.page >= 1);
    }

    remove->trigger();
    CHECK(dialog.set().pageCount() == 1);
    CHECK(dialog.buttons().size() == 3);
    // ...and with one page left there is nothing to remove.
    CHECK(!remove->isEnabled());
}

/// The panel's drop-down says which page is drawn, after a rebuild as well as
/// after a switch.
///
/// A rebuild destroys the box and the new one starts at row 0, so a panel on
/// page 2 came back drawing page 2 under a drop-down reading `Page 1` — and
/// clicking `Page 1` then emitted nothing, so it read as a dead control.
void a_rebuilt_panel_keeps_its_page_selector_in_step()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    QComboBox *box = pageBoxOf(window);
    CHECK(box != nullptr);

    box->setCurrentIndex(1);
    CHECK(spin([bar] { return bar->page() == 2; }, 2000));

    // Every editor OK, Move to page and Remove goes through here.
    QuickButtonSet edited = bar->set();
    edited.buttons[0].label = QStringLiteral("Renamed");
    QString error;
    CHECK(saveQuickButtons(ini, edited, &error));
    CHECK(window.session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), &error));
    CHECK(spin([bar] {
        return bar->buttons().value(0).label == QLatin1String("Renamed");
    }, 2000));

    CHECK(bar->page() == 2);
    box = pageBoxOf(window);
    CHECK(box != nullptr);
    CHECK(box->currentIndex() == 1);
    CHECK(box->currentText() == QLatin1String("BMCs"));
    // ...and the drawn buttons agree with it.
    CHECK(bar->buttonWidget(0) == nullptr);
    CHECK(bar->buttonWidget(1) != nullptr);
}

/// An exported page is an ordinary settings file, and a settings file imports
/// as a page.
void a_page_exported_and_imported_comes_back()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, twoPages());
    const QString out = dir.filePath(QStringLiteral("bmcs.ini"));

    MainWindow window(ini);
    // Export page 2 by hand along the path the dialog takes — the file dialog
    // itself is modal and not something a test can click through.
    QuickButtonSet page;
    for (const QuickButton &button : barOf(window)->buttons()) {
        if (button.page != 2) {
            continue;
        }
        QuickButton copy = button;
        copy.page = 1;
        page.buttons.append(copy);
    }
    page.pageNames.append(QStringLiteral("BMCs"));
    QString error;
    CHECK(saveQuickButtons(out, page, &error));

    QFile file(out);
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray text = file.readAll();
    // No `Page` key at all: an exported page is a one-page file, which is what
    // makes it paste-able into a settings file by hand.
    CHECK(!text.contains("Button1Page"));
    CHECK(text.contains("Page1Name=BMCs"));
    CHECK(text.contains("Button1Label=Power"));

    const QuickButtonSet back = loadQuickButtons(out);
    CHECK(back.buttons.size() == 1);
    CHECK(back.buttons[0].page == 1);
    CHECK(back.pageCount() == 1);
    CHECK(back.pageLabel(1) == QLatin1String("BMCs"));
    // The shortcut travelled with it in the file; the importer is what clears
    // it, so that a key is never taken from a button in *this* file silently.
    CHECK(back.buttons[0].shortcut == QLatin1String("Ctrl+Alt+2"));
}

/// A page name must not hold the panel open any more than a caption does.
void a_narrow_panel_shortens_the_page_name_too()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(
        dir,
        "[Sterna Buttons]\r\nPage2Name=Out-of-band management network\r\n"
        "Button1Label=Version\r\nButton1Value=a$0D\r\n"
        "Button2Label=Power\r\nButton2Page=2\r\nButton2Value=b$0D\r\n");

    MainWindow window(ini);
    window.show();
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && spin([bar] { return bar->isVisible(); }, 2000));
    QComboBox *box = pageBoxOf(window);
    CHECK(box != nullptr);

    CHECK(setPanelWidth(window, 90));
    CHECK(box->width() <= 90);
    // Elision is paint-only: the model still holds the real name, which is
    // what the popup, the tooltip and the settings file all show.
    CHECK(box->itemText(1) == QLatin1String("Out-of-band management network"));
    // The floor is the window's fixed 48 and not this widget's idea of itself.
    CHECK(box->minimumSizeHint().width() == 0);
    CHECK(bar->minimumSizeHint().width() <= 48);
}

/// `--write <dir>`: the bar and its editor as PNGs, for a human to look at.
///
/// `QWidget::grab()` re-renders offscreen, which is the only screenshot that
/// works in this container anyway — and it is what makes a review of a toolbar
/// possible without somebody sitting in front of it.
/// A repeating button sends the number of times it was asked for, at the
/// interval it was asked for, and then stops by itself.
void a_repeat_sends_its_count_and_stops()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Poll\r\nButton1Kind=text\r\n"
                 "Button1Value=poll-marker$0D\r\n"
                 "Button1Repeat=3\r\nButton1IntervalMs=100\r\n");

    MainWindow window(ini);
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && bar->buttons().size() == 1);
    CHECK(bar->buttons()[0].repeat == 3);
    CHECK(bar->buttons()[0].intervalMs == 100);
    CHECK(bar->buttons()[0].repeats() && !bar->buttons()[0].repeatsForever());

    window.show();
    // Into nothing, so the only copy that comes back is the tty's own echo.
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    press(window, 0);
    CHECK(spin([session] { return markerCount(*session, "poll-marker") == 3; },
               4000));
    // Three is the whole of what was asked for: nothing arrives afterwards.
    spin([] { return false; }, 500);
    CHECK(markerCount(*session, "poll-marker") == 3);

    // ...and the button's face goes back to what it was.
    CHECK(!buttonAction(window, 0)->isChecked());
    CHECK(buttonAction(window, 0)->text() == QLatin1String("Poll"));
    TerminalView *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr && !view->stopKeyArmed());
}

/// A run with no end keeps going, and the second press is what stops it.
void a_second_press_stops_a_run_with_no_end()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Keepalive\r\n"
                 "Button1Value=keepalive$0D\r\n"
                 "Button1Repeat=forever\r\nButton1IntervalMs=100\r\n");

    MainWindow window(ini);
    QuickButtonBar *bar = barOf(window);
    CHECK(bar != nullptr && bar->buttons().size() == 1);
    CHECK(bar->buttons()[0].repeatsForever());

    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    press(window, 0);
    // Past the count any finite button could have had, which is the point of
    // this one.
    CHECK(spin([session] { return markerCount(*session, "keepalive") >= 4; },
               4000));
    CHECK(buttonAction(window, 0)->isChecked());
    // The mark is fixed and the count is in the tooltip, so pressing the
    // button next to this one does not become a moving target.
    CHECK(buttonAction(window, 0)->text() == QString::fromUtf8("Keepalive ⟳"));
    CHECK(buttonAction(window, 0)->toolTip().contains(
        QLatin1String("A second press stops the repeat")));
    TerminalView *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr && view->stopKeyArmed());
    if (!g_writeTo.isEmpty()) {
        window.grab().save(g_writeTo
                           + QStringLiteral("/quick-buttons-repeating.png"));
    }

    press(window, 0);
    CHECK(!buttonAction(window, 0)->isChecked());
    CHECK(view != nullptr && !view->stopKeyArmed());
    // Settle first: the screen lags the wire, so what is counted here is what
    // had already been sent. What matters is that it stops growing.
    spin([] { return false; }, 500);
    const int settled = markerCount(*session, "keepalive");
    spin([] { return false; }, 600);
    CHECK(markerCount(*session, "keepalive") == settled);
}

/// A setting that is nothing to do with the buttons does not end a run.
///
/// `reloadQuickButtons` runs on **every** settings change, and it used to stop
/// every repeat before it had looked at what had changed — so a font, a colour
/// or the panel's own width ended a keepalive somebody had started, with
/// nothing on screen saying why. `docs/deviations.md` entry 7 lists the ways a
/// run stops and this was not among them.
void an_unrelated_setting_leaves_a_repeat_running()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Keepalive\r\n"
                 "Button1Value=keepalive$0D\r\n"
                 "Button1Repeat=forever\r\nButton1IntervalMs=100\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    press(window, 0);
    CHECK(spin([session] { return markerCount(*session, "keepalive") >= 2; },
               3000));
    CHECK(buttonAction(window, 0)->isChecked());

    CHECK(window.session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), nullptr));
    qApp->processEvents();

    // Still going, and still going in a way that puts bytes on the wire rather
    // than only leaving the face pressed.
    CHECK(buttonAction(window, 0)->isChecked());
    const int settled = markerCount(*session, "keepalive");
    CHECK(spin([session, settled]
               { return markerCount(*session, "keepalive") > settled; },
               3000));

    TerminalView *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr && view->stopKeyArmed());
    press(window, 0);
    CHECK(!buttonAction(window, 0)->isChecked());
}

/// Escape in the terminal stops everything — and the key belongs to the host
/// again the moment nothing is running.
void escape_stops_every_run_and_only_then()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=One\r\n"
                 "Button1Value=first-marker$0D\r\n"
                 "Button1Repeat=forever\r\nButton1IntervalMs=100\r\n"
                 "Button2Label=Two\r\nButton2Value=second-marker$0D\r\n"
                 "Button2Repeat=forever\r\nButton2IntervalMs=100\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    TerminalView *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr);
    // Nothing running: the terminal has its Escape, as it must, or every
    // full-screen program on the far end loses a key it needs.
    CHECK(!view->stopKeyArmed());

    press(window, 0);
    press(window, 1);
    CHECK(spin([session] { return markerCount(*session, "first-marker") >= 2; },
               4000));
    CHECK(spin([session] { return markerCount(*session, "second-marker") >= 2; },
               4000));
    CHECK(view->stopKeyArmed());

    QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
    QApplication::sendEvent(view, &escape);
    CHECK(escape.isAccepted());
    CHECK(!buttonAction(window, 0)->isChecked());
    CHECK(!buttonAction(window, 1)->isChecked());
    CHECK(!view->stopKeyArmed());

    spin([] { return false; }, 500);
    const int first = markerCount(*session, "first-marker");
    const int second = markerCount(*session, "second-marker");
    spin([] { return false; }, 600);
    CHECK(markerCount(*session, "first-marker") == first);
    CHECK(markerCount(*session, "second-marker") == second);
}

/// A run ends with the line it was sending down. Losing the connection is not
/// a reason to keep a timer alive.
void a_repeat_ends_with_the_connection()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini =
        writeIni(dir,
                 "[Sterna Buttons]\r\nButton1Label=Poll\r\n"
                 "Button1Value=connected-marker$0D\r\n"
                 "Button1Repeat=forever\r\nButton1IntervalMs=100\r\n");

    MainWindow window(ini);
    window.show();
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat > /dev/null")});
    Session *session = window.session();
    CHECK(spin([session] { return session->isConnected(); }, 3000));
    CHECK(spin([&window] { return buttonAction(window, 0)->isEnabled(); }, 2000));

    press(window, 0);
    CHECK(spin([session] { return markerCount(*session, "connected-marker") >= 2; },
               4000));
    CHECK(buttonAction(window, 0)->isChecked());

    // `disconnectPort`, not `disconnect` — the second is `QObject`'s and
    // silently unhooks every signal this window is listening to.
    session->disconnectPort();
    CHECK(spin([session] { return !session->isConnected(); }, 3000));
    CHECK(spin([&window] { return !buttonAction(window, 0)->isChecked(); }, 2000));
    TerminalView *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr && !view->stopKeyArmed());
}

/// The editor's two fields, including the count below one that means a run
/// with no end.
void the_editor_round_trips_a_repeat()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = writeIni(dir, "");

    MainWindow window(ini);
    QuickButtonsDialog dialog(QuickButtonSet(), 1, window.session(), &window);
    QuickButton seed;
    seed.kind = TT_QUICK_BUTTON_TEXT;
    seed.text = QStringLiteral("show clock\r");
    dialog.appendButton(seed);

    auto *repeat = dialog.findChild<QSpinBox *>(QStringLiteral("quickButtonRepeat"));
    auto *interval =
        dialog.findChild<QDoubleSpinBox *>(QStringLiteral("quickButtonInterval"));
    CHECK(repeat != nullptr && interval != nullptr);
    // A new button sends once, and the interval is not offered until it is
    // asked to send more than that.
    CHECK(repeat->value() == 1);
    CHECK(!dialog.buttons()[0].repeats());

    repeat->setValue(5);
    interval->setValue(2.5);
    CHECK(dialog.buttons()[0].repeat == 5);
    CHECK(dialog.buttons()[0].intervalMs == 2500);

    // The minimum is shown as words and stored as the sentinel, not as zero.
    repeat->setValue(0);
    CHECK(repeat->text() == QLatin1String("Until stopped"));
    CHECK(dialog.buttons()[0].repeatsForever());

    QString error;
    CHECK(saveQuickButtons(ini, dialog.set(), &error));
    const QVector<QuickButton> back = loadQuickButtons(ini).buttons;
    CHECK(back.size() == 1);
    CHECK(back[0].repeatsForever());
    CHECK(back[0].intervalMs == 2500);
    // ...and the file says it in a word somebody can read.
    QFile file(ini);
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray text = file.readAll();
    CHECK(text.contains("Button1Repeat=forever"));
    CHECK(text.contains("Button1IntervalMs=2500"));
}

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
        "Button5Label=Break\r\nButton5Kind=command\r\nButton5Value=50430\r\n"
        "Button6Label=Poll\r\nButton6Value=show clock$0D\r\n"
        "Button6Repeat=forever\r\nButton6IntervalMs=5000\r\n");

    MainWindow window(ini);
    window.resize(760, 400);
    window.show();
    spin([] { return false; }, 300);
    window.grab().save(g_writeTo + QStringLiteral("/quick-buttons-window.png"));

    // ...and wider, which is the same window with a wider panel in it and a
    // terminal that has not lost a column to pay for it.
    setPanelWidth(window, barOf(window)->width() + 90);
    spin([] { return false; }, 300);
    window.grab().save(g_writeTo + QStringLiteral("/quick-buttons-wide.png"));

    QuickButtonsDialog dialog(loadQuickButtons(ini), 1, window.session(), &window);
    dialog.selectRow(3);
    // Without this the dialog is grabbed before layout and the wrapped warning
    // overlaps the fields in the image and nowhere else.
    dialog.adjustSize();
    dialog.grab().save(g_writeTo + QStringLiteral("/quick-buttons-editor.png"));

    // ...and the repeat row with something in it, which is the only state in
    // which it shows an interval.
    QuickButtonsDialog repeating(loadQuickButtons(ini), 1, window.session(),
                                 &window);
    repeating.selectRow(5);
    repeating.adjustSize();
    repeating.grab().save(g_writeTo + QStringLiteral("/quick-buttons-repeat.png"));

    // The panel with pages on it, and the editor showing the second one —
    // where the page row is a control rather than a greyed reminder that the
    // feature exists.
    QTemporaryDir paged;
    const QString pagedIni = writeIni(
        paged,
        "[Sterna Buttons]\r\nPage2Name=BMCs\r\n"
        "Button1Label=Show version\r\nButton1Value=show version$0D\r\n"
        "Button2Label=Interfaces\r\nButton2Value=show ip int brief$0D\r\n"
        "Button3Label=Save config\r\nButton3Value=write memory$0D\r\n"
        "Button4Label=Power status\r\nButton4Page=2\r\n"
        "Button4Value=power status$0D\r\n"
        "Button5Label=Power cycle\r\nButton5Page=2\r\n"
        "Button5Value=power cycle$0D\r\nButton5Confirm=on\r\n"
        "Button6Label=SOL console\r\nButton6Page=2\r\n"
        "Button6Value=sol activate$0D\r\n");

    MainWindow pagedWindow(pagedIni);
    pagedWindow.resize(760, 400);
    pagedWindow.show();
    spin([] { return false; }, 300);
    pagedWindow.grab().save(g_writeTo + QStringLiteral("/quick-buttons-pages.png"));

    barOf(pagedWindow)->setPage(2);
    spin([] { return false; }, 300);
    pagedWindow.grab().save(g_writeTo
                            + QStringLiteral("/quick-buttons-page-two.png"));

    QuickButtonsDialog pages(loadQuickButtons(pagedIni), 2,
                             pagedWindow.session(), &pagedWindow);
    pages.selectRow(4);
    pages.adjustSize();
    pages.grab().save(g_writeTo + QStringLiteral("/quick-buttons-pages-editor.png"));
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
    an_unrelated_setting_leaves_the_buttons_alone();
    an_empty_list_keeps_the_add_button();
    the_editor_round_trips_a_button();
    the_editor_preserves_an_unknown_command();
    the_editor_warns_about_a_key_the_host_wants();
    a_shortcut_is_installed_and_released_with_the_bar();
    the_panel_opens_down_the_right_and_a_width_widens_it();
    widening_the_panel_grows_the_window_and_not_the_grid();
    a_width_stops_at_the_edge_of_the_screen();
    showing_the_panel_leaves_every_terminal_alone();
    the_configured_panel_width_is_opened_at();
    the_width_has_a_control_in_setup();
    the_panel_menu_offers_the_width();
    a_narrow_panel_shortens_its_captions();
    adding_starts_on_a_new_row();
    a_repeat_sends_its_count_and_stops();
    a_second_press_stops_a_run_with_no_end();
    an_unrelated_setting_leaves_a_repeat_running();
    escape_stops_every_run_and_only_then();
    a_second_page_puts_a_selector_on_the_panel();
    the_panel_shows_only_the_current_page();
    a_shortcut_fires_from_another_page();
    switching_pages_leaves_a_repeat_running();
    the_remembered_page_is_opened_at();
    a_page_that_stops_existing_moves_the_setting_down();
    the_panel_menu_offers_the_pages();
    the_editor_moves_a_button_between_pages();
    the_editor_opens_on_the_page_it_was_given();
    removing_a_page_keeps_its_buttons();
    a_rebuilt_panel_keeps_its_page_selector_in_step();
    a_page_exported_and_imported_comes_back();
    a_narrow_panel_shortens_the_page_name_too();
    a_repeat_ends_with_the_connection();
    the_editor_round_trips_a_repeat();
    render_widgets();

    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("buttons ok");
    return 0;
}
