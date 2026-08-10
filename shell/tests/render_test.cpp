// What the painter actually puts on screen.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
// `QWidget::grab()` re-renders the widget offscreen, which is exactly the
// question here — and it is also the only screenshot that works in this
// environment. GNOME's screenshot D-Bus API has been locked down since 45,
// `QScreen::grabWindow(0)` is blank under xcb and null under Wayland, and
// neither would be testing our painting anyway.
//
// The assertions are on background fills rather than on glyph pixels.
// Backgrounds are solid rectangles whose colour is the whole output of
// `Theme::resolve`, so they pin the colour model exactly; glyph coverage
// depends on the font, hinting and antialiasing, and asserting on it would
// make this fail on a machine with different fonts installed. Text is checked
// only for "there is ink here and none there", which is font-independent and
// catches the failure that actually happens — a screen full of the right
// codepoints rendering blank.

#include <QApplication>
#include <QClipboard>
#include <QColor>
#include <QImage>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QPixmap>

#include <cstdio>
#include <cstring>

#include <QComboBox>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QFileInfo>
#include <QLineEdit>
#include <QStandardPaths>
#include <QTabWidget>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QThread>
#include <QTimer>

#include "MainWindow.h"
#include "PasteDialog.h"
#include "Session.h"
#include "SettingsDialog.h"
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

class TestTerminalView : public TerminalView {
public:
    using TerminalView::TerminalView;

    QString openedUrl;

protected:
    void openUrl(const QString &url) override { openedUrl = url; }
};

class ProductionTerminalView : public TerminalView {
public:
    using TerminalView::TerminalView;
    using TerminalView::openUrl;
};

struct Harness {
    Session session { 80, 24 };
    TestTerminalView view { &session };
    QImage image;

    Harness()
    {
        view.resize(80 * view.theme().cellWidth(), 24 * view.theme().cellHeight());
    }

    void feed(const char *bytes)
    {
        session.feed(QByteArray(bytes, static_cast<int>(strlen(bytes))));
    }

    void render() { image = view.grab().toImage(); }

    /// Give the view keyboard focus, which the cursor's appearance depends on.
    /// Works under the offscreen platform, which is why the cursor can be
    /// tested at all.
    void activate()
    {
        view.show();
        view.activateWindow();
        view.setFocus();
        qApp->processEvents();
    }

    /// A pixel `frac` of the way across a column — 0 is its left edge, and
    /// anything past 0.5 rounds the selection boundary to the next character.
    int px(int col, double frac = 0.0) const
    {
        return static_cast<int>((col + frac) * view.theme().cellWidth());
    }
    int py(int row, double frac = 0.5) const
    {
        return static_cast<int>((row + frac) * view.theme().cellHeight());
    }

    /// Post a mouse event the way a real one arrives, so the widget's own
    /// handlers run — a test that called them directly would not be testing
    /// the click counting, which is where the triple click lives.
    void mouse(QEvent::Type type, int x, int y)
    {
        const Qt::MouseButtons held =
            type == QEvent::MouseButtonRelease ? Qt::NoButton : Qt::LeftButton;
        QMouseEvent ev(type, QPointF(x, y), QPointF(x, y), Qt::LeftButton, held,
                       Qt::NoModifier);
        QCoreApplication::sendEvent(&view, &ev);
    }

    void hover(int x, int y)
    {
        QMouseEvent ev(QEvent::MouseMove, QPointF(x, y), QPointF(x, y),
                       Qt::NoButton, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(&view, &ev);
    }

    /// One notch of the wheel, posted the way a real one arrives. `notches`
    /// above 1 is the coalesced message a fast flick produces, which upstream
    /// deliberately does *not* multiply by `MouseWheelScrollLine`.
    void wheel(int notches, Qt::KeyboardModifiers mods = Qt::NoModifier)
    {
        const QPointF p(px(0), py(0));
        QWheelEvent ev(p, view.mapToGlobal(p), QPoint(), QPoint(0, 120 * notches),
                       Qt::NoButton, mods, Qt::NoScrollPhase, false);
        QCoreApplication::sendEvent(&view, &ev);
    }

    void drag(int fromX, int fromY, int toX, int toY)
    {
        mouse(QEvent::MouseButtonPress, fromX, fromY);
        mouse(QEvent::MouseMove, toX, toY);
        mouse(QEvent::MouseButtonRelease, toX, toY);
    }

    QString copied()
    {
        QApplication::clipboard()->clear(QClipboard::Clipboard);
        view.copySelection();
        return QApplication::clipboard()->text(QClipboard::Clipboard);
    }

    /// The middle of a cell. Unambiguous for a blank or a solid fill; for a
    /// cell with a glyph in it this lands on ink, which is what `bgAt` is for.
    QColor at(int col, int row) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        return image.pixelColor(col * cw + cw / 2, row * ch + ch / 2);
    }

    /// The bottom-left corner of a cell — below the baseline and left of where
    /// a glyph starts, so it reports the background even under text.
    QColor bgAt(int col, int row) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        return image.pixelColor(col * cw, row * ch + ch - 1);
    }

    /// How many pixels in a cell differ from its background — "is there a
    /// glyph here".
    int ink(int col, int row) const
    {
        const int cw = view.theme().cellWidth();
        const int ch = view.theme().cellHeight();
        const QColor bg = image.pixelColor(col * cw, row * ch + ch - 1);
        int n = 0;
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                if (image.pixelColor(col * cw + x, row * ch + y) != bg) {
                    n++;
                }
            }
        }
        return n;
    }

    QImage cell(int col, int row) const
    {
        return image.copy(col * view.theme().cellWidth(), row * view.theme().cellHeight(),
                          view.theme().cellWidth(), view.theme().cellHeight());
    }
};

/// Tera Term's shipping defaults, which is what the shell starts from.
const QColor kWhite(255, 255, 255);
const QColor kBlack(0, 0, 0);
const QColor kDarkRed(128, 0, 0);   // palette index 1
const QColor kBrightRed(255, 0, 0); // palette index 9
const QColor kBlue(0, 0, 255);      // VTBoldColor's foreground

void test_default_screen()
{
    Harness h;
    h.render();
    // Black on white, because that is what `VTColor` defaults to. Painting the
    // palette's index 0 for an uncoloured cell instead would give black on
    // black, which looks like a broken parser rather than a wrong default.
    CHECK(h.at(10, 10) == kWhite);
    CHECK(h.ink(10, 10) == 0);
}

void test_text_is_drawn()
{
    Harness h;
    h.feed("Hello");
    h.render();
    // The failure this guards is a screen holding all the right codepoints and
    // rendering entirely blank — which is what a wrong cell-text read looks
    // like, and it cost real time on the oracle for the same reason.
    CHECK(h.ink(0, 0) > 0);
    CHECK(h.ink(1, 0) > 0);
    CHECK(h.ink(6, 0) == 0);
    // The background behind plain text is still the configured one.
    CHECK(h.bgAt(0, 0) == kWhite);
}

void test_sgr_background_colours()
{
    Harness h;
    // `SGR 41` is palette index 1, which in Tera Term's palette is dark red —
    // the 16-colour table is ordered dim-then-bright after the permutation
    // `GetIndex256From16` applies.
    //
    // `SGR 101` — aixterm's bright red background — does **nothing**, because
    // `Aixterm16Color` ships off (`ttset.c:770`) and `vtterm.c:2435` gates the
    // whole 100-107 range on it. So the second cell keeps the pen `SGR 41`
    // set. It looks like a bug in the painter and is a default in the
    // terminal, which is exactly the kind of thing someone later "fixes".
    h.feed("\033[41m \033[101m \033[48;5;9m \033[0m");
    h.render();
    CHECK(h.at(0, 0) == kDarkRed);
    CHECK(h.at(1, 0) == kDarkRed);
    CHECK(h.at(2, 0) == kBrightRed);
    CHECK(h.at(3, 0) == kWhite);
}

void test_truecolor_resolves_through_upstreams_search()
{
    Harness h;
    // Pure red as a truecolor background. Upstream's nearest-colour search
    // flips bright and dim when a full-colour mode is on, and 256-colour ships
    // on — so this lands on index 1 and paints *dark* red. It looks wrong and
    // is not: the grid stores what Tera Term stores, and a renderer that
    // "corrected" it here would disagree with the differential suite.
    h.feed("\033[48;2;255;0;0m \033[0m");
    h.render();
    CHECK(h.at(0, 0) == kDarkRed);
}

void test_ansi_palette_changes_the_search_and_the_painter_together()
{
    Harness h;
    QString error;

    // The file uses the legacy order: 9 becomes drawing index 1. Index 0 does
    // not move, and gives the nearest-colour search a result whose final
    // bright/dim flip deliberately leaves it alone.
    CHECK(h.session.setSetting(QStringLiteral("color.ansi_palette"),
                               QStringLiteral("0,1,2,3,9,12,34,56"), &error));
    h.view.applySettings();
    h.feed("\033[41m \033[48;2;1;2;3m \033[0m");
    h.render();

    CHECK(h.bgAt(0, 0) == QColor(12, 34, 56));
    CHECK(h.bgAt(1, 0) == QColor(1, 2, 3));
}

void test_reverse_and_screen_reverse()
{
    Harness h;
    h.feed("\033[7m \033[0m");
    h.render();
    // Reverse with no SGR colour swaps the configured pair, so the background
    // becomes the normal foreground.
    CHECK(h.at(0, 0) == kBlack);
    CHECK(h.at(1, 0) == kWhite);

    // DECSCNM inverts the whole screen, including the cell that was already
    // reversed — two reverses is none.
    h.feed("\033[?5h");
    h.render();
    CHECK(h.at(0, 0) == kWhite);
    CHECK(h.at(1, 0) == kBlack);
}

/// The visual bell inverts the screen for `BeepVBellWait` and then puts it
/// back — and it does it by XORing DECSCNM's own flag, which is upstream's
/// mechanism rather than a colour of its own.
void test_a_visual_bell_inverts_the_screen_and_puts_it_back()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("bell.mode"), QStringLiteral("visual"), &error));
    // Long enough that the wait cannot expire between the feed and the grab.
    CHECK(h.session.setSetting(QStringLiteral("bell.visual_wait_ms"),
                               QStringLiteral("400"), &error));

    h.feed("\007");
    h.render();
    CHECK(h.bgAt(10, 10) == kBlack);

    // A screen the host has already reversed goes the *normal* way round for
    // the duration, because two inversions are none.
    h.feed("\033[?5h\007");
    h.render();
    CHECK(h.bgAt(10, 10) == kWhite);

    // The flash ends on a timer, so the wait has to run the event loop.
    QEventLoop loop;
    QTimer::singleShot(500, &loop, &QEventLoop::quit);
    loop.exec(QEventLoop::AllEvents);
    h.render();
    CHECK(h.bgAt(10, 10) == kBlack);

    // ...and with the bell off there is no flash at all.
    h.feed("\033[?5l");
    CHECK(h.session.setSetting(QStringLiteral("bell.mode"), QStringLiteral("off"), &error));
    h.feed("\007");
    h.render();
    CHECK(h.bgAt(10, 10) == kWhite);
}

void test_bold_has_its_own_colour()
{
    Harness h;
    // `VTBoldColor` defaults to blue and `EnableBoldAttrColor` ships on, so
    // bold text with no SGR colour is blue. This is Tera Term's look, not a
    // choice made here.
    h.feed("\033[1mB\033[0m");
    h.render();
    bool sawBlue = false;
    const int cw = h.view.theme().cellWidth();
    const int ch = h.view.theme().cellHeight();
    for (int y = 0; y < ch && !sawBlue; y++) {
        for (int x = 0; x < cw; x++) {
            const QColor c = h.image.pixelColor(x, y);
            if (c.blue() > c.red() + 40 && c.blue() > c.green() + 40) {
                sawBlue = true;
                break;
            }
        }
    }
    CHECK(sawBlue);
}

void test_a_wide_character_covers_two_cells()
{
    Harness h;
    // A background-coloured wide character. The lead cell carries the colour
    // and the pad carries *zeroed* attributes (`buffer.c:3400`), so a painter
    // that drew the pad from its own cell would leave a white half-block in
    // the middle of a coloured one.
    h.feed("\033[44m\xe5\x8c\x97\033[0m");
    h.render();
    // Sampled at the corners: the middle of both cells is inside the glyph.
    const QColor lead = h.bgAt(0, 0);
    const QColor pad = h.bgAt(1, 0);
    CHECK(lead == pad);
    CHECK(lead != kWhite);

    // And it stops there. Measured as a width rather than by sampling the
    // next cell, because a CJK glyph's ink can overhang its own advance by a
    // pixel — the fill is what has to be exactly two cells wide.
    const int cw = h.view.theme().cellWidth();
    const int ch = h.view.theme().cellHeight();
    int filled = 0;
    while (h.image.pixelColor(filled, ch - 1) == lead) {
        filled++;
    }
    CHECK(filled == 2 * cw);
}

void test_dec_special_graphics_draws_a_line()
{
    Harness h;
    // `ESC ( 0` then `q`. The grid stores the byte `q` with `TT_ATTR_SPECIAL`
    // rather than U+2500, because upstream's mapping direction defaults to
    // "do not map" — so the renderer has to do it, and a painter that did not
    // would draw a literal `q` here.
    h.feed("\033(0qqq\033(B q");
    h.render();

    // A horizontal line is ink on one band of rows and nothing above or below
    // it; the letter `q` is not. Compare the two: the line's ink sits in a
    // strictly narrower band of rows than the letter's.
    const int cw = h.view.theme().cellWidth();
    const int ch = h.view.theme().cellHeight();
    auto inkRows = [&](int col) {
        int rows = 0;
        for (int y = 0; y < ch; y++) {
            for (int x = 0; x < cw; x++) {
                if (h.image.pixelColor(col * cw + x, y) != kWhite) {
                    rows++;
                    break;
                }
            }
        }
        return rows;
    };
    const int lineRows = inkRows(0);
    const int letterRows = inkRows(4);
    CHECK(lineRows > 0);
    CHECK(letterRows > 0);
    CHECK(lineRows < letterRows);
}

void test_the_cursor_is_drawn_where_the_core_says()
{
    Harness h;
    h.activate();
    h.feed("\033[5;10H");
    h.render();
    // A focused cursor is a filled block, so the cell it is on stops being the
    // background colour while its neighbour does not. Row 5 column 10 in the
    // escape sequence is cell (9, 4) here, which is the off-by-one worth
    // pinning.
    CHECK(h.at(9, 4) == kBlack);
    CHECK(h.at(8, 4) == kWhite);

    // DECTCEM hides it, and the cell goes back to normal.
    h.feed("\033[?25l");
    h.render();
    CHECK(h.at(9, 4) == kWhite);
}

void test_an_unfocused_cursor_is_hollow()
{
    // The convention every terminal uses to say "typing goes somewhere else",
    // and the state a window spends most of its time in.
    Harness h;
    h.feed("\033[5;10H");
    h.render();
    CHECK(h.at(9, 4) == kWhite);  // not filled
    CHECK(h.ink(9, 4) > 0);       // but outlined
    CHECK(h.ink(8, 4) == 0);
}

/// Feed `n` lines, each a single space on its own background colour, so a
/// rendered row says exactly which line of history it is.
void feedNumberedLines(Harness &h, int n)
{
    QByteArray out;
    for (int i = 0; i < n; i++) {
        out += QByteArray("\033[48;5;") + QByteArray::number(16 + i) + "m \033[0m\r\n";
    }
    h.session.feed(out);
}

QColor paletteColour(int index)
{
    uint8_t r = 0, g = 0, b = 0;
    tt_palette_rgb(static_cast<uint32_t>(index), &r, &g, &b);
    return QColor(r, g, b);
}

void test_scrolling_back_paints_the_history()
{
    Harness h;
    feedNumberedLines(h, 60);
    const int history = h.session.scrollbackLen();
    CHECK(history > 0);

    // Live: the top visible row is the first line still on the page.
    h.render();
    CHECK(h.bgAt(0, 0) == paletteColour(16 + history));

    // Scrolled back by five, it is five lines earlier — and the painter reads
    // it through the same `row()` it uses for the live screen, which is why
    // there is no second code path to get wrong.
    h.view.setViewOffset(5);
    h.render();
    CHECK(h.session.viewOffset() == 5);
    CHECK(h.bgAt(0, 0) == paletteColour(16 + history - 5));

    // All the way back is the oldest line retained.
    h.view.setViewOffset(1 << 20);
    h.render();
    CHECK(h.session.viewOffset() == history);
    CHECK(h.bgAt(0, 0) == paletteColour(16 + 0));
}

void test_the_cursor_is_not_painted_onto_the_history()
{
    Harness h;
    h.activate();
    feedNumberedLines(h, 60);
    h.render();
    const int cursorRow = h.session.cursorViewRow();
    CHECK(cursorRow >= 0);
    // A focused cursor is a filled block, so the cell reads as the foreground.
    CHECK(h.at(0, cursorRow) == kBlack);

    // Scroll back past the whole screen and the cursor has no row to be on.
    // Painting `TtCursor::y` regardless would stamp a block onto a line of
    // history, which looks like a prompt that is not there.
    h.view.setViewOffset(30);
    h.render();
    CHECK(h.session.cursorViewRow() < 0);
    CHECK(h.at(0, cursorRow) != kBlack);
}

void test_output_does_not_move_a_scrolled_back_view()
{
    // The same claim `tt-session` tests, but through the painter — because
    // this is the one the user sees, and a frontend that re-read the offset
    // wrongly would undo it without any core test noticing.
    //
    // It needs the key: `AutoScrollOnlyInBottomLine` ships off, and a terminal
    // with the shipped default is dragged back to the cursor instead.
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("window.auto_scroll_only_at_bottom"),
                               QStringLiteral("on"), &error));
    feedNumberedLines(h, 60);
    h.view.setViewOffset(5);
    h.render();
    const QColor held = h.bgAt(0, 0);

    feedNumberedLines(h, 3);
    h.render();
    CHECK(h.bgAt(0, 0) == held);
}

void test_the_wheel_scrolls_by_the_setting()
{
    Harness h;
    feedNumberedLines(h, 60);
    CHECK(h.session.viewOffset() == 0);

    // The shipped `MouseWheelScrollLine` is 3, and this is the whole reason
    // the setting exists: `QApplication::wheelScrollLines()` is the desktop's
    // answer to a different question and used to be what this read.
    h.wheel(1);
    CHECK(h.session.viewOffset() == 3);

    // A coalesced flick is *not* multiplied (`vtwin.cpp:2539` tests
    // `line == 1`), so two notches in one message move two lines rather than
    // six. Upstream's quirk, reproduced.
    h.wheel(2);
    CHECK(h.session.viewOffset() == 5);

    QString error;
    CHECK(h.session.setSetting(QStringLiteral("mouse.wheel_scroll_line"),
                               QStringLiteral("10"), &error));
    h.view.applySettings();
    h.wheel(-1);
    CHECK(h.session.viewOffset() == 0);
}

void test_the_wheel_goes_to_the_host_when_it_asked_for_it()
{
    Harness h;
    feedNumberedLines(h, 60);
    // `DECSET 7786` plus the application cursor mode is upstream's
    // `WheelToCursorMode`, and it turns the wheel into cursor keys — which is
    // how it scrolls inside a pager that has no mouse support.
    h.feed("\x1b[?7786h\x1b[?1h");
    h.wheel(1);
    CHECK(h.session.viewOffset() == 0);

    // Ctrl is the way back to the terminal's own history, and it is a setting
    // rather than a convention.
    h.wheel(1, Qt::ControlModifier);
    CHECK(h.session.viewOffset() == 3);

    QString error;
    CHECK(h.session.setSetting(QStringLiteral("mouse.ctrl_disables_wheel_to_cursor"),
                               QStringLiteral("off"), &error));
    h.wheel(1, Qt::ControlModifier);
    CHECK(h.session.viewOffset() == 3);
}

// --- selection ---------------------------------------------------------------
//
// A selected cell is drawn in reverse, so on the default white background a
// highlighted column reads black at its corner. That is the whole assertion
// for *where* the selection is; `copied()` is the assertion for *what* it is.

void test_a_drag_selects_the_characters_it_covers()
{
    Harness h;
    h.feed("hello world");

    // From the left edge of `h` to past the middle of `o`. The endpoint is the
    // nearest boundary between characters, which is what makes this select
    // `hello` rather than `hell` — upstream rounds the same way.
    h.drag(h.px(0), h.py(0), h.px(4, 0.7), h.py(0));
    h.render();
    CHECK(h.bgAt(0, 0) == kBlack);
    CHECK(h.bgAt(4, 0) == kBlack);
    CHECK(h.bgAt(5, 0) == kWhite);
    CHECK(h.copied() == QStringLiteral("hello"));

    // Stopping short of the middle of `o` leaves it out.
    h.drag(h.px(0), h.py(0), h.px(4, 0.2), h.py(0));
    h.render();
    CHECK(h.bgAt(4, 0) == kWhite);
    CHECK(h.copied() == QStringLiteral("hell"));

    // A click on its own selects nothing, and clears what there was.
    h.mouse(QEvent::MouseButtonPress, h.px(2), h.py(0));
    h.mouse(QEvent::MouseButtonRelease, h.px(2), h.py(0));
    h.render();
    CHECK(!h.view.hasSelection());
    CHECK(h.bgAt(0, 0) == kWhite);
}

void test_a_double_click_selects_a_word()
{
    Harness h;
    h.feed("foo bar_baz-qux");

    // Underscore is not a delimiter and hyphen is — upstream's `DelimList`
    // default, which is what makes a path or a flag break at the hyphen.
    h.mouse(QEvent::MouseButtonPress, h.px(5), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(5), h.py(0));
    h.render();
    CHECK(h.copied() == QStringLiteral("bar_baz"));
    CHECK(h.bgAt(3, 0) == kWhite);
    CHECK(h.bgAt(4, 0) == kBlack);
    CHECK(h.bgAt(10, 0) == kBlack);
    CHECK(h.bgAt(11, 0) == kWhite);

    // Dragging *leftwards* out of a double-clicked word keeps that word's far
    // edge, which is why the anchor is the whole unit rather than the point
    // the drag began at.
    h.mouse(QEvent::MouseMove, h.px(0), h.py(0));
    CHECK(h.copied() == QStringLiteral("foo bar_baz"));
    h.mouse(QEvent::MouseButtonRelease, h.px(0), h.py(0));

    // A double click on a delimiter takes the run of *that* character, so the
    // gap between two columns of output selects as the gap.
    h.mouse(QEvent::MouseButtonPress, h.px(3), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(3), h.py(0));
    CHECK(h.copied() == QString());  // one space, trimmed as trailing padding
    CHECK(h.view.hasSelection());
    h.render();
    CHECK(h.bgAt(3, 0) == kBlack);
    CHECK(h.bgAt(2, 0) == kWhite);

    // A change of character width is itself a break — upstream's `DelimDBCS`,
    // on by default — and a wide character is taken whole from either half.
    h.feed("\033[2J\033[Habc\xe5\x8c\x97\xe4\xba\xac""def");
    for (int col : {3, 4}) {
        h.mouse(QEvent::MouseButtonPress, h.px(col), h.py(0));
        h.mouse(QEvent::MouseButtonDblClick, h.px(col), h.py(0));
        h.mouse(QEvent::MouseButtonRelease, h.px(col), h.py(0));
        CHECK(h.copied() == QString::fromUtf8("\xe5\x8c\x97\xe4\xba\xac"));
    }
    h.mouse(QEvent::MouseButtonPress, h.px(8), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(8), h.py(0));
    CHECK(h.copied() == QStringLiteral("def"));
}

void test_clickable_url_controls_only_the_cursor_and_launch()
{
    Harness h;
    h.feed("x http://example.test end");

    // Recognition, colour and underline happen with the setting off, but the
    // cursor and double-click stay those of ordinary text.
    h.hover(h.px(4), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::IBeamCursor);

    QString error;
    CHECK(h.session.setSetting(QStringLiteral("mouse.clickable_url"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.hover(h.px(4), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::PointingHandCursor);
    h.hover(h.px(0), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::IBeamCursor);

    h.mouse(QEvent::MouseButtonPress, h.px(4), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(4), h.py(0));
    CHECK(h.view.openedUrl == QStringLiteral("http://example.test"));
    CHECK(!h.view.hasSelection());
}

void test_configured_browser_receives_its_arguments_before_the_url()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString output = dir.filePath(QStringLiteral("browser-args"));

    Session session { 80, 24 };
    ProductionTerminalView view { &session };
    QString error;
    CHECK(session.setSetting(QStringLiteral("url.browser"),
                             QCoreApplication::applicationFilePath(), &error));
    CHECK(session.setSetting(
        QStringLiteral("url.browser_args"),
        QStringLiteral("--url-helper \"%1\" --flag \"two words\"").arg(output),
        &error));
    view.applySettings();
    view.openUrl(QStringLiteral("http://example.test"));

    QElapsedTimer wait;
    wait.start();
    while ((!QFileInfo::exists(output) || QFileInfo(output).size() == 0) &&
           wait.elapsed() < 2000) {
        qApp->processEvents();
        QThread::msleep(10);
    }
    QFile file(output);
    CHECK(file.open(QIODevice::ReadOnly));
    CHECK(file.readAll() == QByteArray("--flag\ntwo words\nhttp://example.test\n"));
}

/// `DelimList` is a setting, and it is stored in `Hex2StrW`'s escape — so this
/// is as much about the decoding as about the wiring. A list written the way a
/// user would write it puts a `$20` at the front for the space.
void test_the_delimiter_list_is_the_setting_and_not_a_constant()
{
    Harness h;
    QString error;
    // Underscore in, hyphen out — the opposite of upstream's default, and the
    // thing somebody editing this setting is usually after.
    CHECK(h.session.setSetting(QStringLiteral("keyboard.word_delimiters"),
                               QStringLiteral("$20_"), &error));
    h.view.applySettings();

    h.feed("foo bar_baz-qux");
    h.mouse(QEvent::MouseButtonPress, h.px(9), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(9), h.py(0));
    CHECK(h.copied() == QStringLiteral("baz-qux"));
    h.mouse(QEvent::MouseButtonRelease, h.px(9), h.py(0));

    // The `$20` really is a space: without the decoding the list would be the
    // four characters `$`, `2`, `0` and `_`, and the space would join.
    h.mouse(QEvent::MouseButtonPress, h.px(1), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(1), h.py(0));
    CHECK(h.copied() == QStringLiteral("foo"));
}

void test_a_triple_click_selects_the_line()
{
    Harness h;
    h.feed("first line\r\nsecond line");

    // Qt has no triple-click event: the third press arrives as an ordinary one
    // and the run is counted in the widget, so this is what proves the count.
    h.mouse(QEvent::MouseButtonPress, h.px(3), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(3), h.py(0));
    h.mouse(QEvent::MouseButtonRelease, h.px(3), h.py(0));
    h.mouse(QEvent::MouseButtonPress, h.px(3), h.py(0));
    h.render();
    CHECK(h.copied() == QStringLiteral("first line"));
    CHECK(h.bgAt(0, 0) == kBlack);
    CHECK(h.bgAt(79, 0) == kBlack);
    CHECK(h.bgAt(0, 1) == kWhite);

    // And it extends by whole lines.
    h.mouse(QEvent::MouseMove, h.px(3), h.py(1));
    h.mouse(QEvent::MouseButtonRelease, h.px(3), h.py(1));
    CHECK(h.copied() == QStringLiteral("first line\nsecond line"));
}

/// `n` lines of ordinary text, so a copy has something in it to compare.
void feedTextLines(Harness &h, int n)
{
    QByteArray out;
    for (int i = 0; i < n; i++) {
        out += "line" + QByteArray::number(i) + "\r\n";
    }
    h.session.feed(out);
}

void test_a_selection_holds_on_to_its_text_not_its_place()
{
    // The reason the core numbers lines at all. Held as rows, a selection
    // stays put while the text walks up the screen underneath it — which is
    // precisely the case someone copies in: a line off a device that is still
    // printing.
    Harness h;
    h.feed("keep me\r\n");
    h.drag(h.px(0), h.py(0), h.px(6, 0.7), h.py(0));
    CHECK(h.copied() == QStringLiteral("keep me"));
    h.render();
    CHECK(h.bgAt(0, 0) == kBlack);

    // Thirty lines through a 24-row screen: the selected line is now up in the
    // history and nothing on screen should be highlighted.
    feedTextLines(h, 30);
    h.render();
    CHECK(h.copied() == QStringLiteral("keep me"));
    for (int y = 0; y < 24; y++) {
        // Except the cursor's row: unfocused, it is an outline, and the corner
        // `bgAt` samples is on it.
        if (y != h.session.cursorViewRow()) {
            CHECK(h.bgAt(0, y) == kWhite);
        }
    }

    // Scroll back to it — `topLine` is how far, since the selected line is the
    // first one the terminal ever showed — and the highlight is on it, not on
    // where it used to be.
    h.view.setViewOffset(static_cast<int>(h.session.topLine()));
    h.render();
    CHECK(h.bgAt(0, 0) == kBlack);
    CHECK(h.bgAt(0, 1) == kWhite);
}

void test_a_selection_survives_scrolling_back()
{
    Harness h;
    feedTextLines(h, 40);
    h.view.setViewOffset(10);
    h.drag(h.px(0), h.py(2), h.px(5, 0.7), h.py(2));
    const QString text = h.copied();
    CHECK(text.startsWith(QStringLiteral("line")));

    // Live, the selected line is not on screen at all — and the copy is still
    // the line that was selected rather than whatever now sits in that row.
    h.view.setViewOffset(0);
    CHECK(h.copied() == text);
    h.render();
    CHECK(h.bgAt(0, 2) == kWhite);

    h.view.setViewOffset(10);
    CHECK(h.copied() == text);
    h.render();
    CHECK(h.bgAt(0, 2) == kBlack);
}

void test_dragging_off_the_edge_scrolls_the_view()
{
    Harness h;
    feedNumberedLines(h, 60);
    h.view.setViewOffset(20);
    const int before = h.session.viewOffset();

    // Held above the top of the widget, a drag has to keep going or a
    // selection can never be longer than one screen.
    h.mouse(QEvent::MouseButtonPress, h.px(0), h.py(0));
    h.mouse(QEvent::MouseMove, h.px(0), -5);
    CHECK(h.session.viewOffset() == before + 1);
    h.mouse(QEvent::MouseButtonRelease, h.px(0), -5);

    // ...and it stops when the button comes up, rather than scrolling on.
    const int after = h.session.viewOffset();
    QCoreApplication::processEvents();
    CHECK(h.session.viewOffset() == after);
}

/// `EnableContinuedLineCopy` — a row the terminal wrapped onto is the same
/// line as the one above it, and copying the pair puts no break between them.
void test_continued_line_copy_joins_a_wrapped_line()
{
    Harness h;
    QString error;
    // 100 characters across an 80-column screen, so the terminal breaks it and
    // the host never did.
    const QString wide(100, QLatin1Char('x'));
    h.feed(wide.toLatin1().constData());

    // Off, which is how it ships: two rows, two lines.
    h.drag(h.px(0), h.py(0), h.px(19, 0.7), h.py(1));
    CHECK(h.copied() == wide.left(80) + QLatin1Char('\n') + wide.left(20));

    CHECK(h.session.setSetting(QStringLiteral("clipboard.continued_line_copy"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    CHECK(h.copied() == wide);

    // A break the *host* sent is still a break, which is the distinction the
    // attribute exists to make.
    Harness g;
    CHECK(g.session.setSetting(QStringLiteral("clipboard.continued_line_copy"),
                               QStringLiteral("on"), &error));
    g.view.applySettings();
    g.feed("one\r\ntwo");
    g.drag(g.px(0), g.py(0), g.px(3, 0.7), g.py(1));
    CHECK(g.copied() == QStringLiteral("one\ntwo"));
}

/// `SelectOnlyByLButton` and `AutoTextCopy`, which are one condition upstream:
/// with the first on, a middle or right button coming up over a standing
/// selection must not copy it (`vtwin.cpp:819`).
void test_the_other_buttons_do_not_start_or_copy_a_selection()
{
    Harness h;
    h.feed("hello world");
    // The right button *pastes* on the way up, which is upstream's default and
    // is not what is being tested here — and a clipboard holding a line break
    // would raise the confirmation dialog and park this suite on it for ever.
    QApplication::clipboard()->clear(QClipboard::Clipboard);

    // A right-button drag selects nothing at all, because it never started.
    const auto rightDrag = [&h](int fromX, int toX) {
        for (auto type : { QEvent::MouseButtonPress, QEvent::MouseMove,
                           QEvent::MouseButtonRelease }) {
            const int x = type == QEvent::MouseButtonPress ? fromX : toX;
            const Qt::MouseButtons held =
                type == QEvent::MouseButtonRelease ? Qt::NoButton : Qt::RightButton;
            QMouseEvent ev(type, QPointF(x, h.py(0)), QPointF(x, h.py(0)),
                           Qt::RightButton, held, Qt::NoModifier);
            QCoreApplication::sendEvent(&h.view, &ev);
        }
    };
    rightDrag(h.px(0), h.px(4, 0.7));
    CHECK(!h.view.hasSelection());

    // With the setting off it selects like the left one.
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("clipboard.select_only_by_lbutton"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    rightDrag(h.px(0), h.px(4, 0.7));
    CHECK(h.view.hasSelection());
    CHECK(h.copied() == QStringLiteral("hello"));
}

/// `ConfirmChangePaste`, which ships **on**: a paste holding a line break is
/// shown before it is sent, because the host cannot tell a pasted newline from
/// a typed one and a shell runs every line of it.
void test_a_paste_with_a_line_break_is_confirmed()
{
    CHECK(!PasteDialog::shouldConfirm(QStringLiteral("one word"), QString()));
    CHECK(PasteDialog::shouldConfirm(QStringLiteral("two\nlines"), QString()));
    CHECK(PasteDialog::shouldConfirm(QStringLiteral("cmd\r"), QString()));

    // And the dictionary, which is how a site adds a string of its own to the
    // list. A substring match, one needle per line.
    QTemporaryFile dict;
    CHECK(dict.open());
    dict.write("rm -rf\nshutdown\n");
    dict.flush();
    CHECK(PasteDialog::shouldConfirm(QStringLiteral("sudo rm -rf /tmp/x"), dict.fileName()));
    CHECK(!PasteDialog::shouldConfirm(QStringLiteral("ls -l"), dict.fileName()));
    // A path that is not there is no dictionary rather than an error, which is
    // upstream's `LoadFileWW` returning NULL.
    CHECK(!PasteDialog::shouldConfirm(QStringLiteral("rm -rf /"),
                                      QStringLiteral("/nonexistent/dict.txt")));

    // The dialog opens at the size the settings hold, which is the whole
    // reason `PasteDialogSize` is a setting: upstream writes it back.
    PasteDialog dialog(QStringLiteral("two\nlines"), QSize(400, 300));
    dialog.adjustSize();
    dialog.resize(400, 300);
    CHECK(dialog.text() == QStringLiteral("two\nlines"));
    CHECK(dialog.size() == QSize(400, 300));
}




/// The colour settings reach the painter, which is the whole point of wiring
/// them: `VTColor` is what makes Tera Term black on white, and a user who
/// wants a dark terminal changes exactly that.
void test_settings_change_the_painted_colours()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("color.normal"),
                               QStringLiteral("200,200,200,20,20,20"), &error));
    h.view.applySettings();
    h.feed("dark");
    h.render();
    CHECK(h.bgAt(10, 10) == QColor(20, 20, 20));
    CHECK(h.ink(0, 0) > 0);

    // And an attribute's own pair, which is a separate setting and a separate
    // priority arm rather than a shade of the normal one.
    CHECK(h.session.setSetting(QStringLiteral("color.bold"),
                               QStringLiteral("0,255,0,20,20,20"), &error));
    h.view.applySettings();
    h.feed("\033[2J\033[H\033[1mbold\033[0m");
    h.render();
    CHECK(h.at(0, 0) == QColor(0, 255, 0) || h.ink(0, 0) > 0);
    CHECK(h.bgAt(0, 0) == QColor(20, 20, 20));

    // Switching the attribute's colour off drops it back to the normal pair,
    // because upstream composes the attribute with its enable flag before
    // looking at anything — it does not paint a disabled-looking bold.
    CHECK(h.session.setSetting(QStringLiteral("color.bold_enabled"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.bgAt(0, 0) == QColor(20, 20, 20));
    CHECK(h.at(40, 0) == QColor(20, 20, 20));
}

void test_url_colour_and_underline_are_independent()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("color.url"),
                               QStringLiteral("1,2,3,10,20,30"), &error));
    h.view.applySettings();
    h.feed("h http://");
    h.render();

    // The URL pair participates after ordinary underline in the same priority
    // chain as upstream. A solid background makes the assertion independent
    // of font rasterisation.
    CHECK(h.bgAt(2, 0) == QColor(10, 20, 30));
    CHECK(h.bgAt(0, 0) == kWhite);

    // URLUnderline is a font switch of its own. Turn the colour arm off first
    // so the two identical `h` glyphs differ only by that line.
    CHECK(h.session.setSetting(QStringLiteral("color.url_enabled"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.cell(0, 0) != h.cell(2, 0));

    CHECK(h.session.setSetting(QStringLiteral("color.url_underline"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.cell(0, 0) == h.cell(2, 0));
}

void test_the_font_attribute_switches_are_independent_of_the_colours()
{
    Harness h;
    QString error;

    // Make the attribute colour arms disappear so the only difference between
    // each pair of cells is the face itself.
    CHECK(h.session.setSetting(QStringLiteral("color.bold_enabled"),
                               QStringLiteral("off"), &error));
    CHECK(h.session.setSetting(QStringLiteral("color.underline_enabled"),
                               QStringLiteral("off"), &error));
    CHECK(h.session.setSetting(QStringLiteral("color.bold_font"),
                               QStringLiteral("off"), &error));
    CHECK(h.session.setSetting(QStringLiteral("color.underline_font"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.feed("A \033[1mA\033[0m B \033[4mB\033[0m");
    h.render();

    // SGR 1 and SGR 4 remain in the cells; these settings gate only the font
    // chosen by the painter. With both gates off, identical glyphs are
    // pixel-identical to their unadorned neighbours.
    CHECK(h.cell(0, 0) == h.cell(2, 0));
    CHECK(h.cell(4, 0) == h.cell(6, 0));
}

void test_attribute_colours_can_keep_the_normal_background()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("color.bold"),
                               QStringLiteral("0,0,255,255,0,0"), &error));
    h.view.applySettings();
    h.feed("\033[1m \033[0m");
    h.render();
    CHECK(h.bgAt(0, 0) == QColor(255, 0, 0));

    CHECK(h.session.setSetting(QStringLiteral("color.use_normal_background"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.bgAt(0, 0) == kWhite);
}

void test_use_text_colour_repairs_only_the_three_same_colour_pairs()
{
    Harness h;
    QString error;
    h.feed("\033[30;40m \033[31;41m \033[0m");
    h.render();
    CHECK(h.bgAt(0, 0) == kBlack);
    CHECK(h.bgAt(1, 0) == kDarkRed);

    CHECK(h.session.setSetting(QStringLiteral("color.use_text_color"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.bgAt(0, 0) == kWhite);
    CHECK(h.bgAt(1, 0) == kDarkRed);

    // The reverse arm uses the configured reverse pair even while the normal
    // reverse-colour gate is off. This is upstream's ordering: UseTextColor is
    // applied after the ordinary attribute-colour decision.
    CHECK(h.session.setSetting(QStringLiteral("color.reverse"),
                               QStringLiteral("10,20,30,40,50,60"), &error));
    CHECK(h.session.setSetting(QStringLiteral("color.reverse_enabled"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.feed("\r\033[30;40;7m \033[0m");
    h.render();
    CHECK(h.bgAt(0, 0) == QColor(40, 50, 60));
}

/// The dialog holds no list of settings: it walks the core's metadata table,
/// so this checks that every row in the table became a widget and that what
/// comes back out is the file's own spelling.
void test_the_settings_dialog_is_built_from_the_schema()
{
    Harness h;
    SettingsDialog dialog(&h.session);

    const size_t fields = tt_settings_field_count();
    CHECK(fields > 0);
    CHECK(dialog.findChildren<QTabWidget *>().size() == 1);

    // A tab per page, in the schema's own order.
    auto *tabs = dialog.findChild<QTabWidget *>();
    CHECK(tabs != nullptr);
    if (tabs) {
        CHECK(tabs->count() > 1);
        CHECK(tabs->tabText(0) == QStringLiteral("Terminal"));
    }

    // A combo box carries the INI's spellings, both ways — `TerminalID` is
    // compared case-sensitively upstream, so a prettified label would read
    // back as the default and silently make a VT320 a VT100.
    QComboBox *termId = nullptr;
    for (QComboBox *combo : dialog.findChildren<QComboBox *>()) {
        if (combo->findText(QStringLiteral("VT525")) >= 0) {
            termId = combo;
        }
    }
    CHECK(termId != nullptr);
    if (termId) {
        CHECK(termId->currentText() == QStringLiteral("VT100"));
        termId->setCurrentIndex(termId->findText(QStringLiteral("VT320")));
        dialog.applyChanges();
        CHECK(h.session.setting(QStringLiteral("terminal.id"))
              == QStringLiteral("VT320"));
    }

    // The search box is what makes 600 settings navigable, and it filters
    // across every tab rather than the visible one.
    auto *search = dialog.findChild<QLineEdit *>();
    CHECK(search != nullptr);
    if (search && termId) {
        search->setText(QStringLiteral("backspace"));
        CHECK(!termId->isVisibleTo(&dialog));
        search->setText(QString());
        CHECK(termId->isVisibleTo(&dialog));
    }
}

/// Only what changed. A dialog that wrote every field would pin all of them
/// into the user's file the first time it was opened, and a pinned setting
/// stops following upstream's default for ever.
void test_the_dialog_writes_only_what_changed()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("terminal.title"),
                               QStringLiteral("before"), &error));
    SettingsDialog dialog(&h.session);
    // Nothing touched, so applying is a no-op even though every row has a
    // value to hand.
    dialog.applyChanges();
    CHECK(h.session.setting(QStringLiteral("terminal.title"))
          == QStringLiteral("before"));
}


/// A configured terminal size has to be the size the window *opens* at, not a
/// resize the user watches happen — which means it reaches `sizeHint` before
/// the first layout rather than a `resize()` afterwards.
///
/// Test mode moves `settingsPath()` into `~/.qttest`, so this neither reads
/// nor writes the developer's own settings.
void test_the_window_opens_at_the_configured_size()
{
    QStandardPaths::setTestModeEnabled(true);
    const QString path = MainWindow::settingsPath();
    QDir().mkpath(QFileInfo(path).absolutePath());
    {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        file.write("[Tera Term]\r\nTerminalSize=100,30\r\nVTColor=200,200,200,20,20,20\r\n");
    }

    {
        MainWindow window;
        // Straight out of the file, before anything has been laid out.
        CHECK(window.session()->cols() == 100);
        CHECK(window.session()->rows() == 30);

        // Qt caps a window's *initial* size at two thirds of the screen
        // (`QWidgetPrivate::adjustedSize`), which on the 800x800 offscreen
        // screen is smaller than 100x30 cells — and then the terminal follows
        // the window, as it does whenever a user drags an edge. So this asks
        // for the size a big-enough screen would have given and checks that
        // the layout lands exactly on the configured grid.
        window.resize(window.sizeHint());
        window.show();
        qApp->processEvents();
        CHECK(window.session()->cols() == 100);
        CHECK(window.session()->rows() == 30);
    }

    QFile::remove(path);
    QStandardPaths::setTestModeEnabled(false);
}

} // namespace

int main(int argc, char **argv)
{
    // The configured-browser test launches this executable detached. Handle
    // that tiny mode before constructing Qt so it is also valid on a machine
    // with no display server at all.
    if (argc >= 3 && strcmp(argv[1], "--url-helper") == 0) {
        QFile file(QString::fromUtf8(argv[2]));
        if (!file.open(QIODevice::WriteOnly)) {
            return 2;
        }
        for (int i = 3; i < argc; i++) {
            file.write(argv[i]);
            file.write("\n");
        }
        return 0;
    }

    // Offscreen so this runs in CI, where there is no display. `grab()`
    // renders through the raster paint engine either way, so the pixels are
    // the same ones a real window would show.
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORM")) {
        qputenv("QT_QPA_PLATFORM", "offscreen");
    }
    QApplication app(argc, argv);

    test_default_screen();
    test_text_is_drawn();
    test_sgr_background_colours();
    test_truecolor_resolves_through_upstreams_search();
    test_ansi_palette_changes_the_search_and_the_painter_together();
    test_reverse_and_screen_reverse();
    test_a_visual_bell_inverts_the_screen_and_puts_it_back();
    test_bold_has_its_own_colour();
    test_a_wide_character_covers_two_cells();
    test_dec_special_graphics_draws_a_line();
    test_the_cursor_is_drawn_where_the_core_says();
    test_an_unfocused_cursor_is_hollow();
    test_scrolling_back_paints_the_history();
    test_the_cursor_is_not_painted_onto_the_history();
    test_output_does_not_move_a_scrolled_back_view();
    test_the_wheel_scrolls_by_the_setting();
    test_the_wheel_goes_to_the_host_when_it_asked_for_it();
    test_a_drag_selects_the_characters_it_covers();
    test_a_double_click_selects_a_word();
    test_clickable_url_controls_only_the_cursor_and_launch();
    test_configured_browser_receives_its_arguments_before_the_url();
    test_the_delimiter_list_is_the_setting_and_not_a_constant();
    test_a_triple_click_selects_the_line();
    test_a_selection_holds_on_to_its_text_not_its_place();
    test_a_selection_survives_scrolling_back();
    test_continued_line_copy_joins_a_wrapped_line();
    test_the_other_buttons_do_not_start_or_copy_a_selection();
    test_a_paste_with_a_line_break_is_confirmed();
    test_dragging_off_the_edge_scrolls_the_view();
    test_settings_change_the_painted_colours();
    test_url_colour_and_underline_are_independent();
    test_the_font_attribute_switches_are_independent_of_the_colours();
    test_attribute_colours_can_keep_the_normal_background();
    test_use_text_colour_repairs_only_the_three_same_colour_pairs();
    test_the_settings_dialog_is_built_from_the_schema();
    test_the_dialog_writes_only_what_changed();
    test_the_window_opens_at_the_configured_size();

    // `--write <dir>` dumps what was rendered, for looking at a failure rather
    // than guessing at it.
    for (int i = 1; i + 1 < argc; i++) {
        if (strcmp(argv[i], "--write") == 0) {
            Harness h;
            h.feed("\033[2J\033[H\033[1;32msterna\033[0m on \033[31mserial\033[0m\r\n"
                   "\033[4munderline\033[0m \033[7mreverse\033[0m \033[44;93mcolour\033[0m\r\n"
                   "\033(0lqqqk\033(B box  \xe5\x8c\x97\xe4\xba\xac wide  e\xcc\x81 combining\r\n");
            h.render();
            const QString dir = QString::fromUtf8(argv[i + 1]);
            const QString path = dir + "/screen.png";
            h.image.save(path);
            printf("wrote %s\n", qPrintable(path));

            // The setup dialog too, because its layout is the part of it no
            // assertion can judge — what a generated dialog *looks* like is
            // exactly the question the schema approach has to answer.
            SettingsDialog dialog(&h.session);
            dialog.adjustSize();
            const QString dialogPath = dir + "/settings.png";
            dialog.grab().save(dialogPath);
            printf("wrote %s\n", qPrintable(dialogPath));
        }
    }

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("render ok\n");
    return 0;
}
