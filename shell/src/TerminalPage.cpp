// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "TerminalPage.h"

#include <QHBoxLayout>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QVBoxLayout>

#include "LineNumberGutter.h"
#include "Macro.h"
#include "PageStatusBar.h"
#include "Plugins.h"
#include "Printer.h"
#include "Session.h"
#include "TerminalView.h"
#include "XferDialog.h"

TerminalPage::TerminalPage(const I18n *i18n, QWidget *macroWindow,
                           const QString &pluginsDirectory,
                           const QString &settingsPath,
                           QWidget *parent)
    : QWidget(parent)
    , m_session(new Session(80, 24, this))
    , m_printer(new Printer(m_session, this))
    , m_view(new TerminalView(m_session, this, i18n))
    // After the view, because it borrows the view's theme — that is the same
    // object the terminal paints with, so the two can never disagree about a
    // cell's size, the font or the background.
    , m_gutter(new LineNumberGutter(m_session, m_view->theme(), this))
    , m_scroll(new QScrollBar(Qt::Vertical, this))
    // Declaration order is what actually decides construction order, so this
    // sits where the header puts it. The strip points at nothing and nothing
    // points at it, so its position among the six is free.
    , m_status(new PageStatusBar(this))
    , m_macro(new Macro(m_session, macroWindow, this, i18n))
    , m_plugins(
          new Plugins(m_session, m_macro, pluginsDirectory, settingsPath, this))
{
    m_scroll->setObjectName(QStringLiteral("terminalScrollBar"));
    // A plain QWidget plus a scrollbar rather than a QAbstractScrollArea: the
    // painter draws straight onto the widget in cell coordinates, and a
    // scroll area would add a viewport child and a coordinate translation to
    // hold a scrollbar we can place in a layout for nothing.
    // A container widget for the row rather than a nested `QHBoxLayout`: a
    // layout added with `addLayout` does not carry its items' size hints out to
    // the parent widget, and the symptom is a window that opens at 80x24
    // whatever `TerminalSize` says — the terminal's own hint was 900x630 and
    // the page quoted 720x525.
    m_terminalRow = new QWidget(this);
    auto *terminal = m_terminalRow;
    auto *row = new QHBoxLayout(terminal);
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(0);
    // The gutter is a sibling of the terminal rather than columns inside it,
    // which is what keeps line numbers out of the clipboard: the view builds a
    // copy out of the core's cells, and this widget owns none. See
    // `LineNumberGutter`.
    row->addWidget(m_gutter);
    row->addWidget(m_view, 1);
    row->addWidget(m_scroll);
    // A wheel notch over the numbers is aimed at the text, not at the widget.
    m_gutter->setWheelTarget(m_view);
    // Hidden until a setting says otherwise, so a page that is never told
    // anything looks exactly as it did before this existed.
    m_gutter->hide();

    // The status line belongs to the page rather than to the window or to the
    // pane holding it: it then follows this session between tiles and between
    // layouts without anything having to move it, and with one terminal it
    // lands exactly where a `QMainWindow` status bar would have been.
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(terminal, 1);
    layout->addWidget(m_status);

    connect(m_view, &TerminalView::viewChanged, this,
            &TerminalPage::syncScrollBar);
    // The one signal the gutter needs. `TerminalView` emits it from its own
    // `Session::damaged` handler as well as from a scroll gesture, so it
    // already covers both things that can move a line number: output pushing
    // the page along, and the viewport moving over the history.
    //
    // `update()` schedules and Qt coalesces, so this adds no repaint path that
    // could get around the view's 8 ms frame floor. It can put the gutter one
    // frame ahead of the text during heavy output, which is under the floor
    // and has not been visible; the fix if it ever is would be to repaint from
    // the view's own `paintEvent`, at the cost of a coupling.
    connect(m_view, &TerminalView::viewChanged, this,
            [this] { m_gutter->update(); });
    // The two things that move the gutter's *colours* without moving a line
    // number, so neither reaches `viewChanged`: a host repainting the
    // background with `OSC 11`, and connecting or disconnecting, which moves
    // every background the host did not choose by `color.disconnected_shade`.
    // The gutter reads both out of the same `Theme` the terminal paints with,
    // and `update()` only schedules — so it does not matter that the view's
    // handlers are the ones that refresh that theme, or in which order.
    connect(m_session, &Session::colorsChanged, this,
            [this] { m_gutter->update(); });
    connect(m_session, &Session::connectionChanged, this,
            [this] { m_gutter->update(); });
    connect(m_scroll, &QScrollBar::valueChanged, this, [this](int value) {
        // The scrollbar counts down from the top of the history; the session
        // counts back from the live screen. One subtraction, in one place.
        m_view->setViewOffset(m_scroll->maximum() - value);
    });
    syncScrollBar();
}

TerminalPage::~TerminalPage()
{
    delete m_xferDialog;
    m_xferDialog = nullptr;
    // The plugin callbacks point at Macro's UI adapter, and both point at the
    // session. Stop that worker before either of its two dependencies.
    delete m_plugins;
    m_plugins = nullptr;
    // QObject children normally die in construction order. The session is
    // deliberately the first child, but Macro::~Macro unlinks itself from the
    // session, so that order would be a use-after-free. Make the one ordering
    // dependency explicit at the lifetime boundary which owns both.
    delete m_macro;
    m_macro = nullptr;
}

QSize TerminalPage::sizeHint() const
{
    m_status->ensurePolished();
    QSize out = m_view->sizeHint();
    // The gutter is added on top of the terminal's own size rather than taken
    // out of it: a configured 80x24 keeps its 80 columns and the window opens
    // wider to hold the numbers. That is the whole of "the window grows" —
    // `MainWindow::onSettingsChanged` does the rest for a live toggle, because
    // it resizes the window by however many columns the *view* lost.
    if (!m_gutter->isHidden()) {
        out.rwidth() += m_gutter->sizeHint().width();
    }
    // The scrollbar is hidden while there is nothing to scroll, so an 80x24
    // window is not permanently a few pixels narrower than the terminal in it.
    if (!m_scroll->isHidden()) {
        out.rwidth() += m_scroll->sizeHint().width();
    }
    out.rheight() += m_status->sizeHint().height();
    return out;
}

void TerminalPage::applySettings()
{
    const bool on = m_session->setting(QStringLiteral("terminal.line_numbers"))
                    == QLatin1String("on");
    bool ok = false;
    const int digits =
        m_session->setting(QStringLiteral("terminal.line_number_width")).toInt(&ok);
    // How much of the row the gutter was taking, so that a changed digit count
    // counts as a change as well as a changed switch — both move the terminal.
    const int before = m_gutter->isHidden() ? 0 : m_gutter->width();

    m_gutter->setDigits(ok ? digits : 4);
    // Even while hidden: the font or the cell size may have moved, and a
    // gutter shown later must already be the right width for them.
    m_gutter->updateMetrics();
    m_gutter->setVisible(on);
    const int after = on ? m_gutter->width() : 0;

    if (before == after) {
        return;
    }
    // **Synchronously**, and this is the one non-obvious line in the feature.
    //
    // `MainWindow::onSettingsChanged` decides whether to resize the window by
    // asking how many cells the *view* has room for, and it does that a few
    // dozen lines after calling this. A `QLayout` marked dirty does not
    // re-lay-out until the next event-loop turn, so without this the view
    // still reports its pre-gutter width, the window does not grow, and the
    // deferred layout then squeezes the terminal instead — which refits to 75
    // columns and writes `TerminalSize` down to 75x24 behind the user's back.
    // That is `AGENTS.md`'s `TerminalSize` write-back trap, reached by a new
    // route: the symptom would be a window that keeps shrinking every time
    // line numbers are switched on.
    if (QLayout *layout = m_terminalRow->layout()) {
        layout->activate();
    }
}

void TerminalPage::setTransferDialog(XferProgressDialog *dialog)
{
    if (dialog == m_xferDialog) {
        return;
    }
    delete m_xferDialog;
    m_xferDialog = dialog;
    if (m_xferDialog) {
        connect(m_xferDialog, &QObject::destroyed, this,
                [this] { m_xferDialog = nullptr; });
    }
}

void TerminalPage::syncScrollBar()
{
    const int history = m_session->scrollbackLen();
    const int offset = m_session->viewOffset();
    // Blocked because this is a *reaction* to the session moving: letting it
    // emit would turn every pump into a write back into the session, and the
    // rounding would fight the offset the core just chose.
    const QSignalBlocker block(m_scroll);
    m_scroll->setRange(0, history);
    m_scroll->setPageStep(qMax(1, m_session->rows()));
    m_scroll->setSingleStep(1);
    m_scroll->setValue(history - offset);
    // Hidden when there is nothing to scroll, so an 80x24 window is not
    // permanently a few pixels narrower than the terminal in it.
    m_scroll->setVisible(history > 0);
}
