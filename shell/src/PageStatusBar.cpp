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

bool PageStatusBar::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_log && event->type() == QEvent::MouseButtonPress && m_logging) {
        auto *press = static_cast<QMouseEvent *>(event);
        if (press->button() == Qt::LeftButton) {
            emit logClicked();
            return true;
        }
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
