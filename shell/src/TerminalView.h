// The grid: painting, keyboard, mouse, selection.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QElapsedTimer>
#include <QPoint>
#include <QWidget>

#include "Theme.h"
#include "sterna.h"

class QTimer;
class Session;

/// One end of a selection.
///
/// An absolute line number rather than a row, because a selection has to
/// survive the host printing underneath it — a row number means "wherever this
/// has slid to by now". And a column *boundary* rather than a cell, because
/// that is what makes dragging across `abc` select `abc` rather than `ab`:
/// the endpoint is the nearest edge between characters, which is how upstream
/// rounds it too (`buffer.c:GetCharCell`).
struct SelPoint {
    quint64 line = 0;
    int x = 0;

    bool operator<(const SelPoint &o) const
    {
        return line != o.line ? line < o.line : x < o.x;
    }
    bool operator==(const SelPoint &o) const { return line == o.line && x == o.x; }
};

/// The terminal screen.
///
/// A plain `QWidget` with a `QPainter`, no GPU. The measured baseline on the
/// Qt the desktop actually runs is a full 80x24 repaint in 3.9 ms — about 40x
/// what a 115200 baud link can dirty — so the scarce resource here is not fill
/// rate, and spending a GPU context and 60 MB on it would be spending it on
/// the wrong thing. See `PLAN.md`.
class TerminalView : public QWidget {
    Q_OBJECT

public:
    explicit TerminalView(Session *session, QWidget *parent = nullptr);

    Theme &theme() { return m_theme; }
    const Theme &theme() const { return m_theme; }
    /// Re-measure the font and re-fit the terminal to the window.
    void applyFont(const QFont &font);
    /// Take the colours from the session's settings and repaint. The size is
    /// the window's business, not the painter's — see `MainWindow`.
    void applySettings();

    QSize sizeHint() const override;
    /// The pixel size this many cells needs, at the current font.
    QSize sizeForCells(int cols, int rows) const;

    /// `enablekeyb` — swallow keystrokes instead of sending them, so a macro's
    /// own prompts are not typed over.
    ///
    /// The keyboard alone, which is upstream's `KeybEnabled`: scrolling the
    /// history, copying and pasting still work, because none of those puts
    /// anything on the wire.
    void setKeyboardEnabled(bool on);
    bool keyboardEnabled() const { return m_keyboardEnabled; }

    /// Copy the selection, if there is one.
    void copySelection() const;
    /// Paste the system clipboard.
    void pasteClipboard();
    /// Paste a string — the one path everything that pastes goes through, so
    /// that the trim and the confirmation dialog happen once. The rest of what
    /// a paste is happens in the core; see `tt_session::paste`.
    void pasteText(const QString &text);
    bool hasSelection() const { return m_hasSelection; }

public slots:
    /// Scroll the view back by `offset` lines; 0 is the live screen.
    void setViewOffset(int offset);

signals:
    /// The viewport moved, or the history grew. A scrollbar watches this
    /// rather than assuming its own last write is still current — the core
    /// moves the offset itself to keep a scrolled-back view on the same lines.
    void viewChanged();

protected:
    void paintEvent(QPaintEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;

private:
    /// Make the bell the core asked for: a noise, or a flash of the screen.
    ///
    /// The terminal's, rather than the window's, because it is the terminal
    /// that has a bell — and because the visual form is a way of painting.
    void ring(bool visual);
    /// Repaint now, or at the frame floor — whichever is later. Everything
    /// that reacts to *output* goes through this; the frontend's own changes
    /// (selection, focus, a new font) still call `update` directly, because
    /// they happen at the speed a hand moves.
    void requestRepaint();
    /// The character boundary nearest a widget position — one end of a
    /// selection. Clamped to the grid, and never inside a wide character.
    SelPoint boundaryAt(const QPointF &pos) const;
    /// The character *under* a widget position, named by its leading cell.
    /// What a double or triple click acts on, where rounding to the nearest
    /// edge would sometimes pick the next word along.
    SelPoint cellAt(const QPointF &pos) const;
    /// Start a drag whose anchor covers the whole unit around `at`.
    void startSelection(SelPoint at, const QPointF &pos);
    /// The selection as an ordered pair, expanded to whole words or lines when
    /// that is the unit. False when there is nothing selected.
    bool selectionRange(SelPoint *from, SelPoint *to) const;
    /// The start of the unit the character *at* `p` belongs to, and the end of
    /// the one *before* `p` — `p` being a boundary, so which character it
    /// refers to depends on which side of the selection it is.
    SelPoint unitStart(SelPoint p) const;
    SelPoint unitEnd(SelPoint p) const;
    QString selectedText() const;
    void clearSelection();
    /// Extend the drag to a widget position, scrolling if it is off the edge.
    void dragTo(const QPointF &pos);
    /// Re-fit the terminal to the widget, in whole cells.
    void refit();

    Session *m_session;
    Theme m_theme;

    /// `KeybEnabled`. A macro's `enablekeyb 0` clears it.
    bool m_keyboardEnabled = true;

    /// The `clipboard.*` settings this widget acts on, refreshed by
    /// `applySettings` rather than read per event.
    ///
    /// The defaults here are the schema's, and two of them will surprise a
    /// Linux user: Tera Term pastes on the **right** button and not on the
    /// middle one (`ttset.c:1422`, `:1425`). This shell used to do the
    /// opposite by hand; it is one line in `sterna.ini` either way now.
    struct Clipboard {
        bool autoCopy = true;
        bool selectOnlyByLButton = true;
        bool pasteRButtonDisabled = false;
        bool pasteMButtonDisabled = true;
        bool confirmPasteRButton = false;
        bool continuedLineCopy = false;
        bool confirmPaste = true;
        bool trimTrailingNewline = false;
        QString dictionary;
        int dialogWidth = 330;
        int dialogHeight = 220;
    } m_clipboard;

    /// `MouseWheelScrollLine` — how many lines one notch of the wheel moves,
    /// and how many cursor keys it sends when the host has asked for those.
    /// Refreshed by `applySettings` like the block above.
    ///
    /// Upstream applies it **only to a notch that arrived alone**
    /// (`vtwin.cpp:2539`), so a flick fast enough to coalesce two notches into
    /// one message scrolls two lines rather than six. Reproduced, quirk and
    /// all: the alternative is a wheel that behaves differently here at
    /// exactly the speeds people scroll fastest.
    int m_wheelScrollLine = 3;

    /// `DelimList`, decoded — what a double-click stops at. Refreshed by
    /// `applySettings` like the block above.
    ///
    /// The default is upstream's and two things about it are worth knowing:
    /// it holds a space and every ASCII punctuation mark **except**
    /// underscore, so `some_name` is one word and `some-name` is three; and
    /// it is stored in `Hex2StrW`'s escape, so the raw setting reads
    /// `$20!"#$24%…` and only the core knows what that means.
    QString m_delimiters = QStringLiteral(" !\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~");

    /// Since the last frame was painted, for the floor in `requestRepaint`.
    QElapsedTimer m_sincePaint;
    /// The deferred repaint, alive only while output outruns that floor.
    QTimer *m_repaint;

    /// The visual bell: the screen is inverted while this is set, and
    /// `m_bellOff` puts it back after `bell.visual_wait_ms`.
    ///
    /// Painted as an XOR against DECSCNM rather than as a colour of its own,
    /// which is what upstream does — `VisualBell` toggles the same
    /// `CF_REVERSEVIDEO` flag either side of its `Sleep` (`vtterm.c:5784`), so
    /// a flash on a screen the host has already reversed shows it the normal
    /// way round. The difference here is that the flash does not stop the
    /// terminal: upstream sleeps on the thread that is parsing, and this is a
    /// timer, so output keeps arriving underneath it.
    bool m_visualBell = false;
    QTimer *m_bellOff;

    // Selection is a frontend concept — the core only has to support it, and
    // what it supports is naming a line (`Session::line`) so that a highlight
    // can outlive the output that scrolls under it.
    //
    // What a click selects: a character, the word under it, or the line.
    enum class SelUnit { Char, Word, Line };

    bool m_hasSelection = false;
    bool m_selecting = false;
    SelUnit m_selUnit = SelUnit::Char;
    /// The unit the drag started on, already expanded — both ends of it,
    /// because a word dragged *leftwards* still has to keep its right edge.
    /// Upstream keeps the same pair (`DblClkStart`/`DblClkEnd`).
    SelPoint m_selAnchor;
    SelPoint m_selAnchorEnd;
    /// The boundary the pointer is at now. The selection is this reaching out
    /// of the anchor unit in whichever direction it has gone.
    SelPoint m_selHead;
    /// The terminal size the selection was made at. A resize re-flows every
    /// line, so the numbers stop meaning what they meant.
    QSize m_selSize;

    // Qt has no triple-click event, so the run is counted here: the second
    // press arrives as `mouseDoubleClickEvent` and the third as an ordinary
    // press soon after it.
    QElapsedTimer m_sinceClick;
    int m_clicks = 0;
    QPoint m_clickPos;

    /// Scrolls the view while a drag is held outside the widget, and exists
    /// only for as long as that is true — same shape as the repaint floor.
    QTimer *m_autoScroll;
    /// Where the drag was last seen, in widget coordinates. The autoscroll
    /// re-reads the boundary from it after each scroll, so the head follows
    /// the edge the pointer is past.
    QPointF m_dragPos;
};
