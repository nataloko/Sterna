// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "ConnectBar.h"

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QEvent>
#include <QIcon>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPalette>
#include <QPixmap>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QToolButton>
#include <QWidget>

#include <functional>

#include "I18n.h"
#include "Session.h"

namespace {

/// A port dropdown that re-enumerates as it opens.
///
/// The alternative is a timer, which is what the connect *dialog* uses — a
/// dialog is open for seconds and is the only thing on screen. This bar is open
/// for the life of the window, and a terminal that enumerates `/dev` every
/// second is a terminal that never lets the machine idle. The list is only
/// interesting at the moment somebody opens it.
class PortCombo : public QComboBox {
public:
    using QComboBox::QComboBox;

    /// A callback rather than a signal: `Q_OBJECT` on a class inside a `.cpp`
    /// wants the moc output included by hand, and this has one caller.
    std::function<void()> onOpen;

    void showPopup() override
    {
        if (onOpen) {
            onOpen();
        }
        QComboBox::showPopup();
    }
};

QIcon appearanceIcon(bool darkMode, const QColor &colour)
{
    QIcon icon;
    for (int scale : {1, 2}) {
        QPixmap pixmap(16 * scale, 16 * scale);
        pixmap.fill(Qt::transparent);

        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.scale(scale, scale);

        if (!darkMode) {
            // The action enters dark mode, so show a moon.
            QPainterPath moon;
            moon.setFillRule(Qt::OddEvenFill);
            moon.addEllipse(QRectF(2.0, 1.5, 11.5, 13.0));
            moon.addEllipse(QRectF(6.0, 0.5, 9.0, 11.0));
            painter.fillPath(moon, colour);
        } else {
            // In dark mode the same action returns to the light theme.
            QPen pen(colour, 1.4, Qt::SolidLine, Qt::RoundCap,
                     Qt::RoundJoin);
            painter.setPen(pen);
            painter.setBrush(Qt::NoBrush);
            painter.drawEllipse(QRectF(5.0, 5.0, 6.0, 6.0));
            const QLineF rays[] = {
                {8.0, 1.0, 8.0, 3.0},   {8.0, 13.0, 8.0, 15.0},
                {1.0, 8.0, 3.0, 8.0},   {13.0, 8.0, 15.0, 8.0},
                {3.0, 3.0, 4.4, 4.4},   {11.6, 11.6, 13.0, 13.0},
                {3.0, 13.0, 4.4, 11.6}, {11.6, 4.4, 13.0, 3.0},
            };
            painter.drawLines(rays, 8);
        }
        painter.end();
        pixmap.setDevicePixelRatio(scale);
        icon.addPixmap(pixmap);
    }
    return icon;
}

} // namespace

ConnectBar::ConnectBar(const I18n *i18n, QWidget *parent) : QToolBar(parent)
{
    const auto plain = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->plainText(key, fallback) : fallback;
    };

    setObjectName(QStringLiteral("connectBar"));
    setMovable(false);
    setFloatable(false);
    // Text, because there is no icon theme this program ships and a themed icon
    // for "local echo" does not exist anywhere.
    setToolButtonStyle(Qt::ToolButtonTextOnly);

    auto *label = new QLabel(plain("DLG_SERIAL_PORT", tr("Port:")), this);
    label->setContentsMargins(4, 0, 4, 0);
    addWidget(label);

    auto *combo = new PortCombo(this);
    combo->setObjectName(QStringLiteral("connectBarPort"));
    combo->setMinimumWidth(240);
    combo->setSizeAdjustPolicy(QComboBox::AdjustToContentsOnFirstShow);
    combo->onOpen = [this] { refreshPorts(); };
    m_port = combo;
    addWidget(m_port);

    m_connectText = tr("Connect");
    m_disconnectText = plain("MENU_FILE_DISCONNECT", tr("Disconnect"));
    m_connect = addAction(m_connectText);
    m_connect->setObjectName(QStringLiteral("connectBarConnect"));
    connect(m_connect, &QAction::triggered, this, [this] {
        // Which of the two this is comes from the session, not from the button:
        // the window refreshes the text, and a stale label must not open a port
        // somebody asked to close.
        if (m_connect->data().toBool()) {
            emit disconnectRequested();
        } else {
            emit connectRequested(portPath());
        }
    });

    addSeparator();
    m_echo = new QCheckBox(plain("DLG_TERM_LOCALECHO", tr("Local echo")), this);
    m_echo->setObjectName(QStringLiteral("connectBarLocalEcho"));
    m_echo->setContentsMargins(4, 0, 4, 0);
    m_echo->setToolTip(tr("Shows your keystrokes locally. Turn this on when the "
                          "connected device does not echo what you type; leave "
                          "it off if characters appear twice."));
    connect(m_echo, &QCheckBox::toggled, this, &ConnectBar::localEchoRequested);
    addWidget(m_echo);

    m_lineEdit = new QCheckBox(tr("Line edit"), this);
    m_lineEdit->setObjectName(QStringLiteral("connectBarLineEdit"));
    m_lineEdit->setContentsMargins(4, 0, 4, 0);
    m_lineEdit->setToolTip(
        tr("Keeps ordinary text local and editable at the terminal cursor "
           "until Enter sends the line."));
    connect(m_lineEdit, &QCheckBox::toggled, this,
            &ConnectBar::lineEditRequested);
    addWidget(m_lineEdit);

    auto *spacer = new QWidget(this);
    spacer->setObjectName(QStringLiteral("connectBarDarkModeSpacer"));
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    addWidget(spacer);

    m_darkMode = addAction(tr("Dark mode"));
    m_darkMode->setObjectName(QStringLiteral("connectBarDarkMode"));
    m_darkMode->setCheckable(true);
    connect(m_darkMode, &QAction::toggled, this, [this](bool on) {
        updateDarkModeAction(on);
        emit darkModeRequested(on);
    });
    auto *darkButton = qobject_cast<QToolButton *>(widgetForAction(m_darkMode));
    if (darkButton) {
        darkButton->setObjectName(QStringLiteral("connectBarDarkModeButton"));
        darkButton->setToolButtonStyle(Qt::ToolButtonIconOnly);
        darkButton->setIconSize(QSize(16, 16));
        darkButton->setAutoRaise(true);
    }
    updateDarkModeAction(false);

    refreshPorts();
}

void ConnectBar::refreshPorts()
{
    TtPortList *list = tt_serial_enumerate();
    if (!list) {
        return;
    }

    // Rebuilt only when the set really changed, so a dropdown opened over a
    // list nothing has unplugged keeps its selection.
    const size_t n = tt_port_list_len(list);
    bool same = static_cast<size_t>(m_port->count()) == n;
    for (size_t i = 0; same && i < n; i++) {
        const TtPortInfo *info = tt_port_list_at(list, i);
        same = info
            && m_port->itemData(static_cast<int>(i)).toString()
                == QString::fromUtf8(info->open_path);
    }
    if (same) {
        tt_port_list_free(list);
        return;
    }

    const QString keep = portPath();
    m_port->clear();
    for (size_t i = 0; i < n; i++) {
        const TtPortInfo *info = tt_port_list_at(list, i);
        if (!info) {
            continue;
        }
        m_port->addItem(QString::fromUtf8(info->label),
                        QString::fromUtf8(info->open_path));
    }
    tt_port_list_free(list);

    const int back = m_port->findData(keep);
    if (back >= 0) {
        m_port->setCurrentIndex(back);
    }
}

QString ConnectBar::portPath() const
{
    return m_port->currentData().toString();
}

void ConnectBar::setPortPath(const QString &path)
{
    const int at = m_port->findData(path);
    if (at >= 0) {
        m_port->setCurrentIndex(at);
    }
}

void ConnectBar::refresh(const Session *session)
{
    if (!session) {
        return;
    }
    const bool connected = session->isConnected();
    const bool connecting = session->isConnecting();
    const bool live = connected || connecting;

    // Enabled while connecting, like File > Disconnect: abandoning a handshake
    // that is waiting on a slow key exchange is a thing people need.
    m_connect->setData(live);
    m_connect->setText(live ? m_disconnectText : m_connectText);
    m_connect->setEnabled(live || !portPath().isEmpty());
    // The port cannot move under a live session, and the one shown is the one
    // that is open.
    m_port->setEnabled(!live);

    // Local echo is not only ours to set: the host assigns it through SRM, and
    // a script can. So it is read back rather than remembered.
    const bool lineEdit =
        session->setting(QStringLiteral("terminal.line_edit")) == QLatin1String("on");
    const bool preferredEcho =
        session->setting(QStringLiteral("terminal.local_echo")) == QLatin1String("on");
    const bool effectiveEcho = lineEdit || preferredEcho;
    if (m_echo->isChecked() != effectiveEcho) {
        // Blocked, or refreshing the view would write the setting back.
        QSignalBlocker block(m_echo);
        m_echo->setChecked(effectiveEcho);
    }
    // Neither control can act on a session that has no far end. Keep the
    // check visible — it is still the saved/live preference and will take
    // effect when a connection opens — but grey it until then. Connecting is
    // not enough: a host-key or authentication prompt is not a terminal yet.
    m_echo->setEnabled(connected && !lineEdit);
    if (m_lineEdit->isChecked() != lineEdit) {
        QSignalBlocker block(m_lineEdit);
        m_lineEdit->setChecked(lineEdit);
    }
    m_lineEdit->setEnabled(connected);

    const bool darkMode =
        session->setting(QStringLiteral("terminal.dark_mode")) == QLatin1String("on");
    if (m_darkMode->isChecked() != darkMode) {
        QSignalBlocker block(m_darkMode);
        m_darkMode->setChecked(darkMode);
    }
    updateDarkModeAction(darkMode);
}

void ConnectBar::changeEvent(QEvent *event)
{
    QToolBar::changeEvent(event);
    if (event->type() == QEvent::PaletteChange && m_darkMode) {
        updateDarkModeAction(m_darkMode->isChecked());
    }
}

void ConnectBar::updateDarkModeAction(bool darkMode)
{
    m_darkMode->setIcon(
        appearanceIcon(darkMode, palette().color(QPalette::ButtonText)));
    m_darkMode->setToolTip(
        darkMode
            ? tr("Use the light palette for terminal views. Menus and dialogs "
                 "continue to use the desktop theme.")
            : tr("Use the dark palette for terminal views. Menus and dialogs "
                 "continue to use the desktop theme."));
}
