// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "TerminalView.h"

#include <QApplication>
#include <QClipboard>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QWheelEvent>

#include "DecGraphics.h"
#include "Session.h"

namespace {

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

    connect(m_session, &Session::damaged, this, [this] {
        // Output can move the offset — the core keeps a scrolled-back view on
        // the same lines — so the scrollbar has to hear about every pump, not
        // only about the scrolls this widget made.
        update();
        emit viewChanged();
    });

    m_session->setCellPixels(m_theme.cellWidth(), m_theme.cellHeight());
}

QSize TerminalView::sizeHint() const
{
    return QSize(80 * m_theme.cellWidth(), 24 * m_theme.cellHeight());
}

void TerminalView::applyFont(const QFont &font)
{
    m_theme.setFont(font);
    m_session->setCellPixels(m_theme.cellWidth(), m_theme.cellHeight());
    refit();
    update();
}

// --- painting ----------------------------------------------------------------

void TerminalView::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    const int cw = m_theme.cellWidth();
    const int ch = m_theme.cellHeight();
    const bool screenReverse = m_session->reverseVideo();
    const int rows = m_session->rows();

    // Whatever is not grid — the few pixels left over when the window is not
    // an exact multiple of the cell size.
    p.fillRect(rect(), screenReverse ? QColor(Qt::black) : m_theme.defaultBackground());

    for (int y = 0; y < rows; y++) {
        size_t len = 0;
        const TtCell *cells = m_session->row(y, &len);
        if (!cells) {
            continue;
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
            m_theme.resolve(cell, isSelected(x, y), screenReverse, &fg, &bg);
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

QPoint TerminalView::cellAt(const QPointF &pos) const
{
    const int x = qBound(0, static_cast<int>(pos.x()) / m_theme.cellWidth(),
                         m_session->cols() - 1);
    const int y = qBound(0, static_cast<int>(pos.y()) / m_theme.cellHeight(),
                         m_session->rows() - 1);
    return QPoint(x, y);
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
        m_selecting = true;
        m_hasSelection = false;
        m_selAnchor = cellAt(p);
        m_selHead = m_selAnchor;
        update();
    }
}

void TerminalView::mouseMoveEvent(QMouseEvent *event)
{
    const QPointF p = event->position();
    if (m_selecting) {
        const QPoint head = cellAt(p);
        if (head != m_selHead) {
            m_selHead = head;
            m_hasSelection = head != m_selAnchor;
            update();
        }
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
    // Word selection wants the same word-boundary rules the eventual
    // scrollback selection will use, so it is left until there is one thing to
    // write rather than two. A double click starts a fresh drag meanwhile.
    mousePressEvent(event);
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

void TerminalView::setViewOffset(int offset)
{
    const int before = m_session->viewOffset();
    m_session->setViewOffset(offset);
    if (m_session->viewOffset() == before) {
        return;
    }
    // The selection is held in viewport coordinates, so scrolling would leave
    // the highlight sitting on whatever text moved under it. Dropping it is
    // the honest minimum; anchoring a selection to the history is a refinement
    // that wants the same work as selecting *across* a scroll.
    clearSelection();
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

bool TerminalView::isSelected(int x, int y) const
{
    if (!m_hasSelection) {
        return false;
    }
    QPoint a = m_selAnchor;
    QPoint b = m_selHead;
    if (a.y() > b.y() || (a.y() == b.y() && a.x() > b.x())) {
        std::swap(a, b);
    }
    if (y < a.y() || y > b.y()) {
        return false;
    }
    const int from = (y == a.y()) ? a.x() : 0;
    const int to = (y == b.y()) ? b.x() : m_session->cols();
    return x >= from && x < to;
}

QString TerminalView::selectedText() const
{
    if (!m_hasSelection) {
        return QString();
    }
    QPoint a = m_selAnchor;
    QPoint b = m_selHead;
    if (a.y() > b.y() || (a.y() == b.y() && a.x() > b.x())) {
        std::swap(a, b);
    }

    QStringList lines;
    for (int y = a.y(); y <= b.y() && y < m_session->rows(); y++) {
        size_t len = 0;
        const TtCell *cells = m_session->row(y, &len);
        if (!cells) {
            continue;
        }
        const int from = (y == a.y()) ? a.x() : 0;
        const int to = qMin((y == b.y()) ? b.x() : static_cast<int>(len),
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
