// The telnet connect dialog.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>

#include "termitta.h"

class QCheckBox;
class QComboBox;
class QLineEdit;
class QSpinBox;

/// Where to connect, and how much of the protocol to speak there.
///
/// The mode is the only interesting field, and it is why this is a dialog
/// rather than a line edit. A terminal server puts one TCP port on each serial
/// line; those ports are not telnet servers, and opening at one with
/// `WILL TERMINAL-TYPE` puts five bytes of protocol into somebody's console.
/// So the mode follows the port the way upstream's does — negotiate on 23,
/// auto-detect elsewhere — and can be forced either way.
class TelnetDialog : public QDialog {
    Q_OBJECT

public:
    explicit TelnetDialog(QWidget *parent = nullptr);

    QString host() const;
    quint16 port() const;
    /// Fill `out` from the fields. Its strings live in this dialog, so `out`
    /// must not outlive it.
    void fill(TtTelnetParams *out);

    void setInitial(const QString &host, quint16 port, TtTelnetMode mode);

private slots:
    /// Follow the port until the user says otherwise, which is what makes the
    /// default correct without anyone having to know the rule.
    void portChanged(int port);

private:
    QLineEdit *m_host;
    QSpinBox *m_port;
    QComboBox *m_mode;
    QCheckBox *m_binary;
    /// True once the mode has been chosen by hand, after which the port stops
    /// moving it.
    bool m_modePinned = false;

    QByteArray m_hostUtf8;
};
