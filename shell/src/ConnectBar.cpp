// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "ConnectBar.h"

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QEvent>
#include <QFileInfo>
#include <QFont>
#include <QHash>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPainter>
#include <QPainterPath>
#include <QPalette>
#include <QPixmap>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QStandardItemModel>
#include <QStringList>
#include <QToolButton>
#include <QWidget>

#include <algorithm>
#include <functional>

#include "I18n.h"
#include "Session.h"

namespace {

/// A dropdown that rebuilds itself as it opens.
///
/// The alternative is a timer, which is what the connect *dialog* uses — a
/// dialog is open for seconds and is the only thing on screen. This bar is open
/// for the life of the window, and a terminal that enumerates `/dev` every
/// second is a terminal that never lets the machine idle. The list is only
/// interesting at the moment somebody opens it, and that is also the moment
/// `~/.ssh/config` is worth re-reading.
class DestinationCombo : public QComboBox {
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

/// A bold, unselectable row: the group captions inside the dropdown.
void markHeader(QComboBox *combo, int at)
{
    auto *model = qobject_cast<QStandardItemModel *>(combo->model());
    QStandardItem *item = model ? model->item(at) : nullptr;
    if (!item) {
        return;
    }
    item->setFlags(item->flags() & ~(Qt::ItemIsEnabled | Qt::ItemIsSelectable));
    QFont font = item->font();
    font.setBold(true);
    item->setFont(font);
}

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
            // The action enters dark mode, so show a moon. The star is not
            // decoration: at 16 px a bare crescent can collapse into a narrow
            // ring, which looks like almost anything except a moon. The two
            // shapes keep the meaning legible at the toolbar's real size.
            QPainterPath moon;
            moon.addEllipse(QRectF(1.5, 1.0, 11.0, 13.5));
            QPainterPath sky;
            sky.addEllipse(QRectF(5.0, -0.5, 10.0, 11.0));
            painter.fillPath(moon.subtracted(sky), colour);

            QPainterPath star;
            star.moveTo(12.5, 0.5);
            star.lineTo(13.1, 2.4);
            star.lineTo(15.0, 3.0);
            star.lineTo(13.1, 3.6);
            star.lineTo(12.5, 5.5);
            star.lineTo(11.9, 3.6);
            star.lineTo(10.0, 3.0);
            star.lineTo(11.9, 2.4);
            star.closeSubpath();
            painter.fillPath(star, colour);
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

    auto *label = new QLabel(tr("Connect to:"), this);
    label->setContentsMargins(4, 0, 4, 0);
    addWidget(label);

    auto *combo = new DestinationCombo(this);
    combo->setObjectName(QStringLiteral("connectBarDestination"));
    combo->setEditable(true);
    combo->setInsertPolicy(QComboBox::NoInsert);
    // A fixed minimum and an expanding policy, never `AdjustToContents`: the
    // widest row in this dropdown is a `by-path` device name, and a combo that
    // sizes to its contents would set the window's minimum width from whatever
    // happens to be plugged in.
    combo->setMinimumWidth(300);
    // An explicit length, so the hint is a constant rather than a function of
    // the widest row *or* of nothing at all. Left at zero this policy answers
    // 29 pixels, and a toolbar deciding which items fit works from hints: a
    // widget whose hint bears no relation to its enforced minimum is one the
    // layout can change its mind about.
    combo->setMinimumContentsLength(24);
    combo->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
    combo->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    combo->lineEdit()->setPlaceholderText(
        tr("host, ssh://user@host, telnet://host:23, /dev/ttyUSB0, shell"));
    combo->onOpen = [this] { rebuildList(); };
    m_destination = combo;
    addWidget(m_destination);

    connect(m_destination, &QComboBox::activated, this, &ConnectBar::chose);
    connect(m_destination->lineEdit(), &QLineEdit::returnPressed, this,
            [this] { commit(); });
    // Typed, not polled: `refresh` runs on the window's status update, which a
    // keystroke in this field is not — so without this the Connect button
    // stays greyed over a destination somebody has just finished typing, on a
    // machine with nothing remembered and nothing plugged in.
    connect(m_destination->lineEdit(), &QLineEdit::textEdited, this, [this] {
        m_chosen = -1;
    });
    connect(m_destination->lineEdit(), &QLineEdit::textChanged, this, [this] {
        if (!m_connect->data().toBool()) {
            m_connect->setEnabled(!destination().isEmpty());
        }
    });

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
            commit();
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

    m_darkMode = addAction(tr("Dark mode"));
    m_darkMode->setObjectName(QStringLiteral("connectBarDarkMode"));
    m_darkMode->setCheckable(true);
    connect(m_darkMode, &QAction::toggled, this, [this](bool on) {
        updateDarkModeAction(on);
        emit darkModeRequested(on);
    });
    // Reserve the wider of Connect and Disconnect, once. Otherwise the button
    // changes width the moment a session opens, the toolbar reflows, and the
    // expanding field beside it absorbs the difference — so connecting makes
    // the destination box visibly resize, which is the sort of thing that
    // reads as a bug in the box rather than in the button.
    if (auto *connectButton =
            qobject_cast<QToolButton *>(widgetForAction(m_connect))) {
        connectButton->setObjectName(QStringLiteral("connectBarConnectButton"));
        m_connect->setText(m_disconnectText.size() > m_connectText.size()
                               ? m_disconnectText
                               : m_connectText);
        const int wide = connectButton->sizeHint().width();
        m_connect->setText(m_connectText);
        connectButton->setMinimumWidth(
            qMax(wide, connectButton->sizeHint().width()));
    }

    auto *darkButton = qobject_cast<QToolButton *>(widgetForAction(m_darkMode));
    if (darkButton) {
        darkButton->setObjectName(QStringLiteral("connectBarDarkModeButton"));
        darkButton->setToolButtonStyle(Qt::ToolButtonIconOnly);
        darkButton->setIconSize(QSize(16, 16));
        darkButton->setAutoRaise(true);
    }
    updateDarkModeAction(false);

    rebuildList();
}

QString ConnectBar::destination() const
{
    return m_destination->currentText().trimmed();
}

void ConnectBar::setDestination(const QString &text)
{
    QSignalBlocker block(m_destination);
    m_destination->setCurrentText(text);
}

void ConnectBar::showConnection(const RecentConnection &recent)
{
    QHash<QString, QString> deviceFor;
    // Only a serial record has anything to look up, and this runs on the
    // connect path: enumerating `/dev` to render the words "Local shell" is
    // work in the way of the thing somebody actually asked for.
    if (recent.kind != RecentConnection::Kind::Serial) {
        setDestination(recent.label());
        return;
    }
    if (TtPortList *list = tt_serial_enumerate()) {
        for (size_t i = 0; i < tt_port_list_len(list); i++) {
            if (const TtPortInfo *info = tt_port_list_at(list, i)) {
                deviceFor.insert(QString::fromUtf8(info->open_path),
                                 QString::fromUtf8(info->device));
            }
        }
        tt_port_list_free(list);
    }
    setDestination(recent.label(deviceFor));
}

void ConnectBar::setRecents(const QVector<RecentConnection> &recents)
{
    m_recents = recents;
    rebuildList();
}

QVector<ConnectBar::Entry> ConnectBar::composeList() const
{
    QVector<Entry> rows;
    const auto row = [&rows](Row kind, const QString &text,
                             const QString &payload = QString()) {
        rows.append({kind, text, payload});
    };

    QHash<QString, QString> deviceFor;
    QVector<QPair<QString, QString>> ports; // label, open_path
    if (TtPortList *list = tt_serial_enumerate()) {
        for (size_t i = 0; i < tt_port_list_len(list); i++) {
            const TtPortInfo *info = tt_port_list_at(list, i);
            if (!info) {
                continue;
            }
            deviceFor.insert(QString::fromUtf8(info->open_path),
                             QString::fromUtf8(info->device));
            ports.append({QString::fromUtf8(info->label),
                          QString::fromUtf8(info->open_path)});
        }
        tt_port_list_free(list);
    }

    if (!m_recents.isEmpty()) {
        row(Row::Header, tr("Recent"));
        for (int i = 0; i < m_recents.size(); i++) {
            row(Row::Recent, m_recents.at(i).label(deviceFor),
                QString::number(i));
        }
        row(Row::Separator, QString());
    }

    if (!ports.isEmpty()) {
        row(Row::Header, tr("Ports plugged in now"));
        // Enumeration is not a shortlist. A desktop answers with its
        // thirty-two motherboard `ttyS` UARTs, none of which has anything on
        // the far end, and burying the one adapter somebody owns in the middle
        // of them is what the dropdown this replaced did. Real adapters first,
        // then a bounded tail, and the dialog for the rest.
        std::stable_partition(ports.begin(), ports.end(),
                              [](const QPair<QString, QString> &port) {
                                  return !port.second.contains(
                                      QLatin1String("/ttyS"));
                              });
        constexpr int kShown = 6;
        for (int i = 0; i < ports.size() && i < kShown; i++) {
            row(Row::Port, ports.at(i).first, ports.at(i).second);
        }
        if (ports.size() > kShown) {
            row(Row::Header, tr("...and %1 more, in New connection")
                                 .arg(ports.size() - kShown));
        }
        row(Row::Separator, QString());
    }

    QStringList aliases;
    if (TtStringList *list = tt_ssh_config_aliases()) {
        for (size_t i = 0; i < tt_string_list_len(list); i++) {
            aliases.append(QString::fromUtf8(tt_string_list_at(list, i)));
        }
        tt_string_list_free(list);
    }
    if (!aliases.isEmpty()) {
        row(Row::Header, tr("SSH hosts (~/.ssh/config)"));
        for (const QString &alias : aliases) {
            row(Row::Alias, alias, alias);
        }
        row(Row::Separator, QString());
    }

    row(Row::Shell, tr("Local shell"));
    row(Row::Separator, QString());
    row(Row::New, tr("New connection..."));
    if (!m_recents.isEmpty()) {
        row(Row::Forget, tr("Forget these connections"));
    }
    return rows;
}

void ConnectBar::rebuildList()
{
    const QVector<Entry> rows = composeList();
    // Nothing has been plugged in, unplugged or connected to since the last
    // time: leave the model alone. Touching it costs a geometry invalidation
    // on a widget the toolbar is about to open a popup over, and the field
    // moves under the pointer.
    if (rows == m_rows) {
        return;
    }
    m_rows = rows;

    // The field is the user's, not the list's: rebuilding must not retype it,
    // and a combo assigns a current index as it fills.
    const QString typed = m_destination->currentText();
    m_filling = true;
    QSignalBlocker block(m_destination);
    m_destination->clear();

    for (const Entry &entry : rows) {
        if (entry.kind == Row::Separator) {
            m_destination->insertSeparator(m_destination->count());
            continue;
        }
        m_destination->addItem(entry.text);
        const int at = m_destination->count() - 1;
        m_destination->setItemData(at, static_cast<int>(entry.kind), RoleKind);
        m_destination->setItemData(at, entry.payload, RolePayload);
        if (entry.kind == Row::Header) {
            markHeader(m_destination, at);
        }
    }

    m_destination->setCurrentIndex(-1);
    m_destination->setCurrentText(typed);
    m_filling = false;
}

void ConnectBar::commit()
{
    if (m_chosen >= 0 && m_chosen < m_recents.size()) {
        emit recentChosen(m_recents.at(m_chosen));
        return;
    }
    if (!destination().isEmpty()) {
        emit destinationEntered(destination());
    }
}

void ConnectBar::chose(int index)
{
    if (m_filling || index < 0) {
        return;
    }
    const auto kind = static_cast<Row>(
        m_destination->itemData(index, RoleKind).toInt());
    const QString payload =
        m_destination->itemData(index, RolePayload).toString();

    // **The last row chosen is the only one that counts.** An `activated` that
    // nobody meant is a fact of this widget — the popup opens under the
    // pointer, so the release that opened it lands on a row — and the answer
    // to that is to fill the field rather than connect, below. But a *record*
    // survives being replaced by whatever the user picks next, unlike text,
    // and the one they picked by accident was still the one Connect opened:
    // choose a recent shell by opening the dropdown, choose `myrouter` on
    // purpose, press Connect, and a local shell opens. So every other row
    // clears it, and only [`textEdited`] is left to clear the rest.
    m_chosen = -1;

    // **Choosing a row fills the field; Connect is what connects.** A
    // connection is not something to start by accident, and it means the
    // destination can be read before it is committed.
    switch (kind) {
    case Row::Recent: {
        const int at = payload.toInt();
        if (at >= 0 && at < m_recents.size()) {
            setDestination(m_destination->itemText(index));
            m_chosen = at;
        }
        return;
    }
    case Row::Port:
        // The device path, not the row's label: the label carries the USB
        // product name so somebody can tell two adapters apart, and it is not
        // a name anything can be opened by.
        setDestination(payload);
        return;
    case Row::Alias:
        setDestination(payload);
        return;
    case Row::Shell:
        setDestination(QStringLiteral("shell"));
        return;
    case Row::New:
        setDestination(QString());
        emit newConnectionRequested();
        return;
    case Row::Forget:
        setDestination(QString());
        emit forgetRecentsRequested();
        return;
    case Row::Header:
    case Row::Separator:
        break;
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
    m_connect->setEnabled(live || !destination().isEmpty());
    // The field stays live under a live session, unlike the port list it
    // replaced: `ensureIdlePage` gives a second destination its own page, so
    // going somewhere else is opening a tab and never closing this one.

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
