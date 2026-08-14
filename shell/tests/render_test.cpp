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
#include <QKeyEvent>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QPixmap>

#include <climits>
#include <cstdio>
#include <cstring>
#include <functional>

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QFileInfo>
#include <QLineEdit>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPushButton>
#include <QScreen>
#include <QRadioButton>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QThread>
#include <QTimer>
#include <QToolButton>

#include "MainWindow.h"
#include "ConnectBar.h"
#include "I18n.h"
#include "PasteDialog.h"
#include "ConnectDialog.h"
#include "Session.h"
#include "SettingsDialog.h"
#include "SshPrompts.h"
#include "TabRows.h"
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
        // A session this harness feeds bytes into is a session with something
        // on the other end, and saying so keeps `color.disconnected_shade` out
        // of every background these tests assert. The shade has a test of its
        // own; see `test_an_idle_terminal_is_a_different_shade`.
        view.theme().setConnected(true);
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

void key(TerminalView &view, int code,
         Qt::KeyboardModifiers modifiers = Qt::NoModifier,
         const QString &text = QString())
{
    QKeyEvent event(QEvent::KeyPress, code, modifiers, text);
    QCoreApplication::sendEvent(&view, &event);
}

QString rowText(const Session &session, int y)
{
    size_t len = 0;
    const TtCell *cells = session.row(y, &len);
    QString out;
    for (size_t x = 0; cells && x < len; x++) {
        if (cells[x].width_class == TT_WIDTH_PAD) {
            continue;
        }
        const uint32_t cp = cells[x].text[0];
        out += cp ? QString::fromUcs4(reinterpret_cast<const char32_t *>(&cp), 1)
                  : QStringLiteral(" ");
    }
    return out.trimmed();
}

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

void test_sixel_is_painted_and_later_text_erases_it()
{
    Harness h;
    const int cw = h.view.theme().cellWidth();
    const int ch = h.view.theme().cellHeight();
    QByteArray sixel = QByteArrayLiteral("\033P7;1q\"1;1;")
                       + QByteArray::number(cw) + ';' + QByteArray::number(ch)
                       + QByteArrayLiteral("#2;2;100;0;0");
    for (int y = 0; y < ch; y += 6) {
        if (y != 0) {
            sixel += '-';
        }
        const int bits = (1 << qMin(6, ch - y)) - 1;
        sixel += '!';
        sixel += QByteArray::number(cw);
        sixel += static_cast<char>('?' + bits);
    }
    sixel += QByteArrayLiteral("\033\\");
    h.session.feed(sixel);
    h.render();
    CHECK(h.at(0, 0) == QColor(255, 0, 0));
    CHECK(h.bgAt(0, 0) == QColor(255, 0, 0));

    h.feed("\033[1;1HX");
    h.render();
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

void test_dark_mode_changes_only_the_terminal_palette()
{
    Harness h;
    const QPalette applicationPalette = qApp->palette();
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("terminal.dark_mode"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.render();

    CHECK(h.view.theme().defaultForeground() == QColor(0xd4, 0xd4, 0xd4));
    CHECK(h.view.theme().defaultBackground() == QColor(0x1e, 0x1e, 0x1e));
    // Away from the cursor cell, whose block is intentionally foreground.
    CHECK(h.bgAt(10, 0) == QColor(0x1e, 0x1e, 0x1e));
    CHECK(qApp->palette() == applicationPalette);

    CHECK(h.session.setSetting(QStringLiteral("terminal.dark_mode"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.view.theme().defaultForeground() == kBlack);
    CHECK(h.view.theme().defaultBackground() == kWhite);
}

/// A terminal with nobody on the other end paints its own background a step
/// towards its own foreground — and nothing else: not the text, and not a
/// colour the host asked for by name.
void test_an_idle_terminal_is_a_different_shade()
{
    Harness h;
    h.feed("\033[41mred\033[0m plain");
    h.render();
    // The harness says "connected" so the rest of this file can assert plain
    // colours; that is also this test's first half. `SGR 41` is read back
    // rather than written down — which drawing index it lands on is
    // `color.ansi_palette`'s business and is pinned elsewhere.
    const QColor hostRed = h.bgAt(0, 0);
    CHECK(hostRed != kWhite);
    CHECK(h.view.theme().defaultBackground() == kWhite);
    CHECK(h.bgAt(10, 0) == kWhite);

    h.view.theme().setConnected(false);
    h.render();

    // 12 percent of the way from white towards black, the shipped default.
    const QColor idle(225, 225, 225);
    CHECK(h.view.theme().defaultBackground() == idle);
    CHECK(h.bgAt(10, 0) == idle);
    // Under the text as well as beside it: the shade is the terminal's
    // background wherever the host did not choose one.
    CHECK(h.bgAt(5, 0) == idle);
    // And not under `SGR 41`, which is a colour the host did choose.
    CHECK(h.bgAt(0, 0) == hostRed);
    // The text is untouched — a dimmed screen would be saying something about
    // the output rather than about the session. Asked of the resolver rather
    // than of a pixel, because a glyph's edge pixels are a blend of the two.
    TtCell plain {};
    QColor fg;
    QColor bg;
    h.view.theme().resolve(plain, false, false, &fg, &bg);
    CHECK(fg == kBlack);
    CHECK(bg == idle);

    // Zero is off, and the setting is read where every other colour setting is.
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("color.disconnected_shade"),
                               QStringLiteral("0"), &error));
    h.view.applySettings();
    h.render();
    CHECK(h.view.theme().defaultBackground() == kWhite);
    CHECK(h.bgAt(10, 0) == kWhite);
}

void test_osc_colours_reach_the_painter()
{
    Harness h;
    // `OSC 4` moves a palette entry, and the painter has to hear about it
    // without anyone applying settings: the core raises `colorsChanged` and
    // the view refills its cache. Index 1 is `SGR 41`'s dark red.
    h.feed("\033]4;1;#0c2238\033\\\033[41m \033[0m");
    h.render();
    CHECK(h.bgAt(0, 0) == QColor(0x0c, 0x22, 0x38));

    // `OSC 10` and `OSC 11` are the default pair, which is where a cell with
    // no explicit colour of its own is painted from — so this repaints the
    // whole window and not one attribute.
    h.feed("\033]10;#ffcc00;#102030\033\\");
    h.render();
    CHECK(h.bgAt(10, 0) == QColor(0x10, 0x20, 0x30));

    // And `OSC 110` puts the pair back to what the settings say, which is
    // Tera Term's black on white.
    h.feed("\033]110;11\033\\");
    h.render();
    CHECK(h.bgAt(10, 0) == kWhite);
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
    // `KillFocusCursor` ships on, so the state a window spends most of its
    // time in is a full-cell outline regardless of the active shape.
    Harness h;
    h.feed("\033[5;10H");
    h.render();
    CHECK(h.at(9, 4) == kWhite);  // not filled
    CHECK(h.ink(9, 4) > 0);       // but outlined
    CHECK(h.ink(8, 4) == 0);

    CHECK(h.session.setSetting(QStringLiteral("cursor.show_unfocused"),
                               QStringLiteral("off"), nullptr));
    h.view.applySettings();
    h.render();
    CHECK(h.ink(9, 4) == 0);
}

void test_cursor_shape_is_live_terminal_state()
{
    Harness h;
    CHECK(h.session.setSetting(QStringLiteral("cursor.shape"),
                               QStringLiteral("horizontal"), nullptr));
    CHECK(h.session.setSetting(QStringLiteral("cursor.nonblinking"),
                               QStringLiteral("on"), nullptr));
    h.view.applySettings();
    h.activate();
    h.feed("\033[5;10H");
    h.render();

    const int cw = h.view.theme().cellWidth();
    const int ch = h.view.theme().cellHeight();
    QImage cursor = h.cell(9, 4);
    CHECK(cursor.pixelColor(cw / 2, ch / 2) == kWhite);
    CHECK(cursor.pixelColor(cw / 2, ch - 1) == kBlack);
    CHECK(cursor.pixelColor(cw / 2, ch - 2) == kBlack);

    // Once permitted, DECSCUSR changes what the painter sees without changing
    // the file's setting. Six is a steady vertical bar.
    CHECK(h.session.setSetting(QStringLiteral("window.cursor_ctrl_allowed"),
                               QStringLiteral("on"), nullptr));
    h.feed("\033[6 q");
    h.render();
    cursor = h.cell(9, 4);
    CHECK(cursor.pixelColor(0, ch / 2) == kBlack);
    CHECK(cursor.pixelColor(1, ch / 2) == kBlack);
    CHECK(cursor.pixelColor(cw / 2, ch / 2) == kWhite);
}

void test_cursor_blinks_unless_the_live_style_is_steady()
{
    const int oldFlashTime = QApplication::cursorFlashTime();
    QApplication::setCursorFlashTime(200);
    {
        Harness h;
        h.activate();
        h.render();
        CHECK(h.at(0, 0) == kBlack);

        // The view uses half the desktop flash cycle, as Qt's own text widgets
        // do. This lands after one transition and before the second.
        QEventLoop blink;
        QTimer::singleShot(150, &blink, &QEventLoop::quit);
        blink.exec();
        h.render();
        CHECK(h.at(0, 0) == kWhite);

        CHECK(h.session.setSetting(QStringLiteral("cursor.nonblinking"),
                                   QStringLiteral("on"), nullptr));
        h.view.applySettings();
        h.render();
        CHECK(h.at(0, 0) == kBlack);

        QEventLoop steady;
        QTimer::singleShot(150, &steady, &QEventLoop::quit);
        steady.exec();
        h.render();
        CHECK(h.at(0, 0) == kBlack);
    }
    QApplication::setCursorFlashTime(oldFlashTime);
}

/// Local echo is the one thing on the screen the far end did not put there,
/// and it is on the screen the moment the key is pressed — but the frontend
/// only learns that a repaint is due by draining the core's events, and the
/// input paths used to drain nothing. A typed character then waited for the
/// next thing the host said, or, on a quiet line, for the cursor's own blink:
/// half a second of lag on every keystroke.
void test_local_echo_reaches_the_screen_without_waiting_for_the_host()
{
    Harness h;
    CHECK(h.session.setSetting(QStringLiteral("terminal.local_echo"),
                               QStringLiteral("on"), nullptr));

    int repaints = 0;
    QObject::connect(&h.session, &Session::damaged, [&repaints] { repaints++; });

    // A blank tab has no transport. Its saved preference must not manufacture
    // terminal output from ordinary typing.
    QKeyEvent press(QEvent::KeyPress, Qt::Key_A, Qt::NoModifier, QStringLiteral("a"));
    QCoreApplication::sendEvent(&h.view, &press);
    CHECK(repaints == 0);

    // The Session seam remains transportless for focused frontend tests and
    // log replay. Every direct input path still drains its damage at once.
    h.session.sendText(QStringLiteral("a"));
    h.session.sendBytes(QByteArray("b"));
    h.session.sendKey(TT_KEY_KP1);
    h.session.paste(QStringLiteral("p"));
    CHECK(repaints == 4);

    h.render();
    for (int col = 0; col < 4; col++) {
        CHECK(h.ink(col, 0) > 0);
    }

    // And nothing is echoed with it off, so no repaint is owed either.
    CHECK(h.session.setSetting(QStringLiteral("terminal.local_echo"),
                               QStringLiteral("off"), nullptr));
    repaints = 0;
    h.session.sendText(QStringLiteral("x"));
    CHECK(repaints == 0);
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
    h.mouse(QEvent::MouseButtonRelease, h.px(8), h.py(0));

    // With `DelimDBCS` off, width is no longer a boundary. Clicking either
    // half of a wide character still names that whole character, but the word
    // now reaches through both the narrow and wide runs.
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("keyboard.width_delimits_word"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.mouse(QEvent::MouseButtonPress, h.px(4), h.py(0));
    h.mouse(QEvent::MouseButtonDblClick, h.px(4), h.py(0));
    CHECK(h.copied() ==
          QString::fromUtf8("abc\xe5\x8c\x97\xe4\xba\xac" "def"));
}

void test_clickable_url_controls_only_the_cursor_and_launch()
{
    Harness h;
    h.feed("x http://example.test end");

    // Recognition, colour and underline happen with the setting off, but the
    // cursor and double-click stay those of ordinary text. That cursor is a
    // setting of its own rather than necessarily an I-beam.
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("mouse.cursor"),
                               QStringLiteral("ibeam"), &error));
    h.view.applySettings();
    CHECK(h.view.cursor().shape() == Qt::IBeamCursor);
    CHECK(h.session.setSetting(QStringLiteral("mouse.cursor"),
                               QStringLiteral("HAND"), &error));
    h.view.applySettings();
    CHECK(h.view.cursor().shape() == Qt::PointingHandCursor);
    CHECK(h.session.setSetting(QStringLiteral("mouse.cursor"),
                               QStringLiteral("ARROW"), &error));
    h.view.applySettings();
    h.hover(h.px(4), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::ArrowCursor);

    CHECK(h.session.setSetting(QStringLiteral("mouse.clickable_url"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.hover(h.px(4), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::PointingHandCursor);
    h.hover(h.px(0), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::ArrowCursor);

    // The names are case-insensitive at the point of use, but their file
    // spelling stays untouched. URL hover must restore the configured cross.
    CHECK(h.session.setSetting(QStringLiteral("mouse.cursor"),
                               QStringLiteral("cross"), &error));
    h.view.applySettings();
    CHECK(h.view.cursor().shape() == Qt::CrossCursor);
    h.hover(h.px(4), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::PointingHandCursor);
    h.hover(h.px(0), h.py(0));
    CHECK(h.view.cursor().shape() == Qt::CrossCursor);

    // An unknown raw value is upstream's no-op, not an implicit I-beam.
    CHECK(h.session.setSetting(QStringLiteral("mouse.cursor"),
                               QStringLiteral("MY-CURSOR"), &error));
    h.view.applySettings();
    CHECK(h.view.cursor().shape() == Qt::CrossCursor);

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

void test_clear_commands_keep_or_drop_selection()
{
    Harness h;
    h.feed("keep me\r\nsecond");
    h.drag(h.px(0), h.py(0), h.px(6, 0.7), h.py(0));
    CHECK(h.copied() == QStringLiteral("keep me"));

    h.view.clearScreen();
    CHECK(h.session.scrollbackLen() == h.session.rows());
    CHECK(rowText(h.session, 0).isEmpty());
    CHECK(h.copied() == QStringLiteral("keep me"));
    const TtCursor cleared = h.session.cursor();
    CHECK(cleared.x == 0 && cleared.y == 0);

    h.feed("new page");
    h.view.clearBuffer();
    CHECK(h.session.scrollbackLen() == 0);
    CHECK(h.session.viewOffset() == 0);
    CHECK(rowText(h.session, 0).isEmpty());
    CHECK(!h.view.hasSelection());
    const TtCursor emptied = h.session.cursor();
    CHECK(emptied.x == 0 && emptied.y == 0);

    Harness noHistory;
    QString error;
    CHECK(noHistory.session.setSetting(QStringLiteral("terminal.scrollback_enabled"),
                                       QStringLiteral("off"), &error));
    noHistory.view.applySettings();
    noHistory.feed("gone");
    noHistory.drag(noHistory.px(0), noHistory.py(0), noHistory.px(3, 0.7),
                   noHistory.py(0));
    CHECK(noHistory.view.hasSelection());
    noHistory.view.clearScreen();
    CHECK(noHistory.session.scrollbackLen() == 0);
    CHECK(!noHistory.view.hasSelection());
}

void test_the_edit_menu_and_key_map_share_clear_commands()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));
    auto *view = window.findChild<TerminalView *>();
    auto *edit = window.findChild<QMenu *>(QStringLiteral("editMenu"));
    auto *clearScreen =
        window.findChild<QAction *>(QStringLiteral("clearScreenAction"));
    auto *clearBuffer =
        window.findChild<QAction *>(QStringLiteral("clearBufferAction"));
    auto *quick = window.findChild<QAction *>(
        QStringLiteral("quickButtonFromSelectionAction"));
    CHECK(view != nullptr);
    CHECK(edit != nullptr);
    CHECK(clearScreen != nullptr);
    CHECK(clearBuffer != nullptr);
    CHECK(quick != nullptr);
    if (!view || !edit || !clearScreen || !clearBuffer || !quick) {
        return;
    }

    CHECK(clearScreen->text() == QStringLiteral("Clear screen"));
    CHECK(clearBuffer->text() == QStringLiteral("Clear buffer"));
    CHECK(clearScreen->statusTip().contains(QStringLiteral("keeps it in scrollback")));
    CHECK(clearBuffer->statusTip().contains(
        QStringLiteral("permanently removes all scrollback")));
    CHECK(clearScreen->shortcut().isEmpty());
    CHECK(clearBuffer->shortcut().isEmpty());
    const int screenAt = edit->actions().indexOf(clearScreen);
    const int bufferAt = edit->actions().indexOf(clearBuffer);
    const int quickAt = edit->actions().indexOf(quick);
    CHECK(screenAt >= 0);
    CHECK(bufferAt == screenAt + 1);
    CHECK(quickAt > bufferAt);

    window.session()->feed(QByteArrayLiteral("menu page"));
    clearScreen->trigger();
    CHECK(window.session()->scrollbackLen() == window.session()->rows());
    CHECK(rowText(*window.session(), 0).isEmpty());
    window.session()->feed(QByteArrayLiteral("menu history"));
    clearBuffer->trigger();
    CHECK(window.session()->scrollbackLen() == 0);
    CHECK(rowText(*window.session(), 0).isEmpty());

    const QString keyMap = dir.filePath(QStringLiteral("KEYBOARD.CNF"));
    QFile file(keyMap);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Shortcut keys]\nEditCLS=63\nEditCLB=64\n");
    file.close();
    QVector<quint16> duplicates;
    QString error;
    CHECK(window.session()->loadKeyMap(keyMap, &duplicates, &error));
    CHECK(duplicates.isEmpty());
    // A disconnected blank tab intentionally ignores physical key mappings;
    // local line-edit mode keeps its keyboard live without needing a test
    // transport, and the configured shortcut still outranks the editor.
    CHECK(window.session()->setSetting(QStringLiteral("terminal.line_edit"),
                                       QStringLiteral("on"), &error));
    CHECK(view->lineEditEnabled());

    window.session()->feed(QByteArrayLiteral("key page"));
    key(*view, Qt::Key_F5);
    CHECK(window.session()->scrollbackLen() == window.session()->rows());
    CHECK(rowText(*window.session(), 0).isEmpty());
    window.session()->feed(QByteArrayLiteral("key history"));
    key(*view, Qt::Key_F6);
    CHECK(window.session()->scrollbackLen() == 0);
    CHECK(rowText(*window.session(), 0).isEmpty());
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

/// Line edit overlays a second selectable text surface on the grid. Copy must
/// follow the selection the user made last, through every route that can ask
/// for the command, and an unselected draft must not mask terminal output.
void test_line_edit_copy_uses_the_active_selection()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("terminal.line_edit"),
                               QStringLiteral("on"), &error));
    h.view.applySettings();
    h.activate();
    h.feed("terminal text");

    auto *editor = h.view.findChild<QLineEdit *>(
        QStringLiteral("terminalLineEditor"));
    CHECK(editor != nullptr);
    if (!editor) {
        return;
    }
    editor->setText(QStringLiteral("draft text"));

    // The built-in Ctrl+Shift+C: a grid drag takes the selection away from
    // the draft and the empty draft selection falls through to the grid.
    editor->setSelection(0, 5);
    h.drag(h.px(0), h.py(0), h.px(7, 0.7), h.py(0));
    CHECK(editor->selectedText().isEmpty());
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    key(h.view, Qt::Key_C, Qt::ControlModifier | Qt::ShiftModifier);
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("terminal"));

    // Selecting the draft does the reverse, so the same shortcut copies it
    // without leaving two highlighted answers on screen.
    editor->setSelection(0, 5);
    CHECK(!h.view.hasSelection());
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    key(h.view, Qt::Key_C, Qt::ControlModifier | Qt::ShiftModifier);
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("draft"));

    // A KEYBOARD.CNF shortcut reaches the same decision point.
    QTemporaryDir keys;
    CHECK(keys.isValid());
    const QString keyMap = keys.filePath(QStringLiteral("KEYBOARD.CNF"));
    QFile file(keyMap);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Shortcut keys]\nEditCopy=63\n");
    file.close();
    QVector<quint16> duplicates;
    CHECK(h.session.loadKeyMap(keyMap, &duplicates, &error));
    CHECK(duplicates.isEmpty());

    h.drag(h.px(0), h.py(0), h.px(7, 0.7), h.py(0));
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    key(h.view, Qt::Key_F5);
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("terminal"));
    editor->setSelection(6, 4);
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    key(h.view, Qt::Key_F5);
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("text"));

    // The menu action belongs to MainWindow, but still delegates the whole
    // choice to TerminalView.
    QTemporaryDir config;
    CHECK(config.isValid());
    MainWindow window(config.filePath(QStringLiteral("copy.ini")));
    auto *view = window.findChild<TerminalView *>();
    auto *menuCopy = window.findChild<QAction *>(QStringLiteral("copyAction"));
    CHECK(view != nullptr);
    CHECK(menuCopy != nullptr);
    if (!view || !menuCopy) {
        return;
    }
    CHECK(window.session()->setSetting(QStringLiteral("terminal.line_edit"),
                                       QStringLiteral("on"), &error));
    view->applySettings();
    view->resize(80 * view->theme().cellWidth(),
                 24 * view->theme().cellHeight());
    window.session()->feed(QByteArrayLiteral("menu output"));
    auto *menuEditor = view->findChild<QLineEdit *>(
        QStringLiteral("terminalLineEditor"));
    CHECK(menuEditor != nullptr);
    if (!menuEditor) {
        return;
    }
    menuEditor->setText(QStringLiteral("menu draft"));

    const auto drag = [view](int from, int to) {
        const int cw = view->theme().cellWidth();
        const int ch = view->theme().cellHeight();
        for (const auto type : {QEvent::MouseButtonPress, QEvent::MouseMove,
                                QEvent::MouseButtonRelease}) {
            const int col = type == QEvent::MouseButtonPress ? from : to;
            const double fraction = type == QEvent::MouseButtonPress ? 0.0 : 0.7;
            const Qt::MouseButtons held = type == QEvent::MouseButtonRelease
                                              ? Qt::NoButton
                                              : Qt::LeftButton;
            QMouseEvent event(type, QPointF((col + fraction) * cw, 0.5 * ch),
                              QPointF((col + fraction) * cw, 0.5 * ch),
                              Qt::LeftButton, held, Qt::NoModifier);
            QCoreApplication::sendEvent(view, &event);
        }
    };
    drag(0, 3);
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    menuCopy->trigger();
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("menu"));
    menuEditor->setSelection(5, 5);
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    menuCopy->trigger();
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("draft"));
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

    // `Paste<CR>` asks a different question (`clipboar.c:150`): the CR being
    // *added* is the change, so `ConfirmChangePasteCR` decides alone and the
    // text is not searched for a line break at all. One word is confirmed...
    CHECK(PasteDialog::shouldConfirm(QStringLiteral("one word"), QString(), true,
                                     true));
    // ...and with the key off, even a paste full of them is not.
    CHECK(!PasteDialog::shouldConfirm(QStringLiteral("two\nlines"), QString(),
                                      true, false));
    // The dictionary still runs afterwards on that path, and can only turn
    // confirmation on.
    CHECK(PasteDialog::shouldConfirm(QStringLiteral("sudo rm -rf /tmp/x"),
                                     dict.fileName(), true, false));
    CHECK(!PasteDialog::shouldConfirm(QStringLiteral("ls -l"), dict.fileName(),
                                      true, false));

    // The dialog opens at the size the settings hold, which is the whole
    // reason `PasteDialogSize` is a setting: upstream writes it back.
    QString error;
    I18n i18n;
    CHECK(i18n.load(QStringLiteral("lang\\ja_JP.lng"), QString(), &error));
    PasteDialog dialog(QStringLiteral("two\nlines"), QSize(400, 300), nullptr,
                       &i18n);
    dialog.adjustSize();
    dialog.resize(400, 300);
    CHECK(dialog.text() == QStringLiteral("two\nlines"));
    CHECK(dialog.size() == QSize(400, 300));
    auto *buttons = dialog.findChild<QDialogButtonBox *>();
    CHECK(buttons != nullptr);
    if (buttons) {
        CHECK(buttons->button(QDialogButtonBox::Cancel)->text()
              == QStringLiteral("キャンセル"));
    }
}

/// OSC 52 stops at the core/toolkit boundary: the parser decides whether the
/// request is allowed, and this `Session` is the first layer which may touch
/// the operating system clipboard.
void test_remote_clipboard_access_is_permissioned_and_notified()
{
    Session session(80, 24);
    QStringList notices;
    QObject::connect(&session, &Session::notice,
                     [&notices](const QString &text) { notices.append(text); });

    QApplication::clipboard()->setText(QStringLiteral("local"), QClipboard::Clipboard);
    session.feed(QByteArray("\033]52;c;cmVtb3Rl\007"));
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("local"));
    CHECK(notices == QStringList{QStringLiteral("Remote clipboard write rejected")});

    QString error;
    CHECK(session.setSetting(QStringLiteral("clipboard.remote_access"),
                             QStringLiteral("write"), &error));
    notices.clear();
    session.feed(QByteArray("\033]52;c;cmVtb3Rl\033\\"));
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("remote"));
    CHECK(notices == QStringList{QStringLiteral("Remote host wrote the clipboard")});

    // Notification is independent of permission: turning it off makes the
    // same authorised write quiet rather than refusing it.
    CHECK(session.setSetting(QStringLiteral("clipboard.remote_notify"),
                             QStringLiteral("off"), &error));
    notices.clear();
    session.feed(QByteArray("\033]52;c;cXVpZXQ=\007"));
    CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
          == QStringLiteral("quiet"));
    CHECK(notices.isEmpty());
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

void test_font_quality_and_resizing_are_live_settings()
{
    Harness h;
    QString error;
    CHECK(h.view.theme().drawsResizedFont());

    CHECK(h.session.setSetting(QStringLiteral("font.draw_resized"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    CHECK(!h.view.theme().drawsResizedFont());

    CHECK(h.session.setSetting(QStringLiteral("font.quality"),
                               QStringLiteral("nonantialiased"), &error));
    h.view.applySettings();
    CHECK(h.view.theme().font().styleStrategy() == QFont::NoAntialias);

    CHECK(h.session.setSetting(QStringLiteral("font.quality"),
                               QStringLiteral("antialiased"), &error));
    h.view.applySettings();
    CHECK(h.view.theme().font().styleStrategy() == QFont::PreferAntialias);

    // Qt deliberately leaves subpixel rasterisation to the platform, so the
    // Win32 ClearType request has the explicit antialias strategy here too.
    CHECK(h.session.setSetting(QStringLiteral("font.quality"),
                               QStringLiteral("cleartype"), &error));
    h.view.applySettings();
    CHECK(h.view.theme().font().styleStrategy() == QFont::PreferAntialias);

    CHECK(h.session.setSetting(QStringLiteral("font.quality"),
                               QStringLiteral("default"), &error));
    h.view.applySettings();
    CHECK(h.view.theme().font().styleStrategy() == QFont::PreferDefault);
}

void test_vt_font_space_changes_the_cell_and_glyph_origin()
{
    Harness h;
    QString error;
    // In the *second* column, with a blank one to its left. A glyph may put
    // ink slightly outside its own advance — DejaVu Sans Mono's `A` does, by a
    // pixel, at the size Qt 6.4.2 picks here — and measured from column 0 that
    // pixel is off the image, so the search saturates at x=0 and reports the
    // margin as one less than it is. It cost a CI failure on a painter that
    // was doing exactly the right thing.
    h.feed("\033[?25l A");
    h.render();

    const int oldWidth = h.view.theme().cellWidth();
    const int oldHeight = h.view.theme().cellHeight();
    const int oldBaseline = h.view.theme().baseline();
    /// Where the ink in column 1 starts, relative to that column's left edge —
    /// so a leftward overhang is a negative number rather than a clamp.
    auto firstInk = [&h]() {
        const int cell = h.view.theme().cellWidth();
        QPoint first(INT_MAX, INT_MAX);
        for (int y = 0; y < h.view.theme().cellHeight(); y++) {
            for (int x = cell - 4; x < 2 * cell; x++) {
                if (h.image.pixelColor(x, y) != kWhite) {
                    first.setX(qMin(first.x(), x - cell));
                    first.setY(qMin(first.y(), y));
                }
            }
        }
        return first;
    };
    const QPoint oldInk = firstInk();
    CHECK(oldInk.x() != INT_MAX && oldInk.y() != INT_MAX);

    CHECK(h.session.setSetting(QStringLiteral("font.space_left"),
                               QStringLiteral("3"), &error));
    CHECK(h.session.setSetting(QStringLiteral("font.space_right"),
                               QStringLiteral("4"), &error));
    CHECK(h.session.setSetting(QStringLiteral("font.space_top"),
                               QStringLiteral("5"), &error));
    CHECK(h.session.setSetting(QStringLiteral("font.space_bottom"),
                               QStringLiteral("6"), &error));
    h.view.applySettings();
    h.render();

    CHECK(h.view.theme().cellWidth() == oldWidth + 7);
    CHECK(h.view.theme().cellHeight() == oldHeight + 11);
    CHECK(h.view.theme().baseline() == oldBaseline + 5);
    CHECK(h.view.theme().textOffsetX() == 3);
    CHECK(firstInk() == oldInk + QPoint(3, 5));

    // Applying an unchanged file must not measure the old cell spacing as if
    // it were part of the font and grow the grid a second time.
    h.view.applySettings();
    CHECK(h.view.theme().cellWidth() == oldWidth + 7);
    CHECK(h.view.theme().cellHeight() == oldHeight + 11);
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
    // Position explicitly: the shipped receive-CR mode is Auto, where a bare
    // CR is a line ending rather than an overwrite of row zero.
    h.feed("\033[1;1H\033[30;40;7m \033[0m");
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
    CHECK(dialog.findChildren<TabRows *>().size() == 1);

    // A tab per page, in the schema's own order.
    auto *tabs = dialog.findChild<TabRows *>();
    CHECK(tabs != nullptr);
    if (tabs) {
        CHECK(tabs->count() == 26);
        CHECK(tabs->tabText(0) == QStringLiteral("Terminal"));
        // Selecting a tab shows its page, which is the whole of what the two
        // widgets have to agree about.
        auto *pages = dialog.findChild<QStackedWidget *>();
        CHECK(pages != nullptr);
        if (pages) {
            CHECK(pages->count() == tabs->count());
            tabs->setCurrentIndex(3);
            CHECK(pages->currentIndex() == 3);
            // Back to the first page: the filter checks below ask whether a
            // widget is visible, and a page the stack is not showing is not.
            tabs->setCurrentIndex(0);
        }
        // And they wrap, which is the point of the widget: 25 schema pages do
        // not fit on one row at any width a dialog can have. Asked of the
        // layout rather than of a shown dialog, because Qt caps a window's
        // initial size at two thirds of the screen and this would then be a
        // test of the screen.
        CHECK(tabs->rowsForWidth(tabs->widthForRows(2)) == 2);
        CHECK(tabs->rowsForWidth(tabs->widthForRows(1)) == 1);
        CHECK(tabs->rowsForWidth(tabs->widthForRows(3)) == 3);
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
    auto *search = dialog.findChild<QLineEdit *>(QStringLiteral("settingsSearch"));
    auto *noResults = dialog.findChild<QLabel *>(
        QStringLiteral("settingsNoResultsLabel"));
    CHECK(search != nullptr);
    CHECK(noResults != nullptr);
    if (search && termId && tabs && noResults) {
        const int beforeSearch = tabs->currentIndex();
        search->setText(QStringLiteral("keyboard.backspace"));
        int visibleTabs = 0;
        for (int i = 0; i < tabs->count(); i++) {
            visibleTabs += tabs->isTabVisible(i) ? 1 : 0;
        }
        CHECK(visibleTabs == 1);
        CHECK(tabs->tabText(tabs->currentIndex()) == QStringLiteral("Keyboard"));
        CHECK(!termId->isVisibleTo(&dialog));
        CHECK(noResults->isHidden());

        search->setText(QStringLiteral("nothing-can-match-this-setting"));
        CHECK(tabs->isHidden());
        CHECK(!noResults->isHidden());
        CHECK(noResults->text() == QStringLiteral("No settings match your search."));

        search->setText(QString());
        CHECK(!tabs->isHidden());
        CHECK(tabs->currentIndex() == beforeSearch);
        for (int i = 0; i < tabs->count(); i++) {
            CHECK(tabs->isTabVisible(i));
        }
        CHECK(termId->isVisibleTo(&dialog));

        // A matching current page stays put, rather than jumping to the first
        // matching page on every keystroke.
        const int color = 4;
        tabs->setCurrentIndex(color);
        search->setText(QStringLiteral("color"));
        CHECK(tabs->currentIndex() == color);
        search->clear();

        // Horizontal keyboard navigation crosses the stable index of a
        // hidden tab instead of getting stuck on it.
        tabs->setCurrentIndex(0);
        tabs->setTabVisible(1, false);
        QKeyEvent right(QEvent::KeyPress, Qt::Key_Right, Qt::NoModifier);
        QCoreApplication::sendEvent(tabs, &right);
        CHECK(tabs->currentIndex() == 2);
        tabs->setTabVisible(1, true);
    }
}

void test_the_setup_menu_opens_one_settings_dialog()
{
    const QVector<SettingsDialog::Page> pages = SettingsDialog::corePages();
    CHECK(pages.size() == 26);
    const QStringList expected = {
        QStringLiteral("Terminal"),   QStringLiteral("Encoding"),
        QStringLiteral("Ime"),        QStringLiteral("Keyboard"),
        QStringLiteral("Color"),      QStringLiteral("Cursor"),
        QStringLiteral("Window"),     QStringLiteral("Font"),
        QStringLiteral("Mouse"),      QStringLiteral("Url"),
        QStringLiteral("Debug"),      QStringLiteral("Bell"),
        QStringLiteral("Clipboard"),  QStringLiteral("Connection"),
        QStringLiteral("Proxy"),      QStringLiteral("Macro"),
        QStringLiteral("Settings"),   QStringLiteral("Serial"),
        QStringLiteral("Log"),        QStringLiteral("Transfer"),
        QStringLiteral("Printer"),    QStringLiteral("Tek"),
        QStringLiteral("Broadcast"),  QStringLiteral("Menu"),
        QStringLiteral("Recent"),     QStringLiteral("Updates"),
    };
    for (int i = 0; i < pages.size(); i++) {
        CHECK(pages.at(i).title == expected.at(i));
    }

    Harness h;
    SettingsDialog initial(&h.session, nullptr, nullptr, nullptr, 19);
    auto *initialTabs = initial.findChild<TabRows *>();
    CHECK(initialTabs != nullptr);
    if (initialTabs) {
        CHECK(initialTabs->currentIndex() == 19);
        CHECK(initialTabs->tabText(19) == QStringLiteral("Transfer"));
    }

    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString menuPath = dir.filePath(QStringLiteral("menu.ini"));
    QFile menuFile(menuPath);
    CHECK(menuFile.open(QIODevice::WriteOnly));
    menuFile.write("[Sterna]\nAutoSaveSettings=off\n");
    menuFile.close();
    MainWindow window(menuPath);
    auto *setup = window.findChild<QMenu *>(QStringLiteral("setupMenu"));
    auto *font = window.findChild<QAction *>(QStringLiteral("chooseFontAction"));
    CHECK(setup != nullptr);
    CHECK(font != nullptr);
    if (!setup || !font) {
        return;
    }
    CHECK(font->text() == QStringLiteral("Choose font…"));

    // One item for all 26 pages, not one item each. The tabs and the search
    // box inside the dialog are the way to a page; a menu that reproduced them
    // was 26 lines long and opened the same dialog every time.
    auto *preferences =
        window.findChild<QAction *>(QStringLiteral("preferencesAction"));
    CHECK(preferences != nullptr);
    if (!preferences) {
        return;
    }
    CHECK(preferences->text() == QStringLiteral("Preferences..."));
    CHECK(setup->actions().indexOf(font) == setup->actions().indexOf(preferences) + 1);
    for (QAction *action : window.findChildren<QAction *>()) {
        CHECK(!action->objectName().startsWith(
            QStringLiteral("settingsPageAction")));
    }

    int opened = -1;
    QTimer::singleShot(0, [&opened] {
        auto *dialog =
            qobject_cast<SettingsDialog *>(QApplication::activeModalWidget());
        CHECK(dialog != nullptr);
        if (!dialog) {
            return;
        }
        auto *tabs = dialog->findChild<TabRows *>();
        CHECK(tabs != nullptr);
        if (tabs) {
            opened = tabs->currentIndex();
        }
        dialog->reject();
    });
    preferences->trigger();
    CHECK(opened == 0);

    // Loading a catalog translates the stable menu chrome, the font picker and
    // Preferences — which takes `MENU_SETUP_ADDITION`, upstream's key for the
    // item that opens *its* tabbed everything-else dialog. The page names
    // themselves are the schema's and are not in any catalog.
    const QString translatedPath = dir.filePath(QStringLiteral("translated.ini"));
    QFile translatedFile(translatedPath);
    CHECK(translatedFile.open(QIODevice::WriteOnly));
    translatedFile.write("[Tera Term]\nUILanguageFile=lang\\ja_JP.lng\n"
                         "[Sterna]\nAutoSaveSettings=off\n");
    translatedFile.close();
    MainWindow translated(translatedPath);
    auto *translatedSetup =
        translated.findChild<QMenu *>(QStringLiteral("setupMenu"));
    auto *translatedFont =
        translated.findChild<QAction *>(QStringLiteral("chooseFontAction"));
    auto *translatedPreferences =
        translated.findChild<QAction *>(QStringLiteral("preferencesAction"));
    CHECK(translatedSetup != nullptr);
    CHECK(translatedFont != nullptr);
    CHECK(translatedPreferences != nullptr);
    if (translatedSetup && translatedFont && translatedPreferences) {
        CHECK(translatedSetup->title() == QStringLiteral("設定"));
        CHECK(translatedFont->text() == QStringLiteral("フォント..."));
        CHECK(translatedPreferences->text() == QStringLiteral("その他の設定..."));
    }
}

/// What the two menus each own: View decides whether a thing is on screen,
/// Setup decides what the thing is. The three switches were in Setup, under
/// the 26 page links, where the menu was long enough that nobody found them.
void test_the_view_menu_owns_the_three_switches()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("view.ini"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\nAutoSaveSettings=off\n");
    file.close();

    MainWindow window(path);
    auto *view = window.findChild<QMenu *>(QStringLiteral("viewMenu"));
    auto *setup = window.findChild<QMenu *>(QStringLiteral("setupMenu"));
    CHECK(view != nullptr);
    CHECK(setup != nullptr);
    if (!view || !setup) {
        return;
    }

    const auto menuOf = [](MainWindow &w, const char *name) -> QMenu * {
        auto *action = w.findChild<QAction *>(QString::fromLatin1(name));
        CHECK(action != nullptr);
        if (!action || action->associatedObjects().isEmpty()) {
            return nullptr;
        }
        return qobject_cast<QMenu *>(action->associatedObjects().first());
    };

    CHECK(menuOf(window, "tiledAction") == view);
    CHECK(menuOf(window, "showToolbarAction") == view);
    CHECK(menuOf(window, "showQuickButtonsAction") == view);
    CHECK(menuOf(window, "highlightMatchesAction") == view);
    // The editors are settings, and settings are Setup's.
    CHECK(menuOf(window, "highlightingAction") == setup);
    CHECK(menuOf(window, "quickButtonsAction") == setup);
    CHECK(menuOf(window, "preferencesAction") == setup);

    // Still switches, wherever they hang: each writes its own setting.
    auto *toolbar = window.findChild<QAction *>(QStringLiteral("showToolbarAction"));
    CHECK(toolbar != nullptr);
    if (toolbar) {
        CHECK(toolbar->isCheckable());
        const bool before = toolbar->isChecked();
        toolbar->trigger();
        CHECK(window.session()->setting(QStringLiteral("window.toolbar"))
              == (before ? QStringLiteral("off") : QStringLiteral("on")));
        toolbar->trigger();
        CHECK(toolbar->isChecked() == before);
    }
}

/// The release page moved out of Help and into the dialog that names the
/// version it is a page about.
void test_the_about_dialog_carries_the_release_page()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("about.ini"));
    QFile file(path);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Sterna]\nAutoSaveSettings=off\n");
    file.close();

    MainWindow window(path);
    auto *help = window.findChild<QMenu *>(QStringLiteral("helpMenu"));
    auto *about = window.findChild<QAction *>(QStringLiteral("aboutAction"));
    CHECK(help != nullptr);
    CHECK(about != nullptr);
    if (!help || !about) {
        return;
    }
    // Help is one item now: everything it used to link is inside that item.
    CHECK(help->actions().size() == 1);
    CHECK(help->actions().first() == about);

    bool sawReleases = false;
    bool sawUpdate = false;
    QTimer::singleShot(0, [&] {
        auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
        CHECK(box != nullptr);
        if (!box) {
            return;
        }
        sawReleases = box->findChild<QPushButton *>(
                          QStringLiteral("aboutReleasesButton"))
                      != nullptr;
        sawUpdate =
            box->findChild<QPushButton *>(QStringLiteral("aboutUpdateButton"))
            != nullptr;
        box->reject();
    });
    about->trigger();
    CHECK(sawReleases);
    CHECK(sawUpdate);
}

void test_the_settings_dialog_uses_a_language_catalog()
{
    Harness h;
    QString error;
    I18n i18n;
    CHECK(i18n.load(QStringLiteral("lang\\ja_JP.lng"), QString(), &error));
    CHECK(i18n.text("MENU_FILE", QStringLiteral("File"))
          == QStringLiteral("ファイル(&F)"));

    CHECK(h.session.setSetting(QStringLiteral("settings.language_file"),
                               QStringLiteral("lang\\ja_JP.lng"), &error));
    SettingsDialog dialog(&h.session, nullptr, &i18n);

    bool translated = false;
    for (QLabel *label : dialog.findChildren<QLabel *>()) {
        if (label->text() == QStringLiteral("送信(&M):")) {
            translated = true;
            break;
        }
    }
    CHECK(translated);

    QComboBox *languages = nullptr;
    for (QComboBox *combo : dialog.findChildren<QComboBox *>()) {
        if (combo->findData(QStringLiteral("lang\\ja_JP.lng")) >= 0) {
            languages = combo;
            break;
        }
    }
    CHECK(languages != nullptr);
    if (languages) {
        CHECK(languages->count() == 14);
        CHECK(languages->currentData().toString()
              == QStringLiteral("lang\\ja_JP.lng"));
    }
}

void test_the_connection_dialogs_use_the_language_catalog()
{
    QString error;
    I18n i18n;
    CHECK(i18n.load(QStringLiteral("lang\\ja_JP.lng"), QString(), &error));

    const auto hasLabel = [](const QDialog &dialog, const QString &wanted) {
        for (const QLabel *label : dialog.findChildren<QLabel *>()) {
            if (label->text() == wanted) {
                return true;
            }
        }
        return false;
    };
    const auto cancelText = [](const QDialog &dialog) {
        const auto *buttons = dialog.findChild<QDialogButtonBox *>();
        return buttons && buttons->button(QDialogButtonBox::Cancel)
                   ? buttons->button(QDialogButtonBox::Cancel)->text()
                   : QString();
    };

    // One screen now, so one dialog carries every label the three used to —
    // the panels behind Details are where the per-transport ones live.
    ConnectDialog connect(nullptr, &i18n);
    CHECK(connect.windowTitle() == QStringLiteral("新しい接続"));
    CHECK(hasLabel(connect, QStringLiteral("ホスト(&O):")));
    CHECK(hasLabel(connect, QStringLiteral("フロー制御(&F):")));
    CHECK(hasLabel(connect, QStringLiteral("ユーザ名(&N):")));
    CHECK(hasLabel(connect, QStringLiteral("秘密鍵(&K):")));
    CHECK(cancelText(connect) == QStringLiteral("キャンセル"));
}

void test_the_ssh_prompts_use_the_language_catalog()
{
    QString error;
    I18n i18n;
    CHECK(i18n.load(QStringLiteral("lang\\ja_JP.lng"), QString(), &error));

    HostKeyRequest request;
    request.host = QStringLiteral("console.example.com");
    request.algorithm = QStringLiteral("ssh-ed25519");
    request.fingerprint = QStringLiteral("SHA256:test");
    request.verdict = TT_HOST_KEY_UNKNOWN;
    HostKeyDialog hostKey(request, nullptr, &i18n);
    CHECK(hostKey.windowTitle() == QStringLiteral("セキュリティ警告"));

    bool fingerprintLabel = false;
    bool rememberButton = false;
    bool disconnectButton = false;
    for (const QLabel *label : hostKey.findChildren<QLabel *>()) {
        fingerprintLabel |=
            label->text() == QStringLiteral("サーバ側のホスト鍵指紋:");
    }
    for (const QPushButton *button : hostKey.findChildren<QPushButton *>()) {
        rememberButton |= button->text()
                          == QStringLiteral("このホストをknown hostsリストに追加する(&A)");
        disconnectButton |= button->text() == QStringLiteral("接続断(&D)");
    }
    CHECK(fingerprintLabel);
    CHECK(rememberButton);
    CHECK(disconnectButton);

    AuthRequest auth;
    auth.kind = TT_SSH_AUTH_KEYBOARD_INTERACTIVE;
    auth.lines = {{QStringLiteral("Password:"), false}};
    AuthDialog authentication(auth, nullptr, &i18n);
    CHECK(authentication.windowTitle() == QStringLiteral("SSH 認証チャレンジ"));
    const auto *buttons = authentication.findChild<QDialogButtonBox *>();
    CHECK(buttons != nullptr);
    if (buttons) {
        CHECK(buttons->button(QDialogButtonBox::Cancel)->text()
              == QStringLiteral("キャンセル"));
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
    CHECK(dialog.appliedCoreChanges().isEmpty());
    CHECK(dialog.appliedPluginChanges().isEmpty());
}

/// Six settings ship a negative sentinel, and a spin box that cannot hold one
/// is two bugs: it shows the wrong number, and — because `original` is read
/// back off the editor — it silently swallows a real 0 the user asks for,
/// since the box already reads 0 and OK sees no change. `serial.rts`'s -1 is
/// "derive from `ts.Flow`"; taking it as a value holds the line low.
void test_sentinel_defaults_survive_the_settings_dialog()
{
    Harness h;
    SettingsDialog dialog(&h.session);

    const QStringList sentinels {QStringLiteral("serial.rts"),
                                 QStringLiteral("serial.dtr"),
                                 QStringLiteral("window.x"),
                                 QStringLiteral("window.y"),
                                 QStringLiteral("tek.x"),
                                 QStringLiteral("tek.y")};
    for (const QString &name : sentinels) {
        auto *spin = dialog.findChild<QSpinBox *>(
            QStringLiteral("settingEditor:%1").arg(name));
        CHECK(spin != nullptr);
        if (!spin) {
            continue;
        }
        // The live value, not a value clamped into the editor's range.
        CHECK(QString::number(spin->value()) == h.session.setting(name));
        CHECK(spin->minimum() < 0);
    }

    // And an explicit 0 is now expressible: it differs from the sentinel the
    // box opened with, so it applies.
    auto *rts = dialog.findChild<QSpinBox *>(
        QStringLiteral("settingEditor:serial.rts"));
    CHECK(rts != nullptr);
    if (rts) {
        CHECK(rts->value() == -1);
        rts->setValue(0);
        dialog.applyChanges();
        CHECK(h.session.setting(QStringLiteral("serial.rts"))
              == QStringLiteral("0"));
    }
}

void test_settings_dialog_persistence_is_opt_in_and_selective()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const auto write = [](const QString &path, const QByteArray &bytes) {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        CHECK(file.write(bytes) == bytes.size());
    };
    const auto read = [](const QString &path) {
        QFile file(path);
        CHECK(file.open(QIODevice::ReadOnly));
        return file.readAll();
    };
    using Edit = std::function<void(SettingsDialog *)>;

    // Drive the real Setup action through its nested prompt and dialog. An
    // empty `promptButton` means no prompt is expected. The optional warning
    // is the automatic write failing after the live changes were accepted.
    const auto runDialog = [](MainWindow &window, const QString &promptButton,
                              const Edit &edit, bool accept, int *promptCount,
                              bool expectWarning = false,
                              int *warningCount = nullptr,
                              bool expectPromptWarning = false) {
        auto *action =
            window.findChild<QAction *>(QStringLiteral("preferencesAction"));
        CHECK(action != nullptr);
        if (!action) {
            return;
        }
        bool droveDialog = false;
        std::function<void()> finishDialog;
        finishDialog = [&] {
            auto *dialog = qobject_cast<SettingsDialog *>(
                QApplication::activeModalWidget());
            CHECK(dialog != nullptr);
            if (!dialog) {
                return;
            }
            droveDialog = true;
            edit(dialog);
            auto *buttons = dialog->findChild<QDialogButtonBox *>();
            CHECK(buttons != nullptr);
            if (!buttons) {
                dialog->reject();
                return;
            }
            if (accept && expectWarning) {
                QTimer::singleShot(0, [warningCount] {
                    auto *warning = qobject_cast<QMessageBox *>(
                        QApplication::activeModalWidget());
                    CHECK(warning != nullptr);
                    if (warning) {
                        CHECK(warning->text().contains(
                            QStringLiteral("Could not save")));
                        if (warningCount) {
                            (*warningCount)++;
                        }
                        warning->accept();
                    }
                });
            }
            buttons->button(accept ? QDialogButtonBox::Ok
                                   : QDialogButtonBox::Cancel)
                ->click();
        };

        QTimer::singleShot(0, [&] {
            auto *prompt = qobject_cast<QMessageBox *>(
                QApplication::activeModalWidget());
            if (prompt && prompt->objectName()
                              == QStringLiteral("autoSaveSettingsPrompt")) {
                CHECK(!promptButton.isEmpty());
                if (promptCount) {
                    (*promptCount)++;
                }
                CHECK(prompt->defaultButton() != nullptr);
                if (prompt->defaultButton()) {
                    CHECK(prompt->defaultButton()->objectName()
                          == QStringLiteral("autoSaveSettingsManualButton"));
                }
                auto *answer = prompt->findChild<QPushButton *>(
                    promptButton.isEmpty()
                        ? QStringLiteral("autoSaveSettingsManualButton")
                        : promptButton);
                CHECK(answer != nullptr);
                if (expectPromptWarning) {
                    QTimer::singleShot(0, [warningCount, finishDialog] {
                        auto *warning = qobject_cast<QMessageBox *>(
                            QApplication::activeModalWidget());
                        CHECK(warning != nullptr);
                        if (warning) {
                            CHECK(warning->text().contains(
                                QStringLiteral("Could not save")));
                            if (warningCount) {
                                (*warningCount)++;
                            }
                            QTimer::singleShot(0, finishDialog);
                            warning->accept();
                        }
                    });
                } else {
                    QTimer::singleShot(0, finishDialog);
                }
                if (answer) {
                    answer->click();
                } else {
                    prompt->reject();
                }
                return;
            }
            CHECK(promptButton.isEmpty());
            finishDialog();
        });
        action->trigger();
        CHECK(droveDialog);
    };

    // Keeping manual saving is the default answer. It is persisted even when
    // the following dialog is cancelled, is not asked twice in this window or
    // after a restart, and leaves later accepted changes live but unsaved.
    const QString manualPath = dir.filePath(QStringLiteral("manual.ini"));
    int prompts = 0;
    {
        MainWindow window(manualPath);
        runDialog(window, QStringLiteral("autoSaveSettingsManualButton"),
                  [](SettingsDialog *) {}, false, &prompts);
        CHECK(prompts == 1);
        QByteArray bytes = read(manualPath);
        CHECK(bytes.contains("AutoSaveSettings=off"));

        runDialog(
            window, QString(),
            [](SettingsDialog *dialog) {
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(title != nullptr);
                if (title) {
                    title->setText(QStringLiteral("manual live"));
                }
            },
            true, &prompts);
        CHECK(prompts == 1);
        CHECK(window.session()->setting(QStringLiteral("terminal.title"))
              == QStringLiteral("manual live"));
        bytes = read(manualPath);
        CHECK(!bytes.contains("Title=manual live"));
    }
    {
        MainWindow restart(manualPath);
        runDialog(restart, QString(), [](SettingsDialog *) {}, false, &prompts);
        CHECK(prompts == 1);
    }

    // The automatic answer writes only the accepted row, retaining comments,
    // order and unknown keys without pinning untouched schema defaults.
    const QString automaticPath = dir.filePath(QStringLiteral("automatic.ini"));
    const QByteArray automaticBefore =
        "; retained\n[Unrelated]\nKey=kept\n[Tera Term]\nSomethingElse=here\n";
    write(automaticPath, automaticBefore);
    {
        MainWindow window(automaticPath);
        runDialog(
            window, QStringLiteral("autoSaveSettingsEnableButton"),
            [](SettingsDialog *dialog) {
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(title != nullptr);
                if (title) {
                    title->setText(QStringLiteral("automatic saved"));
                }
            },
            true, &prompts);
        CHECK(prompts == 2);
        const QByteArray bytes = read(automaticPath);
        CHECK(bytes.contains("; retained"));
        CHECK(bytes.contains("[Unrelated]\nKey=kept"));
        CHECK(bytes.contains("SomethingElse=here"));
        CHECK(bytes.contains("AutoSaveSettings=on"));
        CHECK(bytes.contains("Title=automatic saved"));
        CHECK(!bytes.contains("TerminalSize="));
        CHECK(bytes.indexOf("SomethingElse=here")
              < bytes.indexOf("Title=automatic saved"));
        CHECK(QDir(dir.path())
                  .entryList(QStringList {QStringLiteral("*_automatic.ini")},
                             QDir::Files)
                  .isEmpty());
    }
    {
        MainWindow restart(automaticPath);
        CHECK(restart.session()->setting(QStringLiteral("terminal.title"))
              == QStringLiteral("automatic saved"));
        runDialog(restart, QString(), [](SettingsDialog *) {}, false, &prompts);
        CHECK(prompts == 2);
    }

    // Cancel applies and writes nothing, even while automatic saving is on.
    const QString cancelPath = dir.filePath(QStringLiteral("cancel.ini"));
    const QByteArray cancelBefore =
        "[Tera Term]\nTitle=before\n[Sterna]\nAutoSaveSettings=on\n";
    write(cancelPath, cancelBefore);
    {
        MainWindow window(cancelPath);
        runDialog(
            window, QString(),
            [](SettingsDialog *dialog) {
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(title != nullptr);
                if (title) {
                    title->setText(QStringLiteral("cancelled"));
                }
            },
            false, &prompts);
        CHECK(window.session()->setting(QStringLiteral("terminal.title"))
              == QStringLiteral("before"));
        CHECK(read(cancelPath) == cancelBefore);
    }

    // The option's final value decides its siblings from the same OK. Turning
    // it on saves all successful changes; turning it off saves only itself.
    const QString toggleOnPath = dir.filePath(QStringLiteral("toggle-on.ini"));
    write(toggleOnPath,
          "[Tera Term]\nTitle=old\n[Sterna]\nAutoSaveSettings=off\n");
    {
        MainWindow window(toggleOnPath);
        runDialog(
            window, QString(),
            [](SettingsDialog *dialog) {
                auto *automatic = dialog->findChild<QCheckBox *>(
                    QStringLiteral("settingEditor:settings.auto_save_changes"));
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(automatic != nullptr);
                CHECK(title != nullptr);
                if (automatic) {
                    automatic->setChecked(true);
                }
                if (title) {
                    title->setText(QStringLiteral("saved together"));
                }
            },
            true, &prompts);
        const QByteArray bytes = read(toggleOnPath);
        CHECK(bytes.contains("AutoSaveSettings=on"));
        CHECK(bytes.contains("Title=saved together"));
    }

    const QString toggleOffPath = dir.filePath(QStringLiteral("toggle-off.ini"));
    write(toggleOffPath,
          "[Tera Term]\nTitle=file value\n[Sterna]\nAutoSaveSettings=on\n");
    {
        MainWindow window(toggleOffPath);
        runDialog(
            window, QString(),
            [](SettingsDialog *dialog) {
                auto *automatic = dialog->findChild<QCheckBox *>(
                    QStringLiteral("settingEditor:settings.auto_save_changes"));
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(automatic != nullptr);
                CHECK(title != nullptr);
                if (automatic) {
                    automatic->setChecked(false);
                }
                if (title) {
                    title->setText(QStringLiteral("live only"));
                }
            },
            true, &prompts);
        CHECK(window.session()->setting(QStringLiteral("terminal.title"))
              == QStringLiteral("live only"));
        const QByteArray bytes = read(toggleOffPath);
        CHECK(bytes.contains("AutoSaveSettings=off"));
        CHECK(bytes.contains("Title=file value"));
        CHECK(!bytes.contains("Title=live only"));
    }

    // A write failure is reported after OK without rolling the successful
    // live change back.
    const QString lockedDir = dir.filePath(QStringLiteral("locked"));
    CHECK(QDir().mkpath(lockedDir));
    const QString failurePath = QDir(lockedDir).filePath(QStringLiteral("failure.ini"));
    const QByteArray failureBefore =
        "[Tera Term]\nTitle=before\n[Sterna]\nAutoSaveSettings=on\n";
    write(failurePath, failureBefore);
    CHECK(QFile::setPermissions(
        lockedDir, QFileDevice::ReadOwner | QFileDevice::ExeOwner));
    int warnings = 0;
    {
        MainWindow window(failurePath);
        runDialog(
            window, QString(),
            [](SettingsDialog *dialog) {
                auto *title = dialog->findChild<QLineEdit *>(
                    QStringLiteral("settingEditor:terminal.title"));
                CHECK(title != nullptr);
                if (title) {
                    title->setText(QStringLiteral("unsaved but live"));
                }
            },
            true, &prompts, true, &warnings);
        CHECK(warnings == 1);
        CHECK(window.session()->setting(QStringLiteral("terminal.title"))
              == QStringLiteral("unsaved but live"));
        CHECK(read(failurePath) == failureBefore);
    }
    CHECK(QFile::setPermissions(lockedDir,
                                QFileDevice::ReadOwner | QFileDevice::WriteOwner
                                    | QFileDevice::ExeOwner));

    // If recording the first answer fails, this window does not nag again,
    // but a new window asks because the file still has no explicit key.
    const QString choiceDir = dir.filePath(QStringLiteral("choice-locked"));
    CHECK(QDir().mkpath(choiceDir));
    const QString choicePath =
        QDir(choiceDir).filePath(QStringLiteral("choice.ini"));
    write(choicePath, "[Tera Term]\nTitle=before\n");
    int choicePrompts = 0;
    int choiceWarnings = 0;
    {
        MainWindow window(choicePath);
        CHECK(QFile::setPermissions(
            choiceDir, QFileDevice::ReadOwner | QFileDevice::ExeOwner));
        runDialog(window, QStringLiteral("autoSaveSettingsEnableButton"),
                  [](SettingsDialog *) {}, false, &choicePrompts, false,
                  &choiceWarnings, true);
        CHECK(choicePrompts == 1);
        CHECK(choiceWarnings == 1);
        CHECK(window.session()->setting(
                  QStringLiteral("settings.auto_save_changes"))
              == QStringLiteral("on"));
        CHECK(!read(choicePath).contains("AutoSaveSettings="));

        runDialog(window, QString(), [](SettingsDialog *) {}, false,
                  &choicePrompts);
        CHECK(choicePrompts == 1);
    }
    CHECK(QFile::setPermissions(choiceDir,
                                QFileDevice::ReadOwner | QFileDevice::WriteOwner
                                    | QFileDevice::ExeOwner));
    {
        MainWindow restart(choicePath);
        runDialog(restart, QStringLiteral("autoSaveSettingsManualButton"),
                  [](SettingsDialog *) {}, false, &choicePrompts);
        CHECK(choicePrompts == 2);
        CHECK(read(choicePath).contains("AutoSaveSettings=off"));
    }
}

/// `AlphaBlendActive` and `AlphaBlend` are two focus states, both expressed
/// as bytes in the file and as a 0.0..1.0 property in Qt. The inactive value
/// is also the active value's fallback when that key is absent.
void test_window_opacity_follows_activation()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("sterna.ini"));
    {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        file.write("[Tera Term]\r\nAlphaBlend=120\r\n");
    }

    MainWindow window(path);
    const auto opacity = [&window] {
        return qRound(window.windowOpacity() * 255.0);
    };

    // Startup uses the active value, which inherited the inactive one here.
    CHECK(opacity() == 120);

    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.opacity_active"),
                                       QStringLiteral("210"), &error));
    CHECK(window.session()->setSetting(QStringLiteral("window.opacity_inactive"),
                                       QStringLiteral("70"), &error));
    CHECK(opacity() == 210);

    QEvent deactivate(QEvent::WindowDeactivate);
    QCoreApplication::sendEvent(&window, &deactivate);
    CHECK(opacity() == 70);

    QEvent activate(QEvent::WindowActivate);
    QCoreApplication::sendEvent(&window, &activate);
    CHECK(opacity() == 210);
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

/// `PopupMenu` hides the ordinary bar and Ctrl+left-click puts the same menu
/// tree under the pointer. `EnablePopupMenu` gates that gesture independently,
/// and `EnableShowMenu` is the route back — three settings whose names make
/// them look like one switch.
void test_the_hidden_menu_is_the_ordinary_menu_as_a_popup()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));
    window.show();
    qApp->processEvents();

    auto *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr);
    CHECK(!window.menuBar()->isHidden());

    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.popup_menu"),
                                       QStringLiteral("on"), &error));
    CHECK(window.menuBar()->isHidden());

    const QPointF local(4, 4);
    const auto ctrlClick = [&] {
        QMouseEvent press(QEvent::MouseButtonPress, local,
                          view->mapToGlobal(local), Qt::LeftButton,
                          Qt::LeftButton, Qt::ControlModifier);
        QCoreApplication::sendEvent(view, &press);
        qApp->processEvents();
        return window.findChild<QMenu *>(QStringLiteral("terminalPopupMenu"));
    };

    QMenu *popup = ctrlClick();
    CHECK(popup != nullptr);
    if (popup) {
        // The File menu's own action, rather than a copy of its text and
        // commands, is associated with the popup too.
        CHECK(popup->actions().contains(window.menuBar()->actions().constFirst()));

        QAction *show = nullptr;
        for (QAction *action : popup->actions()) {
            if (action->text() == QStringLiteral("Show menu bar")) {
                show = action;
            }
        }
        CHECK(show != nullptr);
        if (show) {
            show->trigger();
            CHECK(window.session()->setting(QStringLiteral("window.popup_menu"))
                  == QStringLiteral("off"));
            CHECK(!window.menuBar()->isHidden());
        }
        popup->close();
        popup->deleteLater();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }

    // Hiding the title removes the menu independently of `PopupMenu`, which
    // is why the popup predicate combines the two.
    CHECK(window.session()->setSetting(QStringLiteral("window.hide_title"),
                                       QStringLiteral("on"), &error));
    CHECK(window.menuBar()->isHidden());
    CHECK(window.session()->setSetting(QStringLiteral("window.hide_title"),
                                       QStringLiteral("off"), &error));
    CHECK(!window.menuBar()->isHidden());

    // The popup gate does not show the bar and does not change PopupMenu; it
    // makes the Ctrl-click inert. No temporary QMenu should be constructed.
    CHECK(window.session()->setSetting(QStringLiteral("window.popup_menu"),
                                       QStringLiteral("on"), &error));
    CHECK(window.session()->setSetting(
        QStringLiteral("window.popup_menu_enabled"), QStringLiteral("off"), &error));
    CHECK(ctrlClick() == nullptr);

    CHECK(window.session()->setSetting(
        QStringLiteral("window.popup_menu_enabled"), QStringLiteral("on"), &error));
    CHECK(window.session()->setSetting(
        QStringLiteral("window.show_menu_enabled"), QStringLiteral("off"), &error));
    popup = ctrlClick();
    CHECK(popup != nullptr);
    if (popup) {
        bool hasShow = false;
        for (QAction *action : popup->actions()) {
            hasShow |= action->text() == QStringLiteral("Show menu bar");
        }
        CHECK(!hasShow);
        popup->close();
    }
}

/// The right button adds Copy to upstream's Paste/Paste<CR> menu. Copy follows
/// the selection, while the paste pair follows upstream's availability rules.
/// This port ships `ConfirmPasteMouseRButton` on where upstream ships it off,
/// which is deviation 11; everything else here is upstream's condition.
void test_the_right_button_offers_a_paste_menu()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));
    window.show();
    qApp->processEvents();

    auto *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr);
    QString error;
    QApplication::clipboard()->setText(QStringLiteral("show version"),
                                       QClipboard::Clipboard);

    const QPointF local(4, 4);
    const auto rightPress = [&] {
        QMouseEvent press(QEvent::MouseButtonPress, local,
                          view->mapToGlobal(local), Qt::RightButton,
                          Qt::RightButton, Qt::NoModifier);
        QCoreApplication::sendEvent(view, &press);
        qApp->processEvents();
        return window.findChild<QMenu *>(QStringLiteral("pasteMenu"));
    };
    const auto closeMenu = [](QMenu *menu) {
        menu->close();
        menu->deleteLater();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    };

    // Unconnected there is nothing to paste into, so no menu — upstream's
    // `cv.Ready`, and the reason the gesture cannot be tested without a link.
    CHECK(rightPress() == nullptr);

    // `cat` echoes what it is given, so what comes back on the screen is
    // proof the bytes really left — the point of the menu is the wire, not
    // the widget.
    window.connectPty({QStringLiteral("/bin/sh"), QStringLiteral("-c"),
                       QStringLiteral("cat")});
    qApp->processEvents();
    CHECK(window.session()->isConnected());

    const auto screenHas = [&window](const char *text) {
        QElapsedTimer wait;
        wait.start();
        while (wait.elapsed() < 3000) {
            qApp->processEvents();
            for (int y = 0; y < window.session()->rows(); y++) {
                size_t len = 0;
                const TtCell *row = window.session()->row(y, &len);
                QString line;
                for (size_t x = 0; row && x < len; x++) {
                    if (row[x].width_class == TT_WIDTH_PAD) {
                        continue;
                    }
                    const uint32_t cp = row[x].text[0];
                    line += cp ? QChar(static_cast<char16_t>(cp)) : QLatin1Char(' ');
                }
                if (line.contains(QLatin1String(text))) {
                    return true;
                }
            }
            QThread::msleep(10);
        }
        return false;
    };

    QMenu *menu = rightPress();
    CHECK(menu != nullptr);
    if (menu) {
        // The Edit menu's own actions, not copies of them — the same
        // argument `showPopupMenu` makes, and what keeps the `.lng` text and
        // the `KEYBOARD.CNF` shortcuts attached.
        CHECK(menu->actions().size() == 4);
        auto *copy = window.findChild<QAction *>(QStringLiteral("copyAction"));
        auto *paste = window.findChild<QAction *>(QStringLiteral("pasteAction"));
        auto *pasteCr =
            window.findChild<QAction *>(QStringLiteral("pasteCrAction"));
        CHECK(copy != nullptr && paste != nullptr && pasteCr != nullptr);
        CHECK(menu->actions().contains(copy));
        CHECK(menu->actions().contains(paste));
        CHECK(menu->actions().contains(pasteCr));
        CHECK(copy && !copy->isEnabled());
        CHECK(paste && paste->isEnabled());
        CHECK(pasteCr && pasteCr->isEnabled());
        closeMenu(menu);

        // Paste puts the clipboard on the wire and nothing else. The word has
        // no line break in it, so `ConfirmChangePaste` does not stop here.
        if (paste) {
            paste->trigger();
            CHECK(screenHas("show version"));
        }
        // ...and Paste<CR> adds the Return, which `cat` answers by echoing
        // the line and starting a new one. `ConfirmChangePasteCR` ships on,
        // so this one *is* confirmed — turned off for the assertion, which is
        // also the check that the key reaches this path at all.
        if (pasteCr) {
            CHECK(window.session()->setSetting(
                QStringLiteral("clipboard.confirm_paste_cr"),
                QStringLiteral("off"), &error));
            view->applySettings();
            QApplication::clipboard()->setText(QStringLiteral("second-line"),
                                               QClipboard::Clipboard);
            pasteCr->trigger();
            CHECK(screenHas("second-line"));
            // The CR really went: `cat` echoed the line and the terminal
            // returned to column zero, which a paste without the Return
            // would not have done.
            CHECK(window.session()->cursor().x == 0);
        }
    }

    // A selection raises the menu even with nothing to paste. Copy is live,
    // while Paste and Paste<CR> are greyed out rather than silently doing
    // nothing. The temporary state does not disable their Edit shortcuts once
    // the menu closes.
    window.session()->feed(QByteArray("\r\ncopy me"));
    qApp->processEvents();
    const int row = window.session()->cursor().y;
    const int cw = view->theme().cellWidth();
    const int ch = view->theme().cellHeight();
    const auto left = [&](QEvent::Type type, qreal x) {
        const QPointF point(x, (row + 0.5) * ch);
        const Qt::MouseButtons held =
            type == QEvent::MouseButtonRelease ? Qt::NoButton : Qt::LeftButton;
        QMouseEvent event(type, point, view->mapToGlobal(point), Qt::LeftButton,
                          held, Qt::NoModifier);
        QCoreApplication::sendEvent(view, &event);
    };
    left(QEvent::MouseButtonPress, 0);
    left(QEvent::MouseMove, 6.7 * cw);
    left(QEvent::MouseButtonRelease, 6.7 * cw);
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    menu = rightPress();
    CHECK(menu != nullptr);
    if (menu) {
        auto *copy = window.findChild<QAction *>(QStringLiteral("copyAction"));
        auto *paste = window.findChild<QAction *>(QStringLiteral("pasteAction"));
        auto *pasteCr =
            window.findChild<QAction *>(QStringLiteral("pasteCrAction"));
        CHECK(copy && copy->isEnabled());
        CHECK(paste && !paste->isEnabled());
        CHECK(pasteCr && !pasteCr->isEnabled());
        if (copy) {
            copy->trigger();
            CHECK(QApplication::clipboard()->text(QClipboard::Clipboard)
                  == QStringLiteral("copy me"));
        }
        closeMenu(menu);
        CHECK(copy && copy->isEnabled());
        CHECK(paste && paste->isEnabled());
        CHECK(pasteCr && pasteCr->isEnabled());
    }
    left(QEvent::MouseButtonPress, 0);
    left(QEvent::MouseButtonRelease, 0);

    // With no selection, `DisablePasteMouseRButton` takes the button out of
    // the clipboard's business altogether. A selection would still provide
    // the independent Copy route tested above.
    CHECK(window.session()->setSetting(
        QStringLiteral("clipboard.paste_rbutton_disabled"),
        QStringLiteral("on"), &error));
    view->applySettings();
    CHECK(rightPress() == nullptr);
    CHECK(window.session()->setSetting(
        QStringLiteral("clipboard.paste_rbutton_disabled"),
        QStringLiteral("off"), &error));
    view->applySettings();

    // With no selection, an empty clipboard is upstream's
    // `IsClipboardFormatAvailable` failing: no menu, and no paste on the way
    // up either.
    QApplication::clipboard()->clear(QClipboard::Clipboard);
    CHECK(rightPress() == nullptr);
    QApplication::clipboard()->setText(QStringLiteral("show version"),
                                       QClipboard::Clipboard);

    // With no selection, off is upstream's shipped value and the way back to
    // a right button that pastes the instant it is pressed.
    CHECK(window.session()->setSetting(
        QStringLiteral("clipboard.confirm_paste_rbutton"), QStringLiteral("off"),
        &error));
    view->applySettings();
    CHECK(rightPress() == nullptr);
}

/// One New connection screen for every transport — upstream's `IDD_HOSTDLG`
/// as TTSSH extends it. The radios decide which half is live, the service
/// moves the port, and Details holds everything this port has that upstream's
/// screen does not.
void test_the_connect_dialog_covers_every_transport()
{
    ConnectDialog dialog;

    // SSH is preselected, which is what upstream does with TTSSH enabled. Zero
    // is the ABI's meaningful default: an alias may supply its own Port, with
    // 22 only the fallback when the config supplies none.
    CHECK(dialog.kind() == ConnectDialog::Kind::Ssh);
    CHECK(dialog.port() == 0);
    TtSshParams ssh;
    dialog.fillSsh(&ssh);
    CHECK(ssh.port == 0);
    CHECK(ssh.use_ssh_config);

    auto *telnetService =
        dialog.findChild<QRadioButton *>(QStringLiteral("connectServiceTelnet"));
    auto *otherService =
        dialog.findChild<QRadioButton *>(QStringLiteral("connectServiceOther"));
    auto *serialRadio =
        dialog.findChild<QRadioButton *>(QStringLiteral("connectSerial"));
    CHECK(telnetService && otherService && serialRadio);
    if (!telnetService || !otherService || !serialRadio) {
        return;
    }

    telnetService->setChecked(true);
    CHECK(dialog.kind() == ConnectDialog::Kind::Telnet);
    CHECK(dialog.port() == 23);

    // "Other" is upstream's name for a TCP connection with telnet switched
    // off, which is this port's raw mode. It is a temporary service choice:
    // returning to Telnet restores that service's own mode.
    otherService->setChecked(true);
    TtTelnetParams params;
    dialog.fillTelnet(&params);
    CHECK(params.mode == TT_TELNET_RAW);
    CHECK(dialog.kind() == ConnectDialog::Kind::Telnet);
    telnetService->setChecked(true);
    dialog.fillTelnet(&params);
    CHECK(params.mode == TT_TELNET_NEGOTIATE);

    // The serial half greys the TCP one rather than hiding it, so the shape of
    // the dialog never changes under the pointer.
    auto *tcpHost = dialog.findChild<QComboBox *>(QStringLiteral("connectHost"));
    CHECK(tcpHost != nullptr);
    serialRadio->setChecked(true);
    CHECK(dialog.kind() == ConnectDialog::Kind::Serial);
    CHECK(tcpHost && !tcpHost->isEnabled());

    // Details is collapsed on open — the dialog opens as upstream's — and the
    // page behind it follows whatever is selected.
    auto *details =
        dialog.findChild<QToolButton *>(QStringLiteral("connectDetails"));
    auto *pages =
        dialog.findChild<QStackedWidget *>(QStringLiteral("connectDetailsPages"));
    CHECK(details && pages);
    if (!details || !pages) {
        return;
    }
    // `isHidden`, not `isVisible`: a child of a dialog that has never been
    // shown is invisible whatever its own flag says, so only the explicit hide
    // is observable here.
    CHECK(!details->isChecked());
    CHECK(pages->isHidden());
    details->setChecked(true);
    CHECK(!pages->isHidden());

    // A remembered custom port stops following the service, which is the same
    // rule the telnet mode follows about the port.
    ConnectDialog pinned;
    pinned.setInitialSsh(QStringLiteral("myrouter"), QString(), 2222,
                         QString(), false);
    auto *pinnedTelnet = pinned.findChild<QRadioButton *>(
        QStringLiteral("connectServiceTelnet"));
    CHECK(pinnedTelnet != nullptr);
    CHECK(pinned.host() == QStringLiteral("myrouter"));
    CHECK(pinned.port() == 2222);
    if (pinnedTelnet) {
        pinnedTelnet->setChecked(true);
        CHECK(pinned.port() == 2222);
    }

    // Seeding every details panel must not let SSH overwrite the shared fields
    // when a Telnet panel was what opened the dialog.
    ConnectDialog recentTelnet;
    recentTelnet.selectKind(ConnectDialog::Kind::Telnet);
    recentTelnet.setInitialSsh(QStringLiteral("ssh.example"), QString(), 2222,
                               QString(), false);
    recentTelnet.setInitialTelnet(QStringLiteral("telnet.example"), 2001,
                                  TT_TELNET_AUTO);
    CHECK(recentTelnet.host() == QStringLiteral("telnet.example"));
    CHECK(recentTelnet.port() == 2001);
}

/// The host drop-down is `HistoryList`, which ships off. What it offers is the
/// remembered hosts ahead of the `~/.ssh/config` aliases the combo is seeded
/// with, deduplicated against them.
void test_the_connect_dialog_offers_the_host_history()
{
    ConnectDialog dialog;
    auto *host = dialog.findChild<QComboBox *>(QStringLiteral("connectHost"));
    CHECK(host != nullptr);
    if (!host) {
        return;
    }
    const int seeded = host->count();

    dialog.setHistory({QStringLiteral("router.example"),
                       QStringLiteral("switch.example"),
                       QStringLiteral("router.example")});
    // The duplicate is one entry, not two, and the newest is first.
    CHECK(host->count() == seeded + 2);
    CHECK(host->itemText(0) == QStringLiteral("router.example"));
    CHECK(host->itemText(1) == QStringLiteral("switch.example"));
    // Nothing is preselected: the field is for typing into.
    CHECK(dialog.host().isEmpty());

    CHECK(!dialog.remembersHistory());
    dialog.setRemembersHistory(true);
    CHECK(dialog.remembersHistory());
}

/// The History checkbox and the list it gates are preferences, not state that
/// disappears with this MainWindow. Both go through the narrow settings writer
/// and survive a restart without pinning the rest of the schema into the file.
void test_the_connect_dialog_persists_the_host_history()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("history.ini"));
    MainWindow window(path);
    auto *action = window.findChild<QAction *>(QStringLiteral("connectAction"));
    CHECK(action != nullptr);
    if (!action) {
        return;
    }

    QTimer::singleShot(0, [&] {
        auto *dialog = qobject_cast<ConnectDialog *>(
            QApplication::activeModalWidget());
        CHECK(dialog != nullptr);
        if (!dialog) {
            return;
        }
        dialog->setRemembersHistory(true);
        // The deliberately empty host raises a warning after the connect
        // dialog closes. Dismiss it in its nested event loop.
        QTimer::singleShot(0, [] {
            if (auto *box = qobject_cast<QMessageBox *>(
                    QApplication::activeModalWidget())) {
                box->accept();
            }
        });
        dialog->accept();
    });
    action->trigger();

    QFile saved(path);
    CHECK(saved.open(QIODevice::ReadOnly));
    QByteArray bytes = saved.readAll();
    saved.close();
    CHECK(bytes.contains("HistoryList=on"));

    CHECK(QMetaObject::invokeMethod(
        &window, "rememberHost", Qt::DirectConnection,
        Q_ARG(QString, QStringLiteral("router.example")), Q_ARG(bool, true)));
    CHECK(saved.open(QIODevice::ReadOnly));
    bytes = saved.readAll();
    CHECK(bytes.contains("HostHistory=router.example"));

    MainWindow reopened(path);
    CHECK(reopened.session()->setting(QStringLiteral("connection.history_list"))
          == QStringLiteral("on"));
    CHECK(reopened.session()->setting(QStringLiteral("recent.host_history"))
          == QStringLiteral("router.example"));
}

/// The About item is the visible version check for an installed build. The
/// real application takes this value from the Rust core, so the dialog must
/// read Qt's application version live rather than duplicate it in the shell.
void test_about_shows_the_application_version()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("sterna.ini")));
    auto *about =
        window.findChild<QAction *>(QStringLiteral("aboutAction"));
    CHECK(about != nullptr);
    if (!about) {
        return;
    }

    const QString previous = QCoreApplication::applicationVersion();
    QCoreApplication::setApplicationVersion(QStringLiteral("9.8.7-test"));
    bool inspected = false;
    QTimer::singleShot(0, [&] {
        auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
        CHECK(box != nullptr);
        if (box) {
            CHECK(box->windowTitle() == QStringLiteral("About Sterna"));
            CHECK(box->text().contains(QStringLiteral("Sterna 9.8.7-test")));
            CHECK(!box->iconPixmap().isNull());
            CHECK(box->iconPixmap().size() == QSize(128, 128));
            auto *update = box->findChild<QPushButton *>(
                QStringLiteral("aboutUpdateButton"));
            CHECK(update != nullptr);
            if (update) {
                CHECK(update->text() == QStringLiteral("Check for Updates..."));
            }
            inspected = true;
            box->accept();
        }
    });
    about->trigger();
    CHECK(inspected);
    auto *help = window.findChild<QMenu *>(QStringLiteral("helpMenu"));
    CHECK(help != nullptr);
    if (help) {
        for (QAction *action : help->actions()) {
            CHECK(action->text() != QStringLiteral("Check for Updates..."));
        }
    }
    QCoreApplication::setApplicationVersion(previous);
}

/// The bar under the menu holds no state of its own: its button says what the
/// session says, its checkbox is the live `terminal.local_echo`, and it is on
/// screen while `window.toolbar` is on. Nothing here is upstream's — Tera Term
/// has no toolbar — so the whole contract is that the bar and the menu cannot
/// disagree.
void test_the_connect_bar_is_a_view_of_the_session()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString settingsPath = dir.filePath(QStringLiteral("bar.ini"));
    MainWindow window(settingsPath);
    window.show();
    qApp->processEvents();

    auto *bar = window.findChild<ConnectBar *>();
    CHECK(bar != nullptr);
    auto *connectAction =
        window.findChild<QAction *>(QStringLiteral("connectBarConnect"));
    auto *echoBox =
        window.findChild<QCheckBox *>(QStringLiteral("connectBarLocalEcho"));
    auto *lineBox =
        window.findChild<QCheckBox *>(QStringLiteral("connectBarLineEdit"));
    auto *darkAction =
        window.findChild<QAction *>(QStringLiteral("connectBarDarkMode"));
    auto *darkButton = window.findChild<QToolButton *>(
        QStringLiteral("connectBarDarkModeButton"));
    auto *showAction =
        window.findChild<QAction *>(QStringLiteral("showToolbarAction"));
    auto *status =
        window.findChild<QLabel *>(QStringLiteral("connectionStatus"));
    CHECK(connectAction != nullptr);
    CHECK(echoBox != nullptr);
    CHECK(lineBox != nullptr);
    CHECK(darkAction != nullptr);
    CHECK(darkButton != nullptr);
    CHECK(showAction != nullptr);
    CHECK(status != nullptr);
    if (!bar || !connectAction || !echoBox || !lineBox || !darkAction
        || !darkButton || !showAction || !status) {
        return;
    }

    CHECK(echoBox->toolTip()
          == QStringLiteral("Shows your keystrokes locally. Turn this on when "
                            "the connected device does not echo what you type; "
                            "leave it off if characters appear twice."));
    CHECK(lineBox->toolTip().contains(QStringLiteral("until Enter sends the line")));
    CHECK(darkAction->toolTip().contains(QStringLiteral("terminal views")));
    CHECK(!darkAction->icon().isNull());
    CHECK(darkButton->toolButtonStyle() == Qt::ToolButtonIconOnly);
    CHECK(darkButton->iconSize() == QSize(16, 16));
    // The moon's small star is what keeps a 16 px crescent from reading as a
    // narrow ring. Pin one pixel in its solid centre; the rest of the icon is
    // still reviewed in the window capture below.
    const QImage moon = darkAction->icon()
                            .pixmap(QSize(16, 16))
                            .toImage()
                            .scaled(16, 16, Qt::IgnoreAspectRatio,
                                    Qt::SmoothTransformation);
    CHECK(qAlpha(moon.pixel(13, 3)) != 0);
    window.resize(window.sizeHint());
    qApp->processEvents();
    CHECK(darkButton->geometry().right() > bar->width() - 48);

    CHECK(status->text() == QStringLiteral("not connected"));
    CHECK(status->styleSheet().contains(
        QStringLiteral("background-color: #b71c1c")));
    CHECK(!echoBox->isEnabled());
    CHECK(!lineBox->isEnabled());
    CHECK(darkAction->isEnabled());

    // Dark mode is an appearance preference rather than connection state. It
    // applies to the terminal alone, persists immediately, and remains usable
    // on a blank tab.
    const QPalette windowPalette = window.palette();
    auto *view = window.findChild<TerminalView *>();
    CHECK(view != nullptr);
    if (view) {
        // This window has connected to nothing, so its background carries
        // `color.disconnected_shade` — which is a different question from the
        // one below, and would put the shade into every number here.
        view->theme().setConnected(true);
    }
    darkAction->trigger();
    CHECK(window.session()->setting(QStringLiteral("terminal.dark_mode"))
          == QStringLiteral("on"));
    CHECK(view && view->theme().defaultBackground() == QColor(0x1e, 0x1e, 0x1e));
    CHECK(darkAction->isChecked());
    CHECK(darkAction->toolTip().contains(QStringLiteral("light palette")));
    CHECK(window.palette() == windowPalette);
    QFile saved(settingsPath);
    CHECK(saved.open(QIODevice::ReadOnly));
    CHECK(saved.readAll().contains("DarkMode=on"));
    saved.close();
    darkAction->trigger();
    CHECK(view && view->theme().defaultBackground() == kWhite);

    // Shipped on, and the View item is the same switch as the setting.
    CHECK(!bar->isHidden());
    CHECK(showAction->isChecked());
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("window.toolbar"),
                                       QStringLiteral("off"), &error));
    CHECK(bar->isHidden());
    CHECK(!showAction->isChecked());
    showAction->trigger();
    CHECK(window.session()->setting(QStringLiteral("window.toolbar"))
          == QStringLiteral("on"));
    CHECK(!bar->isHidden());

    // Offline, both boxes display the preference but cannot change it.
    CHECK(!echoBox->isChecked());
    echoBox->click();
    CHECK(window.session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("off"));
    lineBox->click();
    CHECK(window.session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("off"));

    // Once connected they become live controls. Local echo is also assigned
    // by the host through SRM and by scripts, so it is read back rather than
    // remembered by the checkbox.
    const QString connectText = connectAction->text();
    window.connectPty();
    qApp->processEvents();
    CHECK(window.session()->isConnected());
    CHECK(echoBox->isEnabled());
    CHECK(lineBox->isEnabled());
    CHECK(window.session()->setSetting(QStringLiteral("terminal.local_echo"),
                                       QStringLiteral("on"), &error));
    CHECK(echoBox->isChecked());
    echoBox->click();
    CHECK(window.session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("off"));

    // Line edit forces the displayed echo state but does not rewrite the
    // preference hidden under it. Leaving restores that off state.
    CHECK(!lineBox->isChecked());
    lineBox->click();
    CHECK(window.session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("on"));
    CHECK(window.session()->setting(QStringLiteral("terminal.local_echo"))
          == QStringLiteral("off"));
    CHECK(echoBox->isChecked());
    CHECK(!echoBox->isEnabled());
    lineBox->click();
    CHECK(window.session()->setting(QStringLiteral("terminal.line_edit"))
          == QStringLiteral("off"));
    CHECK(!echoBox->isChecked());
    CHECK(echoBox->isEnabled());

    // And the button is whichever of the two the session is, for any kind of
    // session — a local shell has no serial port in it and still disconnects.
    CHECK(connectAction->text() != connectText);
    CHECK(connectAction->isEnabled());
    CHECK(status->styleSheet().isEmpty());
    window.session()->disconnectPort();
    qApp->processEvents();
    CHECK(connectAction->text() == connectText);
    CHECK(!echoBox->isEnabled());
    CHECK(!lineBox->isEnabled());
    CHECK(status->styleSheet().contains(
        QStringLiteral("background-color: #b71c1c")));
}

void test_line_edit_delays_edits_queues_and_reanchors()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("terminal.line_edit"),
                               QStringLiteral("on"), &error));
    CHECK(h.session.setSetting(QStringLiteral("terminal.cr_send"),
                               QStringLiteral("CRLF"), &error));
    CHECK(h.session.setSetting(QStringLiteral("clipboard.confirm_paste"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    h.activate();

    auto *editor = h.view.findChild<QLineEdit *>(
        QStringLiteral("terminalLineEditor"));
    CHECK(editor != nullptr);
    CHECK(h.view.lineEditEnabled());
    CHECK(editor && editor->isVisible());

    key(h.view, Qt::Key_A, Qt::NoModifier, QStringLiteral("a"));
    key(h.view, Qt::Key_C, Qt::NoModifier, QStringLiteral("c"));
    key(h.view, Qt::Key_Left);
    key(h.view, Qt::Key_B, Qt::NoModifier, QStringLiteral("b"));
    CHECK(h.view.lineEditText() == QStringLiteral("abc"));
    CHECK(rowText(h.session, 0).isEmpty());
    CHECK(h.session.cursor().x == 0);

    key(h.view, Qt::Key_Home);
    key(h.view, Qt::Key_Delete);
    CHECK(h.view.lineEditText() == QStringLiteral("bc"));
    key(h.view, Qt::Key_Z, Qt::ControlModifier);
    CHECK(h.view.lineEditText() == QStringLiteral("abc"));
    key(h.view, Qt::Key_A, Qt::ControlModifier);
    key(h.view, Qt::Key_Delete);

    h.view.pasteText(QStringLiteral("one\r\ntwo"));
    CHECK(h.view.lineEditText() == QStringLiteral("one"));
    CHECK(h.view.queuedLineCount() == 1);
    CHECK(rowText(h.session, 0).isEmpty());

    const QRect before = editor ? editor->geometry() : QRect();
    h.session.feed(QByteArrayLiteral("prompt\r\n"));
    CHECK(h.view.lineEditText() == QStringLiteral("one"));
    CHECK(editor && editor->geometry().top() > before.top());

    key(h.view, Qt::Key_Return);
    CHECK(rowText(h.session, 1).contains(QStringLiteral("one")));
    CHECK(h.view.lineEditText() == QStringLiteral("two"));
    CHECK(h.view.queuedLineCount() == 0);
    key(h.view, Qt::Key_Return);
    CHECK(rowText(h.session, 2).contains(QStringLiteral("two")));
    CHECK(!h.view.hasLineEditDraft());
}

void test_line_edit_drains_its_forced_echo_damage()
{
    Harness h;
    int repaints = 0;
    QObject::connect(&h.session, &Session::damaged, [&repaints] { repaints++; });

    h.session.sendEditedLine(QStringLiteral("x"));
    CHECK(repaints == 1);
    CHECK(rowText(h.session, 0).startsWith(QLatin1Char('x')));

    // Local echo still ships off. If the edited-line event were left queued,
    // this unrelated send would drain it and report a duplicate repaint.
    repaints = 0;
    h.session.sendText(QStringLiteral("y"));
    CHECK(repaints == 0);
}

void test_line_edit_keeps_control_input_immediate_and_cleans_up()
{
    Harness h;
    QString error;
    CHECK(h.session.setSetting(QStringLiteral("terminal.local_echo"),
                               QStringLiteral("on"), &error));
    CHECK(h.session.setSetting(QStringLiteral("terminal.line_edit"),
                               QStringLiteral("on"), &error));
    CHECK(h.session.setSetting(QStringLiteral("clipboard.confirm_paste"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();

    key(h.view, Qt::Key_X, Qt::NoModifier, QStringLiteral("x"));
    CHECK(h.view.lineEditText() == QStringLiteral("x"));
    const size_t row = h.session.cursor().y;
    key(h.view, Qt::Key_J, Qt::ControlModifier, QStringLiteral("\n"));
    CHECK(h.session.cursor().y == row + 1);
    CHECK(h.view.lineEditText() == QStringLiteral("x"));

    // A connection edge discards both the visible line and queued lines.
    h.view.pasteText(QStringLiteral("\nnext"));
    CHECK(h.view.hasLineEditDraft());
    h.session.connectionChanged();
    CHECK(!h.view.hasLineEditDraft());

    // Generic setting changes have no modal UI and discard immediately.
    key(h.view, Qt::Key_Y, Qt::NoModifier, QStringLiteral("y"));
    CHECK(h.view.hasLineEditDraft());
    CHECK(h.session.setSetting(QStringLiteral("terminal.line_edit"),
                               QStringLiteral("off"), &error));
    h.view.applySettings();
    CHECK(!h.view.lineEditEnabled());
    CHECK(!h.view.hasLineEditDraft());
}

void test_line_edit_toggle_confirms_an_unsent_draft()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("line-edit.ini")));
    window.show();
    qApp->processEvents();

    auto *line = window.findChild<QCheckBox *>(
        QStringLiteral("connectBarLineEdit"));
    auto *view = window.findChild<TerminalView *>();
    CHECK(line != nullptr);
    CHECK(view != nullptr);
    if (!line || !view) {
        return;
    }
    window.connectPty();
    qApp->processEvents();
    CHECK(window.session()->isConnected());
    CHECK(line->isEnabled());
    line->click();
    key(*view, Qt::Key_X, Qt::NoModifier, QStringLiteral("x"));

    bool sawCancel = false;
    QTimer::singleShot(0, [&] {
        auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
        CHECK(box != nullptr);
        if (box) {
            CHECK(box->objectName() == QStringLiteral("lineEditDiscardDialog"));
            sawCancel = true;
            box->done(QMessageBox::Cancel);
        }
    });
    line->click();
    CHECK(sawCancel);
    CHECK(line->isChecked());
    CHECK(view->lineEditText() == QStringLiteral("x"));

    bool sawDiscard = false;
    QTimer::singleShot(0, [&] {
        auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
        CHECK(box != nullptr);
        if (box) {
            sawDiscard = true;
            box->done(QMessageBox::Discard);
        }
    });
    line->click();
    CHECK(sawDiscard);
    CHECK(!line->isChecked());
    CHECK(!view->hasLineEditDraft());
}

/// `ClearOnResize` is about the terminal's *size*, and toggling a frontend
/// setting does not change it. The screen used to be scrolled into history on
/// every settings change because `Vt::set_config` resized unconditionally and
/// the flag defeats `Grid::resize`'s early return — see the trap in AGENTS.md.
void test_a_frontend_toggle_does_not_clear_on_resize()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    MainWindow window(dir.filePath(QStringLiteral("clear-on-resize.ini")));
    window.show();
    qApp->processEvents();

    auto *line = window.findChild<QCheckBox *>(
        QStringLiteral("connectBarLineEdit"));
    auto *view = window.findChild<TerminalView *>();
    CHECK(line != nullptr && view != nullptr);
    if (!line || !view) {
        return;
    }

    QString error;
    CHECK(window.session()->setSetting(
        QStringLiteral("terminal.clear_on_resize"), QStringLiteral("on"),
        &error));
    view->applySettings();

    window.connectPty();
    qApp->processEvents();
    CHECK(window.session()->isConnected());

    window.session()->feed(QByteArray("still here\r\n"));
    qApp->processEvents();
    const int before = window.session()->scrollbackLen();

    // The reported gesture: the connect bar's checkbox, nothing else.
    line->click();
    qApp->processEvents();
    CHECK(view->lineEditEnabled());
    CHECK(window.session()->scrollbackLen() == before);

    // And back off, which goes through the same path.
    line->click();
    qApp->processEvents();
    CHECK(!view->lineEditEnabled());
    CHECK(window.session()->scrollbackLen() == before);

    size_t len = 0;
    const TtCell *row = window.session()->row(0, &len);
    QString first;
    for (size_t x = 0; row && x < len; x++) {
        const uint32_t cp = row[x].text[0];
        first += cp ? QChar(static_cast<char16_t>(cp)) : QLatin1Char(' ');
    }
    // `contains`, not equality: the far end is a real login shell and its
    // prompt arrives when it arrives, so whether it shares this line is a race
    // and not the property. What is being asserted is that the text is still
    // on the page at all — the two scrollback checks above say it did not
    // scroll, and this says it was not erased in place.
    CHECK(first.contains(QStringLiteral("still here")));
}

/// `AutoWinClose` is decided in the core, but only the frontend owns a
/// window. The request closes an ordinary window and honours upstream's
/// IsWindowEnabled guard when a modal child has disabled its parent.
void test_an_auto_close_request_respects_window_state()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());

    {
        MainWindow window(dir.filePath(QStringLiteral("close.ini")));
        window.show();
        qApp->processEvents();
        CHECK(window.isVisible());
        window.session()->closeRequested();
        CHECK(!window.isVisible());
    }

    {
        MainWindow window(dir.filePath(QStringLiteral("disabled.ini")));
        window.show();
        window.setEnabled(false);
        qApp->processEvents();
        window.session()->closeRequested();
        CHECK(window.isVisible());
        window.setEnabled(true);
        window.close();
    }
}

/// `VTPos` is always read, but is written only when `SaveVTWinPos` is on. A
/// full Save setup captures every setting plus the live geometry; closing the
/// window writes the geometry alone.
void test_window_geometry_has_full_and_close_only_saves()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const auto read = [](const QString &path) {
        QFile file(path);
        CHECK(file.open(QIODevice::ReadOnly));
        return file.readAll();
    };
    const auto write = [](const QString &path, const QByteArray &bytes) {
        QFile file(path);
        CHECK(file.open(QIODevice::WriteOnly));
        CHECK(file.write(bytes) == bytes.size());
    };

    // Save setup: current position and current grid size, not the values from
    // the last load, alongside the rest of the changed settings. Its default-
    // on backup is the exact pre-save file, in a timestamped sibling.
    const QString fullPath = dir.filePath(QStringLiteral("full.ini"));
    const QByteArray fullBefore =
        "[Tera Term]\r\nSaveVTWinPos=on\r\nVTPos=120,80\r\n"
        "TerminalSize=80,24\r\nTitle=before\r\n";
    write(fullPath, fullBefore);
    {
        MainWindow window(fullPath);
        CHECK(window.pos() == QPoint(120, 80));
        QString error;
        CHECK(window.session()->setSetting(QStringLiteral("terminal.title"),
                                           QStringLiteral("after"), &error));
        window.resize(window.sizeHint());
        window.show();
        qApp->processEvents();
        window.move(210, 160);

        auto *view = window.findChild<TerminalView *>();
        CHECK(view != nullptr);
        if (view) {
            window.resize(window.size()
                          + QSize(5 * view->theme().cellWidth(),
                                  2 * view->theme().cellHeight()));
            qApp->processEvents();
        }
        const int cols = window.session()->cols();
        const int rows = window.session()->rows();

        QAction *save = nullptr;
        for (QAction *action : window.findChildren<QAction *>()) {
            if (action->text() == QStringLiteral("Save setup")) {
                save = action;
            }
        }
        CHECK(save != nullptr);
        if (save) {
            save->trigger();
        }

        const QByteArray bytes = read(fullPath);
        CHECK(bytes.contains("VTPos=210,160"));
        const QByteArray expectedSize =
            QStringLiteral("TerminalSize=%1,%2").arg(cols).arg(rows).toUtf8();
        CHECK(bytes.contains(expectedSize));
        CHECK(bytes.contains("Title=after"));

        const QStringList backups =
            QDir(dir.path()).entryList({QStringLiteral("*_full.ini")}, QDir::Files);
        CHECK(backups.size() == 1);
        if (backups.size() == 1) {
            CHECK(read(dir.filePath(backups.constFirst())) == fullBefore);
        }

        // Keep the scope's teardown from exercising the close-only arm too;
        // that arm has its own file below.
        CHECK(window.session()->setSetting(QStringLiteral("window.save_position"),
                                           QStringLiteral("off"), &error));
        window.close();
    }

    // The switch is live rather than a hardwired safety copy.
    const QString noBackupPath = dir.filePath(QStringLiteral("no-backup.ini"));
    write(noBackupPath,
          "[Tera Term]\r\nIniAutoBackup=off\r\nTerminalSize=80,24\r\n");
    {
        MainWindow window(noBackupPath);
        QAction *save = nullptr;
        for (QAction *action : window.findChildren<QAction *>()) {
            if (action->text() == QStringLiteral("Save setup")) {
                save = action;
            }
        }
        CHECK(save != nullptr);
        if (save) {
            save->trigger();
        }
        CHECK(QDir(dir.path())
                  .entryList({QStringLiteral("*_no-backup.ini")}, QDir::Files)
                  .isEmpty());
    }

    // Window close: geometry changes, an unrelated in-memory setting does not,
    // and no defaults are added to a deliberately small file.
    const QString closePath = dir.filePath(QStringLiteral("close.ini"));
    write(closePath,
          "[Tera Term]\r\nSaveVTWinPos=on\r\nVTPos=20,30\r\n"
          "TerminalSize=80,24\r\nTitle=before\r\n");
    int closeCols = 0;
    int closeRows = 0;
    {
        MainWindow window(closePath);
        QString error;
        CHECK(window.session()->setSetting(QStringLiteral("terminal.title"),
                                           QStringLiteral("not-written"), &error));
        window.resize(window.sizeHint());
        window.show();
        qApp->processEvents();
        window.move(220, 170);
        window.resize(window.size() + QSize(30, 20));
        qApp->processEvents();
        closeCols = window.session()->cols();
        closeRows = window.session()->rows();
        CHECK(window.close());
    }
    const QByteArray closed = read(closePath);
    CHECK(closed.contains("VTPos=220,170"));
    const QByteArray expectedCloseSize = QStringLiteral("TerminalSize=%1,%2")
                                             .arg(closeCols)
                                             .arg(closeRows)
                                             .toUtf8();
    CHECK(closed.contains(expectedCloseSize));
    CHECK(closed.contains("Title=before"));
    CHECK(!closed.contains("not-written"));
    CHECK(!closed.contains("CRReceive="));

    // The switch gates writing, not reading. An old position is still applied
    // with it off, and closing leaves even its quotes untouched.
    const QString offPath = dir.filePath(QStringLiteral("off.ini"));
    const QByteArray off =
        "[Tera Term]\r\nSaveVTWinPos=off\r\nVTPos='12,34'\r\n";
    write(offPath, off);
    {
        MainWindow window(offPath);
        CHECK(window.pos() == QPoint(12, 34));
        window.show();
        window.move(300, 200);
        CHECK(window.close());
    }
    CHECK(read(offPath) == off);

    // Upstream forgets a position that has fallen off the virtual desktop,
    // but tolerates twenty pixels above or left of it and clamps those back to
    // the edge (`vtdisp.c:1517`).
    QScreen *screen = QGuiApplication::primaryScreen();
    CHECK(screen != nullptr);
    if (screen) {
        const QRect desktop = screen->virtualGeometry();
        const QString nearPath = dir.filePath(QStringLiteral("near.ini"));
        write(nearPath,
              QStringLiteral("[Tera Term]\r\nVTPos=%1,%2\r\n")
                  .arg(desktop.x() - 20)
                  .arg(desktop.y() - 20)
                  .toUtf8());
        MainWindow nearWindow(nearPath);
        CHECK(nearWindow.pos() == desktop.topLeft());

        const QPoint lost(desktop.x() + desktop.width() + 1, desktop.y());
        const QString lostPath = dir.filePath(QStringLiteral("lost.ini"));
        write(lostPath,
              QStringLiteral("[Tera Term]\r\nVTPos=%1,%2\r\n")
                  .arg(lost.x())
                  .arg(lost.y())
                  .toUtf8());
        MainWindow lostWindow(lostPath);
        CHECK(lostWindow.pos() != lost);
    }
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
    test_sixel_is_painted_and_later_text_erases_it();
    test_sgr_background_colours();
    test_truecolor_resolves_through_upstreams_search();
    test_ansi_palette_changes_the_search_and_the_painter_together();
    test_dark_mode_changes_only_the_terminal_palette();
    test_an_idle_terminal_is_a_different_shade();
    test_osc_colours_reach_the_painter();
    test_reverse_and_screen_reverse();
    test_a_visual_bell_inverts_the_screen_and_puts_it_back();
    test_bold_has_its_own_colour();
    test_a_wide_character_covers_two_cells();
    test_dec_special_graphics_draws_a_line();
    test_the_cursor_is_drawn_where_the_core_says();
    test_an_unfocused_cursor_is_hollow();
    test_cursor_shape_is_live_terminal_state();
    test_cursor_blinks_unless_the_live_style_is_steady();
    test_local_echo_reaches_the_screen_without_waiting_for_the_host();
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
    test_clear_commands_keep_or_drop_selection();
    test_the_edit_menu_and_key_map_share_clear_commands();
    test_continued_line_copy_joins_a_wrapped_line();
    test_line_edit_copy_uses_the_active_selection();
    test_the_other_buttons_do_not_start_or_copy_a_selection();
    test_a_paste_with_a_line_break_is_confirmed();
    test_remote_clipboard_access_is_permissioned_and_notified();
    test_dragging_off_the_edge_scrolls_the_view();
    test_settings_change_the_painted_colours();
    test_url_colour_and_underline_are_independent();
    test_the_font_attribute_switches_are_independent_of_the_colours();
    test_font_quality_and_resizing_are_live_settings();
    test_vt_font_space_changes_the_cell_and_glyph_origin();
    test_attribute_colours_can_keep_the_normal_background();
    test_use_text_colour_repairs_only_the_three_same_colour_pairs();
    test_the_settings_dialog_is_built_from_the_schema();
    test_the_setup_menu_opens_one_settings_dialog();
    test_the_view_menu_owns_the_three_switches();
    test_the_about_dialog_carries_the_release_page();
    test_the_settings_dialog_uses_a_language_catalog();
    test_the_connection_dialogs_use_the_language_catalog();
    test_the_ssh_prompts_use_the_language_catalog();
    test_the_dialog_writes_only_what_changed();
    test_sentinel_defaults_survive_the_settings_dialog();
    test_settings_dialog_persistence_is_opt_in_and_selective();
    test_window_opacity_follows_activation();
    test_the_window_opens_at_the_configured_size();
    test_the_hidden_menu_is_the_ordinary_menu_as_a_popup();
    test_the_right_button_offers_a_paste_menu();
    test_about_shows_the_application_version();
    test_the_connect_bar_is_a_view_of_the_session();
    test_line_edit_delays_edits_queues_and_reanchors();
    test_line_edit_drains_its_forced_echo_damage();
    test_line_edit_keeps_control_input_immediate_and_cleans_up();
    test_line_edit_toggle_confirms_an_unsent_draft();
    test_a_frontend_toggle_does_not_clear_on_resize();
    test_an_auto_close_request_respects_window_state();
    test_window_geometry_has_full_and_close_only_saves();
    test_the_connect_dialog_covers_every_transport();
    test_the_connect_dialog_offers_the_host_history();
    test_the_connect_dialog_persists_the_host_history();

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
            // A never-shown dialog renders before its layout has run, and one
            // `adjustSize` fixes the overlapping labels — but `adjustSize` also
            // caps the result at two thirds of the screen, and the offscreen
            // platform's screen is 800x800. So the size it asked for is put back
            // afterwards, or this dumps a picture of the cap.
            dialog.adjustSize();
            dialog.resize(dialog.sizeHint());
            const QString dialogPath = dir + "/settings.png";
            dialog.grab().save(dialogPath);
            printf("wrote %s\n", qPrintable(dialogPath));

            // And the window, for the menu bar and the connect bar under it —
            // the same argument. Its own settings file, so this is a picture of
            // the defaults rather than of whoever ran it.
            QTemporaryDir home;
            MainWindow window(home.filePath(QStringLiteral("shot.ini")));
            window.resize(820, 420);
            window.show();
            qApp->processEvents();
            window.session()->feed(QByteArray("sterna\r\n"));
            qApp->processEvents();
            const QString windowPath = dir + "/window.png";
            window.grab().save(windowPath);
            printf("wrote %s\n", qPrintable(windowPath));
        }
    }

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("render ok\n");
    return 0;
}
