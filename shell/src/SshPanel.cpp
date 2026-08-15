// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "SshPanel.h"

#include <QCheckBox>
#include <QDir>
#include <QFileDialog>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLineEdit>
#include <QPushButton>

#include "I18n.h"

SshPanel::SshPanel(QWidget *parent, const I18n *i18n)
    : QWidget(parent), m_i18n(i18n)
{
    const auto text = [i18n](const char *key, const QString &fallback,
                             const char *section = "TTSSH") {
        return i18n ? i18n->text(key, fallback, section) : fallback;
    };

    m_user = new QLineEdit(this);
    // Blank is meaningful and is the *default*: it means the config's `User`,
    // or `$USER`. Filling it in with the local user would silently override a
    // config that says otherwise.
    m_user->setPlaceholderText(tr("from ~/.ssh/config, or the local user"));

    m_identity = new QLineEdit(this);
    m_identity->setPlaceholderText(tr("from ~/.ssh/config, or ~/.ssh/id_*"));
    auto *browse = new QPushButton(tr("Browse..."), this);
    connect(browse, &QPushButton::clicked, this, &SshPanel::browseForKey);
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
    m_legacy->setToolTip(
        tr("This option lets Sterna use pre-2020 SSH security methods. Some console "
           "servers cannot operate without these methods. These methods give less "
           "security. This option is only for applicable equipment. Your SSH "
           "configuration can enable these methods automatically."));

    m_useConfig = new QCheckBox(tr("Read ~/.ssh/config"), this);
    m_useConfig->setChecked(true);

    auto *form = new QFormLayout(this);
    form->setContentsMargins(0, 0, 0, 0);
    form->addRow(text("DLG_AUTH_USERNAME", tr("User:")), m_user);
    form->addRow(text("DLG_AUTH_PRIVATEKEY", tr("Private key:")), identityRow);
    form->addRow(QString(), m_agent);
    form->addRow(QString(), m_useConfig);
    form->addRow(QString(), m_legacy);
}

void SshPanel::browseForKey()
{
    const QString title =
        m_i18n ? m_i18n->plainText("FILEDLG_OPEN_PRIVATEKEY_TITLE",
                                   tr("Private key"), "TTSSH")
               : tr("Private key");
    const QString path = QFileDialog::getOpenFileName(
        this, title, QDir::homePath() + QStringLiteral("/.ssh"),
        tr("All files (*)"));
    if (!path.isEmpty()) {
        m_identity->setText(path);
    }
}

QString SshPanel::user() const
{
    return m_user->text().trimmed();
}

QString SshPanel::identity() const
{
    return m_identity->text().trimmed();
}

bool SshPanel::legacy() const
{
    return m_legacy->isChecked();
}

void SshPanel::setInitial(const QString &user, const QString &identity, bool legacy)
{
    m_user->setText(user);
    m_identity->setText(identity);
    m_legacy->setChecked(legacy);
}

void SshPanel::fill(TtSshParams *out, const QString &host, quint16 port)
{
    tt_ssh_params_default(out);

    m_hostUtf8 = host.toUtf8();
    out->host = m_hostUtf8.constData();
    out->port = port;

    // Empty means "take it from the config", so an empty field sends null
    // rather than an empty string — the ABI treats those differently and this
    // is the one place the difference is visible to a user.
    const QString name = user();
    if (!name.isEmpty()) {
        m_userUtf8 = name.toUtf8();
        out->user = m_userUtf8.constData();
    }

    const QString key = identity();
    if (!key.isEmpty()) {
        m_identityUtf8 = key.toUtf8();
        m_identities[0] = m_identityUtf8.constData();
        m_identities[1] = nullptr;
        out->identities = m_identities;
    }

    out->use_agent = m_agent->isChecked();
    out->use_ssh_config = m_useConfig->isChecked();
    out->legacy = m_legacy->isChecked();
}
