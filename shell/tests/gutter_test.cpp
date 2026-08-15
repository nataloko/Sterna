// The line-number gutter, and the one thing about it that must never change.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
// Two questions, and the second is the reason the feature is built the way it
// is: are the numbers on screen, and are they absent from what a copy produces.
// The second cannot be satisfied by remembering to strip something — it is
// satisfied by the gutter being a widget beside the terminal rather than
// columns inside it, so that `TerminalView::selectedText`, which walks cells
// the core owns, has no way to see it. This file is what says so out loud.
//
// It drives a real `MainWindow` rather than a bare widget because the third
// property is a window one: turning the gutter on must widen the window and
// leave the terminal at the size the settings asked for, not quietly take five
// columns off it — and `TerminalSize` follows a shrunken terminal, so getting
// that wrong is permanent.

#include <QApplication>
#include <QColor>
#include <QDir>
#include <QImage>
#include <QStandardPaths>
#include <QTemporaryDir>

#include <cstdio>
#include <cstring>

#include "LineNumberGutter.h"
#include "MainWindow.h"
#include "Session.h"
#include "TerminalView.h"
#include "Theme.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

/// A window with its own settings file, so this measures the defaults rather
/// than whoever ran it — the same reason `bench_shell` and `cmdline_test` take
/// this precaution. See `AGENTS.md`: anything constructing a `MainWindow` reads
/// the developer's own `sterna.ini` otherwise, terminal size included.
struct Harness {
    QTemporaryDir home;
    MainWindow window { home.filePath(QStringLiteral("gutter.ini")) };

    TerminalView *view = nullptr;
    LineNumberGutter *gutter = nullptr;

    Harness()
    {
        view = window.findChild<TerminalView *>();
        gutter = window.findChild<LineNumberGutter *>(
            QStringLiteral("lineNumberGutter"));
        window.show();
        settle();
    }

    void settle()
    {
        for (int i = 0; i < 8; i++) {
            qApp->processEvents();
        }
    }

    void feed(const char *bytes)
    {
        window.session()->feed(QByteArray(bytes, int(strlen(bytes))));
        settle();
    }

    void set(const char *name, const QString &value)
    {
        QString error;
        CHECK(window.session()->setSetting(QString::fromLatin1(name), value,
                                           &error));
        settle();
    }

    /// How many pixels of one gutter cell differ from the gutter's background —
    /// "is there a digit here". The same question `render_test::ink` asks of
    /// the terminal, and asked the same way, because glyph coverage depends on
    /// the font but "blank or not blank" does not.
    int ink(const QImage &image, int col, int row) const
    {
        const int cw = view->theme().cellWidth();
        const int ch = view->theme().cellHeight();
        const QColor bg = view->theme().defaultBackground();
        int n = 0;
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                const int px = col * cw + x;
                const int py = row * ch + y;
                if (px >= image.width() || py >= image.height()) {
                    continue;
                }
                if (image.pixelColor(px, py) != bg) {
                    n++;
                }
            }
        }
        return n;
    }
};

void test_the_gutter_ships_off()
{
    Harness h;
    CHECK(h.gutter != nullptr);
    CHECK(h.view != nullptr);
    CHECK(h.gutter && h.gutter->isHidden());
    // And it takes no room while hidden: the terminal starts where the row it
    // is in starts, exactly as it did before this existed.
    CHECK(h.view && h.view->parentWidget()
          && h.view->mapTo(h.view->parentWidget(), QPoint(0, 0)).x() == 0);
}

void test_turning_it_on_paints_numbers()
{
    Harness h;
    h.feed("alpha\r\nbravo\r\ncharlie\r\n");
    h.set("terminal.line_numbers", QStringLiteral("on"));

    CHECK(!h.gutter->isHidden());
    // Four digits plus the padding column that carries the rule.
    const int cw = h.view->theme().cellWidth();
    CHECK(h.gutter->width() == 5 * cw);

    const QImage image = h.gutter->grab().toImage();
    // Right-aligned in four digits, so a one-digit number lands in column 3 and
    // columns 0..2 of that row are blank. Rows 0, 1 and 2 are lines 1, 2 and 3.
    CHECK(h.ink(image, 3, 0) > 0);
    CHECK(h.ink(image, 3, 1) > 0);
    CHECK(h.ink(image, 3, 2) > 0);
    CHECK(h.ink(image, 0, 0) == 0);
    CHECK(h.ink(image, 1, 0) == 0);
}

void test_the_numbers_are_not_copied()
{
    Harness h;
    h.feed("alpha\r\nbravo\r\ncharlie\r\n");
    h.set("terminal.line_numbers", QStringLiteral("on"));

    h.view->selectAll();
    const QString copied = h.view->selectedText();

    // The text is there...
    CHECK(copied.contains(QStringLiteral("alpha")));
    CHECK(copied.contains(QStringLiteral("bravo")));
    CHECK(copied.contains(QStringLiteral("charlie")));
    // ...and not one digit of the gutter is, on any line. This is the whole
    // point of the feature's shape: the numbers are not cells, so there is
    // nothing here to strip and nothing that can regress into stripping it
    // wrongly.
    for (const QChar c : copied) {
        CHECK(!c.isDigit());
    }
    // Belt and braces on the specific numbers those three lines carry, in case
    // a future line of test text ever legitimately contains a digit.
    CHECK(!copied.contains(QStringLiteral("1 alpha")));
    CHECK(!copied.contains(QStringLiteral("2 bravo")));
}

void test_the_window_grows_and_the_terminal_does_not_shrink()
{
    Harness h;
    const int cols = h.window.session()->cols();
    const int rows = h.window.session()->rows();
    const int width = h.window.width();
    CHECK(cols > 0);

    h.set("terminal.line_numbers", QStringLiteral("on"));

    // The terminal keeps every column it had — the gutter is chrome outside
    // `terminal.cols`, so `TerminalSize` cannot follow it downwards.
    CHECK(h.window.session()->cols() == cols);
    CHECK(h.window.session()->rows() == rows);
    CHECK(h.window.session()->setting(QStringLiteral("terminal.cols")).toInt()
          == cols);
    // ...which it can only do by the window having got wider.
    CHECK(h.window.width() > width);

    // And back again, with nothing left behind.
    h.set("terminal.line_numbers", QStringLiteral("off"));
    CHECK(h.gutter->isHidden());
    CHECK(h.window.session()->cols() == cols);
}

void test_the_width_setting_moves_the_gutter()
{
    Harness h;
    h.set("terminal.line_numbers", QStringLiteral("on"));
    const int cw = h.view->theme().cellWidth();
    CHECK(h.gutter->width() == 5 * cw);

    h.set("terminal.line_number_width", QStringLiteral("6"));
    CHECK(h.gutter->digits() == 6);
    CHECK(h.gutter->width() == 7 * cw);

    // Clamped at both ends rather than falling back to the default, which is
    // what `int_clamp` in the schema says and what a person hand-editing the
    // file will expect.
    h.set("terminal.line_number_width", QStringLiteral("999"));
    CHECK(h.gutter->digits() == 10);
    h.set("terminal.line_number_width", QStringLiteral("0"));
    CHECK(h.gutter->digits() == 1);
}

void test_the_gutter_follows_the_terminals_colours()
{
    Harness h;
    h.set("terminal.line_numbers", QStringLiteral("on"));
    const QColor before = h.gutter->grab().toImage().pixelColor(0, 0);
    CHECK(before == h.view->theme().defaultBackground());

    // A host repainting the background reaches the gutter, which is not a
    // given: `OSC 11` emits `colorsChanged` and no line number moved, so
    // nothing on the `viewChanged` path would have run.
    h.feed("\033]11;#000080\033\\");
    const QColor after = h.gutter->grab().toImage().pixelColor(0, 0);
    CHECK(after == h.view->theme().defaultBackground());
    CHECK(after != before);
}

void test_numbers_follow_the_history()
{
    Harness h;
    h.set("terminal.line_numbers", QStringLiteral("on"));
    // Past a screenful, so the top of the view is no longer line 1.
    for (int i = 0; i < 40; i++) {
        h.feed("line\r\n");
    }
    CHECK(h.window.session()->lineAt(0) > 0);

    const QImage image = h.gutter->grab().toImage();
    // Two digits now, so the tens column has ink where it had none at line 1.
    CHECK(h.ink(image, 2, 0) > 0);
    CHECK(h.ink(image, 3, 0) > 0);
}

} // namespace

int main(int argc, char **argv)
{
    // Before `QApplication`, so the window below cannot read the developer's
    // real settings — see `Harness`.
    QStandardPaths::setTestModeEnabled(true);
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORM")) {
        qputenv("QT_QPA_PLATFORM", "offscreen");
    }
    QApplication app(argc, argv);

    test_the_gutter_ships_off();
    test_turning_it_on_paints_numbers();
    test_the_numbers_are_not_copied();
    test_the_window_grows_and_the_terminal_does_not_shrink();
    test_the_width_setting_moves_the_gutter();
    test_the_gutter_follows_the_terminals_colours();
    test_numbers_follow_the_history();

    // `--write DIR` dumps what this looks like, the way every other test binary
    // here does — the layout of a thing is the part no assertion can judge.
    for (int i = 1; i + 1 < argc; i++) {
        if (strcmp(argv[i], "--write") != 0) {
            continue;
        }
        const QString dir = QString::fromUtf8(argv[i + 1]);
        QDir().mkpath(dir);
        Harness h;
        h.window.resize(820, 420);
        h.settle();
        h.set("terminal.line_numbers", QStringLiteral("on"));
        h.feed("$ ls\r\nAGENTS.md  crates  shell\r\n$ cat PLAN.md\r\n"
               "# Sterna \xe2\x80\x94 plan and status\r\n$ ");
        const QString path = dir + QStringLiteral("/line-numbers.png");
        h.window.grab().save(path);
        printf("wrote %s\n", qPrintable(path));
    }

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("gutter ok\n");
    return 0;
}
