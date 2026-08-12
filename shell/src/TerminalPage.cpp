// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "TerminalPage.h"

#include <QHBoxLayout>
#include <QScrollBar>
#include <QSignalBlocker>

#include "Macro.h"
#include "Plugins.h"
#include "Printer.h"
#include "Session.h"
#include "TerminalView.h"
#include "XferDialog.h"

TerminalPage::TerminalPage(const I18n *i18n, QWidget *macroWindow,
                           const QString &pluginsDirectory,
                           QWidget *parent)
    : QWidget(parent)
    , m_session(new Session(80, 24, this))
    , m_printer(new Printer(m_session, this))
    , m_view(new TerminalView(m_session, this, i18n))
    , m_scroll(new QScrollBar(Qt::Vertical, this))
    , m_macro(new Macro(m_session, macroWindow, this, i18n))
    , m_plugins(new Plugins(m_session, m_macro, pluginsDirectory, this))
{
    // A plain QWidget plus a scrollbar rather than a QAbstractScrollArea: the
    // painter draws straight onto the widget in cell coordinates, and a
    // scroll area would add a viewport child and a coordinate translation to
    // hold a scrollbar we can place in a layout for nothing.
    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(m_view, 1);
    layout->addWidget(m_scroll);

    connect(m_view, &TerminalView::viewChanged, this,
            &TerminalPage::syncScrollBar);
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
