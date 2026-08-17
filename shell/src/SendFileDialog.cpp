// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "SendFileDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QProgressBar>
#include <QPushButton>
#include <QSpinBox>
#include <QTimer>
#include <QVBoxLayout>

#include "I18n.h"

namespace {

/// A size in the units a person reads. The same three steps
/// `XferProgressDialog` uses, kept separate because that one is `static` to its
/// own translation unit and sharing it would put a formatting helper in a
/// header.
QString humanBytes(qint64 n)
{
    if (n < 1024) {
        return SendProgressDialog::tr("%1 B").arg(n);
    }
    if (n < 1024 * 1024) {
        return SendProgressDialog::tr("%1 KiB").arg(n / 1024.0, 0, 'f', 1);
    }
    return SendProgressDialog::tr("%1 MiB").arg(n / (1024.0 * 1024.0), 0, 'f', 1);
}

} // namespace

// --- how to send it ----------------------------------------------------------

SendFileDialog::SendFileDialog(Session *session, QWidget *parent, const I18n *i18n)
    : QDialog(parent), m_session(session)
{
    const auto text = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->text(key, fallback) : fallback;
    };

    setWindowTitle(tr("Send file line by line"));
    setObjectName(QStringLiteral("sendFileDialog"));

    m_pace = new QComboBox(this);
    m_pace->setObjectName(QStringLiteral("sendPace"));
    m_pace->addItem(tr("Do not wait"), TT_SEND_PACE_NONE);
    m_pace->addItem(tr("After each character"), TT_SEND_PACE_PER_CHAR);
    m_pace->addItem(tr("After each line"), TT_SEND_PACE_PER_LINE);
    m_pace->addItem(tr("After each group of bytes"), TT_SEND_PACE_PER_CHUNK);
    m_pace->setToolTip(
        tr("A device with no flow control can lose the text that arrives while "
           "it is busy. A wait after each line gives the device the time to "
           "read the last one."));

    m_interval = new QSpinBox(this);
    m_interval->setObjectName(QStringLiteral("sendInterval"));
    // The ceiling is the one the *file* can hold, not a round number of hours.
    // `SendfileDelayTick` is a `WORD` upstream, so the schema is `uint16` — and
    // the C field's width is part of the bound and runs first, which means
    // `setSetting` narrows rather than clamps: an hour offered here came back
    // from the next load as 3600000 mod 65536, a wait of 34.5 seconds where
    // somebody asked for sixty minutes, with the send itself having honoured the
    // number they typed. Sixty-five seconds is the honest offer.
    m_interval->setRange(0, 65535);
    m_interval->setSuffix(tr(" ms"));
    m_interval->setToolTip(
        tr("Sterna waits for this time after each piece of the file. A value of "
           "0 removes the wait."));
    m_intervalLabel = new QLabel(tr("Interval:"), this);

    m_group = new QSpinBox(this);
    m_group->setObjectName(QStringLiteral("sendGroup"));
    m_group->setRange(1, 1024 * 1024);
    m_group->setToolTip(tr("The number of bytes that Sterna sends before each wait."));
    m_groupLabel = new QLabel(tr("Bytes in each group:"), this);

    m_binary = new QCheckBox(tr("Send the bytes of the file with no change"), this);
    m_binary->setObjectName(QStringLiteral("sendBinary"));
    m_binary->setToolTip(
        tr("Sterna sends a text file with the line ending that the terminal "
           "uses. With this box selected, Sterna sends each byte of the file "
           "unchanged."));

    m_echo = new QCheckBox(tr("Show the sent text on the screen"), this);
    m_echo->setObjectName(QStringLiteral("sendEcho"));
    m_echo->setToolTip(
        tr("Sterna puts a copy of each piece on the screen as it sends it. Some "
           "devices send a copy back. For those devices, this box shows each "
           "line two times."));

    m_gate = new QComboBox(this);
    m_gate->setObjectName(QStringLiteral("sendGate"));
    m_gate->addItem(tr("Nothing"), TT_SEND_GATE_NONE);
    m_gate->addItem(tr("A prompt"), TT_SEND_GATE_PROMPT);
    m_gate->addItem(tr("The echo of the line"), TT_SEND_GATE_ECHO);
    m_gate->addItem(tr("A quiet line"), TT_SEND_GATE_QUIET);
    m_gate->setToolTip(
        tr("A device tells you that it is ready when it prints its prompt. An "
           "interval can only be a guess at the same thing. With this "
           "selection, Sterna sends each line when the device is ready for it."));

    m_pattern = new QLineEdit(this);
    m_pattern->setObjectName(QStringLiteral("sendGatePattern"));
    m_pattern->setPlaceholderText(tr("for example, [#>$] $"));
    m_pattern->setToolTip(
        tr("This is a regular expression. Sterna tests it against the text that "
           "the device sends, after each line feed and again at the end. Thus a "
           "prompt with no line feed also has a match. The CR stays on a "
           "completed line, thus $ does not have a match at the end of one."));
    m_patternLabel = new QLabel(tr("Prompt:"), this);
    m_patternError = new QLabel(this);
    m_patternError->setObjectName(QStringLiteral("sendGatePatternError"));
    m_patternError->setWordWrap(true);

    m_timeout = new QSpinBox(this);
    m_timeout->setObjectName(QStringLiteral("sendGateTimeout"));
    m_timeout->setRange(100, 600000);
    m_timeout->setSuffix(tr(" ms"));
    m_timeout->setToolTip(
        tr("Sterna waits this time for the device. When this time is complete, "
           "Sterna sends the next line and counts a timeout. The send does not "
           "stop: a send that stopped at the first line with no answer would "
           "leave one half of a configuration in the device."));
    m_timeoutLabel = new QLabel(tr("Give up after:"), this);

    m_quiet = new QSpinBox(this);
    m_quiet->setObjectName(QStringLiteral("sendQuiet"));
    m_quiet->setRange(10, 60000);
    m_quiet->setSuffix(tr(" ms"));
    m_quiet->setToolTip(tr("The device must be quiet for this time."));
    m_quietLabel = new QLabel(tr("Quiet for:"), this);

    connect(m_pace, &QComboBox::currentIndexChanged, this, &SendFileDialog::paceChanged);
    connect(m_gate, &QComboBox::currentIndexChanged, this, &SendFileDialog::gateChanged);
    connect(m_pattern, &QLineEdit::textChanged, this, &SendFileDialog::checkPattern);

    auto *form = new QFormLayout;
    form->addRow(tr("Wait for:"), m_gate);
    form->addRow(m_patternLabel, m_pattern);
    form->addRow(QString(), m_patternError);
    form->addRow(m_timeoutLabel, m_timeout);
    form->addRow(m_quietLabel, m_quiet);
    form->addRow(tr("When to wait:"), m_pace);
    form->addRow(m_intervalLabel, m_interval);
    form->addRow(m_groupLabel, m_group);
    form->addRow(QString(), m_binary);
    form->addRow(QString(), m_echo);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->button(QDialogButtonBox::Ok)->setText(text("BTN_OK", tr("OK")));
    buttons->button(QDialogButtonBox::Cancel)->setText(text("BTN_CANCEL", tr("Cancel")));
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(buttons);

    // Seeded from the settings file rather than invented here — the same rule
    // `XferOptionsDialog::defaults` follows, and the reason those four keys
    // exist at all.
    TtSendOptions d {};
    if (m_session) {
        d = m_session->sendDefaults();
    } else {
        d.pace = TT_SEND_PACE_NONE;
        d.tick_ms = 0;
        d.chunk = 4096;
    }
    // A tick of zero collapses any pace to `None` in the core, so a file that
    // says `PerLine` with no tick would open this dialog on "Do not wait" and
    // then be written back as `NoDelay` — losing the user's choice of *type*
    // for want of a number. The combo shows what the file says and the interval
    // shows what it says separately.
    m_pace->setCurrentIndex(m_pace->findData(d.pace));
    if (m_pace->currentIndex() < 0) {
        m_pace->setCurrentIndex(0);
    }
    m_interval->setValue(static_cast<int>(d.tick_ms));
    m_group->setValue(d.chunk > 0 ? static_cast<int>(d.chunk) : 4096);
    m_binary->setChecked(d.binary);
    m_echo->setChecked(d.echo);
    m_gate->setCurrentIndex(qMax(0, m_gate->findData(d.gate)));
    m_pattern->setText(QString::fromUtf8(d.gate_pattern ? d.gate_pattern : ""));
    m_timeout->setValue(d.gate_timeout_ms > 0 ? static_cast<int>(d.gate_timeout_ms) : 500);
    m_quiet->setValue(d.quiet_ms > 0 ? static_cast<int>(d.quiet_ms) : 300);

    paceChanged();
    gateChanged();
    checkPattern();
}

void SendFileDialog::paceChanged()
{
    const auto pace = static_cast<TtSendPace>(m_pace->currentData().toInt());
    const bool waits = pace != TT_SEND_PACE_NONE;
    const bool grouped = pace == TT_SEND_PACE_PER_CHUNK;
    m_interval->setVisible(waits);
    m_intervalLabel->setVisible(waits);
    m_group->setVisible(grouped);
    m_groupLabel->setVisible(grouped);
}

void SendFileDialog::gateChanged()
{
    const auto gate = static_cast<TtSendGate>(m_gate->currentData().toInt());
    const bool gated = gate != TT_SEND_GATE_NONE;
    const bool pattern = gate == TT_SEND_GATE_PROMPT;
    m_pattern->setVisible(pattern);
    m_patternLabel->setVisible(pattern);
    m_timeout->setVisible(gated);
    m_timeoutLabel->setVisible(gated);
    m_quiet->setVisible(gate == TT_SEND_GATE_QUIET);
    m_quietLabel->setVisible(gate == TT_SEND_GATE_QUIET);
    checkPattern();
}

void SendFileDialog::checkPattern()
{
    auto *ok = findChild<QDialogButtonBox *>()
                   ? findChild<QDialogButtonBox *>()->button(QDialogButtonBox::Ok)
                   : nullptr;
    const bool wanted =
        static_cast<TtSendGate>(m_gate->currentData().toInt()) == TT_SEND_GATE_PROMPT;
    if (!wanted) {
        m_patternError->clear();
        // Hidden as well as cleared: an empty label still takes a row, and the
        // gap it leaves reads as a field somebody forgot to fill in.
        m_patternError->setVisible(false);
        if (ok) {
            ok->setEnabled(true);
        }
        return;
    }
    const QByteArray utf8 = m_pattern->text().toUtf8();
    // An empty one is refused before the engine sees it, because the engine
    // takes it: it compiles, it matches everything, and it would make this a
    // send that waits for any byte at all. The settings reader answers "no
    // gate" for the same pair (`send::gate_from`), so accepting it here would
    // pace one way now and another way after a restart. Saying so beats either.
    if (utf8.isEmpty()) {
        m_patternError->setText(tr("Give a pattern for the prompt to wait for."));
        m_patternError->setVisible(true);
        if (ok) {
            ok->setEnabled(false);
        }
        return;
    }
    // Asked as it is typed, the way the highlight editor asks: a pattern the
    // engine refuses would otherwise become a send that holds every line for
    // its whole timeout and says nothing about why.
    const bool good = tt_send_gate_check(utf8.constData()) == TT_OK;
    m_patternError->setText(good ? QString() : QString::fromUtf8(tt_last_error()));
    m_patternError->setVisible(!good);
    if (ok) {
        ok->setEnabled(good);
    }
}

TtSendOptions SendFileDialog::options() const
{
    TtSendOptions out {};
    out.pace = static_cast<TtSendPace>(m_pace->currentData().toInt());
    out.tick_ms = static_cast<uint32_t>(m_interval->value());
    out.chunk = static_cast<uint32_t>(m_group->value());
    out.binary = m_binary->isChecked();
    out.echo = m_echo->isChecked();
    out.gate = static_cast<TtSendGate>(m_gate->currentData().toInt());
    // Kept alive on the dialog, because the ABI borrows it for the call.
    m_patternUtf8 = m_pattern->text().toUtf8();
    out.gate_pattern = m_patternUtf8.constData();
    out.gate_timeout_ms = static_cast<uint32_t>(m_timeout->value());
    out.quiet_ms = static_cast<uint32_t>(m_quiet->value());
    return out;
}

// --- how it is going ---------------------------------------------------------

SendProgressDialog::SendProgressDialog(const QString &title, QWidget *parent,
                                       const I18n *i18n)
    : QDialog(parent), m_i18n(i18n)
{
    setWindowTitle(title);
    setObjectName(QStringLiteral("sendProgressDialog"));

    m_file = new QLabel(tr("Starting…"), this);
    m_file->setTextInteractionFlags(Qt::TextSelectableByMouse);
    m_file->setWordWrap(false);
    m_file->setMinimumWidth(380);

    m_bar = new QProgressBar(this);
    m_bar->setObjectName(QStringLiteral("sendProgressBar"));
    m_bar->setRange(0, 100);
    m_bar->setValue(0);

    m_stats = new QLabel(QString(), this);
    m_stats->setObjectName(QStringLiteral("sendProgressStats"));

    m_buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, this);
    m_buttons->button(QDialogButtonBox::Cancel)
        ->setText(m_i18n ? m_i18n->text("BTN_CANCEL", tr("Stop")) : tr("Stop"));
    m_buttons->button(QDialogButtonBox::Cancel)
        ->setObjectName(QStringLiteral("sendStopButton"));
    // Its own button rather than a third role on the box: a pause is not an
    // answer to the dialog, and giving it `RejectRole` would let Escape trigger
    // it.
    m_pause = m_buttons->addButton(tr("Hold"), QDialogButtonBox::ActionRole);
    m_pause->setObjectName(QStringLiteral("sendPauseButton"));
    m_pause->setCheckable(true);
    connect(m_pause, &QPushButton::toggled, this, [this](bool on) {
        m_pause->setText(on ? tr("Continue") : tr("Hold"));
        emit pauseToggled(on);
    });
    connect(m_buttons, &QDialogButtonBox::rejected, this, [this] {
        if (m_done) {
            accept();
        } else {
            emit cancelled();
        }
    });

    // Ten times a second: fast enough that a line of a config file appears to
    // go out as it happens, slow enough that a 1 ms per-character pace does not
    // spend the frame budget on a text label.
    m_poll = new QTimer(this);
    m_poll->setObjectName(QStringLiteral("sendProgressPollTimer"));
    m_poll->setInterval(100);
    connect(m_poll, &QTimer::timeout, this, &SendProgressDialog::poll);
    m_poll->start();

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(m_file);
    layout->addWidget(m_bar);
    layout->addWidget(m_stats);
    layout->addWidget(m_buttons);
}

void SendProgressDialog::update(const SendProgress &progress)
{
    if (m_done) {
        return;
    }
    if (!progress.name.isEmpty()) {
        m_file->setText(progress.name);
        m_file->setToolTip(progress.name);
    }
    if (progress.total > 0) {
        m_bar->setRange(0, 100);
        m_bar->setValue(static_cast<int>(progress.sent * 100 / progress.total));
    } else {
        m_bar->setRange(0, 0);
    }
    QString more;
    if (progress.paused) {
        more = tr(" · on hold");
    } else if (progress.gated) {
        more = tr(" · waiting for the device");
    } else if (progress.queued > 0) {
        more = tr(" · %n more waiting", nullptr, progress.queued);
    }
    if (progress.timeouts > 0) {
        more += tr(" · %n line(s) with no answer", nullptr,
                   static_cast<int>(progress.timeouts));
    }
    m_stats->setText(tr("%1 of %2%3")
                         .arg(humanBytes(progress.sent), humanBytes(progress.total), more));
}

void SendProgressDialog::finish(const SendResult &result)
{
    m_done = true;
    m_poll->stop();
    m_pause->setEnabled(false);
    m_bar->setRange(0, 100);
    m_bar->setValue(result.total > 0
                        ? static_cast<int>(result.sent * 100 / result.total)
                        : 100);
    switch (result.end) {
    case TT_SEND_END_FINISHED:
        // The count of unanswered lines is the one thing a finished send can
        // still have to say: it usually means the prompt was wrong.
        m_stats->setText(result.timeouts == 0
                             ? tr("Sent %1.").arg(humanBytes(result.sent))
                             : tr("Sent %1. The device did not answer %n line(s).",
                                  nullptr, static_cast<int>(result.timeouts))
                                   .arg(humanBytes(result.sent)));
        break;
    case TT_SEND_END_CANCELLED:
        m_stats->setText(tr("Stopped after %1 of %2.")
                             .arg(humanBytes(result.sent), humanBytes(result.total)));
        break;
    case TT_SEND_END_LINK_LOST:
        m_stats->setText(tr("The connection stopped after %1 of %2.")
                             .arg(humanBytes(result.sent), humanBytes(result.total)));
        break;
    }
    m_buttons->button(QDialogButtonBox::Cancel)
        ->setText(m_i18n ? m_i18n->text("BTN_CLOSE", tr("Close")) : tr("Close"));
}
