// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "TelnetPanel.h"

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>

#include "I18n.h"

TelnetPanel::TelnetPanel(QWidget *parent, const I18n *i18n)
    : QWidget(parent)
{
    Q_UNUSED(i18n);

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

    connect(m_mode, &QComboBox::activated, this, [this] { m_modePinned = true; });

    auto *form = new QFormLayout(this);
    form->setContentsMargins(0, 0, 0, 0);
    form->addRow(tr("Protocol:"), m_mode);
    form->addRow(QString(), m_binary);
}

void TelnetPanel::setPort(quint16 port)
{
    if (m_rawService || m_modePinned) {
        return;
    }
    TtTelnetParams defaults;
    // Asked of the core rather than reimplemented here: the rule is upstream's
    // and belongs in one place.
    tt_telnet_params_default(&defaults, port);
    const int index = m_mode->findData(defaults.mode);
    if (index >= 0) {
        m_mode->setCurrentIndex(index);
    }
}

void TelnetPanel::setRawService(bool on)
{
    if (on == m_rawService) {
        return;
    }
    m_rawService = on;
    if (!on) {
        if (m_savedMode >= 0) {
            m_mode->setCurrentIndex(m_savedMode);
        }
        m_modePinned = m_savedModePinned;
        m_mode->setEnabled(true);
        m_binary->setEnabled(true);
        return;
    }

    m_savedMode = m_mode->currentIndex();
    m_savedModePinned = m_modePinned;
    const int index = m_mode->findData(TT_TELNET_RAW);
    if (index >= 0) {
        m_mode->setCurrentIndex(index);
    }
    // Neither control has meaning for a connection with telnet entirely off.
    m_mode->setEnabled(false);
    m_binary->setEnabled(false);
}

TtTelnetMode TelnetPanel::mode() const
{
    return m_rawService
               ? TT_TELNET_RAW
               : static_cast<TtTelnetMode>(m_mode->currentData().toUInt());
}

void TelnetPanel::setInitial(TtTelnetMode mode)
{
    const int index = m_mode->findData(mode);
    if (index >= 0) {
        m_mode->setCurrentIndex(index);
    }
}

void TelnetPanel::fill(TtTelnetParams *out, quint16 port)
{
    tt_telnet_params_default(out, port);
    out->mode = mode();
    out->binary = m_binary->isChecked();
    // Null means the core's default, which is what the engine implements.
    out->term_type = nullptr;
}
