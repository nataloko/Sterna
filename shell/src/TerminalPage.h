// One terminal session and the widgets and helpers bound to it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QWidget>

class I18n;
class Macro;
class Printer;
class QScrollBar;
class Session;
class TerminalView;

/// The lifetime boundary for one tab.
///
/// A session cannot safely be separated from its view, printer or macro: all
/// three keep a pointer to it, and the macro may still have a worker and a
/// native notifier alive when the page closes. Keeping the five together
/// makes closing a tab one destruction rather than an ordering convention in
/// `MainWindow`.
class TerminalPage : public QWidget {
public:
    TerminalPage(const I18n *i18n, QWidget *macroWindow,
                 QWidget *parent = nullptr);
    ~TerminalPage() override;

    Session *session() const { return m_session; }
    Printer *printer() const { return m_printer; }
    TerminalView *view() const { return m_view; }
    Macro *macro() const { return m_macro; }

    /// Follow the core's viewport after output or a scroll gesture.
    void syncScrollBar();

private:
    Session *m_session = nullptr;
    Printer *m_printer = nullptr;
    TerminalView *m_view = nullptr;
    QScrollBar *m_scroll = nullptr;
    Macro *m_macro = nullptr;
};
