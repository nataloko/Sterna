// The SSH details, as a panel inside the New connection dialog.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QByteArray>
#include <QString>
#include <QWidget>

#include "sterna.h"

class QCheckBox;
class QLineEdit;
class I18n;

/// Who to log in as, and the switches that are not obvious.
///
/// **The host and the port are not here** — they are the dialog's, shared with
/// the other TCP services, which is how upstream's one New connection screen
/// is arranged. What is left is the part TTSSH keeps in dialogs of its own:
/// the user, the key, and the two switches.
///
/// Leaving the user blank is meaningful: it means "whatever `~/.ssh/config`
/// says", which is not the same as empty.
class SshPanel : public QWidget {
    Q_OBJECT

public:
    explicit SshPanel(QWidget *parent = nullptr, const I18n *i18n = nullptr);

    /// Fill `out` from the fields, given the host and port the dialog holds.
    /// The strings it points at live in this panel, so `out` must not outlive
    /// it — which it never does: the caller connects while the dialog is still
    /// on the stack.
    void fill(TtSshParams *out, const QString &host, quint16 port);

    /// Preselect, so reopening does not lose what was last used.
    void setInitial(const QString &user, const QString &identity, bool legacy);

    QString user() const;
    QString identity() const;
    bool legacy() const;

private slots:
    void browseForKey();

private:
    QLineEdit *m_user;
    QLineEdit *m_identity;
    QCheckBox *m_agent;
    QCheckBox *m_legacy;
    QCheckBox *m_useConfig;
    const I18n *m_i18n;

    // The UTF-8 the ABI is handed. Members rather than locals because
    // `TtSshParams` holds borrowed pointers.
    QByteArray m_hostUtf8;
    QByteArray m_userUtf8;
    QByteArray m_identityUtf8;
    const char *m_identities[2] = {nullptr, nullptr};
};
