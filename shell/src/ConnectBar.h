// The bar under the menu: connection, input modes, and terminal dark mode.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QToolBar>

#include "sterna.h"

class I18n;
class QAction;
class QCheckBox;
class QComboBox;
class Session;

/// The input and connection controls that need to stay within reach: which
/// port, open or close it, local echo, and locally edited lines.
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
class ConnectBar : public QToolBar {
    Q_OBJECT

public:
    ConnectBar(const I18n *i18n, QWidget *parent = nullptr);

    /// The device path of the chosen port, empty when nothing is plugged in.
    QString portPath() const;
    /// Select a port by device path, if it is still there.
    void setPortPath(const QString &path);
    /// Re-read the port list. Done on the dropdown's own popup rather than on a
    /// timer: a terminal that wakes up every second to enumerate `/dev` is a
    /// terminal that never lets the CPU idle, and the answer is only wanted at
    /// the moment somebody looks.
    void refreshPorts();
    /// Point every widget at what the session currently says.
    void refresh(const Session *session);

signals:
    /// Open the named port. The window supplies the line settings, which are
    /// the ones the connect dialog and `--baud` also use.
    void connectRequested(const QString &portPath);
    void disconnectRequested();
    void localEchoRequested(bool on);
    void lineEditRequested(bool on);
    void darkModeRequested(bool on);

private:
    QComboBox *m_port = nullptr;
    QAction *m_connect = nullptr;
    /// A check box rather than a checkable button: whether local echo is on is
    /// something people glance at, and a tick says it from across the room
    /// where a pressed-in button does not. It is also the shape they know it
    /// in — upstream's Setup > Terminal has the same box.
    QCheckBox *m_echo = nullptr;
    QCheckBox *m_lineEdit = nullptr;
    QCheckBox *m_darkMode = nullptr;
    QString m_connectText;
    QString m_disconnectText;
};
