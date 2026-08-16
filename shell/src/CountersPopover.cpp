// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "CountersPopover.h"

#include <QCoreApplication>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLocale>
#include <QTimer>
#include <QVBoxLayout>

#include "Session.h"

namespace {

/// The same second the status strip's field ticks on. Nothing here changes
/// faster than a human reads, and a CTS blip shorter than a second is not
/// something a status readout can honestly claim to have seen.
constexpr int kPollMs = 1000;

/// `H:MM:SS`, or a dash when nothing has ever connected — which is not the
/// same as `0:00:00`, and a tab that has never been dialled says so.
QString elapsedText(qint64 ms)
{
    if (ms < 0) {
        return QStringLiteral("—");
    }
    const qint64 total = ms / 1000;
    return QStringLiteral("%1:%2:%3")
        .arg(total / 3600)
        .arg((total / 60) % 60, 2, 10, QLatin1Char('0'))
        .arg(total % 60, 2, 10, QLatin1Char('0'));
}

QString sizeText(quint64 bytes)
{
    return QLocale().formattedDataSize(static_cast<qint64>(bytes));
}

QString rateText(quint64 bytesPerSecond)
{
    return QCoreApplication::translate("CountersPopover", "%1/s")
        .arg(QLocale().formattedDataSize(static_cast<qint64>(bytesPerSecond)));
}

} // namespace

CountersPopover::CountersPopover(QWidget *parent)
    : QFrame(parent, Qt::Popup)
{
    setObjectName(QStringLiteral("countersPopover"));
    setFrameShape(QFrame::StyledPanel);
    // A popup is its own window, so it has to paint its own background or it
    // is a rectangle of whatever happens to be behind it.
    setAutoFillBackground(true);

    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(10, 8, 10, 8);
    outer->setSpacing(6);

    auto *form = new QFormLayout;
    form->setContentsMargins(0, 0, 0, 0);
    form->setHorizontalSpacing(16);
    form->setVerticalSpacing(2);
    outer->addLayout(form);

    const auto row = [&](const QString &caption, const QString &name) {
        auto *value = new QLabel(this);
        value->setObjectName(name);
        value->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        // Selectable, because the first thing anybody does with a byte count
        // is put it in a message to somebody else.
        value->setTextInteractionFlags(Qt::TextSelectableByMouse);
        form->addRow(caption, value);
        return value;
    };

    m_connected = row(tr("Connected"), QStringLiteral("countersConnected"));
    m_received = row(tr("Received"), QStringLiteral("countersReceived"));
    m_sent = row(tr("Sent"), QStringLiteral("countersSent"));
    m_rateIn = row(tr("Receive rate"), QStringLiteral("countersRateIn"));
    m_rateOut = row(tr("Send rate"), QStringLiteral("countersRateOut"));
    m_lines = row(tr("Lines"), QStringLiteral("countersLines"));
    m_breaks = row(tr("Breaks"), QStringLiteral("countersBreaks"));
    // `Send queue` and not `Queued to send`: rule 9's `-ing` restriction and
    // its rule against using a technical noun as a verb both land on the
    // obvious phrasing, and a queue is an ordinary technical noun.
    m_queued = row(tr("Send queue"), QStringLiteral("countersQueued"));

    // The serial half, hidden as one thing: a separator with nothing under it
    // is worse than no separator.
    m_serial = new QWidget(this);
    m_serial->setObjectName(QStringLiteral("countersSerial"));
    auto *serialBox = new QVBoxLayout(m_serial);
    serialBox->setContentsMargins(0, 0, 0, 0);
    serialBox->setSpacing(6);

    auto *rule = new QFrame(m_serial);
    rule->setFrameShape(QFrame::HLine);
    rule->setFrameShadow(QFrame::Sunken);
    serialBox->addWidget(rule);

    auto *lamps = new QHBoxLayout;
    lamps->setContentsMargins(0, 0, 0, 0);
    lamps->setSpacing(12);
    const auto lamp = [&](const QString &text, const QString &name) {
        auto *label = new QLabel(text, m_serial);
        label->setObjectName(name);
        lamps->addWidget(label);
        return label;
    };
    // Exact interface labels, and the names the equipment uses. They are not
    // rewritten and they are not expanded.
    m_cts = lamp(QStringLiteral("CTS"), QStringLiteral("countersCts"));
    m_dsr = lamp(QStringLiteral("DSR"), QStringLiteral("countersDsr"));
    m_cd = lamp(QStringLiteral("CD"), QStringLiteral("countersCd"));
    m_ri = lamp(QStringLiteral("RI"), QStringLiteral("countersRi"));
    lamps->addStretch(1);
    serialBox->addLayout(lamps);

    m_serial->hide();
    outer->addWidget(m_serial);

    m_poll = new QTimer(this);
    m_poll->setObjectName(QStringLiteral("countersPollTimer"));
    m_poll->setInterval(kPollMs);
    connect(m_poll, &QTimer::timeout, this, [this] {
        if (m_session) {
            refresh(m_session);
        }
    });
}

void CountersPopover::refresh(const Session *session)
{
    if (!session) {
        return;
    }
    watch(session);

    const TtCounters c = session->counters();
    m_connected->setText(elapsedText(c.connected_ms));
    m_received->setText(sizeText(c.bytes_in));
    m_sent->setText(sizeText(c.bytes_out));
    m_rateIn->setText(rateText(c.rate_in));
    m_rateOut->setText(rateText(c.rate_out));
    m_lines->setText(QLocale().toString(static_cast<qulonglong>(c.lines_in)));
    m_breaks->setText(QLocale().toString(static_cast<qulonglong>(c.breaks)));
    m_queued->setText(sizeText(session->pendingOut()));

    // The one syscall in here, and the reason the whole widget starts and
    // stops its own timer. `modemLines` answers false without touching
    // anything when the link is not serial, so this costs nothing on an SSH
    // tab beyond the question.
    TtModemLines lines;
    const bool serial = session->modemLines(&lines);
    m_serial->setVisible(serial);
    if (serial) {
        setLamp(m_cts, lines.cts);
        setLamp(m_dsr, lines.dsr);
        setLamp(m_cd, lines.cd);
        setLamp(m_ri, lines.ri);
    }
}

void CountersPopover::watch(const Session *session)
{
    if (session == m_session) {
        return;
    }
    // **The poll outlives nothing.** A `Qt::Popup` grabs the pointer and the
    // keyboard, and it spins no event loop of its own — so the event loop goes
    // on running underneath it, and a macro's `closett` or a control-socket
    // request can delete the page this is showing while it is on screen. The
    // timer would then read a freed `Session` a second later. `closePage`
    // already carries the same rule for `m_pendingSsh`, and states it there.
    if (m_session) {
        disconnect(m_session, &QObject::destroyed, this, nullptr);
    }
    m_session = session;
    connect(session, &QObject::destroyed, this, [this] {
        m_session = nullptr;
        hide();
    });
}

void CountersPopover::setLamp(QLabel *label, bool on)
{
    // Colour and weight rather than a filled or hollow circle. `●`/`○` would
    // turn this into a font-coverage question on a machine whose only font is
    // DejaVu — which is CI's — and a stylesheet is something a test can read
    // back without looking at pixels.
    label->setStyleSheet(on ? QStringLiteral("QLabel { color: #2e7d32; font-weight: bold; }")
                            : QStringLiteral("QLabel { color: palette(mid); }"));
    label->setToolTip(on ? tr("%1 is on.").arg(label->text())
                         : tr("%1 is off.").arg(label->text()));
}

void CountersPopover::popUp(QWidget *anchor, const Session *session)
{
    refresh(session);
    adjustSize();
    if (anchor) {
        // A request, not a placement. Wayland ignores a client's own move, so
        // nothing may depend on where this lands — the contents are the
        // contract.
        const QPoint below = anchor->mapToGlobal(QPoint(0, anchor->height()));
        move(below.x(), below.y() - height() - anchor->height());
    }
    show();
}

void CountersPopover::showEvent(QShowEvent *event)
{
    QFrame::showEvent(event);
    m_poll->start();
}

void CountersPopover::hideEvent(QHideEvent *event)
{
    QFrame::hideEvent(event);
    // Nobody is looking, so nothing asks the port. This is the whole cost
    // argument for reading the control lines live.
    m_poll->stop();
}
