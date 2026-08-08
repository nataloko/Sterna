// The SSH connect dialog, with the aliases from ~/.ssh/config.
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

/// Where to connect, and the two switches that are not obvious.
///
/// The host field is a combo box seeded from `~/.ssh/config`, because on a
/// Linux desktop the machines someone connects to are already in that file
/// and typing them again is the "configured twice" problem this whole path
/// exists to remove. Leaving the user and port blank is meaningful: it means
/// "whatever the config says", which is not the same as empty.
class SshDialog : public QDialog {
    Q_OBJECT

public:
    explicit SshDialog(QWidget *parent = nullptr);

    /// Fill `out` from the fields. The strings it points at live in this
    /// dialog, so `out` must not outlive it — which it never does: the caller
    /// connects while the dialog is still on the stack.
    void fill(TtSshParams *out);

    QString host() const;

    /// Preselect, so reopening does not lose what was last used.
    void setInitial(const QString &host, const QString &user, int port,
                    const QString &identity, bool legacy);

private slots:
    void browseForKey();

private:
    QComboBox *m_host;
    QLineEdit *m_user;
    QSpinBox *m_port;
    QLineEdit *m_identity;
    QCheckBox *m_agent;
    QCheckBox *m_legacy;
    QCheckBox *m_useConfig;

    // The UTF-8 the ABI is handed. Members rather than locals because
    // `TtSshParams` holds borrowed pointers.
    QByteArray m_hostUtf8;
    QByteArray m_userUtf8;
    QByteArray m_identityUtf8;
    const char *m_identities[2] = {nullptr, nullptr};
};
