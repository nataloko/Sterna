// The telnet details, as a panel inside the New connection dialog.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QWidget>

#include "sterna.h"

class QCheckBox;
class QComboBox;
class I18n;

/// How much of the protocol to speak, once the dialog has said where.
///
/// The mode is the only interesting field. A terminal server puts one TCP port
/// on each serial line; those ports are not telnet servers, and opening at one
/// with `WILL TERMINAL-TYPE` puts five bytes of protocol into somebody's
/// console. So the mode follows the port the way upstream's does — negotiate
/// on 23, auto-detect elsewhere — and can be forced either way.
///
/// **The host and the port are the dialog's**, shared with SSH; `setPort` is
/// how the dialog keeps this panel's default in step with the port field.
class TelnetPanel : public QWidget {
    Q_OBJECT

public:
    explicit TelnetPanel(QWidget *parent = nullptr, const I18n *i18n = nullptr);

    /// Fill `out` from the fields and the port the dialog holds.
    void fill(TtTelnetParams *out, quint16 port);

    void setInitial(TtTelnetMode mode);
    TtTelnetMode mode() const;

    /// Follow the dialog's port until the user says otherwise, which is what
    /// makes the default correct without anyone having to know the rule.
    void setPort(quint16 port);

    /// Force the raw mode, which is what upstream's "Other" service means: a
    /// TCP connection with telnet switched off.
    void setRaw();

private:
    QComboBox *m_mode;
    QCheckBox *m_binary;
    /// True once the mode has been chosen by hand, after which the port stops
    /// moving it.
    bool m_modePinned = false;
};
