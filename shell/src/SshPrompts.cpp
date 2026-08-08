// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "SshPrompts.h"

#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QStyle>
#include <QVBoxLayout>

HostKeyDialog::HostKeyDialog(const HostKeyRequest &request, QWidget *parent)
    : QDialog(parent)
{
    const bool changed = request.verdict == TT_HOST_KEY_CHANGED;
    setWindowTitle(changed ? tr("Host key changed") : tr("Unknown host"));

    QString headline;
    QString detail;
    switch (request.verdict) {
    case TT_HOST_KEY_CHANGED:
        headline = tr("The host key for %1 has changed.").arg(request.host);
        detail = tr("This is what an intercepted connection looks like. It is "
                    "also what a rebuilt server looks like — nothing here can "
                    "tell the two apart, and only you know which you were "
                    "expecting.\n\n"
                    "The old key is recorded at %1.")
                     .arg(request.recordedAt);
        break;
    case TT_HOST_KEY_NEW_ALGORITHM:
        headline = tr("%1 is offering a key of a type not recorded for it.")
                       .arg(request.host);
        detail = tr("The host is known, but only by %1. A server that gained a "
                    "new key type looks exactly like this, and so does one "
                    "being impersonated by something that only has the other "
                    "kind.")
                     .arg(request.alsoKnown);
        break;
    default:
        headline = tr("%1 has not been connected to before.").arg(request.host);
        detail = tr("There is no way to verify a first connection from here. "
                    "Compare the fingerprint against one you got some other "
                    "way — the machine's console, the person who set it up — "
                    "if it matters.");
        break;
    }

    auto *icon = new QLabel(this);
    icon->setPixmap(style()
                        ->standardIcon(changed ? QStyle::SP_MessageBoxCritical
                                               : QStyle::SP_MessageBoxQuestion)
                        .pixmap(48, 48));
    icon->setAlignment(Qt::AlignTop);

    auto *headlineLabel = new QLabel(headline, this);
    QFont bold = headlineLabel->font();
    bold.setBold(true);
    headlineLabel->setFont(bold);
    headlineLabel->setWordWrap(true);

    auto *detailLabel = new QLabel(detail, this);
    detailLabel->setWordWrap(true);

    // Both fingerprints, one above the other, when there are two. Showing the
    // new one alone and describing the old one in a paragraph makes the
    // comparison — the only thing a user can actually do here — into an
    // exercise in scrolling their eyes between two places.
    QString keys = changed
                       ? tr("offered   %1 %2\nrecorded  %3 %4")
                             .arg(request.algorithm, request.fingerprint,
                                  request.algorithm, request.recordedFingerprint)
                       : tr("%1 %2").arg(request.algorithm, request.fingerprint);
    // Selectable, because comparing 43 base64 characters by eye is how people
    // convince themselves two fingerprints match when they do not.
    auto *fingerprint = new QLabel(keys, this);
    fingerprint->setTextInteractionFlags(Qt::TextSelectableByMouse);
    QFont mono = fingerprint->font();
    mono.setFamily(QStringLiteral("monospace"));
    mono.setStyleHint(QFont::Monospace);
    fingerprint->setFont(mono);
    fingerprint->setTextFormat(Qt::PlainText);

    auto *text = new QVBoxLayout;
    text->addWidget(headlineLabel);
    text->addWidget(fingerprint);
    text->addWidget(detailLabel);
    text->addStretch(1);

    auto *top = new QHBoxLayout;
    top->addWidget(icon);
    top->addSpacing(12);
    top->addLayout(text, 1);

    auto *buttons = new QDialogButtonBox(this);
    QPushButton *save =
        buttons->addButton(tr("Accept and remember"), QDialogButtonBox::AcceptRole);
    QPushButton *once =
        buttons->addButton(tr("Accept once"), QDialogButtonBox::AcceptRole);
    QPushButton *refuse =
        buttons->addButton(tr("Disconnect"), QDialogButtonBox::RejectRole);

    connect(save, &QPushButton::clicked, this, [this] {
        m_decision = 1;
        accept();
    });
    connect(once, &QPushButton::clicked, this, [this] {
        m_decision = 2;
        accept();
    });
    connect(refuse, &QPushButton::clicked, this, [this] {
        m_decision = 0;
        reject();
    });
    // Closing the window with no answer is a refusal, which `m_decision` is
    // already: nothing here can leave a connection accepted by accident.
    connect(this, &QDialog::rejected, this, [this] { m_decision = 0; });

    // A changed key defaults to refusing, and remembering it is not offered as
    // the obvious button. Return should not be able to overwrite the recorded
    // key of a machine that may be being impersonated.
    refuse->setDefault(changed);
    save->setDefault(!changed);
    if (changed) {
        save->setEnabled(false);
        save->setToolTip(tr("Remove the recorded key at %1 first, once you know "
                            "why it changed.")
                             .arg(request.recordedAt));
    }

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(top);
    layout->addWidget(buttons);
    // Wrapped labels have no useful width of their own, so the dialog needs
    // one before the layout can decide how tall it is.
    setMinimumWidth(560);
    layout->setSizeConstraint(QLayout::SetMinimumSize);
}

AuthDialog::AuthDialog(const AuthRequest &request, QWidget *parent)
    : QDialog(parent)
{
    switch (request.kind) {
    case TT_SSH_AUTH_PASSPHRASE:
        setWindowTitle(tr("Key passphrase"));
        break;
    case TT_SSH_AUTH_KEYBOARD_INTERACTIVE:
        setWindowTitle(tr("Authentication"));
        break;
    default:
        setWindowTitle(tr("Password"));
        break;
    }

    auto *layout = new QVBoxLayout(this);

    // The server's own wording, shown as it came. A device that says "enter
    // your RSA token code" means it, and replacing that with "Password:" is
    // how a user ends up typing the wrong secret.
    for (const QString &line : {request.name, request.instruction}) {
        if (!line.isEmpty()) {
            auto *label = new QLabel(line, this);
            label->setWordWrap(true);
            layout->addWidget(label);
        }
    }

    auto *form = new QFormLayout;
    for (const AuthRequest::Line &line : request.lines) {
        auto *field = new QLineEdit(this);
        // The server chooses whether to echo, and it is entitled to: a
        // keyboard-interactive challenge may ask for something that is not a
        // secret at all.
        field->setEchoMode(line.echo ? QLineEdit::Normal : QLineEdit::Password);
        m_fields.append(field);
        form->addRow(line.text, field);
    }
    layout->addLayout(form);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    if (!m_fields.isEmpty()) {
        m_fields.first()->setFocus();
    }
    setMinimumWidth(400);
}

QStringList AuthDialog::answers() const
{
    QStringList out;
    out.reserve(m_fields.size());
    for (QLineEdit *field : m_fields) {
        out.append(field->text());
    }
    return out;
}
