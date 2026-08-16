// Where a line ends, and what a window narrower than its terminal shows.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
// `terminal.size_follows_window` is `TermIsWin`, and until it was honoured this
// port behaved as though it were permanently on: every window drag was a
// `Grid::resize`, and a `Grid::resize` that narrows truncates every line it
// shortens — the page and the scrollback alike, with nothing to put the ends
// back. The property this file exists for is the one in
// `test_a_narrowed_window_keeps_its_text`: with the switch off, the same drag
// costs nothing at all.
//
// A real `MainWindow`, because the interesting half is a window question: which
// of the two numbers a resize is allowed to move, and whether the scrollbar
// that covers the difference arrives without taking columns to pay for itself.

#include <QAction>
#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QMouseEvent>
#include <QScrollBar>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>
#include <cstring>

#include "MainWindow.h"
#include "Session.h"
#include "SizeIndicator.h"
#include "TerminalPage.h"
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

/// Run the event loop until `done` or the deadline — `buttons_test`'s helper.
/// A timer is the thing under test in one case here, so waiting for it has to
/// be the loop actually running rather than a sleep.
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

/// A window with its own settings file — see `AGENTS.md`, and `gutter_test`,
/// which takes the same precaution for the same reason: anything constructing a
/// `MainWindow` otherwise reads the developer's own `sterna.ini`, terminal size
/// included.
struct Harness {
    QTemporaryDir home;
    MainWindow window { home.filePath(QStringLiteral("wrap.ini")) };

    TerminalView *view = nullptr;
    QScrollBar *hbar = nullptr;

    Harness()
    {
        view = window.findChild<TerminalView *>();
        hbar = window.findChild<QScrollBar *>(
            QStringLiteral("terminalHScrollBar"));
        window.resize(900, 500);
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

    /// Make the window `cells` terminal columns narrower than it is now. In
    /// pixels, because that is the only thing a window drag moves; whether it
    /// reaches the grid is the question every test here is asking.
    void narrowBy(int cells)
    {
        window.resize(window.width() - cells * view->theme().cellWidth(),
                      window.height());
        settle();
    }

    /// Drag out a selection in widget pixels — `render_test`'s helper, kept to
    /// the two events a selection needs. Widget pixels on purpose: the point of
    /// the test is that the view turns them into the right terminal column.
    void dragCells(int fromCell, int toCell)
    {
        const int cw = view->theme().cellWidth();
        const int y = view->theme().cellHeight() / 2;
        const auto send = [&](QEvent::Type type, int x, Qt::MouseButtons held) {
            QMouseEvent ev(type, QPointF(x, y), QPointF(x, y), Qt::LeftButton,
                           held, Qt::NoModifier);
            QCoreApplication::sendEvent(view, &ev);
        };
        send(QEvent::MouseButtonPress, fromCell * cw, Qt::LeftButton);
        send(QEvent::MouseMove, toCell * cw, Qt::LeftButton);
        send(QEvent::MouseButtonRelease, toCell * cw, Qt::NoButton);
        settle();
    }

    int cols() const { return window.session()->cols(); }
    QString setting(const char *name) const
    {
        return window.session()->setting(QString::fromLatin1(name));
    }
};

/// The page's text, one string per row — `buttons_test`'s helper, which reads
/// the core's own cells rather than the painter's guess at them.
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

void test_the_terminal_is_the_window_by_default()
{
    Harness h;
    CHECK(h.view != nullptr);
    CHECK(h.view->sizeFollowsWindow());
    CHECK(h.setting("terminal.size_follows_window") == QLatin1String("on"));

    const int before = h.cols();
    CHECK(before > 20);
    h.narrowBy(6);
    // The terminal followed, which is what this switch being on means — and
    // there is nothing to scroll sideways, because there is no sideways.
    CHECK(h.cols() == before - 6);
    CHECK(h.view->visibleCols() == h.cols());
    CHECK(h.hbar != nullptr);
    CHECK(h.hbar && h.hbar->isHidden());
    CHECK(h.view->originX() == 0);
}

void test_a_narrowed_window_keeps_its_text()
{
    Harness h;
    // A line as wide as the terminal, made of a repeating pattern so that its
    // right-hand end can be told from its left.
    const int width = h.cols();
    QByteArray line;
    for (int i = 0; i < width; i++) {
        line.append(char('a' + i % 26));
    }
    h.feed(line.constData());
    const QString before = screenText(*h.window.session());

    h.set("terminal.size_follows_window", QStringLiteral("off"));
    CHECK(!h.view->sizeFollowsWindow());
    CHECK(h.cols() == width);

    h.narrowBy(20);
    // **The whole point.** The window is twenty columns narrower and the
    // terminal is not, so not one character of that line has been cut.
    CHECK(h.cols() == width);
    CHECK(screenText(*h.window.session()) == before);
    // And the setting did not follow the window down either, which is what
    // would make the loss permanent on the next settings change.
    CHECK(h.setting("terminal.cols").toInt() == width);

    // What covers the difference. The range is upstream's: the columns that do
    // not fit, and no more (`vtdisp.c:3070`).
    CHECK(h.hbar && !h.hbar->isHidden());
    CHECK(h.hbar && h.hbar->minimum() == 0);
    CHECK(h.hbar && h.hbar->maximum() == width - h.view->visibleCols());
    CHECK(h.hbar && h.hbar->maximum() > 0);

    // Widening it back is not a repair — nothing was broken — so the text is
    // still the text, and the bar goes when it has nothing left to cover.
    h.narrowBy(-20);
    CHECK(h.cols() == width);
    CHECK(screenText(*h.window.session()) == before);
    CHECK(h.hbar && h.hbar->isHidden());
}

void test_the_bar_moves_the_origin_and_the_pointer_follows()
{
    Harness h;
    const int width = h.cols();
    QByteArray line;
    for (int i = 0; i < width; i++) {
        line.append(char('a' + i % 26));
    }
    h.feed(line.constData());
    h.set("terminal.size_follows_window", QStringLiteral("off"));
    h.narrowBy(20);
    CHECK(h.hbar && !h.hbar->isHidden());

    h.hbar->setValue(12);
    h.settle();
    CHECK(h.view->originX() == 12);

    // The pointer names the cell it is over, not the one that would be there if
    // the leftmost column on screen were column 0 — asked through a real drag,
    // which is the same conversion the painter and the host's own mouse
    // reporting go through. The fourth cell on screen is terminal column 15,
    // and the pattern makes column 15 a `p`.
    h.dragCells(3, 4);
    CHECK(h.view->selectedText() == QLatin1String("p"));

    // Past the end there is nothing to give: the origin stops where the last
    // column reaches the right edge.
    h.hbar->setValue(h.hbar->maximum() + 50);
    h.settle();
    CHECK(h.view->originX() == width - h.view->visibleCols());
}

void test_turning_it_back_on_makes_the_terminal_the_window()
{
    Harness h;
    const int width = h.cols();
    h.set("terminal.size_follows_window", QStringLiteral("off"));
    h.narrowBy(20);
    CHECK(h.cols() == width);
    h.view->setOriginX(10);
    h.settle();

    h.set("terminal.size_follows_window", QStringLiteral("on"));
    // The terminal came to the window rather than the window going to the
    // terminal — a toggle in the View menu is about the window somebody is
    // looking at. There is then nothing to scroll and nowhere to be scrolled
    // to.
    CHECK(h.cols() == width - 20);
    CHECK(h.view->originX() == 0);
    CHECK(h.hbar && h.hbar->isHidden());
}

void test_a_window_wider_than_its_terminal_letterboxes()
{
    Harness h;
    const int width = h.cols();
    h.set("terminal.size_follows_window", QStringLiteral("off"));
    h.narrowBy(-15);
    // The terminal keeps the width it was frozen at. The surplus is background,
    // not columns: growing the terminal here would make `terminal.cols` follow
    // the window after all, one drag at a time.
    CHECK(h.cols() == width);
    CHECK(h.setting("terminal.cols").toInt() == width);
    CHECK(h.view->visibleCols() == width);
    CHECK(h.hbar && h.hbar->isHidden());
}

void test_the_menu_item_and_the_setting_are_one_switch()
{
    Harness h;
    auto *action =
        h.window.findChild<QAction *>(QStringLiteral("breakLinesAction"));
    CHECK(action != nullptr);
    if (!action) {
        return;
    }
    CHECK(action->isCheckable());
    CHECK(action->isChecked());

    // From the menu.
    action->trigger();
    h.settle();
    CHECK(h.setting("terminal.size_follows_window") == QLatin1String("off"));
    CHECK(!h.view->sizeFollowsWindow());

    // And from anywhere else — the settings dialog, a script, a hand-edited
    // file — with the tick following, because they are the same switch.
    h.set("terminal.size_follows_window", QStringLiteral("on"));
    CHECK(action->isChecked());
    CHECK(h.view->sizeFollowsWindow());
}

void test_the_freeze_is_written_down()
{
    Harness h;
    h.narrowBy(9);
    const int frozen = h.cols();
    auto *action =
        h.window.findChild<QAction *>(QStringLiteral("breakLinesAction"));
    CHECK(action != nullptr);
    if (!action) {
        return;
    }
    action->trigger();
    h.settle();

    // Turning it off from the menu puts the live width in the file, so the
    // terminal the window has just stopped following is still that wide after a
    // restart. Without this the file would carry the switch and somebody else's
    // `TerminalSize`.
    QFile file(h.home.filePath(QStringLiteral("wrap.ini")));
    CHECK(file.open(QIODevice::ReadOnly));
    const QByteArray text = file.readAll();
    CHECK(text.contains(QByteArray("TerminalSize=")
                        + QByteArray::number(frozen)));
    CHECK(text.contains("TermIsWin=off"));
}

void test_the_size_shows_while_the_window_changes()
{
    Harness h;
    auto *box = h.window.findChild<QWidget *>(QStringLiteral("sizeIndicator"));
    CHECK(box != nullptr);
    if (!box) {
        return;
    }
    // Opening a window is a run of resize events like any other, and none of
    // them is news: the first measurement is the baseline.
    CHECK(box->isHidden());

    h.narrowBy(7);
    CHECK(!box->isHidden());
    const auto *label = qobject_cast<const SizeIndicator *>(box);
    CHECK(label != nullptr);
    CHECK(label
          && label->text()
                 == QStringLiteral("%1x%2").arg(h.cols()).arg(
                     h.window.session()->rows()));
    // In the middle of the terminal, which is where an eye watching the text
    // already is.
    CHECK(qAbs(box->geometry().center().x() - h.view->width() / 2) <= 1);
    CHECK(qAbs(box->geometry().center().y() - h.view->height() / 2) <= 1);

    // And gone once the resizing stops. `spin` rather than a sleep: the timer
    // is the thing under test.
    CHECK(spin([&] { return box->isHidden(); }, 4000));
}

void test_a_fixed_terminal_reports_both_pairs()
{
    Harness h;
    const int width = h.cols();
    h.set("terminal.size_follows_window", QStringLiteral("off"));
    h.narrowBy(11);

    const auto *label =
        h.window.findChild<SizeIndicator *>(QStringLiteral("sizeIndicator"));
    CHECK(label != nullptr);
    // The visible pair is what moved; the terminal's pair is what still decides
    // where a line ends. A single pair could not say which of the two it was.
    CHECK(label
          && label->text()
                 == QStringLiteral("%1x%2 of %3x%4")
                        .arg(h.view->visibleCols())
                        .arg(h.window.session()->rows())
                        .arg(width)
                        .arg(h.window.session()->rows()));
}

void test_the_readout_can_be_switched_off()
{
    Harness h;
    h.set("window.show_terminal_size", QStringLiteral("off"));
    auto *box = h.window.findChild<QWidget *>(QStringLiteral("sizeIndicator"));
    CHECK(box != nullptr);
    h.narrowBy(7);
    CHECK(box && box->isHidden());
}

} // namespace

int main(int argc, char **argv)
{
    // Before `QApplication`, so the windows below cannot read the developer's
    // real settings — see `Harness`.
    QStandardPaths::setTestModeEnabled(true);
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORM")) {
        qputenv("QT_QPA_PLATFORM", "offscreen");
    }
    QApplication app(argc, argv);

    test_the_terminal_is_the_window_by_default();
    test_a_narrowed_window_keeps_its_text();
    test_the_bar_moves_the_origin_and_the_pointer_follows();
    test_turning_it_back_on_makes_the_terminal_the_window();
    test_a_window_wider_than_its_terminal_letterboxes();
    test_the_menu_item_and_the_setting_are_one_switch();
    test_the_freeze_is_written_down();
    test_the_size_shows_while_the_window_changes();
    test_a_fixed_terminal_reports_both_pairs();
    test_the_readout_can_be_switched_off();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("wrap ok\n");
    return 0;
}
