// The serial line settings, as a panel inside the New connection dialog.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QWidget>

#include "sterna.h"

class QComboBox;
class I18n;

/// Speed, framing and flow control for a serial connection.
///
/// **The port itself is not here.** Upstream's New connection dialog puts the
/// port in its own Serial group beside the TCP/IP one and keeps the line
/// settings in Setup > Serial port (`ttpdlg.rc:132` has `IDC_HOSTCOM` and
/// nothing else). `ConnectDialog` owns the port for the same reason: it is the
/// thing being chosen, and the rest is how to talk to it.
class SerialPanel : public QWidget {
    Q_OBJECT

public:
    explicit SerialPanel(QWidget *parent = nullptr, const I18n *i18n = nullptr);

    TtSerialParams params() const;

    /// Preselect settings, so reopening does not lose what was last used.
    void setInitial(const TtSerialParams &params);

private:
    QComboBox *m_baud;
    QComboBox *m_dataBits;
    QComboBox *m_parity;
    QComboBox *m_stopBits;
    QComboBox *m_flow;
};
