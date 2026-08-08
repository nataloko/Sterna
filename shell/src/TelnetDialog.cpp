// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "TelnetDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QSpinBox>
#include <QVBoxLayout>

TelnetDialog::TelnetDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Telnet connection"));

    m_host = new QLineEdit(this);
    m_host->setMinimumWidth(320);
    m_host->setPlaceholderText(tr("host name or address"));

    m_port = new QSpinBox(this);
    m_port->setRange(1, 65535);
    m_port->setValue(23);

    m_mode = new QComboBox(this);
    m_mode->addItem(tr("Negotiate (telnet server)"), TT_TELNET_NEGOTIATE);
    m_mode->addItem(tr("Auto-detect"), TT_TELNET_AUTO);
    m_mode->addItem(tr("Raw (no telnet at all)"), TT_TELNET_RAW);
    m_mode->setToolTip(
        tr("A terminal server puts one TCP port on each serial line, and those "
           "ports are not telnet servers — opening at one with a negotiation "
           "puts protocol bytes into the console behind it. Raw never looks at "
           "a byte, which is also what a binary transfer needs. This follows "
           "the port until you change it."));

    m_binary = new QCheckBox(tr("Ask for 8-bit (BINARY) mode"), this);
    m_binary->setToolTip(tr("Stops a carriage return being padded with a NUL. "
                            "Tera Term leaves this off and agrees if the "
                            "server asks."));

    connect(m_port, &QSpinBox::valueChanged, this, &TelnetDialog::portChanged);
    connect(m_mode, &QComboBox::activated, this, [this] { m_modePinned = true; });

    auto *form = new QFormLayout;
    form->addRow(tr("Host:"), m_host);
    form->addRow(tr("Port:"), m_port);
    form->addRow(tr("Protocol:"), m_mode);
    form->addRow(QString(), m_binary);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(buttons);

    m_host->setFocus();
}

void TelnetDialog::portChanged(int port)
{
    if (m_modePinned) {
        return;
    }
    TtTelnetParams defaults;
    // Asked of the core rather than reimplemented here: the rule is upstream's
    // and belongs in one place.
    tt_telnet_params_default(&defaults, static_cast<uint16_t>(port));
    const int index = m_mode->findData(defaults.mode);
    if (index >= 0) {
        m_mode->setCurrentIndex(index);
    }
}

QString TelnetDialog::host() const
{
    return m_host->text().trimmed();
}

quint16 TelnetDialog::port() const
{
    return static_cast<quint16>(m_port->value());
}

void TelnetDialog::setInitial(const QString &host, quint16 port, TtTelnetMode mode)
{
    m_host->setText(host);
    m_port->setValue(port);
    const int index = m_mode->findData(mode);
    if (index >= 0) {
        m_mode->setCurrentIndex(index);
    }
}

void TelnetDialog::fill(TtTelnetParams *out)
{
    tt_telnet_params_default(out, port());
    out->mode = static_cast<TtTelnetMode>(m_mode->currentData().toUInt());
    out->binary = m_binary->isChecked();
    m_hostUtf8 = host().toUtf8();
    // Null means the core's default, which is what the engine implements.
    out->term_type = nullptr;
}
