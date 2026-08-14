// The connect bar: what a destination means, and what the list remembers.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/connect_test
//
// Needs nothing. The record encoding, the list's ordering and the destination
// vocabulary are all pure functions, and the one case that connects opens a
// local shell — three of the four transports would need hardware, a server or
// a name that resolves, which is exactly why `MainWindow::parseDestination` is
// a function that can be asked rather than only a switch that acts.

#include <QApplication>
#include <QGuiApplication>
#include <QAbstractItemView>
#include <QAction>
#include <QComboBox>
#include <QFile>
#include <QLineEdit>
#include <QToolButton>
#include <QMainWindow>
#include <QStandardPaths>

#include <cstdio>

#ifndef Q_OS_WIN
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

#include "ConnectBar.h"
#include "MainWindow.h"
#include "Recent.h"
#include "Session.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

RecentConnection decoded(const QString &text)
{
    RecentConnection out;
    CHECK(RecentConnection::decode(text, &out));
    return out;
}

int rowWithText(const QComboBox *combo, const QString &text)
{
    for (int i = 0; i < combo->count(); i++) {
        if (combo->itemText(i) == text) {
            return i;
        }
    }
    return -1;
}

#ifndef Q_OS_WIN
/// A listening localhost socket. The kernel completes a TCP connection from
/// its backlog, so this needs no thread and no accept call.
class Listener {
public:
    Listener()
    {
        m_fd = ::socket(AF_INET, SOCK_STREAM, 0);
        if (m_fd < 0) {
            return;
        }
        int on = 1;
        ::setsockopt(m_fd, SOL_SOCKET, SO_REUSEADDR, &on, sizeof on);
        sockaddr_in addr = {};
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        socklen_t len = sizeof addr;
        if (::bind(m_fd, reinterpret_cast<sockaddr *>(&addr), len) != 0
            || ::listen(m_fd, 1) != 0
            || ::getsockname(m_fd, reinterpret_cast<sockaddr *>(&addr), &len)
                   != 0) {
            ::close(m_fd);
            m_fd = -1;
            return;
        }
        m_port = ntohs(addr.sin_port);
    }

    ~Listener()
    {
        if (m_fd >= 0) {
            ::close(m_fd);
        }
    }

    Listener(const Listener &) = delete;
    Listener &operator=(const Listener &) = delete;

    quint16 port() const { return m_port; }

private:
    int m_fd = -1;
    quint16 m_port = 0;
};
#endif

} // namespace

/// Every kind survives the settings file, including the two characters that
/// mean something to the format and a `by-path` name full of colons.
void test_a_record_survives_the_settings_file()
{
    TtSerialParams line;
    tt_serial_params_default(&line);
    line.baud = 115200;
    line.data_bits = 7;
    line.parity = TT_PARITY_EVEN;
    line.stop_bits = 2;
    line.flow = TT_FLOW_CONTROL_RTS_CTS;

    const QString path =
        QStringLiteral("/dev/serial/by-path/pci-0000:c6:00.3-usb-0:4.2:1.0");
    const RecentConnection serial = RecentConnection::serial(path, line);
    const RecentConnection back = decoded(serial.encode());
    CHECK(back.kind == RecentConnection::Kind::Serial);
    CHECK(back.path == path);
    CHECK(back.baud == 115200);
    CHECK(back.bits == 7);
    CHECK(back.parity == TT_PARITY_EVEN);
    CHECK(back.stop == 2);
    CHECK(back.flow == TT_FLOW_CONTROL_RTS_CTS);

    const RecentConnection ssh = RecentConnection::ssh(
        QStringLiteral("buildbox"), QStringLiteral("alice"), 2222,
        QStringLiteral("/home/alice/.ssh/id_ed25519"), true);
    const RecentConnection sshBack = decoded(ssh.encode());
    CHECK(sshBack.kind == RecentConnection::Kind::Ssh);
    CHECK(sshBack.host == QStringLiteral("buildbox"));
    CHECK(sshBack.user == QStringLiteral("alice"));
    CHECK(sshBack.port == 2222);
    CHECK(sshBack.identity == QStringLiteral("/home/alice/.ssh/id_ed25519"));
    CHECK(sshBack.legacy);

    // An empty user and a zero port are absences, not values: they mean
    // `~/.ssh/config` decides, and the record has to keep that distinction
    // because the connect call does.
    const RecentConnection bare =
        decoded(RecentConnection::ssh(QStringLiteral("myrouter"), QString(), 0,
                                      QString(), false)
                    .encode());
    CHECK(bare.user.isEmpty());
    CHECK(bare.port == 0);
    CHECK(!bare.legacy);

    const RecentConnection telnet = decoded(
        RecentConnection::telnet(QStringLiteral("10.0.0.5"), 2323,
                                 TT_TELNET_RAW)
            .encode());
    CHECK(telnet.kind == RecentConnection::Kind::Telnet);
    CHECK(telnet.host == QStringLiteral("10.0.0.5"));
    CHECK(telnet.port == 2323);
    CHECK(telnet.mode == TT_TELNET_RAW);

    CHECK(RecentConnection::shell().encode() == QStringLiteral("shell:"));
    CHECK(decoded(QStringLiteral("shell:")).kind
          == RecentConnection::Kind::Shell);

    // The delimiters, escaped and read back. A user name is the one field
    // somebody can put an `@` in.
    const RecentConnection odd = decoded(
        RecentConnection::ssh(QStringLiteral("host"),
                              QStringLiteral("a;b?c&d=e@f"), 0, QString(),
                              false)
            .encode());
    CHECK(odd.user == QStringLiteral("a;b?c&d=e@f"));
    CHECK(odd.host == QStringLiteral("host"));
}

/// A hand-edited file is not a trusted one: a record that does not parse is
/// dropped and the rest of the line survives it.
void test_a_broken_record_is_dropped_and_not_repaired()
{
    RecentConnection out;
    CHECK(!RecentConnection::decode(QStringLiteral("serial:"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("serial:/dev/x"), &out));
    CHECK(!RecentConnection::decode(
        QStringLiteral("serial:/dev/x?baud=0&bits=8&parity=none&stop=1&flow=none"),
        &out));
    CHECK(!RecentConnection::decode(
        QStringLiteral("serial:/dev/x?baud=9600&bits=8&parity=purple&stop=1&flow=none"),
        &out));
    CHECK(!RecentConnection::decode(QStringLiteral("ssh://"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("ssh:myrouter"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("ssh://host:nope"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("ssh://host:0"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("ssh://host:65536"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("telnet://host:nope"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("telnet://host:0"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("telnet://host:65536"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("gopher://x"), &out));
    CHECK(!RecentConnection::decode(QStringLiteral("shell:x"), &out));

    const QVector<RecentConnection> list = recent::decode(
        QStringLiteral("ssh://a;nonsense;telnet://b:23?mode=auto;shell:"));
    CHECK(list.size() == 3);
    CHECK(list.at(0).host == QStringLiteral("a"));
    CHECK(list.at(1).host == QStringLiteral("b"));
    CHECK(list.at(2).kind == RecentConnection::Kind::Shell);
}

/// Newest first, one entry per destination, and bounded.
void test_the_list_is_newest_first_and_bounded()
{
    QVector<RecentConnection> list;
    recent::remember(list, RecentConnection::ssh(QStringLiteral("a"), QString(),
                                                 0, QString(), false));
    recent::remember(list, RecentConnection::ssh(QStringLiteral("b"), QString(),
                                                 0, QString(), false));
    CHECK(list.size() == 2);
    CHECK(list.constFirst().host == QStringLiteral("b"));

    // Back to the first one: it moves to the top rather than appearing twice.
    recent::remember(list, RecentConnection::ssh(QStringLiteral("a"), QString(),
                                                 0, QString(), false));
    CHECK(list.size() == 2);
    CHECK(list.constFirst().host == QStringLiteral("a"));

    // The same port at a different speed is the same place, and the newest
    // parameters are the ones that worked.
    TtSerialParams line;
    tt_serial_params_default(&line);
    line.baud = 9600;
    recent::remember(list, RecentConnection::serial(QStringLiteral("/dev/x"), line));
    line.baud = 115200;
    recent::remember(list, RecentConnection::serial(QStringLiteral("/dev/x"), line));
    CHECK(list.size() == 3);
    CHECK(list.constFirst().baud == 115200);

    for (int i = 0; i < 20; i++) {
        recent::remember(list,
                         RecentConnection::telnet(QStringLiteral("h%1").arg(i),
                                                  23, TT_TELNET_AUTO));
    }
    CHECK(list.size() == recent::Max);
    CHECK(list.constFirst().host == QStringLiteral("h19"));
}

/// The record carries the five fields the dialog asks for and nothing else:
/// everything around them still comes from the settings.
void test_a_record_lays_five_fields_over_the_settings()
{
    TtSerialParams base;
    tt_serial_params_default(&base);
    base.baud = 9600;
    base.dtr = TT_PIN_CONTROL_DISABLE;
    base.detect_break = true;
    base.read_timeout_ms = 1234;

    TtSerialParams line = base;
    line.baud = 115200;
    line.flow = TT_FLOW_CONTROL_XON_XOFF;
    const RecentConnection one =
        RecentConnection::serial(QStringLiteral("/dev/x"), line);

    const TtSerialParams applied = one.appliedTo(base);
    CHECK(applied.baud == 115200);
    CHECK(applied.flow == TT_FLOW_CONTROL_XON_XOFF);
    // Untouched, because they are settings and a settings change should reach
    // a remembered connection too.
    CHECK(applied.dtr == TT_PIN_CONTROL_DISABLE);
    CHECK(applied.detect_break);
    CHECK(applied.read_timeout_ms == 1234);
}

/// The vocabulary of the field, stated once.
void test_what_a_typed_destination_means()
{
    using Kind = MainWindow::Destination::Kind;
    const auto kind = [](const char *text) {
        return MainWindow::parseDestination(QLatin1String(text)).kind;
    };

    CHECK(kind("") == Kind::Empty);
    CHECK(kind("   ") == Kind::Empty);
    CHECK(kind("shell") == Kind::Shell);
    CHECK(kind("SHELL") == Kind::Shell);
    CHECK(kind("/dev/ttyUSB0") == Kind::Serial);
    CHECK(kind("COM3") == Kind::Serial);
    CHECK(kind("com12") == Kind::Serial);
    CHECK(kind("ssh://myrouter") == Kind::Ssh);
    CHECK(kind("telnet://10.0.0.5:2323") == Kind::Telnet);
    CHECK(kind("ssh://myrouter:nope") == Kind::Invalid);
    CHECK(kind("ssh://myrouter:0") == Kind::Invalid);
    CHECK(kind("telnet://10.0.0.5:65536") == Kind::Invalid);
    // A bare word is SSH here and telnet on Tera Term's own line, which is
    // why a line with a space in it is handed to that parser whole.
    CHECK(kind("myrouter") == Kind::Ssh);
    CHECK(kind("alice@buildbox:2222") == Kind::Ssh);
    CHECK(kind("/ssh /auth=publickey myrouter") == Kind::CommandLine);
    CHECK(kind("myhost /nossh") == Kind::CommandLine);

    const MainWindow::Destination ssh =
        MainWindow::parseDestination(QStringLiteral("ssh://alice@buildbox:2222"));
    CHECK(ssh.host == QStringLiteral("buildbox"));
    CHECK(ssh.user == QStringLiteral("alice"));
    CHECK(ssh.port == 2222);

    // An unbracketed IPv6 address is ambiguous here exactly as it is for
    // `ssh`; a bracketed one keeps its colons.
    const MainWindow::Destination six =
        MainWindow::parseDestination(QStringLiteral("ssh://[fe80::1]:22"));
    CHECK(six.host == QStringLiteral("[fe80::1]"));
    CHECK(six.port == 22);

    // Telnet's default port is the one the field does not have to say.
    CHECK(MainWindow::parseDestination(QStringLiteral("telnet://box")).port == 23);
    // ...and SSH's absence is kept as an absence, because zero means
    // `~/.ssh/config` and then 22.
    CHECK(MainWindow::parseDestination(QStringLiteral("ssh://box")).port == 0);
}

/// The dropdown offers the four groups, and a row is a request rather than a
/// selection: choosing one asks the window to go there.
void test_the_dropdown_offers_every_group()
{
    ConnectBar bar(nullptr);
    auto *combo = bar.findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    CHECK(combo != nullptr);
    if (!combo) {
        return;
    }

    // With nothing remembered there is no Recent group and nothing to forget,
    // and the two rows that are always there are still there.
    CHECK(rowWithText(combo, QStringLiteral("Recent")) < 0);
    CHECK(rowWithText(combo, QStringLiteral("Forget these connections")) < 0);
    CHECK(rowWithText(combo, QStringLiteral("Local shell")) >= 0);
    CHECK(rowWithText(combo, QStringLiteral("New connection...")) >= 0);

    QVector<RecentConnection> recents;
    recents.append(RecentConnection::ssh(QStringLiteral("myrouter"), QString(), 0,
                                         QString(), false));
    recents.append(RecentConnection::telnet(QStringLiteral("10.0.0.5"), 2323,
                                            TT_TELNET_AUTO));
    bar.setRecents(recents);

    CHECK(rowWithText(combo, QStringLiteral("Recent")) >= 0);
    CHECK(rowWithText(combo, QStringLiteral("Forget these connections")) >= 0);
    const int router = rowWithText(combo, QStringLiteral("ssh myrouter"));
    CHECK(router >= 0);
    CHECK(rowWithText(combo, QStringLiteral("telnet 10.0.0.5:2323")) >= 0);

    // A group caption is not something anybody can choose.
    const int header = rowWithText(combo, QStringLiteral("Recent"));
    CHECK(!(combo->model()->flags(combo->model()->index(header, 0))
            & Qt::ItemIsSelectable));

    int chosen = -1;
    QString typed;
    int newConnections = 0;
    int forgets = 0;
    QObject::connect(&bar, &ConnectBar::recentChosen,
                     [&](const RecentConnection &one) {
                         chosen = one.port;
                     });
    QObject::connect(&bar, &ConnectBar::destinationEntered,
                     [&](const QString &text) { typed = text; });
    QObject::connect(&bar, &ConnectBar::newConnectionRequested,
                     [&] { newConnections++; });
    QObject::connect(&bar, &ConnectBar::forgetRecentsRequested,
                     [&] { forgets++; });

    const auto choose = [combo](int row) {
        QMetaObject::invokeMethod(combo, "activated", Qt::DirectConnection,
                                  Q_ARG(int, row));
    };

    auto *connectAction =
        bar.findChild<QAction *>(QStringLiteral("connectBarConnect"));
    CHECK(connectAction != nullptr);

    // **Assert what the button looks like, not only what triggering it does.**
    // `QAction::trigger()` emits `triggered` whether or not the action is
    // enabled, so a test that only triggers passes over a Connect button
    // nobody can click — which is every path through this bar on a fresh
    // window, because the field starts empty and Connect starts greyed.
    CHECK(bar.destination().isEmpty());
    CHECK(connectAction && !connectAction->isEnabled());

    // **Choosing a row fills the field and connects to nothing.** A popup
    // opens under the pointer, so the release that opened it lands on a row
    // and `activated` arrives without anybody having chosen anything.
    choose(rowWithText(combo, QStringLiteral("telnet 10.0.0.5:2323")));
    CHECK(chosen == -1);
    CHECK(typed.isEmpty());
    CHECK(bar.destination() == QStringLiteral("telnet 10.0.0.5:2323"));
    // Filling the field is only half of an answer if the button stays grey.
    CHECK(connectAction && connectAction->isEnabled());

    // ...and committing it opens the *record*, not the words: the label has
    // spaces in it and would otherwise be read as a command line.
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(chosen == 2323);
    CHECK(typed.isEmpty());

    // The record is the choice, not its position in a list that can change.
    // A successful connection moves this telnet entry to the front, and the
    // next launch also fills the field from a record without choosing a row.
    // Either path used to leave an index naming something else (or no record
    // at all) behind the same friendly label.
    bar.showConnection(recents.at(1));
    QVector<RecentConnection> reordered = {recents.at(1), recents.at(0)};
    bar.setRecents(reordered);
    chosen = -1;
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(chosen == 2323);
    CHECK(typed.isEmpty());

    // Typing over it is somebody saying something else, so the record goes.
    chosen = -1;
    combo->lineEdit()->setText(QStringLiteral("myrouter"));
    emit combo->lineEdit()->textEdited(QStringLiteral("myrouter"));
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(chosen == -1);
    CHECK(typed == QStringLiteral("myrouter"));

    typed.clear();
    choose(rowWithText(combo, QStringLiteral("Local shell")));
    CHECK(typed.isEmpty());
    CHECK(bar.destination() == QStringLiteral("shell"));
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(typed == QStringLiteral("shell"));

    // **A record does not outlive the row chosen after it.** This is the
    // sequence a user actually performs: the popup picks a row on its way
    // open, they scroll to the one they wanted and click it, then Connect.
    // The second choice replaced the words in the field and left the *record*
    // behind it, so Connect opened the accident — pick a remembered shell by
    // opening the dropdown, pick `myrouter` on purpose, and a shell opens.
    chosen = -1;
    typed.clear();
    choose(rowWithText(combo, QStringLiteral("telnet 10.0.0.5:2323")));
    choose(rowWithText(combo, QStringLiteral("Local shell")));
    CHECK(bar.destination() == QStringLiteral("shell"));
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(chosen == -1);
    CHECK(typed == QStringLiteral("shell"));

    // The two rows that are not destinations still act at once: neither opens
    // a connection, and both are at the far end of the list from where a
    // popup lands.
    choose(rowWithText(combo, QStringLiteral("New connection...")));
    CHECK(newConnections == 1);
    choose(rowWithText(combo, QStringLiteral("Forget these connections")));
    CHECK(forgets == 1);

    // The Connect action follows the field as it is typed. Nothing else runs
    // between the keystroke and the click, so a bar with nothing remembered
    // and nothing plugged in would otherwise offer a dead button.
    bar.setDestination(QString());
    if (connectAction) {
        CHECK(!connectAction->isEnabled());
        bar.setDestination(QStringLiteral("myrouter"));
        CHECK(connectAction->isEnabled());
    }

    // Rebuilding must not retype the field: it is the user's, and the popup
    // rebuilds every time it opens.
    bar.setDestination(QStringLiteral("half-typed"));
    bar.setRecents(recents);
    CHECK(bar.destination() == QStringLiteral("half-typed"));
    CHECK(typed == QStringLiteral("shell"));
}

/// Opening the dropdown must not move the field, at any window width.
///
/// The toolbar decides which of its items fit from their size hints, and it
/// wraps or hides one when they do not — so anything that invalidates the
/// combo's geometry while the user is reaching for its arrow can change the
/// width of the thing being reached for. Two rules keep it still: the model is
/// rebuilt only when the list has actually changed, and the combo's hint is a
/// constant rather than a function of its contents.
/// A port something else has open is greyed out, and the row still says whose
/// it is.
///
/// Serial *recents* rather than plugged-in ports, so this runs on a machine
/// with no adapter: a remembered port is offered whether or not it is there.
void test_a_port_another_program_holds_is_greyed()
{
    ConnectBar bar(nullptr);
    auto *combo = bar.findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    CHECK(combo != nullptr);
    if (!combo) {
        return;
    }

    TtSerialParams params;
    tt_serial_params_default(&params);
    QVector<RecentConnection> recents;
    recents.append(RecentConnection::serial(QStringLiteral("/dev/ttyUSB0"), params));
    recents.append(RecentConnection::serial(QStringLiteral("/dev/ttyUSB1"), params));
    bar.setRecents(recents);

    const auto rowContaining = [combo](const QString &needle) {
        for (int i = 0; i < combo->count(); i++) {
            if (combo->itemText(i).contains(needle)) {
                return i;
            }
        }
        return -1;
    };
    const auto enabled = [combo](int row) {
        return bool(combo->model()->flags(combo->model()->index(row, 0))
                    & Qt::ItemIsEnabled);
    };

    // The list is only re-asked as the popup opens: `setRecents` reaches
    // `rebuildList` after every successful connect, and a `/proc` walk there
    // would land between a connect and its first prompt.
    qunsetenv("STERNA_TEST_BUSY_PORTS");
    combo->showPopup();
    combo->hidePopup();
    const int free0 = rowContaining(QStringLiteral("ttyUSB0"));
    CHECK(free0 >= 0);
    CHECK(free0 >= 0 && enabled(free0));

    qputenv("STERNA_TEST_BUSY_PORTS", "/dev/ttyUSB0=minicom");
    combo->showPopup();
    combo->hidePopup();

    // **The assertion that fails without `Entry::busy` joining
    // `operator==`.** The row's own text is unchanged — the suffix is
    // composed at insert time — so a list that compares only kind, text and
    // payload is equal to the last one and `rebuildList` returns early,
    // leaving the row live.
    const int busy = rowContaining(QStringLiteral("ttyUSB0"));
    CHECK(busy >= 0);
    if (busy >= 0) {
        CHECK(!enabled(busy));
        CHECK(combo->itemText(busy).contains(QStringLiteral("in use by minicom")));
    }
    const int other = rowContaining(QStringLiteral("ttyUSB1"));
    CHECK(other >= 0);
    CHECK(other >= 0 && enabled(other));

    // Greying removes no row, so the record a live row carries is still its
    // own — the payload is an index into the remembered list.
    QString chosen;
    QObject::connect(&bar, &ConnectBar::recentChosen,
                     [&](const RecentConnection &one) { chosen = one.path; });
    QMetaObject::invokeMethod(combo, "activated", Qt::DirectConnection,
                              Q_ARG(int, other));
    auto *connectAction =
        bar.findChild<QAction *>(QStringLiteral("connectBarConnect"));
    CHECK(connectAction != nullptr);
    if (connectAction) {
        connectAction->trigger();
    }
    CHECK(chosen == QStringLiteral("/dev/ttyUSB1"));

    // **Advisory, not a prediction.** A holder that took no exclusive lock
    // does not stop the open, and a root-owned one is invisible to the scan,
    // so typing the busy port still offers Connect — the error on the connect
    // path is where the truth lives.
    bar.setDestination(QStringLiteral("/dev/ttyUSB0"));
    CHECK(connectAction && connectAction->isEnabled());

    qunsetenv("STERNA_TEST_BUSY_PORTS");
}

void test_the_dropdown_does_not_move_the_field()
{
    MainWindow window;
    window.show();
    qApp->processEvents();
    auto *bar = window.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    auto *combo =
        bar->findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    CHECK(combo != nullptr);
    if (!combo) {
        return;
    }

    QVector<RecentConnection> recents;
    recents.append(RecentConnection::ssh(
        QStringLiteral("a-fairly-long-hostname.example.net"),
        QStringLiteral("someone"), 2222, QString(), false));
    bar->setRecents(recents);
    qApp->processEvents();

    // A Wayland client does not decide its own size — the compositor acks the
    // resize later — so a width the test asked for can land *during* the
    // popup and read as the popup having moved the field. The widths are
    // swept under offscreen for the same reason `cmdline_test` is; the case
    // below still runs everywhere at whatever size the window really has.
    QVector<int> widths = {0};
    if (!QGuiApplication::platformName().startsWith(QLatin1String("wayland"))) {
        widths = {900, 800, 720, 700, 650, 600, 560, 520, 480};
    }

    const QSize hint = combo->sizeHint();
    for (int width : widths) {
        if (width > 0) {
            window.resize(width, window.height());
            qApp->processEvents();
        }
        const int before = combo->width();
        const int barHeight = bar->height();
        combo->showPopup();
        qApp->processEvents();
        CHECK(combo->width() == before);
        CHECK(bar->height() == barHeight);
        combo->hidePopup();
        qApp->processEvents();
        CHECK(combo->width() == before);
    }
    // ...and the hint did not move either, though a `by-path` name and a long
    // host name both passed through the list on the way.
    CHECK(combo->sizeHint() == hint);
}

#ifndef Q_OS_WIN
/// End to end, on the one destination that needs nothing: typing it opens a
/// session, and the connection joins the list in the settings file.
void test_a_typed_shell_connects_and_is_remembered()
{
    MainWindow window;
    window.connectDestination(QStringLiteral("shell"));
    qApp->processEvents();
    CHECK(window.session()->isConnected());

    CHECK(window.session()->setting(QStringLiteral("recent.connections"))
          == QStringLiteral("shell:"));

    auto *bar = window.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(bar != nullptr);
    if (bar) {
        CHECK(bar->destination() == QStringLiteral("Local shell"));
    }

    // And the next launch opens on it. The list is read where the rest of the
    // remembered connection is, which is after the bar exists — a guard that
    // silently does nothing would look exactly like an empty list.
    MainWindow next;
    auto *nextBar = next.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(nextBar != nullptr);
    if (nextBar) {
        CHECK(nextBar->destination() == QStringLiteral("Local shell"));
    }
}

/// Opening the dropdown during a session must not disable Disconnect.
///
/// The button is the only way out of a connection on this bar, and choosing a
/// row is something the popup does by itself on the way open — so a rule that
/// touched the button's enabled state from there took Disconnect away from
/// somebody who had merely looked at the list. Enabling it belongs to the
/// field's own signal while nothing is open, and to `refresh` while something
/// is; the choice has no business in it.
void test_choosing_a_row_leaves_disconnect_alive()
{
    MainWindow window;
    window.connectDestination(QStringLiteral("shell"));
    qApp->processEvents();
    CHECK(window.session()->isConnected());

    auto *bar = window.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    auto *combo =
        bar->findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    auto *action = bar->findChild<QAction *>(QStringLiteral("connectBarConnect"));
    CHECK(combo != nullptr);
    CHECK(action != nullptr);
    if (!combo || !action) {
        return;
    }
    CHECK(action->isEnabled());

    // The shell that just connected is the first thing in the list, which is
    // where a popup opening over the field lands.
    int recent = -1;
    for (int i = 0; i < combo->count() && recent < 0; i++) {
        if (combo->itemText(i) == QStringLiteral("Local shell")) {
            recent = i;
        }
    }
    CHECK(recent >= 0);
    if (recent >= 0) {
        QMetaObject::invokeMethod(combo, "activated", Qt::DirectConnection,
                                  Q_ARG(int, recent));
    }
    CHECK(action->isEnabled());
}

/// Connecting must not move the field either. The action's two words are
/// different widths, and the field beside it is the expanding item in the
/// toolbar — so without a reserved width, opening a session resizes the box.
void test_connecting_does_not_move_the_field()
{
    MainWindow window;
    window.show();
    qApp->processEvents();
    auto *bar = window.findChild<ConnectBar *>(QStringLiteral("connectBar"));
    CHECK(bar != nullptr);
    if (!bar) {
        return;
    }
    auto *combo =
        bar->findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    auto *button =
        bar->findChild<QToolButton *>(QStringLiteral("connectBarConnectButton"));
    CHECK(combo != nullptr);
    CHECK(button != nullptr);
    if (!combo || !button) {
        return;
    }

    const int field = combo->width();
    const int action = button->width();
    window.connectDestination(QStringLiteral("shell"));
    qApp->processEvents();
    CHECK(window.session()->isConnected());
    // The word really did change, so the reservation is what is being tested
    // and not the absence of a change.
    CHECK(button->text() != QStringLiteral("Connect"));
    CHECK(button->width() == action);
    CHECK(combo->width() == field);
}

/// A Tera Term line names settings and a connection together. When another
/// session is live, both halves must land in the new tab rather than applying
/// the settings to the old one before `openTarget` allocates the new page.
void test_a_command_line_applies_to_the_page_it_opens()
{
    Listener listener;
    CHECK(listener.port() != 0);
    if (listener.port() == 0) {
        return;
    }

    MainWindow window;
    window.connectDestination(QStringLiteral("shell"));
    qApp->processEvents();
    CHECK(window.session()->isConnected());

    window.connectDestination(
        QStringLiteral("127.0.0.1:%1 /nossh /W=second")
            .arg(listener.port()));
    qApp->processEvents();
    CHECK(window.session()->isConnected());
    CHECK(window.session()->setting(QStringLiteral("terminal.title"))
          == QStringLiteral("second"));
}

/// Off stops recording and leaves what is there: the entries are still on
/// offer, because hiding a list nobody can add to takes away the way back as
/// well as the way forward.
void test_recording_can_be_turned_off_without_losing_the_list()
{
    MainWindow window;
    QString error;
    CHECK(window.session()->setSetting(QStringLiteral("recent.connections"),
                                       QStringLiteral("ssh://kept"), &error));
    CHECK(window.session()->setSetting(QStringLiteral("recent.remember"),
                                       QStringLiteral("off"), &error));

    window.connectDestination(QStringLiteral("shell"));
    qApp->processEvents();
    CHECK(window.session()->isConnected());
    CHECK(window.session()->setting(QStringLiteral("recent.connections"))
          == QStringLiteral("ssh://kept"));
}
#endif

/// `--write <dir>`: the bar and its open dropdown, as PNGs. Every other
/// `*_test` takes the same flag.
void write_images(const QString &dir)
{
    QVector<RecentConnection> recents;
    TtSerialParams line;
    tt_serial_params_default(&line);
    line.baud = 115200;
    recents.append(RecentConnection::serial(
        QStringLiteral("/dev/serial/by-path/pci-0000:c6:00.3-usb-0:4.2:1.0"),
        line));
    recents.append(RecentConnection::ssh(QStringLiteral("myrouter"), QString(),
                                         0, QString(), false));
    recents.append(RecentConnection::ssh(QStringLiteral("buildbox"),
                                         QStringLiteral("alice"), 2222,
                                         QString(), false));
    recents.append(RecentConnection::telnet(QStringLiteral("10.0.0.5"), 2323,
                                            TT_TELNET_AUTO));
    recents.append(RecentConnection::shell());

    QMainWindow window;
    auto *bar = new ConnectBar(nullptr, &window);
    window.addToolBar(Qt::TopToolBarArea, bar);
    bar->setRecents(recents);
    window.resize(1000, 60);
    window.show();
    qApp->processEvents();
    bar->grab().save(dir + QStringLiteral("/connect-bar.png"));

    auto *combo = bar->findChild<QComboBox *>(QStringLiteral("connectBarDestination"));
    if (combo) {
        combo->showPopup();
        qApp->processEvents();
        combo->view()->window()->grab().save(dir
                                             + QStringLiteral("/connect-list.png"));
        combo->hidePopup();
    }
}

int main(int argc, char **argv)
{
    // Or every `MainWindow` here reads the developer's own `sterna.ini` — and
    // writes to it, which for this test would mean putting a local shell into
    // somebody's real list of recent connections.
    QStandardPaths::setTestModeEnabled(true);
    QApplication app(argc, argv);
    // Test mode protects the developer's real file; removing its own makes
    // repeated runs independent too. The connection cases deliberately save
    // recents immediately.
    QFile::remove(MainWindow::settingsPath());

    test_a_record_survives_the_settings_file();
    test_a_broken_record_is_dropped_and_not_repaired();
    test_the_list_is_newest_first_and_bounded();
    test_a_record_lays_five_fields_over_the_settings();
    test_what_a_typed_destination_means();
    test_the_dropdown_offers_every_group();
    test_a_port_another_program_holds_is_greyed();
    test_the_dropdown_does_not_move_the_field();
    for (int i = 1; i + 1 < argc; i++) {
        if (QLatin1String(argv[i]) == QLatin1String("--write")) {
            write_images(QString::fromLocal8Bit(argv[i + 1]));
        }
    }
#ifndef Q_OS_WIN
    test_a_typed_shell_connects_and_is_remembered();
    test_choosing_a_row_leaves_disconnect_alive();
    test_connecting_does_not_move_the_field();
    test_a_command_line_applies_to_the_page_it_opens();
    test_recording_can_be_turned_off_without_losing_the_list();
#endif

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    QFile::remove(MainWindow::settingsPath());
    printf("connect ok\n");
    return 0;
}
