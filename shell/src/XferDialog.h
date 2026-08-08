// The file-transfer dialogs: what to run, and how it is going.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>
#include <QStringList>

#include "Session.h"
#include "termitta.h"

class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QProgressBar;

/// Pick a protocol and its options.
///
/// One dialog for send and receive, because the choice is the same one and
/// only two fields differ. What is *not* the same for both is the file
/// question, and that is deliberately not here: choosing files is
/// `QFileDialog`'s job and a hand-rolled list beside a native file browser is
/// worse at it in every way.
class XferOptionsDialog : public QDialog {
    Q_OBJECT

public:
    XferOptionsDialog(bool sending, QWidget *parent = nullptr);

    /// The job as configured. Ready to hand to `Session::sendFiles`.
    TtXferJob job() const;
    /// The protocol as the user picked it, for a window title.
    QString protocolName() const;
    /// Whether the chosen protocol needs a destination filename from the user.
    /// True for XMODEM and nothing else: its wire format carries no name, so
    /// there is nothing to derive one from.
    bool needsReceiveName() const;

    void setProtocol(TtXferProtocol protocol);

private slots:
    void protocolChanged();

private:
    bool m_sending;
    QComboBox *m_protocol;
    QComboBox *m_option;
    QCheckBox *m_text;
    QLabel *m_optionLabel;
};

/// Progress, and the one button that matters.
///
/// Modeless on purpose. Upstream's is modal, and modal here would be a
/// mistake for a reason that is ours rather than theirs: the transfer is
/// driven by *this* window's event loop, so a dialog that blocks it blocks the
/// transfer. The terminal underneath is already deaf and mute — the core
/// refuses input while a transfer runs — so there is nothing left for modality
/// to protect.
class XferProgressDialog : public QDialog {
    Q_OBJECT

public:
    XferProgressDialog(const QString &title, QWidget *parent = nullptr);

    void update(const TransferProgress &progress);
    /// Show how it ended and turn Cancel into Close. The dialog stays up: a
    /// transfer that failed has something to say, and a window that vanished
    /// at the moment of failure would say it to nobody.
    void finish(const TransferResult &result);

signals:
    /// Cancel was pressed. The transfer does not stop here — the protocol
    /// sends its cancel sequence and finishes on its own terms.
    void cancelled();

private:
    QLabel *m_file;
    QLabel *m_stats;
    QProgressBar *m_bar;
    QDialogButtonBox *m_buttons;
    bool m_done = false;
};
