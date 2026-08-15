// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "XferDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QVBoxLayout>

#include "I18n.h"

namespace {

/// A size in the units a person reads.
QString humanBytes(qint64 n)
{
    if (n < 1024) {
        return XferProgressDialog::tr("%1 B").arg(n);
    }
    if (n < 1024 * 1024) {
        return XferProgressDialog::tr("%1 KiB").arg(n / 1024.0, 0, 'f', 1);
    }
    return XferProgressDialog::tr("%1 MiB").arg(n / (1024.0 * 1024.0), 0, 'f', 1);
}

} // namespace

// --- picking a protocol ------------------------------------------------------

XferOptionsDialog::XferOptionsDialog(bool sending, Session *session, QWidget *parent,
                                     const I18n *i18n)
    : QDialog(parent), m_sending(sending), m_session(session), m_i18n(i18n)
{
    const auto text = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->text(key, fallback) : fallback;
    };
    const auto plainText = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->plainText(key, fallback) : fallback;
    };

    setWindowTitle(
        plainText(sending ? "DLG_FILETRANS_TITLE"
                          : "FILEDLG_TRANS_TITLE_RECVFILE",
                  sending ? tr("Send file") : tr("Receive file")));

    m_protocol = new QComboBox(this);
    m_protocol->addItem(tr("ZMODEM"), TT_XFER_PROTOCOL_Z_MODEM);
    m_protocol->addItem(tr("YMODEM"), TT_XFER_PROTOCOL_Y_MODEM);
    m_protocol->addItem(tr("XMODEM"), TT_XFER_PROTOCOL_X_MODEM);
    m_protocol->addItem(tr("Kermit"), TT_XFER_PROTOCOL_KERMIT);
    // Listed, and listed last, with what is known about them said out loud
    // rather than left for someone to discover.
    m_protocol->addItem(tr("B-Plus (untested)"), TT_XFER_PROTOCOL_B_PLUS);
    m_protocol->addItem(tr("Quick-VAN (untested)"), TT_XFER_PROTOCOL_QUICK_VAN);
    m_protocol->setToolTip(
        tr("The recommended protocol is ZMODEM if both systems support it. The "
           "original online services for B-Plus and Quick-VAN are not available. "
           "Thus, Sterna cannot test these protocols. Sterna will try these "
           "protocols. Protocol failure is possible."));

    m_option = new QComboBox(this);
    m_optionLabel = new QLabel(text("DLG_XOPT", tr("Blocks:")), this);

    m_text = new QCheckBox(tr("Text mode (translate line endings)"), this);
    m_text->setToolTip(
        tr("This option is only for text files received with XMODEM. It changes "
           "CR-only and LF-only line endings to CRLF and removes Ctrl+Z padding. "
           "This option has no effect during transmission."));

    connect(m_protocol, &QComboBox::currentIndexChanged, this,
            &XferOptionsDialog::protocolChanged);

    auto *form = new QFormLayout;
    form->addRow(text("DLG_PROT_PROTO", tr("Protocol:")), m_protocol);
    form->addRow(m_optionLabel, m_option);
    form->addRow(QString(), m_text);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->button(QDialogButtonBox::Ok)
        ->setText(text("BTN_OK", tr("OK")));
    buttons->button(QDialogButtonBox::Cancel)
        ->setText(text("BTN_CANCEL", tr("Cancel")));
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(buttons);

    protocolChanged();
}

void XferOptionsDialog::protocolChanged()
{
    const auto proto = static_cast<TtXferProtocol>(m_protocol->currentData().toInt());
    const auto optionText = [this](const char *key, const QString &fallback) {
        return m_i18n ? m_i18n->plainText(key, fallback) : fallback;
    };

    m_option->clear();
    switch (proto) {
    case TT_XFER_PROTOCOL_X_MODEM:
        // Upstream's four, in the order its own dialog lists them. CRC first
        // because it is what a receiver asks for by sending `C`; checksum is
        // for a peer old enough not to know about CRC.
        m_option->addItem(
            tr("128 bytes, %1").arg(optionText("DLG_XOPT_CRC", tr("CRC"))), 2);
        m_option->addItem(tr("128 bytes, %1")
                              .arg(optionText("DLG_XOPT_CHECKSUM", tr("checksum"))),
                          1);
        m_option->addItem(
            tr("%1, %2")
                .arg(optionText("DLG_XOPT_1K", tr("1K")),
                     optionText("DLG_XOPT_CRC", tr("CRC"))),
            3);
        m_option->addItem(
            tr("%1, %2")
                .arg(optionText("DLG_XOPT_1K", tr("1K")),
                     optionText("DLG_XOPT_CHECKSUM", tr("checksum"))),
            4);
        break;
    case TT_XFER_PROTOCOL_Y_MODEM:
        // 1K is the only value the sender's packet builder has a case for —
        // `filesys_proto.cpp:1409` hardcodes it and `YSendPacket` asserts on
        // anything else. G is receive-only in practice.
        m_option->addItem(tr("1K blocks"), 1);
        if (!m_sending) {
            m_option->addItem(tr("YMODEM-g (no per-block ACK)"), 2);
        }
        break;
    default:
        break;
    }

    const bool hasOption = m_option->count() > 0;
    m_option->setVisible(hasOption);
    m_optionLabel->setVisible(hasOption);
    m_text->setVisible(proto == TT_XFER_PROTOCOL_X_MODEM);

    // Seed from the settings rather than from the list order. Upstream's
    // XMODEM default is plain checksum — the `else` branch of `ttset.c:1039` —
    // so "whichever we listed first" is a different answer from "what the file
    // says", and the file is the one the user can change.
    const TtXferJob d = defaults();
    const int i = m_option->findData(d.option);
    if (i >= 0) {
        m_option->setCurrentIndex(i);
    }
    m_text->setChecked(d.text);
}

TtXferJob XferOptionsDialog::defaults() const
{
    TtXferJob job = {};
    job.protocol = static_cast<TtXferProtocol>(m_protocol->currentData().toInt());
    job.sending = m_sending;
    if (m_session != nullptr && m_session->handle() != nullptr) {
        tt_session_xfer_defaults(m_session->handle(), &job);
    } else {
        // The same values the core would give, for a dialog with no session
        // behind it — `xfer_test` builds one that way.
        job.option = 1;
        job.binary = true;
    }
    return job;
}

void XferOptionsDialog::setProtocol(TtXferProtocol protocol)
{
    const int i = m_protocol->findData(protocol);
    if (i >= 0) {
        m_protocol->setCurrentIndex(i);
    }
}

QString XferOptionsDialog::protocolName() const
{
    return m_protocol->currentText();
}

QString XferOptionsDialog::transferTitle() const
{
    const auto protocol =
        static_cast<TtXferProtocol>(m_protocol->currentData().toInt());
    const char *key = nullptr;
    switch (protocol) {
    case TT_XFER_PROTOCOL_X_MODEM:
        key = m_sending ? "FILEDLG_TRANS_TITLE_XSEND"
                        : "FILEDLG_TRANS_TITLE_XRCV";
        break;
    case TT_XFER_PROTOCOL_Y_MODEM:
        key = m_sending ? "FILEDLG_TRANS_TITLE_YSEND"
                        : "FILEDLG_TRANS_TITLE_YRCV";
        break;
    case TT_XFER_PROTOCOL_Z_MODEM:
        key = m_sending ? "FILEDLG_TRANS_TITLE_ZSEND"
                        : "FILEDLG_TRANS_TITLE_ZRCV";
        break;
    case TT_XFER_PROTOCOL_KERMIT:
        key = m_sending ? "FILEDLG_TRANS_TITLE_KMTSEND"
                        : "FILEDLG_TRANS_TITLE_KMTRCV";
        break;
    case TT_XFER_PROTOCOL_B_PLUS:
        key = m_sending ? "FILEDLG_TRANS_TITLE_BPSEND"
                        : "FILEDLG_TRANS_TITLE_BPRCV";
        break;
    case TT_XFER_PROTOCOL_QUICK_VAN:
        key = m_sending ? "FILEDLG_TRANS_TITLE_QVSEND"
                        : "FILEDLG_TRANS_TITLE_QVRCV";
        break;
    case TT_XFER_PROTOCOL_RAW:
        key = m_sending ? "FILEDLG_TRANS_TITLE_SENDFILE"
                        : "FILEDLG_TRANS_TITLE_RAWRCV";
        break;
    }

    const QString fallback =
        m_sending ? tr("Sending — %1").arg(protocolName())
                  : tr("Receiving — %1").arg(protocolName());
    return m_i18n && key ? m_i18n->plainText(key, fallback) : fallback;
}

bool XferOptionsDialog::needsReceiveName() const
{
    return !m_sending
           && static_cast<TtXferProtocol>(m_protocol->currentData().toInt())
                  == TT_XFER_PROTOCOL_X_MODEM;
}

TtXferJob XferOptionsDialog::job() const
{
    TtXferJob job = defaults();
    job.option = m_option->count() > 0 ? m_option->currentData().toInt() : 0;
    job.text = m_text->isVisible() && m_text->isChecked();
    job.kermit_mode = m_sending ? 3 : 1;
    // Not offered: the auto-start flag says the peer's trigger has already
    // gone past in the terminal stream, which is true of a transfer the
    // terminal started by itself and never of one a user asked for from a
    // menu. `transfer.zmodem_auto` is about the watching, not about this.
    job.auto_start = false;
    return job;
}

// --- watching it happen ------------------------------------------------------

XferProgressDialog::XferProgressDialog(const QString &title, QWidget *parent,
                                       const I18n *i18n)
    : QDialog(parent), m_i18n(i18n)
{
    setWindowTitle(title);

    m_file = new QLabel(tr("Starting…"), this);
    m_file->setTextInteractionFlags(Qt::TextSelectableByMouse);
    // Elide rather than grow: a path can be longer than a screen, and a dialog
    // that resizes itself to fit one jumps under the pointer.
    m_file->setWordWrap(false);
    m_file->setMinimumWidth(380);

    m_bar = new QProgressBar(this);
    m_bar->setRange(0, 100);
    m_bar->setValue(0);

    m_stats = new QLabel(QString(), this);

    m_buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, this);
    m_buttons->button(QDialogButtonBox::Cancel)
        ->setText(m_i18n ? m_i18n->text("BTN_CANCEL", tr("Cancel"))
                         : tr("Cancel"));
    connect(m_buttons, &QDialogButtonBox::rejected, this, [this] {
        if (m_done) {
            accept();
        } else {
            emit cancelled();
        }
    });

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(m_file);
    layout->addWidget(m_bar);
    layout->addWidget(m_stats);
    layout->addWidget(m_buttons);
}

void XferProgressDialog::update(const TransferProgress &progress)
{
    if (m_done) {
        return;
    }
    if (!progress.file.isEmpty()) {
        m_file->setText(progress.file);
        m_file->setToolTip(progress.file);
    }

    // Three states, not two. A protocol that knows the size gives a
    // percentage; one that does not — XMODEM never learns it, and ZMODEM only
    // if the sender said — gets a busy indicator rather than a bar frozen at
    // zero, which reads as "stuck".
    if (progress.total > 0) {
        m_bar->setRange(0, 100);
        m_bar->setValue(progress.percent < 0 ? 0 : progress.percent);
    } else {
        m_bar->setRange(0, 0);
    }

    const double secs = progress.elapsedMs / 1000.0;
    QString rate;
    if (secs > 0.5 && progress.bytes > 0) {
        rate = tr(" · %1/s").arg(humanBytes(static_cast<qint64>(progress.bytes / secs)));
    }
    if (progress.total > 0) {
        m_stats->setText(tr("%1 of %2%3")
                             .arg(humanBytes(progress.done), humanBytes(progress.total), rate));
    } else {
        m_stats->setText(tr("%1%2").arg(humanBytes(progress.bytes), rate));
    }
}

void XferProgressDialog::finish(const TransferResult &result)
{
    m_done = true;
    m_bar->setRange(0, 100);
    m_bar->setValue(result.success ? 100 : m_bar->value());

    if (result.success) {
        m_stats->setText(tr("Complete — %1 in %2 s")
                             .arg(humanBytes(result.bytes))
                             .arg(result.elapsedMs / 1000.0, 0, 'f', 1));
    } else if (result.cancelled) {
        m_stats->setText(tr("Cancelled after %1").arg(humanBytes(result.bytes)));
    } else if (!result.message.isEmpty()) {
        // The protocol's own words. Often the only account of the failure
        // there is — upstream puts these in a message box.
        m_stats->setText(tr("Failed: %1").arg(result.message));
    } else {
        m_stats->setText(tr("Failed after %1").arg(humanBytes(result.bytes)));
    }

    m_buttons->setStandardButtons(QDialogButtonBox::Close);
    if (QPushButton *close = m_buttons->button(QDialogButtonBox::Close)) {
        close->setText(m_i18n ? m_i18n->text("BTN_CLOSE", tr("Close"))
                              : tr("Close"));
        close->setDefault(true);
        close->setFocus();
    }
}
