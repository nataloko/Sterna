// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "PageStatusBar.h"

#include <QFontMetrics>
#include <QHBoxLayout>
#include <QLabel>
#include <QLocale>
#include <QMouseEvent>
#include <QResizeEvent>
#include <QTimer>

namespace {
/// How much of the strip the link state may claim before it is elided. The
/// number matters because `describe()` can be a whole serial device path —
/// `/dev/serial/by-path/pci-0000:c8:00.3-usb-0:1.3.2:1.0-port0 115200` is a
/// real one — and a label that asks for its own text is a label that widens
/// the terminal above it.
constexpr int kConnectionChars = 34;
/// A full red/blank cycle is long enough to catch the eye without turning the
/// status strip into a strobe.
constexpr int kLogBlinkMs = 600;

/// A rate, in as few characters as it can be said in.
///
/// Deliberately not `QLocale::formattedDataSize` with a `/s` on the end: that
/// spells a kilobyte `1.0 kB` and a mebibyte `1.0 MiB`, and four of those plus
/// a clock is a third of a tiled quarter-window's status strip. What the field
/// is for is whether the number is *moving*, so one significant decimal and a
/// single-letter multiplier says everything it has to.
QString shortRate(quint64 bytesPerSecond)
{
    static const char *const suffix[] = {"", "k", "M", "G", "T"};
    double n = static_cast<double>(bytesPerSecond);
    size_t i = 0;
    while (n >= 1000.0 && i + 1 < sizeof suffix / sizeof *suffix) {
        n /= 1000.0;
        i++;
    }
    // No decimal below a thousand: `44` is a byte count, not `44.0`.
    const int digits = (i == 0 || n >= 100.0) ? 0 : 1;
    return QString::number(n, 'f', digits) + QLatin1String(suffix[i]);
}

/// `H:MM:SS`, and hours that keep counting rather than wrapping at a day: a
/// serial console left open over a weekend is the case, not the exception.
QString elapsed(qint64 ms)
{
    const qint64 total = ms / 1000;
    return QStringLiteral("%1:%2:%3")
        .arg(total / 3600)
        .arg((total / 60) % 60, 2, 10, QLatin1Char('0'))
        .arg(total % 60, 2, 10, QLatin1Char('0'));
}

/// What the counter field says. One composer, used for the live reading and
/// for the width template, so the reservation cannot disagree with the text.
QString counterText(qint64 connectedMs, quint64 rateIn, quint64 rateOut)
{
    return QStringLiteral("%1 ↓%2 ↑%3")
        .arg(elapsed(connectedMs < 0 ? 0 : connectedMs), shortRate(rateIn),
             shortRate(rateOut));
}
} // namespace

PageStatusBar::PageStatusBar(QWidget *parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("pageStatusBar"));
    // Painted rather than transparent: the highlight is the active-pane marker,
    // and a strip that only sometimes had a background would change height when
    // it took one.
    setAutoFillBackground(true);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);

    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(6, 1, 6, 1);
    layout->setSpacing(12);

    m_name = new QLabel(this);
    m_name->setObjectName(QStringLiteral("statusName"));
    // `Ignored` rather than `Preferred`: the name is host-supplied and
    // unbounded, and a label that quotes its text as its width would push the
    // page's size hint out — the window would grow at the moment a session
    // connected, which reads as the terminal-size guard misfiring and gets
    // hunted nowhere near here. It takes the space that is left and elides.
    m_name->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    m_name->setMinimumWidth(0);
    layout->addWidget(m_name, 1);

    m_log = new QLabel(this);
    m_log->setObjectName(QStringLiteral("statusLog"));
    m_log->installEventFilter(this);
    layout->addWidget(m_log);

    m_counters = new QLabel(this);
    m_counters->setObjectName(QStringLiteral("statusCounters"));
    m_counters->installEventFilter(this);
    // Hidden rather than empty until somebody says otherwise. A `QBoxLayout`
    // skips a hidden item entirely — no width, and no 12 px of spacing beside
    // it — so a window with the setting off pays nothing at all for this.
    m_counters->hide();
    layout->addWidget(m_counters);

    m_connection = new QLabel(this);
    // The window's single label had this name. Keeping it means a test that
    // asks a one-terminal window for its connection state still finds it.
    m_connection->setObjectName(QStringLiteral("connectionStatus"));
    layout->addWidget(m_connection);

    m_messageTimer = new QTimer(this);
    m_messageTimer->setSingleShot(true);
    connect(m_messageTimer, &QTimer::timeout, this, [this] {
        m_message.clear();
        showName();
    });

    m_logBlinkTimer = new QTimer(this);
    m_logBlinkTimer->setObjectName(QStringLiteral("statusLogBlinkTimer"));
    m_logBlinkTimer->setInterval(kLogBlinkMs);
    connect(m_logBlinkTimer, &QTimer::timeout, this, [this] {
        m_logBlinkOn = !m_logBlinkOn;
        applyLogAppearance();
    });

    setConnection(false, false, QString());
    reserveCounterWidth();
    applyPalette();
}

QSize PageStatusBar::sizeHint() const
{
    // The style's font arrives as a `changeEvent` on first show — after a
    // window has already asked how tall a terminal wants to be — so measure
    // polished, the way `QComboBox` does.
    const_cast<PageStatusBar *>(this)->ensurePolished();
    // Height only. The width is deliberately not the layout's: see the name
    // label's size policy. The terminal above decides how wide the page is.
    return QSize(0, QWidget::sizeHint().height());
}

QSize PageStatusBar::minimumSizeHint() const
{
    const_cast<PageStatusBar *>(this)->ensurePolished();
    return QSize(0, QWidget::minimumSizeHint().height());
}

void PageStatusBar::setName(const QString &name)
{
    if (name == m_nameText) {
        return;
    }
    m_nameText = name;
    if (m_message.isEmpty()) {
        showName();
    }
}

void PageStatusBar::setConnection(bool connected, bool connecting,
                                  const QString &text)
{
    m_connectionText = connected    ? text
                       : connecting ? tr("connecting...")
                                    : tr("not connected");
    m_connection->setToolTip(m_connectionText);
    m_connection->setText(fontMetrics().elidedText(
        m_connectionText, Qt::ElideMiddle,
        fontMetrics().averageCharWidth() * kConnectionChars));
    showName();

    const bool down = !connected && !connecting;
    if (down != m_linkDown) {
        m_linkDown = down;
        applyPalette();
    }
}

void PageStatusBar::setLogging(bool logging, quint64 bytes, bool paused)
{
    // `formattedDataSize` rather than a KiB division, so a log that has only
    // just started reads "REC 44 bytes" instead of "REC 0 KiB" — the number
    // anyone actually checks is whether it is *moving*. Which is also why the
    // paused state says so in the word rather than only in the colour: a
    // number that has stopped looks exactly like an idle line.
    const QString size = QLocale().formattedDataSize(static_cast<qint64>(bytes));
    const QString text = !logging  ? QString()
                         : paused  ? tr("PAUSED %1").arg(size)
                                   : tr("REC %1").arg(size);
    // Compared before it is assigned. This is reached from `Session::damaged`,
    // which fires on every read on every open session, and `QLabel::setText`
    // is a relayout — so an unchanged size must cost a string compare and
    // nothing else.
    if (text != m_log->text()) {
        m_log->setText(text);
    }
    // **Both halves of the state, not just `logging`.** The early return here
    // is what keeps a per-read call cheap, and a pause that was not part of
    // the comparison would never repaint — the same shape as
    // `ConnectBar::Entry::operator==`.
    if (logging == m_logging && paused == m_logPaused) {
        return;
    }
    m_logging = logging;
    m_logPaused = paused;
    // These depend on state, not on the byte count. Keep them behind the same
    // early return as the timer and style: `setLogging` runs for every read of
    // every recording session, and resetting widget properties there turns a
    // cheap size-label update into repeated event and palette work.
    m_log->setCursor(logging ? Qt::PointingHandCursor : Qt::ArrowCursor);
    m_log->setToolTip(logging ? (paused ? tr("This button continues the session log.")
                                        : tr("This button pauses the session log."))
                              : QString());
    m_logBlinkOn = logging;
    if (logging && !paused) {
        m_logBlinkTimer->start();
    } else {
        m_logBlinkTimer->stop();
    }
    applyLogAppearance();
}

void PageStatusBar::setCounters(bool on, qint64 connectedMs, quint64 rateIn,
                                quint64 rateOut, bool live)
{
    if (!on) {
        if (m_countersOn) {
            m_countersOn = false;
            m_counters->hide();
        }
        return;
    }

    const QString text = counterText(connectedMs, rateIn, rateOut);
    // Compared before it is assigned, for `setLogging`'s reason one function
    // up: this is reached from `Session::damaged`, which fires on every read
    // of every open session, and `QLabel::setText` is a relayout.
    if (text != m_counters->text()) {
        m_counters->setText(text);
    }
    // **The whole state, and `live` is part of it.** A connection that ended
    // keeps its totals, so nothing in the digits says the clock has stopped —
    // leave `live` out of this comparison and a disconnected tab wears the
    // connected styling for ever. Same shape as `setLogging` above and as
    // `ConnectBar::Entry::operator==`.
    if (on == m_countersOn && live == m_countersLive) {
        return;
    }
    m_countersOn = on;
    m_countersLive = live;
    m_counters->show();
    m_counters->setCursor(Qt::PointingHandCursor);
    m_counters->setToolTip(tr("This field gives the connection time and the "
                              "data rates. Click the field for more counts."));
    // Dimmed rather than hidden when the line has gone: the numbers are still
    // true of the connection that ended, and that is when somebody reads them.
    m_counters->setStyleSheet(live ? QString()
                                   : QStringLiteral("QLabel { color: palette(mid); }"));
}

bool PageStatusBar::countersVisible() const { return m_countersOn; }

void PageStatusBar::reserveCounterWidth()
{
    // The widest reading the field is expected to hold: a hundred hours, and
    // both rates at their longest. Run through the same composer as the live
    // text, because how wide `1.2M` renders is a question about the font and
    // the locale, not one a literal can answer.
    const QString widest = counterText(100 * 3600 * 1000LL, 999'000'000, 999'000'000);
    // A floor and not a fixed width. Fixed would hold the layout still and
    // then *clip* anything longer — and Qt clips a label from the far end, so
    // a connection open for longer than the reservation allows would lose a
    // digit off its hour and read `0:44:00` for `100:44:00`. That is the
    // `LineNumberGutter` failure exactly: a wrong number on screen with
    // nothing saying so. A floor holds the width still for every reading that
    // fits — which is all of them, for four days — and lets the one that does
    // not make the field wider instead of lying.
    m_counters->setMinimumWidth(fontMetrics().horizontalAdvance(widest));
}

bool PageStatusBar::eventFilter(QObject *watched, QEvent *event)
{
    if (event->type() != QEvent::MouseButtonPress) {
        return QWidget::eventFilter(watched, event);
    }
    auto *press = static_cast<QMouseEvent *>(event);
    if (press->button() != Qt::LeftButton) {
        return QWidget::eventFilter(watched, event);
    }
    if (watched == m_log && m_logging) {
        emit logClicked();
        return true;
    }
    if (watched == m_counters && m_countersOn) {
        emit countersClicked();
        return true;
    }
    return QWidget::eventFilter(watched, event);
}

void PageStatusBar::showMessage(const QString &text, int ms)
{
    m_message = text;
    elideInto(m_name, text);
    if (ms > 0) {
        m_messageTimer->start(ms);
    } else {
        m_messageTimer->stop();
    }
}

void PageStatusBar::clearMessage(const QString &text)
{
    if (m_message != text) {
        return;
    }
    m_message.clear();
    m_messageTimer->stop();
    showName();
}

QString PageStatusBar::currentMessage() const { return m_message; }

void PageStatusBar::setActive(bool active)
{
    if (active == m_active) {
        return;
    }
    m_active = active;
    applyPalette();
}

void PageStatusBar::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    showName();
}

void PageStatusBar::changeEvent(QEvent *event)
{
    QWidget::changeEvent(event);
    // The style's font reaches a widget here, on first show — after the
    // constructor measured whatever the default was. A reservation made
    // against the wrong font is the `LineNumberGutter` failure in a new place:
    // too narrow, and Qt clips the *start* of the field, which is the hours.
    if (event->type() == QEvent::FontChange) {
        reserveCounterWidth();
    }
}

void PageStatusBar::showName()
{
    if (!m_message.isEmpty()) {
        elideInto(m_name, m_message);
        return;
    }
    // A local shell's name and its link description are the same string —
    // `connectionHost()` is empty for a pty, so the label falls back to
    // `describe()`, which is what the right-hand field already says. Print it
    // once. SSH and serial name themselves differently from their description
    // and keep both halves.
    elideInto(m_name, m_nameText == m_connectionText ? QString() : m_nameText);
}

void PageStatusBar::elideInto(QLabel *label, const QString &text)
{
    label->setToolTip(text);
    const int room = label->width();
    label->setText(room > 0 ? fontMetrics().elidedText(text, Qt::ElideRight,
                                                       room)
                            : text);
}

void PageStatusBar::applyPalette()
{
    QPalette palette = QWidget::palette();
    palette.setColor(QPalette::Window,
                     palette.color(m_active ? QPalette::Highlight
                                            : QPalette::AlternateBase));
    palette.setColor(QPalette::WindowText,
                     palette.color(m_active ? QPalette::HighlightedText
                                            : QPalette::Text));
    setPalette(palette);

    // The disconnected chip, which the window's status bar also painted red.
    // A stylesheet rather than a palette because it has to win over the
    // highlight the strip around it may be wearing.
    m_connection->setStyleSheet(
        m_linkDown ? QStringLiteral("QLabel { background-color: #b71c1c; "
                                    "color: white; padding: 1px 6px; }")
                   : QString());
}

void PageStatusBar::applyLogAppearance()
{
    if (!m_logging) {
        m_log->setStyleSheet(QString());
        return;
    }
    if (m_logPaused) {
        // Steady, and a colour that is neither the running red nor the
        // ordinary text: a paused recording is still a recording somebody left
        // open, and it must not read as "nothing is happening".
        m_log->setStyleSheet(QStringLiteral("QLabel { color: #f9a825; font-weight: bold; }"));
        return;
    }
    // Bold in both phases so the label never changes width while it blinks.
    // Transparent rather than hidden for the same reason: the connection chip
    // and the terminal above it must not shift twice a second.
    m_log->setStyleSheet(
        m_logBlinkOn
            ? QStringLiteral("QLabel { color: #d32f2f; font-weight: bold; }")
            : QStringLiteral("QLabel { color: transparent; font-weight: bold; }"));
}
