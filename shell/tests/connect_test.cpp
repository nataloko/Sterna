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
#include <QAbstractItemView>
#include <QComboBox>
#include <QMainWindow>
#include <QStandardPaths>

#include <cstdio>

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

    choose(rowWithText(combo, QStringLiteral("telnet 10.0.0.5:2323")));
    CHECK(chosen == 2323);

    choose(rowWithText(combo, QStringLiteral("Local shell")));
    CHECK(typed == QStringLiteral("shell"));

    choose(rowWithText(combo, QStringLiteral("New connection...")));
    CHECK(newConnections == 1);
    choose(rowWithText(combo, QStringLiteral("Forget these connections")));
    CHECK(forgets == 1);

    // Rebuilding must not retype the field: it is the user's, and the popup
    // rebuilds every time it opens.
    bar.setDestination(QStringLiteral("half-typed"));
    bar.setRecents(recents);
    CHECK(bar.destination() == QStringLiteral("half-typed"));
    CHECK(typed == QStringLiteral("shell"));
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

    test_a_record_survives_the_settings_file();
    test_a_broken_record_is_dropped_and_not_repaired();
    test_the_list_is_newest_first_and_bounded();
    test_a_record_lays_five_fields_over_the_settings();
    test_what_a_typed_destination_means();
    test_the_dropdown_offers_every_group();
    for (int i = 1; i + 1 < argc; i++) {
        if (QLatin1String(argv[i]) == QLatin1String("--write")) {
            write_images(QString::fromLocal8Bit(argv[i + 1]));
        }
    }
#ifndef Q_OS_WIN
    test_a_typed_shell_connects_and_is_remembered();
    test_recording_can_be_turned_off_without_losing_the_list();
#endif

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("connect ok\n");
    return 0;
}
