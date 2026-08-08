// What the painter actually puts on screen.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.
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
#include <QColor>
#include <QImage>
#include <QPixmap>

#include <cstdio>
#include <cstring>

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

struct Harness {
    Session session { 80, 24 };
    TerminalView view { &session };
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

} // namespace

int main(int argc, char **argv)
{
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
    test_reverse_and_screen_reverse();
    test_bold_has_its_own_colour();
    test_a_wide_character_covers_two_cells();
    test_dec_special_graphics_draws_a_line();
    test_the_cursor_is_drawn_where_the_core_says();
    test_an_unfocused_cursor_is_hollow();

    // `--write <dir>` dumps what was rendered, for looking at a failure rather
    // than guessing at it.
    for (int i = 1; i + 1 < argc; i++) {
        if (strcmp(argv[i], "--write") == 0) {
            Harness h;
            h.feed("\033[2J\033[H\033[1;32mtermitta\033[0m on \033[31mserial\033[0m\r\n"
                   "\033[4munderline\033[0m \033[7mreverse\033[0m \033[44;93mcolour\033[0m\r\n"
                   "\033(0lqqqk\033(B box  \xe5\x8c\x97\xe4\xba\xac wide  e\xcc\x81 combining\r\n");
            h.render();
            const QString path = QString::fromUtf8(argv[i + 1]) + "/screen.png";
            h.image.save(path);
            printf("wrote %s\n", qPrintable(path));
        }
    }

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("render ok\n");
    return 0;
}
