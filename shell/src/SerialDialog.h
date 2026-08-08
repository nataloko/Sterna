// The serial connect dialog, with a live port list.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>

#include "sterna.h"

class QComboBox;
class QTimer;

/// Pick a port and its line settings.
///
/// The list refreshes on a timer while the dialog is open, because the thing
/// people actually do is open this, notice the adapter is not plugged in, plug
/// it in, and expect to see it. A refresh button would work and would also be
/// the first thing anyone complains about.
class SerialDialog : public QDialog {
    Q_OBJECT

public:
    explicit SerialDialog(QWidget *parent = nullptr);

    /// The `open_path` of the chosen port — never the `/dev/ttyUSB<n>` name,
    /// which is assigned in attach order and can point at a different physical
    /// port after a replug.
    QString portPath() const;
    TtSerialParams params() const;

    /// Preselect a port and settings, so reopening the dialog does not lose
    /// what was last used.
    void setInitial(const QString &portPath, const TtSerialParams &params);

private slots:
    void refreshPorts();

private:
    QComboBox *m_port;
    QComboBox *m_baud;
    QComboBox *m_dataBits;
    QComboBox *m_parity;
    QComboBox *m_stopBits;
    QComboBox *m_flow;
    QTimer *m_refresh;
};
