// Highlight rules, from the file to the pixels.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   ./build/highlight_test [--write <dir>]
//
// Needs no server and no hardware: the terminal is fed directly for the
// painting checks, and the one end-to-end case runs a local shell.
//
// The assertions are on background fills, for `render_test`'s reason — a
// background is a solid rectangle whose colour is the whole output of
// `Theme::resolve`, while glyph coverage depends on the font. Where a
// foreground has to be checked, it is checked as "there is ink of this colour
// somewhere in the cell" rather than at a fixed pixel.

#include <QApplication>
#include <QCheckBox>
#include <QColor>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QImage>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMouseEvent>
#include <QPixmap>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>
#include <cstring>

#include "Highlights.h"
#include "HighlightsDialog.h"
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

const QColor kRed(255, 0, 0);
const QColor kGreen(0, 128, 0);

template <typename F>
bool spin(F done, int ms)
{
    QElapsedTimer timer;
    timer.start();
    bool ok = done();
    while (!ok && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(20, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
        ok = done();
    }
    return ok;
}

QuickHighlight rule(const QString &pattern, const QColor &fore = kRed)
{
    QuickHighlight made;
    made.pattern = pattern;
    made.fore = fore;
    return made;
}

struct Harness {
    Session session { 40, 6 };
    TerminalView view { &session };
    QImage image;

    Harness()
    {
        view.resize(40 * view.theme().cellWidth(), 6 * view.theme().cellHeight());
    }

    void feed(const char *bytes)
    {
        session.feed(QByteArray(bytes, static_cast<int>(strlen(bytes))));
    }

    void apply(const QVector<QuickHighlight> &rules)
    {
        session.setHighlights(rules);
    }

    void render() { image = view.grab().toImage(); }

    /// The bottom-left corner of a cell — below the baseline and left of where
    /// a glyph *usually* starts. Fine for a blank cell; see `filledWith` for
    /// one with a letter in it.
    QColor bgAt(int col, int row) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        return image.pixelColor(col * cw, row * ch + ch - 1);
    }

    /// Whether a cell's background is `want`, measured as a majority of its
    /// pixels rather than at one of them.
    ///
    /// Sampling a corner is not enough here: at a nine-pixel cell a `j`'s
    /// descender reaches the bottom-left one, and an antialiased edge there
    /// reads as neither colour. A fill covers everything the glyph does not,
    /// and no glyph in a monospace face covers half its cell.
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

    /// Whether any pixel of a cell is `want` — how a *foreground* is checked
    /// without depending on where the font puts its ink.
    bool inkIs(int col, int row, const QColor &want) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                if (image.pixelColor(col * cw + x, row * ch + y) == want) {
                    return true;
                }
            }
        }
        return false;
    }

    void save(const QString &name) const
    {
        if (!writeDir.isEmpty()) {
            image.save(QDir(writeDir).filePath(name));
        }
    }

    int px(int col, double frac = 0.0) const
    {
        return static_cast<int>((col + frac) * view.theme().cellWidth());
    }
    int py(int row, double frac = 0.5) const
    {
        return static_cast<int>((row + frac) * view.theme().cellHeight());
    }

    /// Posted the way a real drag arrives, so the widget's own handlers run.
    void drag(int fromX, int fromY, int toX, int toY)
    {
        for (QEvent::Type type : {QEvent::MouseButtonPress, QEvent::MouseMove,
                                  QEvent::MouseButtonRelease}) {
            const int x = type == QEvent::MouseButtonPress ? fromX : toX;
            const int y = type == QEvent::MouseButtonPress ? fromY : toY;
            const Qt::MouseButtons held =
                type == QEvent::MouseButtonRelease ? Qt::NoButton : Qt::LeftButton;
            QMouseEvent ev(type, QPointF(x, y), QPointF(x, y), Qt::LeftButton, held,
                           Qt::NoModifier);
            QCoreApplication::sendEvent(&view, &ev);
        }
    }
};

/// A rule colours what it matched, and stops there.
void test_a_rule_colours_what_it_matched()
{
    Harness h;
    h.feed("an ERROR here");
    h.apply({rule(QStringLiteral("ERROR"))});
    h.render();
    h.save(QStringLiteral("highlight-match.png"));

    CHECK(h.inkIs(3, 0, kRed));
    CHECK(h.inkIs(7, 0, kRed));
    // The space before it and the `h` after are the host's own colour.
    CHECK(!h.inkIs(2, 0, kRed));
    CHECK(!h.inkIs(9, 0, kRed));
}

/// A background rule fills the cell, which is the assertion that pins the
/// colour model rather than the font.
void test_a_background_rule_fills_the_cells_it_matched()
{
    Harness h;
    h.feed("an ERROR here");
    QuickHighlight back;
    back.pattern = QStringLiteral("ERROR");
    back.back = kGreen;
    h.apply({back});
    h.render();
    h.save(QStringLiteral("highlight-background.png"));

    CHECK(!h.filledWith(0, 0, kGreen));
    for (int col = 3; col < 8; col++) {
        CHECK(h.filledWith(col, 0, kGreen));
    }
    CHECK(!h.filledWith(8, 0, kGreen));
}

/// The whole-line scope reaches the end of the text and no further — the
/// trailing blanks of a row are not part of the line.
void test_a_whole_line_rule_covers_the_line()
{
    Harness h;
    h.feed("an ERROR here");
    QuickHighlight line;
    line.pattern = QStringLiteral("ERROR");
    line.back = kGreen;
    line.wholeLine = true;
    h.apply({line});
    h.render();
    h.save(QStringLiteral("highlight-whole-line.png"));

    for (int col = 0; col < 13; col++) {
        CHECK(h.filledWith(col, 0, kGreen));
    }
    CHECK(!h.filledWith(13, 0, kGreen));
}

/// A wrapped command is one line to whoever typed it, so a match that
/// straddles the wrap is coloured on both rows.
void test_a_match_across_a_wrap_is_coloured_on_both_rows()
{
    Session session(10, 4);
    TerminalView view(&session);
    view.resize(10 * view.theme().cellWidth(), 4 * view.theme().cellHeight());
    session.feed(QByteArrayLiteral("abcdefghijklmno"));
    QuickHighlight back;
    back.pattern = QStringLiteral("hijkl");
    back.back = kGreen;
    session.setHighlights({back});

    const QImage image = view.grab().toImage();
    const int cw = view.theme().cellWidth();
    const int ch = view.theme().cellHeight();
    auto filled = [&](int col, int row) {
        int seen = 0;
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                if (image.pixelColor(col * cw + x, row * ch + y) == kGreen) {
                    seen++;
                }
            }
        }
        return seen * 2 >= cw * ch;
    };
    if (!writeDir.isEmpty()) {
        image.save(QDir(writeDir).filePath(QStringLiteral("highlight-wrapped.png")));
    }
    CHECK(!filled(6, 0));
    for (int col = 7; col < 10; col++) {
        CHECK(filled(col, 0));
    }
    for (int col = 0; col < 2; col++) {
        CHECK(filled(col, 1));
    }
    CHECK(!filled(2, 1));
}

/// Both switches, and each on its own.
void test_the_switches_stop_it()
{
    Harness h;
    h.feed("an ERROR here");
    QuickHighlight back;
    back.pattern = QStringLiteral("ERROR");
    back.back = kGreen;

    h.apply({back});
    h.render();
    CHECK(h.filledWith(4, 0, kGreen));

    // The rule's own switch.
    QuickHighlight off = back;
    off.enabled = false;
    h.apply({off});
    h.render();
    CHECK(!h.filledWith(4, 0, kGreen));

    // ...and the master one, which leaves the rules in the file.
    QString error;
    h.apply({back});
    CHECK(h.session.setSetting(QStringLiteral("color.highlighting"),
                               QStringLiteral("off"), &error));
    h.render();
    CHECK(!h.filledWith(4, 0, kGreen));
    CHECK(h.session.setSetting(QStringLiteral("color.highlighting"),
                               QStringLiteral("on"), &error));
    h.render();
    CHECK(h.filledWith(4, 0, kGreen));
}

/// A selection over highlighted text still inverts, rather than the rule
/// winning outright — a selection is a gesture the user is making right now.
void test_a_selection_still_inverts_a_highlighted_cell()
{
    Harness h;
    h.feed("an ERROR here");
    QuickHighlight back;
    back.pattern = QStringLiteral("ERROR");
    back.back = kGreen;
    h.apply({back});
    h.render();
    CHECK(h.filledWith(4, 0, kGreen));

    h.drag(h.px(0), h.py(0), h.px(12, 0.7), h.py(0));
    h.render();
    h.save(QStringLiteral("highlight-selected.png"));
    // Reversed, so the rule's background is now what the text is drawn in and
    // the cell behind it is not filled with it.
    CHECK(!h.filledWith(4, 0, kGreen));
}

/// A rule that only underlines spends no colour, and composes with one that
/// only colours.
void test_rules_compose_per_channel_in_list_order()
{
    Harness h;
    h.feed("an ERROR here");
    QuickHighlight marker;
    marker.pattern = QStringLiteral("ERROR");
    marker.style = TT_HIGHLIGHT_UNDERLINE;
    QuickHighlight colour;
    colour.pattern = QStringLiteral("ERROR");
    colour.back = kGreen;
    h.apply({marker, colour});
    h.render();
    h.save(QStringLiteral("highlight-composed.png"));

    // The second rule's background survived the first rule claiming the cell.
    CHECK(h.filledWith(4, 0, kGreen));
    // And the first rule's underline is drawn: a stroke under the baseline
    // that the unhighlighted neighbour does not have.
    const int cw = h.view.theme().cellWidth();
    const int uy = h.view.theme().baseline() + 1;
    CHECK(h.image.pixelColor(4 * cw + cw / 2, uy) != kGreen);
    CHECK(h.image.pixelColor(1 * cw + cw / 2, uy) == h.bgAt(1, 0));

    // ...and it did *not* drag in the configured underline colour pair. A
    // rule's underline is a mark, not an `SGR 4`: "underline this" must not
    // mean "and repaint it magenta".
    const QColor underlineColour(255, 0, 255);
    CHECK(!h.inkIs(4, 0, underlineColour));
}

/// The file round-trips, and a rule the engine refuses is reported rather than
/// silently doing nothing.
void test_the_file_round_trips()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = QDir(dir.path()).filePath(QStringLiteral("sterna.ini"));

    QVector<QuickHighlight> rules;
    QuickHighlight first = rule(QStringLiteral("\\b(ERROR|FATAL)\\b"));
    first.label = QStringLiteral("Errors");
    first.style = TT_HIGHLIGHT_BOLD;
    first.wholeLine = true;
    rules.append(first);
    QuickHighlight second;
    second.pattern = QStringLiteral("10.0.0.1");
    second.literal = true;
    second.ignoreCase = true;
    second.back = kGreen;
    second.group = 0;
    second.enabled = false;
    rules.append(second);

    QString error;
    CHECK(saveHighlights(path, rules, &error));
    CHECK(error.isEmpty());

    const QVector<QuickHighlight> read = loadHighlights(path);
    CHECK(read.size() == 2);
    if (read.size() == 2) {
        CHECK(read[0].label == QStringLiteral("Errors"));
        CHECK(read[0].pattern == first.pattern);
        CHECK(read[0].fore == kRed);
        // Absent is not black: the second rule leaves the foreground alone.
        CHECK(!read[1].fore.isValid());
        CHECK(read[1].back == kGreen);
        CHECK(read[1].literal && read[1].ignoreCase && !read[1].enabled);
        CHECK(read[0].wholeLine && !read[1].wholeLine);
    }

    // A pattern only a hand edit can produce.
    Session session(40, 6);
    QuickHighlight broken;
    broken.label = QStringLiteral("typo");
    broken.pattern = QStringLiteral("(unclosed");
    broken.fore = kRed;
    session.setHighlights({broken});
    CHECK(session.highlightProblems().contains(QStringLiteral("typo")));
    session.setHighlights({rule(QStringLiteral("fine"))});
    CHECK(session.highlightProblems().isEmpty());
}

/// The editor: it edits a copy, the pattern is checked as it is typed, and the
/// sample line is coloured by the engine that will do the real colouring.
void test_the_editor()
{
    QVector<QuickHighlight> rules;
    QuickHighlight existing = rule(QStringLiteral("ERROR"));
    existing.label = QStringLiteral("Errors");
    rules.append(existing);

    HighlightsDialog dialog(rules);
    dialog.adjustSize();

    auto *list = dialog.findChild<QListWidget *>(QStringLiteral("highlightList"));
    auto *pattern = dialog.findChild<QLineEdit *>(QStringLiteral("highlightPattern"));
    auto *error = dialog.findChild<QLabel *>(QStringLiteral("highlightPatternError"));
    auto *add = dialog.findChild<QPushButton *>(QStringLiteral("highlightAdd"));
    auto *literal = dialog.findChild<QCheckBox *>(QStringLiteral("highlightLiteral"));
    CHECK(list && pattern && error && add && literal);
    if (!list || !pattern || !error || !add || !literal) {
        return;
    }

    // The row that is shown is the row that looks selected.
    CHECK(list->count() == 1);
    CHECK(list->currentRow() == 0);
    CHECK(pattern->text() == QStringLiteral("ERROR"));
    CHECK(error->text().isEmpty());

    // A pattern the engine refuses says so, in the engine's own words, and is
    // not refused — somebody is still typing.
    pattern->setText(QStringLiteral("(unclosed"));
    CHECK(!error->text().isEmpty());
    // ...and marking it as plain text makes it legal again, because nothing in
    // it is a metacharacter any more.
    literal->setChecked(true);
    CHECK(error->text().isEmpty());
    literal->setChecked(false);
    pattern->setText(QStringLiteral("ERROR"));
    CHECK(error->text().isEmpty());

    // Add lands on a new row without disturbing the one that was there.
    add->click();
    CHECK(list->count() == 2);
    CHECK(list->currentRow() == 1);
    CHECK(pattern->text().isEmpty());
    CHECK(dialog.rules().size() == 2);
    CHECK(dialog.rules().at(0).pattern == QStringLiteral("ERROR"));

    // The sample box, coloured through the core.
    auto *sample = dialog.findChild<QPlainTextEdit *>(QStringLiteral("highlightSample"));
    auto *preview = dialog.findChild<QLabel *>(QStringLiteral("highlightPreview"));
    CHECK(sample && preview);
    if (sample && preview) {
        pattern->setText(QStringLiteral("ERROR"));
        sample->setPlainText(QStringLiteral("an ERROR here"));
        CHECK(preview->text().contains(QStringLiteral("<span")));
        CHECK(preview->text().contains(QStringLiteral("ERROR</span>")));
        // The text either side is still there, unwrapped.
        CHECK(preview->text().startsWith(QStringLiteral("an ")));
    }

    if (!writeDir.isEmpty()) {
        dialog.grab().save(QDir(writeDir).filePath(QStringLiteral("highlight-editor.png")));
    }
}

/// End to end: rules written to a settings file colour a real session's
/// output, and colour the scrollback a rule was written after.
void test_a_shell_session_is_highlighted()
{
    Session session(40, 6);
    TerminalView view(&session);
    view.resize(40 * view.theme().cellWidth(), 6 * view.theme().cellHeight());

    QString error;
    CHECK(session.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                              QStringLiteral("printf 'ERROR one\\r\\n'; sleep 0.2")},
                             &error));
    CHECK(error.isEmpty());
    CHECK(spin(
        [&] {
            size_t len = 0;
            const TtCell *row = session.row(0, &len);
            return row && len > 0 && row[0].text[0] == 'E';
        },
        5000));

    // The rule arrives after the text did, which is the whole point of
    // matching while painting.
    QuickHighlight back;
    back.pattern = QStringLiteral("ERROR");
    back.back = kGreen;
    session.setHighlights({back});

    const QImage image = view.grab().toImage();
    const int cw = view.theme().cellWidth();
    const int ch = view.theme().cellHeight();
    CHECK(image.pixelColor(2 * cw, ch - 1) == kGreen);
    if (!writeDir.isEmpty()) {
        image.save(QDir(writeDir).filePath(QStringLiteral("highlight-session.png")));
    }
    session.disconnectPort();
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

    test_a_rule_colours_what_it_matched();
    test_a_background_rule_fills_the_cells_it_matched();
    test_a_whole_line_rule_covers_the_line();
    test_a_match_across_a_wrap_is_coloured_on_both_rows();
    test_the_switches_stop_it();
    test_a_selection_still_inverts_a_highlighted_cell();
    test_rules_compose_per_channel_in_list_order();
    test_the_file_round_trips();
    test_the_editor();
    test_a_shell_session_is_highlighted();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("highlight ok\n");
    return 0;
}
