// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "SerialPanel.h"

#include <QComboBox>
#include <QFormLayout>
#include <QIntValidator>

#include "I18n.h"

namespace {

/// The rates `commlib.c` offers, plus the ones a USB adapter makes reachable.
/// Editable, because a non-standard divisor is a real thing on embedded gear —
/// 250000 is a DMX bus and it works.
const int kBaudRates[] = {
    300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200,
    230400, 250000, 460800, 500000, 921600, 1000000, 3000000,
};

} // namespace

SerialPanel::SerialPanel(QWidget *parent, const I18n *i18n)
    : QWidget(parent)
{
    const auto text = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->text(key, fallback) : fallback;
    };

    m_baud = new QComboBox(this);
    m_baud->setEditable(true);
    m_baud->setValidator(new QIntValidator(50, 20000000, this));
    for (int rate : kBaudRates) {
        m_baud->addItem(QString::number(rate), rate);
    }
    // From the core rather than a literal, so the shipped speed is written down
    // in one place — the settings schema, which is also where the deviation
    // from upstream's 9600 is recorded. `setInitial` overrides this with what
    // the settings file says and with what was last used.
    TtSerialParams shipped;
    tt_serial_params_default(&shipped);
    m_baud->setCurrentText(QString::number(shipped.baud));

    // Five and six data bits are deliberately absent. An FTDI refuses CS6 and
    // *accepts* CS5 while still putting eight bits on the wire, so the core
    // reads the setting back and refuses — offering them here would mean a
    // dialog whose only outcome is an error message.
    m_dataBits = new QComboBox(this);
    m_dataBits->addItem(QStringLiteral("7"), 7);
    m_dataBits->addItem(QStringLiteral("8"), 8);
    m_dataBits->setCurrentIndex(1);

    m_parity = new QComboBox(this);
    m_parity->addItem(tr("none"), TT_PARITY_NONE);
    m_parity->addItem(tr("odd"), TT_PARITY_ODD);
    m_parity->addItem(tr("even"), TT_PARITY_EVEN);
    m_parity->addItem(tr("mark"), TT_PARITY_MARK);
    m_parity->addItem(tr("space"), TT_PARITY_SPACE);

    m_stopBits = new QComboBox(this);
    m_stopBits->addItem(QStringLiteral("1"), 1);
    m_stopBits->addItem(QStringLiteral("2"), 2);

    m_flow = new QComboBox(this);
    m_flow->addItem(tr("none"), TT_FLOW_CONTROL_NONE);
    m_flow->addItem(tr("XON/XOFF"), TT_FLOW_CONTROL_XON_XOFF);
    m_flow->addItem(tr("RTS/CTS"), TT_FLOW_CONTROL_RTS_CTS);
    m_flow->addItem(tr("DSR/DTR"), TT_FLOW_CONTROL_DSR_DTR);

    auto *form = new QFormLayout(this);
    form->setContentsMargins(0, 0, 0, 0);
    form->addRow(text("DLG_SERIAL_BAUD", tr("Baud rate:")), m_baud);
    form->addRow(text("DLG_SERIAL_DATA", tr("Data bits:")), m_dataBits);
    form->addRow(text("DLG_SERIAL_PARITY", tr("Parity:")), m_parity);
    form->addRow(text("DLG_SERIAL_STOP", tr("Stop bits:")), m_stopBits);
    form->addRow(text("DLG_SERIAL_FLOW", tr("Flow control:")), m_flow);
}

TtSerialParams SerialPanel::params() const
{
    TtSerialParams p;
    tt_serial_params_default(&p);
    p.baud = static_cast<uint32_t>(m_baud->currentText().toUInt());
    p.data_bits = static_cast<uint8_t>(m_dataBits->currentData().toInt());
    p.parity = static_cast<TtParity>(m_parity->currentData().toUInt());
    p.stop_bits = static_cast<uint8_t>(m_stopBits->currentData().toInt());
    p.flow = static_cast<TtFlowControl>(m_flow->currentData().toUInt());
    return p;
}

void SerialPanel::setInitial(const TtSerialParams &params)
{
    m_baud->setCurrentText(QString::number(params.baud));
    const int bits = m_dataBits->findData(params.data_bits);
    if (bits >= 0) {
        m_dataBits->setCurrentIndex(bits);
    }
    const int par = m_parity->findData(params.parity);
    if (par >= 0) {
        m_parity->setCurrentIndex(par);
    }
    const int stop = m_stopBits->findData(params.stop_bits);
    if (stop >= 0) {
        m_stopBits->setCurrentIndex(stop);
    }
    const int flow = m_flow->findData(params.flow);
    if (flow >= 0) {
        m_flow->setCurrentIndex(flow);
    }
}
