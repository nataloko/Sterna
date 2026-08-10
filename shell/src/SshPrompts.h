// The two dialogs an SSH connection raises: the host key, and whatever has to
// be typed.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QStringList>
#include <QVector>

#include "Session.h"

class I18n;
class QLineEdit;

/// "Is this the machine you meant?"
///
/// Three answers rather than two, and the third is the point. "Yes, but do not
/// write it down" is what someone on a network they do not trust means, and
/// collapsing it into "yes" records a key they did not want recorded.
///
/// A **changed** key gets different words, a different icon and a different
/// default button from a first connection. Presenting the two the same way is
/// how users learn to click through the one warning that matters.
class HostKeyDialog : public QDialog {
    Q_OBJECT

public:
    HostKeyDialog(const HostKeyRequest &request, QWidget *parent = nullptr,
                  const I18n *i18n = nullptr);

    /// 1 to accept and record, 2 to accept once, 0 to refuse.
    int decision() const { return m_decision; }

private:
    int m_decision = 0;
};

/// A password, a key passphrase, or a keyboard-interactive challenge.
///
/// One dialog for all three because the protocol makes them the same shape: a
/// list of prompts, each of which the server says whether to echo.
class AuthDialog : public QDialog {
    Q_OBJECT

public:
    AuthDialog(const AuthRequest &request, QWidget *parent = nullptr,
               const I18n *i18n = nullptr);

    /// One string per prompt, in the order asked.
    QStringList answers() const;

private:
    QVector<QLineEdit *> m_fields;
};
