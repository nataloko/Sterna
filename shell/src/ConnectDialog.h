// One New connection screen for every transport — upstream's `IDD_HOSTDLG`.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QDialog>

#include "sterna.h"

class QAbstractButton;
class QCheckBox;
class QComboBox;
class QGroupBox;
class QRadioButton;
class QSpinBox;
class QStackedWidget;
class QTimer;
class QToolButton;
class I18n;
class SerialPanel;
class SshPanel;
class TelnetPanel;

/// Pick a transport and a destination, in one screen.
///
/// This is upstream's `IDD_HOSTDLG` as TTSSH extends it (`ttpdlg.rc:132`,
/// `ttxssh.c`'s `TTXHostDlg`): two radio-selected halves, TCP/IP and Serial.
/// The TCP half carries the host, the service — Telnet, SSH or Other — and the
/// port; the serial half carries the port alone. Choosing the service moves
/// the port the way upstream does.
///
/// **Two of upstream's controls are deliberately absent.** The IP version combo
/// has nothing behind it: `tt_session_connect_*` takes no address family, so a
/// control that changed nothing would be a lie on the screen. The SSH version
/// combo offers SSH1 and SSH2 upstream, and this port speaks SSH2 only.
///
/// Everything Sterna has that upstream's screen does not — the serial line
/// settings, the SSH user and key, the telnet mode — lives in the Details
/// section, collapsed, so the dialog opens as upstream's and gives up nothing.
class ConnectDialog : public QDialog {
    Q_OBJECT

public:
    /// What the dialog was asked for, once it has been accepted.
    enum class Kind { Serial, Ssh, Telnet };

    explicit ConnectDialog(QWidget *parent = nullptr, const I18n *i18n = nullptr);

    Kind kind() const;

    /// The `open_path` of the chosen port — never the `/dev/ttyUSB<n>` name,
    /// which is assigned in attach order and can point at a different physical
    /// port after a replug. Only meaningful for `Kind::Serial`.
    QString portPath() const;
    TtSerialParams serialParams() const;

    /// The typed host, trimmed. Only meaningful for the two TCP kinds.
    QString host() const;
    quint16 port() const;

    /// Fill the transport parameters. The strings they point at live in this
    /// dialog, so they must not outlive it — which they never do: the caller
    /// connects while the dialog is still on the stack.
    void fillSsh(TtSshParams *out);
    void fillTelnet(TtTelnetParams *out);

    SshPanel *sshPanel() const { return m_ssh; }

    /// Seed every field from what was last used, so reopening loses nothing.
    void setInitialSerial(const QString &portPath, const TtSerialParams &params);
    void setInitialSsh(const QString &host, const QString &user, int port,
                       const QString &identity, bool legacy);
    void setInitialTelnet(const QString &host, quint16 port, TtTelnetMode mode);

    /// Open on this half. The window remembers nothing itself; the caller
    /// decides, which is how the connect bar's port button can land straight on
    /// Serial.
    void selectKind(Kind kind);

    /// The hosts offered in the drop-down, newest first.
    void setHistory(const QStringList &hosts);
    /// Whether the accepted host should be remembered — `HistoryList`.
    bool remembersHistory() const;
    void setRemembersHistory(bool on);

private slots:
    void refreshPorts();

private:
    /// Enable exactly the half that is selected, and move the port to match the
    /// service. Upstream greys the other group rather than hiding it, so the
    /// shape of the dialog never changes under the pointer.
    void syncEnabled();
    /// Show the Details page belonging to the current selection.
    void syncDetails();

    QRadioButton *m_tcpip;
    QRadioButton *m_serialRadio;
    QComboBox *m_host;
    QCheckBox *m_history;
    QRadioButton *m_telnetService;
    QRadioButton *m_sshService;
    QRadioButton *m_otherService;
    QSpinBox *m_port;
    QComboBox *m_serialPort;
    QGroupBox *m_tcpBox;
    QGroupBox *m_serialBox;

    QToolButton *m_detailsButton;
    QStackedWidget *m_details;
    SerialPanel *m_serial;
    SshPanel *m_ssh;
    TelnetPanel *m_telnet;

    QTimer *m_refresh;
    /// True once the port has been typed into, after which picking a service
    /// stops moving it.
    bool m_portPinned = false;
};
