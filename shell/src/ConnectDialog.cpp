// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "ConnectDialog.h"

#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QRadioButton>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include "I18n.h"
#include "SerialPanel.h"
#include "SshPanel.h"
#include "TelnetPanel.h"

namespace {

/// The two well-known ports the service radios move between. Upstream's
/// `TTXHostDlg` does the same on `IDC_HOSTSSH`/`IDC_HOSTTELNET`.
constexpr int kTelnetPort = 23;
constexpr int kSshPort = 22;

} // namespace

ConnectDialog::ConnectDialog(QWidget *parent, const I18n *i18n)
    : QDialog(parent)
{
    const auto text = [i18n](const char *key, const QString &fallback,
                             const char *section = "Tera Term") {
        return i18n ? i18n->text(key, fallback, section) : fallback;
    };
    const auto plainText = [i18n](const char *key, const QString &fallback,
                                  const char *section = "Tera Term") {
        return i18n ? i18n->plainText(key, fallback, section) : fallback;
    };

    setWindowTitle(plainText("DLG_HOST_TITLE", tr("New connection"), "TTSSH"));

    // --- TCP/IP -------------------------------------------------------------

    m_tcpip = new QRadioButton(text("DLG_HOST_TCPIP", tr("TCP/IP")), this);
    m_tcpip->setObjectName(QStringLiteral("connectTcpIp"));
    m_tcpBox = new QGroupBox(this);

    m_host = new QComboBox(m_tcpBox);
    m_host->setObjectName(QStringLiteral("connectHost"));
    m_host->setEditable(true);
    m_host->setMinimumWidth(320);
    m_host->setInsertPolicy(QComboBox::NoInsert);
    // Seeded from `~/.ssh/config`, because the machines someone connects to
    // are already in that file. An alias picked here brings its user, port,
    // key and algorithm settings with it. `setHistory` prepends what was
    // actually connected to, which is upstream's own drop-down.
    if (TtStringList *aliases = tt_ssh_config_aliases()) {
        for (size_t i = 0; i < tt_string_list_len(aliases); i++) {
            m_host->addItem(QString::fromUtf8(tt_string_list_at(aliases, i)));
        }
        tt_string_list_free(aliases);
    }
    m_host->setCurrentText(QString());

    m_history = new QCheckBox(text("DLG_HOST_TCPIPHISTORY", tr("History")), m_tcpBox);
    m_history->setObjectName(QStringLiteral("connectHistory"));
    m_history->setToolTip(tr("Remember the hosts connected to, and offer them "
                             "in the list above."));

    m_telnetService = new QRadioButton(tr("Telnet"), m_tcpBox);
    m_telnetService->setObjectName(QStringLiteral("connectServiceTelnet"));
    m_sshService = new QRadioButton(tr("SSH"), m_tcpBox);
    m_sshService->setObjectName(QStringLiteral("connectServiceSsh"));
    m_otherService =
        new QRadioButton(text("DLG_HOST_TCPIPOTHER", tr("Other")), m_tcpBox);
    m_otherService->setObjectName(QStringLiteral("connectServiceOther"));
    m_otherService->setToolTip(tr("A plain TCP connection with telnet switched "
                                  "off entirely."));
    // SSH is the one people want on a Linux desktop, and it is what upstream
    // preselects when TTSSH is enabled (`ttxssh.c`: `pvar->settings.Enabled`).
    m_sshService->setChecked(true);

    auto *services = new QHBoxLayout;
    services->setContentsMargins(0, 0, 0, 0);
    services->addWidget(m_telnetService);
    services->addWidget(m_sshService);
    services->addWidget(m_otherService);
    services->addStretch(1);

    m_port = new QSpinBox(m_tcpBox);
    m_port->setObjectName(QStringLiteral("connectPort"));
    m_port->setRange(1, 65535);
    m_port->setValue(kSshPort);

    auto *tcpForm = new QFormLayout(m_tcpBox);
    tcpForm->addRow(text("DLG_HOST_TCPIPHOST", tr("Host:")), m_host);
    tcpForm->addRow(QString(), m_history);
    tcpForm->addRow(text("DLG_HOST_TCPIPSERVICE", tr("Service:")), services);
    tcpForm->addRow(text("DLG_HOST_TCPIPPORT", tr("TCP port#:")), m_port);

    // --- Serial -------------------------------------------------------------

    m_serialRadio = new QRadioButton(text("DLG_HOST_SERIAL", tr("Serial")), this);
    m_serialRadio->setObjectName(QStringLiteral("connectSerial"));
    m_serialBox = new QGroupBox(this);

    m_serialPort = new QComboBox(m_serialBox);
    m_serialPort->setObjectName(QStringLiteral("connectSerialPort"));
    m_serialPort->setMinimumWidth(320);

    auto *serialForm = new QFormLayout(m_serialBox);
    serialForm->addRow(text("DLG_HOST_SERIALPORT", tr("Port:")), m_serialPort);

    // The two halves are exclusive, and Qt will not group radios across
    // different parents on its own.
    auto *kinds = new QButtonGroup(this);
    kinds->addButton(m_tcpip);
    kinds->addButton(m_serialRadio);
    m_tcpip->setChecked(true);

    // --- Details ------------------------------------------------------------

    // Collapsed, so the dialog opens as upstream's. What is behind it is
    // everything this port has that upstream's screen does not, and it is one
    // click rather than a second dialog.
    m_detailsButton = new QToolButton(this);
    m_detailsButton->setObjectName(QStringLiteral("connectDetails"));
    m_detailsButton->setText(tr("Details"));
    m_detailsButton->setCheckable(true);
    m_detailsButton->setChecked(false);
    m_detailsButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_detailsButton->setArrowType(Qt::RightArrow);
    m_detailsButton->setAutoRaise(true);

    m_serial = new SerialPanel(this, i18n);
    m_ssh = new SshPanel(this, i18n);
    m_telnet = new TelnetPanel(this, i18n);

    m_details = new QStackedWidget(this);
    m_details->setObjectName(QStringLiteral("connectDetailsPages"));
    m_details->addWidget(m_serial);
    m_details->addWidget(m_ssh);
    m_details->addWidget(m_telnet);
    m_details->setVisible(false);

    connect(m_detailsButton, &QToolButton::toggled, this, [this](bool on) {
        m_detailsButton->setArrowType(on ? Qt::DownArrow : Qt::RightArrow);
        m_details->setVisible(on);
        // The dialog was sized for the collapsed form, so it has to be asked
        // to take the new one rather than keeping the old height.
        adjustSize();
    });

    // --- wiring -------------------------------------------------------------

    connect(m_tcpip, &QRadioButton::toggled, this, &ConnectDialog::syncEnabled);
    for (QRadioButton *service : {m_telnetService, m_sshService, m_otherService}) {
        connect(service, &QRadioButton::toggled, this, &ConnectDialog::syncEnabled);
    }
    // A port typed by hand stops following the service, the same way the telnet
    // panel's mode stops following the port.
    connect(m_port, &QSpinBox::valueChanged, this, [this] {
        if (m_port->hasFocus()) {
            m_portPinned = true;
        }
        m_telnet->setPort(static_cast<quint16>(m_port->value()));
    });

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->button(QDialogButtonBox::Ok)->setText(text("BTN_OK", tr("OK")));
    buttons->button(QDialogButtonBox::Cancel)
        ->setText(text("BTN_CANCEL", tr("Cancel")));
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(m_tcpip);
    layout->addWidget(m_tcpBox);
    layout->addWidget(m_serialRadio);
    layout->addWidget(m_serialBox);
    layout->addWidget(m_detailsButton);
    layout->addWidget(m_details);
    layout->addStretch(1);
    layout->addWidget(buttons);

    refreshPorts();
    // The list refreshes while the dialog is open, because the thing people
    // actually do is open this, notice the adapter is not plugged in, plug it
    // in, and expect to see it. A refresh button would work and would also be
    // the first thing anyone complains about.
    m_refresh = new QTimer(this);
    m_refresh->setInterval(1000);
    connect(m_refresh, &QTimer::timeout, this, &ConnectDialog::refreshPorts);
    m_refresh->start();

    syncEnabled();
    m_host->setFocus();
}

void ConnectDialog::syncEnabled()
{
    const bool tcp = m_tcpip->isChecked();
    m_tcpBox->setEnabled(tcp);
    m_serialBox->setEnabled(!tcp);

    // The port follows the service until it is typed into, which is what makes
    // the common case correct without anyone knowing the numbers.
    if (tcp && !m_portPinned) {
        const int want = m_sshService->isChecked() ? kSshPort : kTelnetPort;
        QSignalBlocker block(m_port);
        m_port->setValue(want);
        m_telnet->setPort(static_cast<quint16>(want));
    }
    // "Other" is upstream's name for a TCP connection with telnet off, which
    // is exactly this port's raw mode.
    if (tcp && m_otherService->isChecked()) {
        m_telnet->setRaw();
    }
    syncDetails();
}

void ConnectDialog::syncDetails()
{
    switch (kind()) {
    case Kind::Serial:
        m_details->setCurrentWidget(m_serial);
        break;
    case Kind::Ssh:
        m_details->setCurrentWidget(m_ssh);
        break;
    case Kind::Telnet:
        m_details->setCurrentWidget(m_telnet);
        break;
    }
}

ConnectDialog::Kind ConnectDialog::kind() const
{
    if (!m_tcpip->isChecked()) {
        return Kind::Serial;
    }
    return m_sshService->isChecked() ? Kind::Ssh : Kind::Telnet;
}

void ConnectDialog::selectKind(Kind kind)
{
    switch (kind) {
    case Kind::Serial:
        m_serialRadio->setChecked(true);
        break;
    case Kind::Ssh:
        m_tcpip->setChecked(true);
        m_sshService->setChecked(true);
        break;
    case Kind::Telnet:
        m_tcpip->setChecked(true);
        m_telnetService->setChecked(true);
        break;
    }
    syncEnabled();
}

void ConnectDialog::refreshPorts()
{
    TtPortList *list = tt_serial_enumerate();
    if (!list) {
        return;
    }

    // Rebuild only when the set actually changed. Replacing the model on every
    // tick would reset the dropdown under a user who is halfway through
    // choosing from it.
    const size_t n = tt_port_list_len(list);
    bool same = static_cast<size_t>(m_serialPort->count()) == n;
    for (size_t i = 0; same && i < n; i++) {
        const TtPortInfo *info = tt_port_list_at(list, i);
        same = info && m_serialPort->itemData(static_cast<int>(i)).toString() ==
                           QString::fromUtf8(info->open_path);
    }
    if (same) {
        tt_port_list_free(list);
        return;
    }

    const QString keep = portPath();
    m_serialPort->clear();
    for (size_t i = 0; i < n; i++) {
        const TtPortInfo *info = tt_port_list_at(list, i);
        if (!info) {
            continue;
        }
        m_serialPort->addItem(QString::fromUtf8(info->label),
                              QString::fromUtf8(info->open_path));
    }
    tt_port_list_free(list);

    const int back = m_serialPort->findData(keep);
    if (back >= 0) {
        m_serialPort->setCurrentIndex(back);
    }
}

QString ConnectDialog::portPath() const
{
    return m_serialPort->currentData().toString();
}

TtSerialParams ConnectDialog::serialParams() const
{
    return m_serial->params();
}

QString ConnectDialog::host() const
{
    return m_host->currentText().trimmed();
}

quint16 ConnectDialog::port() const
{
    return static_cast<quint16>(m_port->value());
}

void ConnectDialog::fillSsh(TtSshParams *out)
{
    m_ssh->fill(out, host(), port());
}

void ConnectDialog::fillTelnet(TtTelnetParams *out)
{
    m_telnet->fill(out, port());
}

void ConnectDialog::setHistory(const QStringList &hosts)
{
    // Prepended in order, so the most recent is first, and deduplicated
    // against both the `~/.ssh/config` aliases already in the list and the
    // rest of `hosts` — a list that has been round-tripped through the
    // settings file can hold the same name twice, and removing an entry this
    // loop had already inserted would reorder what it had just placed.
    int at = 0;
    QStringList seen;
    for (const QString &host : hosts) {
        if (host.isEmpty() || seen.contains(host)) {
            continue;
        }
        seen.append(host);
        // Only below the insertion point: anything above it is one of ours.
        const int existing = m_host->findText(host);
        if (existing >= at) {
            m_host->removeItem(existing);
        }
        m_host->insertItem(at++, host);
    }
    m_host->setCurrentText(QString());
}

bool ConnectDialog::remembersHistory() const
{
    return m_history->isChecked();
}

void ConnectDialog::setRemembersHistory(bool on)
{
    m_history->setChecked(on);
}

void ConnectDialog::setInitialSerial(const QString &portPath,
                                     const TtSerialParams &params)
{
    const int idx = m_serialPort->findData(portPath);
    if (idx >= 0) {
        m_serialPort->setCurrentIndex(idx);
    }
    m_serial->setInitial(params);
}

void ConnectDialog::setInitialSsh(const QString &host, const QString &user,
                                  int port, const QString &identity, bool legacy)
{
    if (!host.isEmpty()) {
        m_host->setCurrentText(host);
    }
    if (port > 0) {
        QSignalBlocker block(m_port);
        m_port->setValue(port);
        m_portPinned = true;
    }
    m_ssh->setInitial(user, identity, legacy);
}

void ConnectDialog::setInitialTelnet(const QString &host, quint16 port,
                                     TtTelnetMode mode)
{
    // Only when SSH did not already seed it: the two share one host field, and
    // the last SSH host is the more likely of the two to be wanted.
    if (!host.isEmpty() && m_host->currentText().isEmpty()) {
        m_host->setCurrentText(host);
    }
    m_telnet->setInitial(mode);
    if (m_telnetService->isChecked() && port > 0) {
        QSignalBlocker block(m_port);
        m_port->setValue(port);
    }
}
