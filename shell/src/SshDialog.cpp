// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "SshDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

SshDialog::SshDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("SSH connection"));

    m_host = new QComboBox(this);
    m_host->setEditable(true);
    m_host->setMinimumWidth(320);
    m_host->setInsertPolicy(QComboBox::NoInsert);
    // Seeded from `~/.ssh/config`, because the machines someone connects to
    // are already in that file. An alias picked here brings its user, port,
    // key and algorithm settings with it.
    if (TtStringList *aliases = tt_ssh_config_aliases()) {
        for (size_t i = 0; i < tt_string_list_len(aliases); i++) {
            m_host->addItem(QString::fromUtf8(tt_string_list_at(aliases, i)));
        }
        tt_string_list_free(aliases);
    }
    m_host->setCurrentText(QString());

    m_user = new QLineEdit(this);
    // Blank is meaningful and is the *default*: it means the config's `User`,
    // or `$USER`. Filling it in with the local user would silently override a
    // config that says otherwise.
    m_user->setPlaceholderText(tr("from ~/.ssh/config, or the local user"));

    m_port = new QSpinBox(this);
    m_port->setRange(0, 65535);
    m_port->setValue(0);
    m_port->setSpecialValueText(tr("from ~/.ssh/config, or 22"));

    m_identity = new QLineEdit(this);
    m_identity->setPlaceholderText(tr("from ~/.ssh/config, or ~/.ssh/id_*"));
    auto *browse = new QPushButton(tr("Browse..."), this);
    connect(browse, &QPushButton::clicked, this, &SshDialog::browseForKey);
    auto *identityRow = new QHBoxLayout;
    identityRow->setContentsMargins(0, 0, 0, 0);
    identityRow->addWidget(m_identity, 1);
    identityRow->addWidget(browse);

    m_agent = new QCheckBox(tr("Use ssh-agent"), this);
    m_agent->setChecked(true);

    // Spike 5's first finding, as a switch. russh keeps SHA-1 key exchange,
    // CBC ciphers and `ssh-rsa` host keys out of its defaults — correct
    // posture, and the reason a console server from 2012 will not answer.
    m_legacy = new QCheckBox(tr("Offer pre-2020 algorithms (old equipment)"), this);
    m_legacy->setToolTip(tr("SHA-1 key exchange, CBC ciphers and ssh-rsa host "
                            "keys. Off by default because they are weak; "
                            "needed to reach older console servers and "
                            "switches. A ~/.ssh/config that already names them "
                            "turns this on by itself."));

    m_useConfig = new QCheckBox(tr("Read ~/.ssh/config"), this);
    m_useConfig->setChecked(true);

    auto *form = new QFormLayout;
    form->addRow(tr("Host:"), m_host);
    form->addRow(tr("User:"), m_user);
    form->addRow(tr("Port:"), m_port);
    form->addRow(tr("Private key:"), identityRow);
    form->addRow(QString(), m_agent);
    form->addRow(QString(), m_useConfig);
    form->addRow(QString(), m_legacy);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(buttons);

    m_host->setFocus();
}

void SshDialog::browseForKey()
{
    const QString path = QFileDialog::getOpenFileName(
        this, tr("Private key"), QDir::homePath() + QStringLiteral("/.ssh"),
        tr("All files (*)"));
    if (!path.isEmpty()) {
        m_identity->setText(path);
    }
}

QString SshDialog::host() const
{
    return m_host->currentText().trimmed();
}

void SshDialog::setInitial(const QString &host, const QString &user, int port,
                           const QString &identity, bool legacy)
{
    m_host->setCurrentText(host);
    m_user->setText(user);
    m_port->setValue(port);
    m_identity->setText(identity);
    m_legacy->setChecked(legacy);
}

void SshDialog::fill(TtSshParams *out)
{
    tt_ssh_params_default(out);

    m_hostUtf8 = host().toUtf8();
    out->host = m_hostUtf8.constData();
    out->port = static_cast<uint16_t>(m_port->value());

    // Empty means "take it from the config", so an empty field sends null
    // rather than an empty string — the ABI treats those differently and this
    // is the one place the difference is visible to a user.
    const QString user = m_user->text().trimmed();
    if (!user.isEmpty()) {
        m_userUtf8 = user.toUtf8();
        out->user = m_userUtf8.constData();
    }

    const QString identity = m_identity->text().trimmed();
    if (!identity.isEmpty()) {
        m_identityUtf8 = identity.toUtf8();
        m_identities[0] = m_identityUtf8.constData();
        m_identities[1] = nullptr;
        out->identities = m_identities;
    }

    out->use_agent = m_agent->isChecked();
    out->use_ssh_config = m_useConfig->isChecked();
    out->legacy = m_legacy->isChecked();
}
