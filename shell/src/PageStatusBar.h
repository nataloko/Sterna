// One terminal's own status line.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QWidget>

class QLabel;
class QTimer;

/// The strip along the bottom of one [`TerminalPage`].
///
/// There is one of these per connection rather than one per window, because
/// everything on it — the name, the link, the recording counter, the message a
/// transfer leaves behind — is a fact about *a* session, and a window can be
/// showing nine of them at once. A single `QMainWindow::statusBar()` saying
/// "connected to router1, REC 4.2 MB" is true of one tile and silent about
/// which, which is exactly the confusion this replaces.
///
/// It doubles as the active-pane marker. That is why the pane header this
/// replaced is gone rather than sitting above it: a tile gets one row of
/// chrome, and it is this one.
class PageStatusBar : public QWidget {
    Q_OBJECT

public:
    explicit PageStatusBar(QWidget *parent = nullptr);

    /// Height only: **the strip never decides how wide the page is.** The
    /// terminal above it does. A status label that quoted its own text as its
    /// width would grow the page's hint the moment a long host name or a
    /// serial device path arrived, and the window would resize on connect.
    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;

    /// What this connection is called: the same string the tab carries.
    void setName(const QString &name);
    /// The link, in the three states the window used to paint: connected shows
    /// `describe()`, connecting says so, and disconnected is a red chip.
    void setConnection(bool connected, bool connecting, const QString &text);
    /// `REC <size>`, or nothing when this session is not logging. Driven by
    /// `Session::damaged` rather than a timer — the count changes exactly when
    /// bytes arrive, and that is what `damaged` means.
    void setLogging(bool logging, quint64 bytes);

    /// Say something for `ms` milliseconds, over the name. Upstream's
    /// `QStatusBar::showMessage`, scoped to the terminal it happened in.
    void showMessage(const QString &text, int ms = 5000);
    QString currentMessage() const;

    /// Paint the highlight that says window-level actions go here. Only ever
    /// true when more than one pane is visible: with one terminal there is
    /// nothing to disambiguate and a highlighted strip is noise.
    void setActive(bool active);

protected:
    /// Re-elide the name: how much of it fits is a function of the width the
    /// tile was just given.
    void resizeEvent(QResizeEvent *event) override;

private:
    void applyPalette();
    void showName();
    void elideInto(QLabel *label, const QString &text);

    QLabel *m_name = nullptr;
    QLabel *m_log = nullptr;
    QLabel *m_connection = nullptr;
    QTimer *m_messageTimer = nullptr;
    QString m_nameText;
    QString m_connectionText;
    QString m_message;
    bool m_active = false;
    bool m_linkDown = true;
};
