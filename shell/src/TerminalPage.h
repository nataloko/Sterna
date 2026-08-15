// One terminal session and the widgets and helpers bound to it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QWidget>

#include <optional>

#include "Recent.h"

class I18n;
class LineNumberGutter;
class Macro;
class PageStatusBar;
class Plugins;
class Printer;
class QScrollBar;
class Session;
class TerminalView;
class XferProgressDialog;

/// The lifetime boundary for one tab.
///
/// A session cannot safely be separated from its view, gutter, printer or
/// macro: each of them keeps a pointer to it, and the scripts may still have
/// workers and native notifiers alive when the page closes. Keeping them all
/// together makes closing a tab one destruction rather than an ordering
/// convention in `MainWindow`.
class TerminalPage : public QWidget {
public:
    TerminalPage(const I18n *i18n, QWidget *macroWindow,
                 const QString &pluginsDirectory, const QString &settingsPath,
                 QWidget *parent = nullptr);
    ~TerminalPage() override;

    /// The terminal's own size, plus the gutter, the scrollbar and the status
    /// line.
    ///
    /// Composed here rather than left to the layout. A `QWidgetItem` caches
    /// its widget's size hint and is invalidated by that widget's
    /// `updateGeometry()` — which `TerminalView` calls when `TerminalSize` or
    /// the font moves. With the view inside a row widget, the item the page's
    /// layout holds is the *row's*, and nothing invalidates that one: the page
    /// went on quoting the 80x24 it was constructed with, so a configured
    /// 100x30 window opened at 80x24 and the setting followed it down.
    QSize sizeHint() const override;

    Session *session() const { return m_session; }
    Printer *printer() const { return m_printer; }
    TerminalView *view() const { return m_view; }
    /// This page's own status line. The window has none: see `PageStatusBar`.
    PageStatusBar *status() const { return m_status; }
    Macro *macro() const { return m_macro; }
    Plugins *plugins() const { return m_plugins; }
    XferProgressDialog *transferDialog() const { return m_xferDialog; }
    /// What the shared connection selector shows when this page is active.
    ///
    /// The record is kept as well as its label: a recent SSH row carries an
    /// identity and compatibility mode which cannot be recovered from the
    /// words in the field. The label is cached so selecting a serial tab does
    /// not enumerate every device again merely to redisplay its friendly name.
    const std::optional<RecentConnection> &selectorConnection() const
    {
        return m_selectorConnection;
    }
    QString selectorLabel() const { return m_selectorLabel; }
    void setSelectorConnection(const RecentConnection &connection,
                               const QString &label)
    {
        m_selectorConnection = connection;
        m_selectorLabel = label;
    }
    /// Replace the modeless transfer dialog. The page owns it even though its
    /// visual parent is the window, so closing a tab cannot strand one.
    void setTransferDialog(XferProgressDialog *dialog);

    /// Follow the core's viewport after output or a scroll gesture.
    void syncScrollBar();

    /// Take the page's own settings — currently the line-number gutter.
    ///
    /// Separate from `TerminalView::applySettings` because the gutter is not
    /// the view's: it is the widget beside it, and putting the read there would
    /// mean the view reaching sideways through its parent to configure a
    /// sibling. `MainWindow` calls both, in that order.
    ///
    /// **It re-lays-out the terminal row synchronously**, and that is
    /// load-bearing — see the implementation.
    void applySettings();

private:
    Session *m_session = nullptr;
    Printer *m_printer = nullptr;
    TerminalView *m_view = nullptr;
    LineNumberGutter *m_gutter = nullptr;
    /// The widget holding the gutter, the view and the scrollbar. Kept so that
    /// `applySettings` can activate its layout without going looking for it.
    QWidget *m_terminalRow = nullptr;
    QScrollBar *m_scroll = nullptr;
    PageStatusBar *m_status = nullptr;
    Macro *m_macro = nullptr;
    Plugins *m_plugins = nullptr;
    XferProgressDialog *m_xferDialog = nullptr;
    std::optional<RecentConnection> m_selectorConnection;
    QString m_selectorLabel;
};
