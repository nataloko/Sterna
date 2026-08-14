// The bar under the menu: connection, input modes, and terminal dark mode.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QToolBar>
#include <QVector>

#include "Recent.h"
#include "sterna.h"

class I18n;
class QAction;
class QCheckBox;
class QComboBox;
class QEvent;
class Session;

/// The input and connection controls that need to stay within reach: where to
/// go, open or close it, local echo, and locally edited lines.
///
/// Upstream has no toolbar — its equivalents are a dialog (New connection), a
/// menu item (Disconnect) and a checkbox three tabs into Setup > Terminal.
/// Line edit is Sterna's own input mode. This is deliberately not a general
/// toolbar: it holds only connection and input state used during a session.
///
/// It owns no state. Every widget on it is a view of the session, refreshed by
/// [`refresh`] from the window's own status update, and every activation is a
/// signal for the window to act on — so the bar and the menu cannot disagree
/// about whether the port is open.
///
/// **One destination field, not a serial port list.** The list this replaced
/// could only offer serial ports, which made the bar inert on any machine with
/// no adapter plugged into it and left SSH, telnet and a local shell reachable
/// only through the dialog. The problem with widening it is that a one-line
/// control can carry a *destination* and not a parameter set — baud against
/// user and key against a telnet mode against nothing at all — so the dropdown
/// offers [`RecentConnection`]s, which carry their own, and the field accepts
/// anything the command line accepts, which inherits that kind's last ones.
/// The list is the common case and is exact; the field is the escape hatch and
/// is not.
class ConnectBar : public QToolBar {
    Q_OBJECT

public:
    ConnectBar(const I18n *i18n, QWidget *parent = nullptr);

    /// What is typed in the destination field.
    QString destination() const;
    void setDestination(const QString &text);
    /// Show a connection that was just opened, in the same words the list
    /// would have offered it in.
    void showConnection(const RecentConnection &recent);

    /// The list the dropdown offers. Held rather than read on demand because
    /// the bar has no session to read it from; the window pushes it whenever
    /// it changes.
    void setRecents(const QVector<RecentConnection> &recents);

    /// Point every widget at what the session currently says.
    void refresh(const Session *session);

signals:
    /// A remembered connection, with the parameters it was opened with.
    void recentChosen(const RecentConnection &recent);
    /// Whatever is in the field: an alias, `ssh://user@host`, a device path,
    /// `shell`, or a whole Tera Term command line. The window decides what it
    /// means — the bar has no parser and no session.
    void destinationEntered(const QString &text);
    void newConnectionRequested();
    void forgetRecentsRequested();
    void disconnectRequested();
    void localEchoRequested(bool on);
    void lineEditRequested(bool on);
    void darkModeRequested(bool on);

protected:
    void changeEvent(QEvent *event) override;

private:
    /// What one dropdown row is. `Qt::UserRole` on the item; the payload is
    /// the row after it.
    enum Role { RoleKind = Qt::UserRole, RolePayload };
    enum class Row { Header, Separator, Recent, Port, Alias, Shell, New, Forget };

    /// One row, before it is a widget. The list is composed as values and
    /// compared against what the combo already holds, because **rebuilding the
    /// model is what makes the field change size**: `clear()` invalidates the
    /// combo's geometry, the toolbar redistributes the space its expanding
    /// item was given, and the box visibly moves under the popup that is
    /// opening over it. The bar this replaced had the same guard for its
    /// ports, in one line, and the rewrite lost it.
    struct Entry {
        Row kind = Row::Header;
        QString text;
        QString payload;

        bool operator==(const Entry &other) const
        {
            return kind == other.kind && text == other.text
                && payload == other.payload;
        }
    };

    QVector<Entry> composeList() const;
    void rebuildList();
    void chose(int index);
    void updateDarkModeAction(bool darkMode);

    QComboBox *m_destination = nullptr;
    QAction *m_connect = nullptr;
    /// A check box rather than a checkable button: whether local echo is on is
    /// something people glance at, and a tick says it from across the room
    /// where a pressed-in button does not. It is also the shape they know it
    /// in — upstream's Setup > Terminal has the same box.
    QCheckBox *m_echo = nullptr;
    QCheckBox *m_lineEdit = nullptr;
    QAction *m_darkMode = nullptr;
    QVector<RecentConnection> m_recents;
    QVector<Entry> m_rows;
    QString m_connectText;
    QString m_disconnectText;
    /// True while [`rebuildList`] is repopulating: a combo assigns a current
    /// index as it fills, and acting on that would connect somewhere nobody
    /// asked to go.
    bool m_filling = false;
};
