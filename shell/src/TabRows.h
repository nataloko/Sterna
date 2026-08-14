// A tab bar that wraps onto as many rows as it needs.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QVector>
#include <QWidget>

/// Tabs on more than one row, the way a Win32 tab control with `TCS_MULTILINE`
/// lays them out — which is what Tera Term's own multi-page dialogs use.
///
/// **`QTabWidget` cannot do this and no amount of subclassing gets there**: a
/// `QTabBar` lays its tabs out on one line internally and offers scroll buttons
/// when they do not fit, which is exactly the arrangement that makes a 26-page
/// dialog unnavigable. So the tabs are laid out and painted here, through
/// `QStyle` (`CT_TabBarTab`, `CE_TabBarTab`) rather than by hand, so they are
/// the platform style's tabs in the platform style's metrics — only wrapped.
///
/// Pair it with a `QStackedWidget`: [`currentChanged`] connects straight to
/// `QStackedWidget::setCurrentIndex`.
class TabRows : public QWidget {
    Q_OBJECT

public:
    explicit TabRows(QWidget *parent = nullptr);

    /// Append a tab and return its index. The first one added becomes current.
    int addTab(const QString &text);
    int count() const { return static_cast<int>(m_tabs.size()); }
    int currentIndex() const { return m_current; }
    QString tabText(int index) const;
    /// Hide a tab without changing its index. Search uses this so the page
    /// stack and every stored page number remain stable while only matches are
    /// navigable.
    void setTabVisible(int index, bool visible);
    bool isTabVisible(int index) const;
    /// How many rows the current width needs. For tests, and for the one
    /// caller that wants to know whether its dialog is wide enough.
    int rows() const { return m_rows; }
    /// How many rows some other width would need, without resizing anything.
    int rowsForWidth(int width) const;
    /// The narrowest width whose layout fits in `rows` rows, so a dialog can
    /// open at a size where the tabs are arranged as intended rather than
    /// stacked four deep. Not a promise: a single tab wider than that is its
    /// own floor.
    int widthForRows(int rows) const;

    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;
    bool hasHeightForWidth() const override { return true; }
    int heightForWidth(int width) const override;

public slots:
    void setCurrentIndex(int index);

signals:
    void currentChanged(int index);

protected:
    void resizeEvent(QResizeEvent *event) override;
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void leaveEvent(QEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    /// A font or style change moves every metric this widget computed.
    void changeEvent(QEvent *event) override;

private:
    struct Tab {
        QString text;
        QSize hint;
        QRect rect;
        int row = 0;
        /// Where in its own row the tab sits, which is what decides how the
        /// style rounds its corners.
        bool first = false;
        bool last = false;
        bool visible = true;
    };

    /// One tab's size in the platform style, the way `QTabBar::tabSizeHint`
    /// computes it.
    QSize tabHint(const QString &text) const;
    /// Measure the tabs if a font, a style or a new tab has invalidated them.
    ///
    /// Lazily, and this matters: the measurements are only right once the
    /// widget has been polished, polishing arrives as a `changeEvent`, and one
    /// of the things that provokes the polish is a parent layout asking for
    /// [`sizeHint`]. Remeasuring *inside* that event would have this widget
    /// rewriting its geometry while the layout above it was mid-computation —
    /// which is how the dialog came to open at a width its own tabs did not fit
    /// in. So the event only marks; the next query measures.
    void ensureHints() const;
    /// Fill rows greedily and justify each one to `width`, as a multiline tab
    /// control does. Returns the total height; `out` receives the geometry.
    int layout(int width, QVector<Tab> *out) const;
    void relayout();
    int tabAt(const QPoint &pos) const;
    int nextVisible(int from, int direction) const;
    static int rowCount(const QVector<Tab> &tabs);

    /// Mutable because the measurements are a cache: `sizeHint` is const and is
    /// where they are refreshed.
    mutable QVector<Tab> m_tabs;
    mutable bool m_hintsDirty = true;
    int m_current = -1;
    int m_hover = -1;
    int m_rows = 1;
};
