// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "TerminalView.h"

#include <cstring>

#include <QApplication>
#include <QClipboard>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QTimer>
#include <QWheelEvent>

#include "DecGraphics.h"
#include "Session.h"

namespace {

/// The shortest gap between frames, in milliseconds — 125 a second, which is
/// above every display refresh rate this will meet. See `requestRepaint`.
constexpr qint64 kMinFrameMs = 8;

/// How often a drag held outside the window scrolls it, in milliseconds.
constexpr int kAutoScrollMs = 40;

/// What ends a word, for double-click selection.
///
/// Upstream's `ts.DelimList` default, verbatim (`ttset.c:1167`): a space and
/// every ASCII punctuation mark **except** underscore. So `some_name` is one
/// word and `some-name` is three, which looks arbitrary until you notice that
/// a path, a flag and a hostname all want the hyphen to break.
constexpr const char *kDelimiters = " !\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~";

/// The text one cell draws: the base character plus its combining marks.
QString cellText(const TtCell &cell)
{
    QString out;
    for (int i = 0; i < TT_CELL_TEXT_MAX; i++) {
        uint32_t cp = cell.text[i];
        if (cp == 0) {
            break;
        }
        if (i == 0 && (cell.attrs & TT_ATTR_SPECIAL)) {
            // The grid keeps the raw byte and an attribute bit rather than a
            // box-drawing codepoint, because upstream's `DecSpMappingDir`
            // defaults to "do not map". Drawing the line is our job.
            cp = decSpecialToUnicode(cp);
        }
        out += QString::fromUcs4(reinterpret_cast<const char32_t *>(&cp), 1);
    }
    if (out.isEmpty()) {
        out = QStringLiteral(" ");
    }
    return out;
}

int cellWidthClass(const TtCell &cell)
{
    return cell.width_class == TT_WIDTH_WIDE ? 2 : 1;
}

TtModifiers modifiersOf(Qt::KeyboardModifiers m)
{
    TtModifiers mods;
    mods.shift = (m & Qt::ShiftModifier) != 0;
    mods.alt = (m & Qt::AltModifier) != 0;
    mods.ctrl = (m & Qt::ControlModifier) != 0;
    return mods;
}

/// Qt key to a `TtKey`, for the keys that have one.
///
/// F1-F5 map to xterm's `XF1`-`XF5` rather than to DEC's PF1-PF4. DEC put
/// PF1-PF4 where a PC keyboard has F1-F4, which is why the two numbering
/// schemes exist at all; every host this will meet on Linux expects xterm's.
bool mapKey(const QKeyEvent *e, TtKey *out)
{
    const bool keypad = (e->modifiers() & Qt::KeypadModifier) != 0;

    if (keypad) {
        switch (e->key()) {
        case Qt::Key_0: *out = TT_KEY_KP0; return true;
        case Qt::Key_1: *out = TT_KEY_KP1; return true;
        case Qt::Key_2: *out = TT_KEY_KP2; return true;
        case Qt::Key_3: *out = TT_KEY_KP3; return true;
        case Qt::Key_4: *out = TT_KEY_KP4; return true;
        case Qt::Key_5: *out = TT_KEY_KP5; return true;
        case Qt::Key_6: *out = TT_KEY_KP6; return true;
        case Qt::Key_7: *out = TT_KEY_KP7; return true;
        case Qt::Key_8: *out = TT_KEY_KP8; return true;
        case Qt::Key_9: *out = TT_KEY_KP9; return true;
        case Qt::Key_Minus: *out = TT_KEY_KP_MINUS; return true;
        case Qt::Key_Plus: *out = TT_KEY_KP_PLUS; return true;
        case Qt::Key_Asterisk: *out = TT_KEY_KP_ASTERISK; return true;
        case Qt::Key_Slash: *out = TT_KEY_KP_SLASH; return true;
        case Qt::Key_Period: *out = TT_KEY_KP_PERIOD; return true;
        case Qt::Key_Comma: *out = TT_KEY_KP_COMMA; return true;
        case Qt::Key_Enter: *out = TT_KEY_KP_ENTER; return true;
        default: break;
        }
    }

    switch (e->key()) {
    case Qt::Key_Up: *out = TT_KEY_UP; return true;
    case Qt::Key_Down: *out = TT_KEY_DOWN; return true;
    case Qt::Key_Right: *out = TT_KEY_RIGHT; return true;
    case Qt::Key_Left: *out = TT_KEY_LEFT; return true;

    // The VT220 editing keypad, in the places a PC keyboard puts it.
    case Qt::Key_Home: *out = TT_KEY_FIND; return true;
    case Qt::Key_Insert: *out = TT_KEY_INSERT; return true;
    case Qt::Key_Delete: *out = TT_KEY_REMOVE; return true;
    case Qt::Key_End: *out = TT_KEY_SELECT; return true;
    case Qt::Key_PageUp: *out = TT_KEY_PREV; return true;
    case Qt::Key_PageDown: *out = TT_KEY_NEXT; return true;

    case Qt::Key_F1: *out = TT_KEY_XF1; return true;
    case Qt::Key_F2: *out = TT_KEY_XF2; return true;
    case Qt::Key_F3: *out = TT_KEY_XF3; return true;
    case Qt::Key_F4: *out = TT_KEY_XF4; return true;
    case Qt::Key_F5: *out = TT_KEY_XF5; return true;
    case Qt::Key_F6: *out = TT_KEY_F6; return true;
    case Qt::Key_F7: *out = TT_KEY_F7; return true;
    case Qt::Key_F8: *out = TT_KEY_F8; return true;
    case Qt::Key_F9: *out = TT_KEY_F9; return true;
    case Qt::Key_F10: *out = TT_KEY_F10; return true;
    case Qt::Key_F11: *out = TT_KEY_F11; return true;
    case Qt::Key_F12: *out = TT_KEY_F12; return true;
    case Qt::Key_F13: *out = TT_KEY_F13; return true;
    case Qt::Key_F14: *out = TT_KEY_F14; return true;
    case Qt::Key_F15: *out = TT_KEY_HELP; return true;
    case Qt::Key_F16: *out = TT_KEY_DO; return true;
    case Qt::Key_F17: *out = TT_KEY_F17; return true;
    case Qt::Key_F18: *out = TT_KEY_F18; return true;
    case Qt::Key_F19: *out = TT_KEY_F19; return true;
    case Qt::Key_F20: *out = TT_KEY_F20; return true;

    case Qt::Key_Backtab: *out = TT_KEY_BACK_TAB; return true;
    // Keypad Enter without the keypad modifier, which some layouts report.
    case Qt::Key_Enter: *out = TT_KEY_KP_ENTER; return true;
    default: return false;
    }
}

} // namespace

TerminalView::TerminalView(Session *session, QWidget *parent)
    : QWidget(parent)
    , m_session(session)
{
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setCursor(Qt::IBeamCursor);
    // Every pixel is painted every time, so there is nothing for Qt to clear
    // first — and the clear is a full-window fill we would immediately
    // overwrite.
    setAttribute(Qt::WA_OpaquePaintEvent);
    // CJK is deferred, and an input context that is never fed only costs
    // startup time and a preedit that cannot be committed.
    setAttribute(Qt::WA_InputMethodEnabled, false);

    // Only ever runs while output is arriving faster than the frame floor
    // below, and stops itself at the next frame. Same shape as the session's
    // pending-out retry: a timer that exists during a burst and not otherwise.
    m_repaint = new QTimer(this);
    m_repaint->setSingleShot(true);
    connect(m_repaint, &QTimer::timeout, this, [this] { update(); });

    // Alive only while a drag is held outside the window, which is the only
    // way to select more than a screenful.
    m_autoScroll = new QTimer(this);
    m_autoScroll->setInterval(kAutoScrollMs);
    connect(m_autoScroll, &QTimer::timeout, this, [this] { dragTo(m_dragPos); });

    connect(m_session, &Session::damaged, this, [this] {
        // A resize that arrived in the byte stream — DECCOLM, or a telnet
        // NAWS from the far end — re-flows every line, so a selection made
        // against the old width is pointing at text that has moved. The
        // frontend's own resize path clears it in `refit`; this is the half
        // that arrives without the widget changing size.
        if (m_hasSelection && m_selSize != QSize(m_session->cols(), m_session->rows())) {
            clearSelection();
        }
        requestRepaint();
        // Output can move the offset — the core keeps a scrolled-back view on
        // the same lines — so the scrollbar has to hear about every pump, not
        // only about the scrolls this widget made.
        emit viewChanged();
    });

    m_session->setCellPixels(m_theme.cellWidth(), m_theme.cellHeight());
}

QSize TerminalView::sizeHint() const
{
    // The terminal's size, not a constant: `TerminalSize` in the settings is
    // read before the window is laid out, so this is what makes a configured
    // 132x50 open at 132x50.
    return sizeForCells(m_session->cols(), m_session->rows());
}

void TerminalView::applySettings()
{
    m_theme.applySettings(*m_session);
    update();
}

void TerminalView::applyFont(const QFont &font)
{
    m_theme.setFont(font);
    m_session->setCellPixels(m_theme.cellWidth(), m_theme.cellHeight());
    refit();
    update();
}

// --- painting ----------------------------------------------------------------

/// Repaint now, or at the frame floor — whichever is later.
///
/// The session pumps once per wake of its notifier, so a burst arrives as one
/// damage per 8 KB read, each on its own turn of the event loop. Without a
/// floor that is **one frame per read**: 10 MB of `cat` painted 3,000 times,
/// and since a frame costs about as much as parsing 8 KB, the transfer takes
/// roughly twice as long as it needs to.
///
/// Wayland already coalesces about eight reads into a frame, through its own
/// frame callbacks. X11 has no such brake — measured at 4 MB/s against
/// Wayland's 39 on the same machine — and neither does the offscreen platform,
/// which is why a headless measurement *understates* the desktop rather than
/// flattering it. `bench/README.md` has the table.
///
/// So: a floor, not a timer in the idle path. An idle window has not painted
/// for a long time, so a keystroke still repaints on the spot — measured at
/// 1.03 ms, unchanged by this — and the timer only exists while output is
/// outrunning 125 frames a second.
void TerminalView::requestRepaint()
{
    const qint64 since = m_sincePaint.isValid() ? m_sincePaint.elapsed() : kMinFrameMs;
    if (since >= kMinFrameMs) {
        update();
        return;
    }
    if (!m_repaint->isActive()) {
        m_repaint->start(int(kMinFrameMs - since));
    }
}

void TerminalView::paintEvent(QPaintEvent *)
{
    m_repaint->stop();
    m_sincePaint.restart();

    QPainter p(this);
    const int cw = m_theme.cellWidth();
    const int ch = m_theme.cellHeight();
    const bool screenReverse = m_session->reverseVideo();
    const int rows = m_session->rows();

    // Whatever is not grid — the few pixels left over when the window is not
    // an exact multiple of the cell size.
    p.fillRect(rect(), screenReverse ? QColor(Qt::black) : m_theme.defaultBackground());

    // Once per frame, not once per row: expanding a word selection reads the
    // line it is on, and this is the path a screenful of output repaints
    // through.
    SelPoint selFirst;
    SelPoint selLast;
    const bool selected = selectionRange(&selFirst, &selLast);

    for (int y = 0; y < rows; y++) {
        size_t len = 0;
        const TtCell *cells = m_session->row(y, &len);
        if (!cells) {
            continue;
        }

        // Which columns of *this* row are highlighted, once for the row rather
        // than a lookup per cell.
        int selFrom = 0;
        int selTo = 0;
        if (selected) {
            const quint64 line = m_session->lineAt(y);
            if (line >= selFirst.line && line <= selLast.line) {
                selFrom = (line == selFirst.line) ? selFirst.x : 0;
                selTo = (line == selLast.line) ? selLast.x : m_session->cols();
            }
        }

        // One `drawText` per run of cells that look alike. Real console output
        // is mostly long runs of one colour, so this is a large win over a
        // call per cell — and it is only safe because the font was given
        // letter spacing that makes its advance exactly one cell, so a long
        // run cannot drift off the grid. See `Theme::recomputeMetrics`.
        int runStart = -1;
        int runCells = 0;
        QString runText;
        QColor runFg;
        QColor runBg;
        bool runBold = false;
        bool runUnder = false;

        auto flush = [&]() {
            if (runStart < 0) {
                return;
            }
            const QRect box(runStart * cw, y * ch, runCells * cw, ch);
            p.fillRect(box, runBg);
            p.setPen(runFg);
            p.setFont(runBold ? m_theme.boldFont() : m_theme.font());
            p.drawText(QPoint(box.left(), y * ch + m_theme.baseline()), runText);
            if (runUnder) {
                const int uy = y * ch + m_theme.baseline() + 1;
                p.drawLine(box.left(), uy, box.right(), uy);
            }
            runStart = -1;
            runCells = 0;
            runText.clear();
        };

        for (int x = 0; x < static_cast<int>(len);) {
            const TtCell &cell = cells[x];
            if (cell.width_class == TT_WIDTH_PAD) {
                // The right half of a wide character, reached only if the lead
                // cell was somehow skipped. It holds no text and zeroed
                // attributes, so painting it would put a default-coloured
                // block in the middle of a coloured one.
                x++;
                continue;
            }

            QColor fg;
            QColor bg;
            m_theme.resolve(cell, x >= selFrom && x < selTo, screenReverse, &fg, &bg);
            const bool bold = (cell.attrs & TT_ATTR_BOLD) != 0;
            const bool under = (cell.attrs & TT_ATTR_UNDER) != 0;
            const int width = cellWidthClass(cell);

            const bool joins = runStart >= 0 && fg == runFg && bg == runBg &&
                               bold == runBold && under == runUnder && width == 1;
            if (!joins) {
                flush();
                runStart = x;
                runFg = fg;
                runBg = bg;
                runBold = bold;
                runUnder = under;
            }
            runText += cellText(cell);
            runCells += width;
            x += width;
            if (width == 2) {
                // A wide glyph advances by its own metrics, not by the letter
                // spacing that keeps narrow runs on the grid, so it is drawn
                // alone in its two-cell box.
                flush();
            }
        }
        flush();
    }

    // The cursor last, so it is never painted over by the row it sits on.
    //
    // Its row comes from the core rather than from `cur.y`: the cursor belongs
    // to the live screen, so scrolling back moves it down and eventually off
    // the bottom, and painting `cur.y` would stamp a cursor onto a line of
    // history it has nothing to do with.
    const TtCursor cur = m_session->cursor();
    const int cursorRow = m_session->cursorViewRow();
    if (cur.visible && cursorRow >= 0) {
        size_t len = 0;
        const TtCell *cells = m_session->row(cursorRow, &len);
        const int cx = static_cast<int>(cur.x);
        if (cells && cx < static_cast<int>(len)) {
            const TtCell &cell = cells[cx];
            const int width = cellWidthClass(cell);
            const QRect box(cx * cw, cursorRow * ch, width * cw, ch);
            if (hasFocus()) {
                QColor fg;
                QColor bg;
                m_theme.resolve(cell, false, screenReverse, &fg, &bg);
                p.fillRect(box, fg);
                p.setPen(bg);
                p.setFont((cell.attrs & TT_ATTR_BOLD) ? m_theme.boldFont() : m_theme.font());
                p.drawText(QPoint(box.left(), cursorRow * ch + m_theme.baseline()),
                           cellText(cell));
            } else {
                // Hollow when the window is not focused, which is the
                // convention every terminal uses to say "typing goes
                // elsewhere".
                p.setPen(m_theme.cursorColor());
                p.drawRect(box.adjusted(0, 0, -1, -1));
            }
        }
    }
}

// --- geometry ----------------------------------------------------------------

QSize TerminalView::sizeForCells(int cols, int rows) const
{
    return QSize(cols * m_theme.cellWidth(), rows * m_theme.cellHeight());
}

void TerminalView::resizeEvent(QResizeEvent *)
{
    refit();
}

void TerminalView::refit()
{
    const int cols = qBound(1, width() / m_theme.cellWidth(), TT_BUFF_X_MAX);
    const int rows = qMax(1, height() / m_theme.cellHeight());
    if (cols != m_session->cols() || rows != m_session->rows()) {
        clearSelection();
        m_session->resize(cols, rows);
    }
}

/// The character boundary nearest a widget position.
///
/// `buffer.c:GetCharCell` with the wide-character arm folded in: the pointer
/// snaps to the start of the character it is over, or past its end once it is
/// beyond the middle of it. Selecting `abc` therefore means dragging from
/// before the `a` to after the `c`, rather than one character further, and a
/// wide character is taken or left whole.
SelPoint TerminalView::cellAt(const QPointF &pos) const
{
    const int cols = m_session->cols();
    const int row = qBound(0, static_cast<int>(pos.y()) / m_theme.cellHeight(),
                           m_session->rows() - 1);
    int x = qBound(0, static_cast<int>(pos.x()) / m_theme.cellWidth(), cols - 1);

    size_t len = 0;
    const TtCell *cells = m_session->row(row, &len);
    // Step back off the padding half, so a click anywhere in a wide character
    // is a decision about that character rather than about the cell it landed
    // in.
    if (cells && x < static_cast<int>(len) && cells[x].width_class == TT_WIDTH_PAD &&
        x > 0) {
        x--;
    }
    return SelPoint {m_session->lineAt(row), x};
}

SelPoint TerminalView::boundaryAt(const QPointF &pos) const
{
    const int cw = m_theme.cellWidth();
    const int cols = m_session->cols();
    SelPoint out = cellAt(pos);

    size_t len = 0;
    const TtCell *cells = m_session->line(out.line, &len);
    const int width = (cells && out.x < static_cast<int>(len) &&
                       cells[out.x].width_class == TT_WIDTH_WIDE)
                          ? 2
                          : 1;
    const int px = qBound(0, static_cast<int>(pos.x()), cols * cw - 1);
    if (px >= out.x * cw + width * cw / 2) {
        out.x = qMin(out.x + width, cols);
    }
    return out;
}

// --- keyboard ----------------------------------------------------------------

void TerminalView::keyPressEvent(QKeyEvent *event)
{
    const Qt::KeyboardModifiers mods = event->modifiers();

    // Scrolling the history, before anything else looks at these keys —
    // PageUp is otherwise a `TtKey` and would go to the host.
    if (mods.testFlag(Qt::ShiftModifier)) {
        const int page = qMax(1, m_session->rows() - 1);
        if (event->key() == Qt::Key_PageUp) {
            setViewOffset(m_session->viewOffset() + page);
            return;
        }
        if (event->key() == Qt::Key_PageDown) {
            setViewOffset(m_session->viewOffset() - page);
            return;
        }
    }

    // Typing goes to the live screen, so show it. Every terminal does this,
    // and the alternative — typing blind into a screen you cannot see — is
    // worse than losing your place in the history.
    if (m_session->viewOffset() != 0 && !mods.testFlag(Qt::ControlModifier)) {
        setViewOffset(0);
    }

    // The two clipboard bindings, which have to be checked before anything
    // sends: Ctrl+C on its own is an interrupt and must stay one.
    if ((mods & Qt::ControlModifier) && (mods & Qt::ShiftModifier)) {
        if (event->key() == Qt::Key_C) {
            copySelection();
            return;
        }
        if (event->key() == Qt::Key_V) {
            pasteClipboard();
            return;
        }
    }

    TtKey key;
    if (mapKey(event, &key)) {
        m_session->sendKey(key);
        return;
    }

    switch (event->key()) {
    case Qt::Key_Return:
        // Not a `TtKey`: upstream handles VK_RETURN outside its key table and
        // marks it text so that newline mode applies. `send_text` does that.
        m_session->sendText(QStringLiteral("\r"));
        return;
    case Qt::Key_Backspace:
        // Also outside the key table, and also state-dependent: DECBKM decides
        // between BS and DEL. Sending the wrong one erases nothing and the
        // host beeps, which reads as a broken keyboard rather than as a mode.
        m_session->sendText(QString(QChar(m_session->backspaceSendsBs() ? 0x08 : 0x7F)));
        return;
    default:
        break;
    }

    QString text = event->text();
    if (!text.isEmpty()) {
        // Alt as Meta: an ESC prefix, which is what every Linux line editor
        // and Emacs expects. Tera Term's `ts.MetaKey` ships off, so this is a
        // deliberate divergence rather than an oversight — it becomes a
        // setting when the schema exists.
        if ((mods & Qt::AltModifier) && !(mods & Qt::ControlModifier)) {
            text.prepend(QChar(0x1B));
        }
        m_session->sendText(text);
        return;
    }

    // Qt hands back no text for Ctrl+Space, and NUL is a control character
    // people genuinely send.
    if ((mods & Qt::ControlModifier) && event->key() == Qt::Key_Space) {
        m_session->sendText(QString(QChar(QChar::Null)));
        return;
    }

    QWidget::keyPressEvent(event);
}

// --- mouse -------------------------------------------------------------------

void TerminalView::mousePressEvent(QMouseEvent *event)
{
    uint8_t button = TT_BUTTON_LEFT;
    if (event->button() == Qt::MiddleButton) {
        button = TT_BUTTON_MIDDLE;
    } else if (event->button() == Qt::RightButton) {
        button = TT_BUTTON_RIGHT;
    }

    // Always offered to the terminal first. Whether it wants it depends on the
    // tracking mode *and* on Ctrl being held, which upstream uses to keep
    // ctrl-click available for selection — so the core's answer is the one to
    // branch on rather than a mode check of our own.
    const QPointF p = event->position();
    if (m_session->mouse(TT_MOUSE_EVENT_PRESS, button, static_cast<int>(p.x()),
                         static_cast<int>(p.y()), modifiersOf(event->modifiers()))) {
        return;
    }

    if (event->button() == Qt::MiddleButton) {
        // The X11 convention, and the one thing every Linux terminal user
        // reaches for without thinking.
        const QClipboard *clip = QApplication::clipboard();
        const QString sel = clip->supportsSelection()
                                ? clip->text(QClipboard::Selection)
                                : clip->text();
        m_session->paste(sel);
        return;
    }

    if (event->button() == Qt::LeftButton) {
        // A third press soon after a double click is a triple click, which Qt
        // does not deliver as an event of its own.
        const QPoint cell(static_cast<int>(p.x()) / m_theme.cellWidth(),
                          static_cast<int>(p.y()) / m_theme.cellHeight());
        const bool run = m_sinceClick.isValid() && cell == m_clickPos &&
                         m_sinceClick.elapsed() < QApplication::doubleClickInterval();
        m_clicks = (run && m_clicks == 2) ? 3 : 1;
        m_clickPos = cell;
        m_sinceClick.restart();

        m_selUnit = m_clicks == 3 ? SelUnit::Line : SelUnit::Char;
        startSelection(m_clicks == 3 ? cellAt(p) : boundaryAt(p), p);
    }
}

/// Begin a drag at `at`, with the anchor covering the whole unit around it.
void TerminalView::startSelection(SelPoint at, const QPointF &pos)
{
    m_selecting = true;
    m_selSize = QSize(m_session->cols(), m_session->rows());
    m_selAnchor = unitStart(at);
    m_selAnchorEnd = unitEnd(m_selUnit == SelUnit::Char ? at : SelPoint {at.line, at.x + 1});
    m_selHead = m_selAnchorEnd;
    // A single click selects nothing until it moves; a double or triple click
    // has already selected the word or the line under it.
    m_hasSelection = m_selUnit != SelUnit::Char;
    m_dragPos = pos;
    update();
}

void TerminalView::mouseMoveEvent(QMouseEvent *event)
{
    const QPointF p = event->position();
    if (m_selecting) {
        dragTo(p);
        return;
    }

    uint8_t button = TT_BUTTON_RELEASE;
    if (event->buttons() & Qt::LeftButton) {
        button = TT_BUTTON_LEFT;
    } else if (event->buttons() & Qt::MiddleButton) {
        button = TT_BUTTON_MIDDLE;
    } else if (event->buttons() & Qt::RightButton) {
        button = TT_BUTTON_RIGHT;
    }
    m_session->mouse(TT_MOUSE_EVENT_MOVE, button, static_cast<int>(p.x()),
                     static_cast<int>(p.y()), modifiersOf(event->modifiers()));
}

void TerminalView::mouseReleaseEvent(QMouseEvent *event)
{
    if (m_selecting) {
        m_selecting = false;
        m_autoScroll->stop();
        if (m_hasSelection) {
            // Copy to the primary selection on release, so middle-click paste
            // works between this window and every other X11 application.
            QClipboard *clip = QApplication::clipboard();
            if (clip->supportsSelection()) {
                clip->setText(selectedText(), QClipboard::Selection);
            }
        }
        return;
    }

    uint8_t button = TT_BUTTON_LEFT;
    if (event->button() == Qt::MiddleButton) {
        button = TT_BUTTON_MIDDLE;
    } else if (event->button() == Qt::RightButton) {
        button = TT_BUTTON_RIGHT;
    }
    const QPointF p = event->position();
    m_session->mouse(TT_MOUSE_EVENT_RELEASE, button, static_cast<int>(p.x()),
                     static_cast<int>(p.y()), modifiersOf(event->modifiers()));
}

void TerminalView::mouseDoubleClickEvent(QMouseEvent *event)
{
    const QPointF p = event->position();
    if (event->button() != Qt::LeftButton ||
        m_session->mouse(TT_MOUSE_EVENT_PRESS, TT_BUTTON_LEFT, static_cast<int>(p.x()),
                         static_cast<int>(p.y()), modifiersOf(event->modifiers()))) {
        return;
    }

    // The second press of the run. Qt sends this *instead of* a press, so the
    // counter has to be advanced here or a triple click never reaches three.
    m_clicks = 2;
    m_clickPos = QPoint(static_cast<int>(p.x()) / m_theme.cellWidth(),
                        static_cast<int>(p.y()) / m_theme.cellHeight());
    m_sinceClick.restart();

    m_selUnit = SelUnit::Word;
    startSelection(cellAt(p), p);
}

void TerminalView::wheelEvent(QWheelEvent *event)
{
    const int delta = event->angleDelta().y();
    if (delta == 0) {
        event->ignore();
        return;
    }
    // `vtwin.cpp:2542` passes `zDelta < 0`, so 0 is up and 1 is down.
    const uint8_t button = delta > 0 ? 0 : 1;
    const QPointF p = event->position();
    // Offered to the terminal first: a full-screen application that asked for
    // mouse tracking wants the wheel, and `less` scrolling its own buffer is
    // not the same thing as us scrolling ours.
    if (m_session->mouse(TT_MOUSE_EVENT_WHEEL, button, static_cast<int>(p.x()),
                         static_cast<int>(p.y()), modifiersOf(event->modifiers()))) {
        event->accept();
        return;
    }

    const int lines = qMax(1, QApplication::wheelScrollLines());
    const int steps = delta / 120 != 0 ? delta / 120 : (delta > 0 ? 1 : -1);
    setViewOffset(m_session->viewOffset() + steps * lines);
    event->accept();
}

/// Extend the drag, scrolling the view while the pointer is off the edge.
///
/// Held outside the window it keeps scrolling, which is the only way to select
/// more than one screenful. The head is re-read from the pointer position each
/// time rather than moved by a line, so it stays on the edge the pointer is
/// past while the text underneath it moves.
void TerminalView::dragTo(const QPointF &pos)
{
    m_dragPos = pos;

    const int step = pos.y() < 0 ? 1 : (pos.y() >= height() ? -1 : 0);
    if (step != 0) {
        setViewOffset(m_session->viewOffset() + step);
        if (!m_autoScroll->isActive()) {
            m_autoScroll->start();
        }
    } else {
        m_autoScroll->stop();
    }

    const SelPoint head = boundaryAt(pos);
    if (head == m_selHead) {
        return;
    }
    m_selHead = head;
    if (m_selUnit == SelUnit::Char) {
        m_hasSelection = !(head == m_selAnchor);
    }
    update();
}

void TerminalView::setViewOffset(int offset)
{
    const int before = m_session->viewOffset();
    m_session->setViewOffset(offset);
    if (m_session->viewOffset() == before) {
        return;
    }
    // The selection is *not* dropped here. It is held in absolute line
    // numbers, so scrolling moves the text and the highlight together — which
    // is the whole reason the core can name a line at all.
    update();
    emit viewChanged();
}

void TerminalView::focusInEvent(QFocusEvent *event)
{
    m_session->focus(true);
    QWidget::focusInEvent(event);
    update();
}

void TerminalView::focusOutEvent(QFocusEvent *event)
{
    m_session->focus(false);
    QWidget::focusOutEvent(event);
    update();
}

// --- selection ---------------------------------------------------------------

namespace {

/// Whether a cell ends a word. An erased cell holds nothing and counts as the
/// space it is drawn as.
bool isDelimiter(const TtCell &cell)
{
    const uint32_t cp = cell.text[0];
    if (cp >= 0x80) {
        return false;
    }
    const char c = cp == 0 ? ' ' : static_cast<char>(cp);
    return std::strchr(kDelimiters, c) != nullptr;
}

/// Widen `[from, to)` to the word under `at` on `cells`.
///
/// `buffer.c:CheckDelimiterChar`, which has two rules rather than one. Starting
/// on a delimiter takes the run of *that same character* — so double-clicking
/// the gap between two columns of a table selects the gap, not the table.
/// Starting anywhere else takes the run of non-delimiters, and stops as well
/// where the character width changes, which is upstream's `DelimDBCS` and is on
/// by default.
void wordAt(const TtCell *cells, int len, int at, int *from, int *to)
{
    at = qBound(0, at, len - 1);
    if (cells[at].width_class == TT_WIDTH_PAD && at > 0) {
        at--;
    }
    const bool delim = isDelimiter(cells[at]);
    // Erased cells hold nothing and are drawn as spaces, so they compare as
    // one — otherwise the run of blanks after a line stops at the first cell
    // the host never wrote to.
    const auto glyph = [](const TtCell &c) { return c.text[0] == 0 ? uint32_t(' ') : c.text[0]; };
    const uint32_t start = glyph(cells[at]);
    const bool startWide = cells[at].width_class == TT_WIDTH_WIDE;

    auto joins = [&](int x) {
        if (cells[x].width_class == TT_WIDTH_PAD) {
            return true;
        }
        if (delim) {
            return glyph(cells[x]) == start;
        }
        return !isDelimiter(cells[x]) &&
               (cells[x].width_class == TT_WIDTH_WIDE) == startWide;
    };

    *from = at;
    while (*from > 0 && joins(*from - 1)) {
        (*from)--;
    }
    // Padding always joins, so the walk can stop on the padding half of a wide
    // character whose lead it then refused. Step over it: half a character is
    // not a word boundary.
    if (cells[*from].width_class == TT_WIDTH_PAD) {
        (*from)++;
    }
    *to = at + 1;
    while (*to < len && joins(*to)) {
        (*to)++;
    }
}

} // namespace

SelPoint TerminalView::unitStart(SelPoint p) const
{
    if (m_selUnit == SelUnit::Line) {
        p.x = 0;
        return p;
    }
    size_t len = 0;
    const TtCell *cells = m_session->line(p.line, &len);
    if (m_selUnit == SelUnit::Char || !cells || len == 0) {
        return p;
    }
    int from = 0;
    int to = 0;
    wordAt(cells, static_cast<int>(len), p.x, &from, &to);
    p.x = from;
    return p;
}

SelPoint TerminalView::unitEnd(SelPoint p) const
{
    if (m_selUnit == SelUnit::Line) {
        p.x = m_session->cols();
        return p;
    }
    size_t len = 0;
    const TtCell *cells = m_session->line(p.line, &len);
    if (m_selUnit == SelUnit::Char || !cells || len == 0) {
        return p;
    }
    int from = 0;
    int to = 0;
    // A boundary at `x` ends the character at `x - 1`.
    wordAt(cells, static_cast<int>(len), qMax(0, p.x - 1), &from, &to);
    p.x = to;
    return p;
}

/// The selection as an ordered pair.
///
/// The anchor is kept as the *whole* unit the drag started on rather than as
/// the point it started at, which is what makes dragging leftwards out of a
/// double-clicked word keep the right-hand edge of that word. Upstream keeps
/// the same pair for the same reason (`DblClkStart`/`DblClkEnd`).
bool TerminalView::selectionRange(SelPoint *from, SelPoint *to) const
{
    if (!m_hasSelection) {
        return false;
    }
    *from = m_selAnchor;
    *to = m_selAnchorEnd;
    if (m_selHead < *from) {
        *from = unitStart(m_selHead);
    } else if (*to < m_selHead) {
        *to = unitEnd(m_selHead);
    }
    return true;
}

QString TerminalView::selectedText() const
{
    SelPoint a;
    SelPoint b;
    if (!selectionRange(&a, &b)) {
        return QString();
    }

    QStringList lines;
    for (quint64 n = a.line; n <= b.line; n++) {
        size_t len = 0;
        const TtCell *cells = m_session->line(n, &len);
        if (!cells) {
            // Aged out of the scrollback while it was selected. Skipping it
            // beats inventing a blank line where text used to be.
            continue;
        }
        const int from = (n == a.line) ? a.x : 0;
        const int to = qMin((n == b.line) ? b.x : static_cast<int>(len),
                            static_cast<int>(len));
        QString line;
        for (int x = from; x < to; x++) {
            if (cells[x].width_class == TT_WIDTH_PAD) {
                continue;
            }
            line += cellText(cells[x]);
        }
        // Trailing blanks are padding, not content — a terminal line is always
        // full width, and copying the padding turns every paste into a
        // rectangle.
        while (line.endsWith(QLatin1Char(' '))) {
            line.chop(1);
        }
        lines << line;
    }
    return lines.join(QLatin1Char('\n'));
}

void TerminalView::clearSelection()
{
    if (m_hasSelection || m_selecting) {
        m_hasSelection = false;
        m_selecting = false;
        m_autoScroll->stop();
        update();
    }
}

void TerminalView::copySelection() const
{
    const QString text = selectedText();
    if (!text.isEmpty()) {
        QApplication::clipboard()->setText(text, QClipboard::Clipboard);
    }
}

void TerminalView::pasteClipboard()
{
    m_session->paste(QApplication::clipboard()->text(QClipboard::Clipboard));
}
