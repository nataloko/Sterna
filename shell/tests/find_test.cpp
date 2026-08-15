// Find, from the shortcut to the pixels.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   ./build/find_test [--write <dir>]
//
// Needs no server and no hardware: the terminal is fed directly.
//
// The assertions on colour are on background fills, for `render_test`'s reason
// — a background is a solid rectangle whose colour is the whole output of
// `Theme::resolve`, while glyph coverage depends on the font.
//
// The load-bearing case in here is `test_the_bar_does_not_resize_the_terminal`.
// Everything else is behaviour; that one is the reason the bar floats over the
// terminal instead of sitting in the page's layout, and it is the case that
// will fail if somebody later decides the layout would be tidier.

#include <QApplication>
#include <QCheckBox>
#include <QColor>
#include <QComboBox>
#include <QDir>
#include <QImage>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QToolButton>

#include <cstdio>
#include <cstring>

#include "FindBar.h"
#include "MainWindow.h"
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

/// `color.find`'s shipped background — what a match that is not the current one
/// is filled with.
const QColor kFindBg(255, 220, 120);

struct Harness {
    Session session {40, 6};
    TerminalView view {&session};
    QImage image;

    Harness()
    {
        view.resize(40 * view.theme().cellWidth(), 6 * view.theme().cellHeight());
        // A session a test feeds bytes into is a session with something on the
        // other end. Without this every background these cases assert moves by
        // `color.disconnected_shade`.
        view.theme().setConnected(true);
    }

    void feed(const char *bytes)
    {
        session.feed(QByteArray(bytes, static_cast<int>(strlen(bytes))));
    }

    FindBar *bar() const { return view.findBar(); }
    QComboBox *field() const
    {
        return bar()->findChild<QComboBox *>(QStringLiteral("findPattern"));
    }
    QLabel *status() const
    {
        return bar()->findChild<QLabel *>(QStringLiteral("findStatus"));
    }

    /// Type a pattern and search at once, without waiting out the debounce —
    /// the timer is about not scanning the history once per keystroke, and a
    /// test asserting *what* was found should not also be asserting when.
    void search(const QString &pattern)
    {
        view.openFind();
        field()->lineEdit()->setText(pattern);
        bar()->findNext();
    }

    void render() { image = view.grab().toImage(); }

    bool filledWith(int col, int row, const QColor &want) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        int seen = 0;
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                if (image.pixelColor(col * cw + x, row * ch + y) == want) {
                    seen++;
                }
            }
        }
        return seen * 2 >= cw * ch;
    }

    void save(const QString &name) const
    {
        if (!writeDir.isEmpty()) {
            image.save(QDir(writeDir).filePath(name));
        }
    }
};

/// Typing a pattern selects the first match and says how many there are.
void test_typing_finds_the_first_match()
{
    Harness h;
    h.feed("one hit\r\ntwo hit\r\nthree hit");
    h.search(QStringLiteral("hit"));

    CHECK(h.view.hasSelection());
    CHECK(h.view.selectedText() == QLatin1String("hit"));
    CHECK(h.status()->text() == QLatin1String("1 of 3"));
}

/// Next and Previous step, and wrap at both ends.
void test_next_and_previous_step_and_wrap()
{
    Harness h;
    h.feed("one hit\r\ntwo hit\r\nthree hit");
    h.search(QStringLiteral("hit"));
    CHECK(h.status()->text() == QLatin1String("1 of 3"));

    h.bar()->findNext();
    CHECK(h.status()->text() == QLatin1String("2 of 3"));
    h.bar()->findNext();
    CHECK(h.status()->text() == QLatin1String("3 of 3"));
    // Off the end, round to the top: with somewhere to go, a Next button that
    // said "no matches" would be lying about a terminal full of them.
    h.bar()->findNext();
    CHECK(h.status()->text() == QLatin1String("1 of 3"));
    h.bar()->findPrevious();
    CHECK(h.status()->text() == QLatin1String("3 of 3"));
}

/// The three boxes are spellings of one pattern, and each re-searches.
void test_the_three_boxes()
{
    Harness h;
    h.feed("ERROR errors err");
    // The boxes ship unticked, so this is three: `ERROR`, `errors` and `err`.
    h.search(QStringLiteral("err"));
    CHECK(h.status()->text() == QLatin1String("1 of 3"));

    // Ticking a box re-searches by itself — it has changed what the pattern
    // means, and leaving the old answer on screen would be a lie about it.
    h.bar()->findChild<QCheckBox *>(QStringLiteral("findCaseBox"))->setChecked(true);
    CHECK(h.status()->text() == QLatin1String("1 of 2"));

    h.bar()->findChild<QCheckBox *>(QStringLiteral("findWholeWordBox"))
        ->setChecked(true);
    CHECK(h.status()->text() == QLatin1String("1 of 1"));
    CHECK(h.view.selectedText() == QLatin1String("err"));

    h.bar()->findChild<QCheckBox *>(QStringLiteral("findWholeWordBox"))
        ->setChecked(false);
    h.bar()->findChild<QCheckBox *>(QStringLiteral("findRegexBox"))->setChecked(true);
    h.field()->lineEdit()->setText(QStringLiteral("err(or)?s"));
    h.bar()->findNext();
    CHECK(h.view.selectedText() == QLatin1String("errors"));
}

/// Every match is painted, and the one being stepped through is the selection.
void test_every_match_is_painted()
{
    Harness h;
    h.feed("hit and hit");
    h.search(QStringLiteral("hit"));
    h.render();
    h.save(QStringLiteral("find-matches.png"));

    // The second match wears `color.find`; the first is the current one and so
    // is drawn as a selection, which is the ordinary reverse of the text pair.
    CHECK(h.filledWith(8, 0, kFindBg));
    CHECK(h.filledWith(9, 0, kFindBg));
    CHECK(h.filledWith(10, 0, kFindBg));
    CHECK(!h.filledWith(0, 0, kFindBg));
    // And nothing between them.
    CHECK(!h.filledWith(4, 0, kFindBg));

    // Closing puts the screen back.
    h.bar()->close();
    h.render();
    CHECK(!h.filledWith(8, 0, kFindBg));
}

/// A match in the scrollback is scrolled to rather than merely reported.
void test_a_match_in_the_scrollback_is_scrolled_to()
{
    Harness h;
    // Five lines ahead of it, so the match is not the oldest line in the
    // buffer — the view can then be centred on it rather than clamped, which
    // is what the row assertion below is about.
    for (int i = 0; i < 5; i++) {
        h.feed("plain\r\n");
    }
    h.feed("the ERROR line\r\n");
    for (int i = 0; i < 30; i++) {
        h.feed("plain\r\n");
    }
    CHECK(h.session.viewOffset() == 0);
    CHECK(h.session.scrollbackLen() > 0);

    h.search(QStringLiteral("ERROR"));
    CHECK(h.view.selectedText() == QLatin1String("ERROR"));
    // Scrolled back far enough that the match is on screen.
    CHECK(h.session.viewOffset() > 0);
    int row = -1;
    for (int y = 0; y < h.session.rows(); y++) {
        if (h.session.lineAt(y) == 5) {
            row = y;
        }
    }
    CHECK(row >= 0);
    // Not against the top edge: a match with no context above it is a match
    // somebody has to scroll to read around.
    CHECK(row > 0);

    h.render();
    h.save(QStringLiteral("find-scrollback.png"));
    if (row >= 0) {
        // The current match is the selection, so it is painted in the reverse
        // of the text pair rather than in `color.find` — which is what makes it
        // tellable from the others.
        const QColor plain = h.view.theme().defaultBackground();
        CHECK(!h.filledWith(4, row, plain));
        CHECK(!h.filledWith(4, row, kFindBg));
        CHECK(h.filledWith(0, row, plain));
    }
}

/// **The reason the bar floats.** A bar in the page's layout would take a row
/// from the terminal, and `Session::resize` sends a scrolled-back view live and
/// rewrites `TerminalSize` — so closing it would throw away the position
/// somebody had just searched to.
void test_the_bar_does_not_resize_the_terminal()
{
    Harness h;
    h.feed("the ERROR line\r\n");
    for (int i = 0; i < 30; i++) {
        h.feed("plain\r\n");
    }
    const int rows = h.session.rows();
    const int cols = h.session.cols();

    h.search(QStringLiteral("ERROR"));
    const int offset = h.session.viewOffset();
    CHECK(offset > 0);
    CHECK(h.session.rows() == rows);
    CHECK(h.session.cols() == cols);

    h.bar()->close();
    CHECK(h.session.rows() == rows);
    CHECK(h.session.cols() == cols);
    // Still where the search left it, and still selected — which is what makes
    // Copy work on a match after the bar has gone.
    CHECK(h.session.viewOffset() == offset);
    CHECK(h.view.hasSelection());
    CHECK(h.view.selectedText() == QLatin1String("ERROR"));
}

/// Escape closes the bar and gives the keyboard back to the terminal.
void test_escape_closes_the_bar()
{
    Harness h;
    h.view.show();
    h.view.activateWindow();
    h.feed("an ERROR here");

    h.view.openFind();
    CHECK(!h.bar()->isHidden());
    QCoreApplication::processEvents();

    QKeyEvent press(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
    QCoreApplication::sendEvent(h.field()->lineEdit(), &press);
    QCoreApplication::processEvents();
    CHECK(h.bar()->isHidden());
    CHECK(!h.session.hasFind());
}

/// A pattern the engine will not take says why, and leaves the last one
/// running — somebody typing `(ERROR)` passes through `(ERROR` on the way.
void test_a_broken_pattern_complains_without_unpainting()
{
    Harness h;
    h.feed("an (ERROR) here");
    h.view.openFind();
    h.bar()->findChild<QCheckBox *>(QStringLiteral("findRegexBox"))->setChecked(true);
    h.field()->lineEdit()->setText(QStringLiteral("ERROR"));
    h.bar()->findNext();
    CHECK(h.status()->text() == QLatin1String("1 of 1"));

    // As typed, not as committed: this is the path a keystroke takes.
    h.field()->lineEdit()->setText(QStringLiteral("(ERROR"));
    emit h.field()->lineEdit()->textEdited(QStringLiteral("(ERROR"));
    CHECK(!h.status()->text().isEmpty());
    CHECK(h.status()->text() != QLatin1String("1 of 1"));
    // The search is still the one that compiled, so what was on screen is
    // still on screen: an unclosed parenthesis on the way to a longer pattern
    // must not blank out the matches somebody is looking at.
    CHECK(h.session.hasFind());
    CHECK(h.view.selectedText() == QLatin1String("ERROR"));
}

/// Nothing to find says so rather than saying nothing.
void test_no_matches_says_so()
{
    Harness h;
    h.feed("an ERROR here");
    h.search(QStringLiteral("nowhere"));
    CHECK(h.status()->text() == QLatin1String("No matches"));
    CHECK(!h.view.hasSelection());
}

/// The menu item, its shortcut, and the key it deliberately does not take.
void test_the_menu_item_and_its_shortcut()
{
    QTemporaryDir dir;
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));

    auto *find = window.findChild<QAction *>(QStringLiteral("findAction"));
    CHECK(find != nullptr);
    if (!find) {
        return;
    }
    CHECK(find->text() == QLatin1String("Find..."));
    CHECK(find->shortcut() == QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_F));

    // In Edit, beside the two other commands that act on the whole buffer.
    auto *edit = window.findChild<QMenu *>(QStringLiteral("editMenu"));
    CHECK(edit != nullptr);
    if (edit) {
        auto *all = window.findChild<QAction *>(QStringLiteral("selectAllAction"));
        CHECK(edit->actions().indexOf(find) > edit->actions().indexOf(all));
    }

    // **Ctrl+F is not taken.** A `QAction` shortcut silently outranks
    // `TerminalView::keyPressEvent`, so a Ctrl+F here would be `^F` — vim's
    // page forward, readline's forward-character — leaving the host for good.
    const QKeySequence plain(Qt::CTRL | Qt::Key_F);
    for (const QAction *action : window.findChildren<const QAction *>()) {
        CHECK(action->shortcut() != plain);
    }

    find->trigger();
    auto *bar = window.findChild<FindBar *>(QStringLiteral("findBar"));
    CHECK(bar != nullptr && !bar->isHidden());
}

/// A committed pattern is remembered, and offered to the next tab.
void test_patterns_are_remembered()
{
    QTemporaryDir dir;
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    {
        MainWindow window(ini);
        auto *bar = window.findChild<FindBar *>(QStringLiteral("findBar"));
        CHECK(bar != nullptr);
        if (!bar) {
            return;
        }
        auto *field = bar->findChild<QComboBox *>(QStringLiteral("findPattern"));
        bar->open();
        // A pattern with a `;` in it, which is what the percent-encoding is
        // for: `recent.host_history` can drop such a value and a search cannot.
        field->lineEdit()->setText(QStringLiteral("a;b"));
        bar->findNext();
        CHECK(bar->history().value(0) == QLatin1String("a;b"));
    }
    {
        MainWindow window(ini);
        auto *bar = window.findChild<FindBar *>(QStringLiteral("findBar"));
        CHECK(bar != nullptr);
        if (bar) {
            CHECK(bar->history().value(0) == QLatin1String("a;b"));
        }
    }
}

} // namespace

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--write") == 0 && i + 1 < argc) {
            writeDir = QString::fromLocal8Bit(argv[++i]);
        }
    }
    // Or it reads the developer's own sterna.ini, terminal size and all.
    QStandardPaths::setTestModeEnabled(true);
    QApplication app(argc, argv);

    test_typing_finds_the_first_match();
    test_next_and_previous_step_and_wrap();
    test_the_three_boxes();
    test_every_match_is_painted();
    test_a_match_in_the_scrollback_is_scrolled_to();
    test_the_bar_does_not_resize_the_terminal();
    test_escape_closes_the_bar();
    test_a_broken_pattern_complains_without_unpainting();
    test_no_matches_says_so();
    test_the_menu_item_and_its_shortcut();
    test_patterns_are_remembered();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("find ok\n");
    return 0;
}
