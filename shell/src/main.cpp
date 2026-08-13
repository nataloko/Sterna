// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QApplication>
#include <QCommandLineParser>
#include <QDir>
#include <QFileInfo>
#include <QIcon>

#include "MainWindow.h"
#include "sterna.h"

namespace {

/// Is this a Tera Term command line rather than one of ours?
///
/// **The two spellings cannot be merged, because on one point they disagree:**
/// a bare host name is telnet to `ttermpro` and SSH to `sterna`, which is the
/// right default on each side — Tera Term grew up on serial and telnet, and
/// anyone typing a host name into a terminal on Linux means `ssh`. So the line
/// is read one way or the other, never half of each.
///
/// The rule, in order:
///
/// 1. Anything led by `-` is ours (or Qt's own, which `QApplication` has
///    already taken out). This is what keeps `sterna --shell -- /bin/sh`
///    working, whose *positional* arguments are a command full of slashes.
/// 2. Otherwise, anything that looks like a `/OPTION` is Tera Term's. "Looks
///    like" means the keyword before any `=` has no second `/` in it, so
///    `/F=/home/me/x.ini` is an option and `/usr/bin/less` is a path.
/// 3. Otherwise ours, which is the case of a bare `sterna myrouter`.
///
/// TTSSH's dash spellings — `-ssh`, `-auth=…` — therefore fall to rule 1 and
/// are reported as unknown options rather than parsed. That is deliberate:
/// `-` is Qt's own option lead here, and a line where the two overlap would
/// have to guess.
bool looksLikeTeraTerm(const QStringList &args)
{
    bool sawOption = false;
    for (const QString &arg : args) {
        if (arg.startsWith(QLatin1Char('-'))) {
            return false;
        }
        if (!arg.startsWith(QLatin1Char('/'))) {
            continue;
        }
        const QString keyword = arg.left(arg.indexOf(QLatin1Char('=')) < 0
                                             ? arg.size()
                                             : arg.indexOf(QLatin1Char('=')));
        if (!keyword.mid(1).contains(QLatin1Char('/'))) {
            sawOption = true;
        }
    }
    return sawOption;
}

/// Where a `/F=` points.
///
/// A bare name resolves against the directory the settings live in, which is
/// this platform's answer to upstream's `ts.HomeDirW` — the place a *second*
/// settings file would be kept. Anything with a separator in it is a path and
/// is taken as given, so `/F=./other.ini` still means the working directory.
///
/// Upstream would also append `.INI` to a name with no dot in it. That half is
/// dropped: on a case-sensitive filesystem `work.INI` is not the `work.ini`
/// the user has, so the rule would turn a name that works into one that does
/// not.
QString settingsFile(const char *given)
{
    const QString path = QString::fromUtf8(given);
    if (path.contains(QLatin1Char('/'))) {
        return path;
    }
    return QDir(QFileInfo(MainWindow::settingsPath()).absolutePath())
        .filePath(path);
}

/// The Tera Term command line, from `/F=` through to the connection.
int runTeraTerm(QApplication &app, const QStringList &args)
{
    QList<QByteArray> owned;
    QList<const char *> argv;
    owned.reserve(args.size());
    for (const QString &arg : args) {
        owned.append(arg.toUtf8());
    }
    for (const QByteArray &arg : owned) {
        argv.append(arg.constData());
    }

    TtCmdLine *cmd =
        tt_cmdline_parse(argv.constData(), static_cast<size_t>(argv.size()), 0);
    if (!cmd) {
        qCritical("%s", tt_last_error());
        return 1;
    }

    // Upstream parses twice, and this is why: `/F=` names the settings file,
    // and `MaxComPort=` — which is what bounds `/C=` — is *in* that file. The
    // first parse is only there to find the name.
    //
    // Everything read out of it is copied here, because a `TtCmdLineInfo`'s
    // strings are borrowed from the handle and the handle may be replaced
    // below.
    QString ini;
    {
        TtCmdLineInfo info = {};
        tt_cmdline_info(cmd, &info);
        if (info.setup_file) {
            ini = settingsFile(info.setup_file);
        }
    }
    MainWindow window(ini);
    const int maxComPort =
        window.session()->setting(QStringLiteral("serial.max_com_port")).toInt();
    if (maxComPort > 0 && maxComPort != 256) {
        tt_cmdline_free(cmd);
        cmd = tt_cmdline_parse(argv.constData(),
                               static_cast<size_t>(argv.size()),
                               static_cast<uint16_t>(maxComPort));
        if (!cmd) {
            qCritical("%s", tt_last_error());
            return 1;
        }
    }

    window.startFrom(cmd);
    tt_cmdline_free(cmd);
    return app.exec();
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("sterna"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Sterna"));
    QCoreApplication::setApplicationVersion(QString::fromUtf8(tt_version()));
    app.setWindowIcon(QIcon(QStringLiteral(":/branding/sterna.svg")));

    // Qt's own arguments are gone by here — `QApplication` takes `-platform`
    // and friends out of `argc`/`argv` — so this is what was meant for us.
    QStringList args = QCoreApplication::arguments();
    if (!args.isEmpty()) {
        args.removeFirst();
    }
    if (looksLikeTeraTerm(args)) {
        return runTeraTerm(app, args);
    }

    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("A serial and SSH terminal.\n\n"
                       "Tera Term's own command line is accepted as well: an "
                       "argument spelled /OPTION switches to it, so a "
                       "converted shortcut such as `sterna /ssh /auth=publickey "
                       "myhost` works as it did. A bare host name means SSH "
                       "here and telnet there, which is why the two are read "
                       "one way or the other and never half of each."));
    parser.addHelpOption();
    parser.addVersionOption();

    // Enough to skip the dialog when opening the same console for the tenth
    // time today, which is the whole reason a serial terminal gets opened.
    QCommandLineOption portOption(
        {QStringLiteral("p"), QStringLiteral("port")},
        QStringLiteral("Serial port to open. Prefer a /dev/serial/by-path name: "
                       "/dev/ttyUSB<n> is assigned in attach order."),
        QStringLiteral("path"));
    QCommandLineOption baudOption(
        {QStringLiteral("b"), QStringLiteral("baud")},
        QStringLiteral("Baud rate. Defaults to the last one connected at, or to "
                       "the settings file's BaudRate, or to 115200 — where "
                       "Tera Term's default is 9600."),
        QStringLiteral("rate"));
    // `user@host`, or a bare alias out of ~/.ssh/config — the same thing that
    // would be typed after `ssh`, because anyone reaching for this already
    // knows that spelling.
    parser.addPositionalArgument(
        QStringLiteral("[user@]host[:port]"),
        QStringLiteral("Connect over SSH. May be an alias from ~/.ssh/config."));
    QCommandLineOption telnetOption(
        {QStringLiteral("t"), QStringLiteral("telnet")},
        QStringLiteral("Treat the positional argument as telnet rather than SSH. "
                       "The protocol follows the port: negotiated on 23, "
                       "auto-detected elsewhere, which is what a terminal "
                       "server's per-line port needs."));
    // A local shell takes no argument, so the positional list is free for the
    // command to run — `sterna --shell -- journalctl -f` — the same spelling
    // `xterm -e` and `gnome-terminal --` use.
    QCommandLineOption shellOption(
        {QStringLiteral("s"), QStringLiteral("shell")},
        QStringLiteral("Run a local shell. Any positional arguments are the "
                       "command to run instead of the login shell."));
    parser.addOption(telnetOption);
    parser.addOption(shellOption);
    parser.addOption(portOption);
    parser.addOption(baudOption);
    parser.process(app);

    MainWindow window;
    window.show();

    if (parser.isSet(shellOption)) {
        window.connectPty(parser.positionalArguments());
    } else if (parser.isSet(portOption)) {
        // The settings file's line settings, which after a connection are also
        // the last ones used — upstream's `/C=1` reads the same keys. `--baud`
        // overrides just the speed, as `/BAUD=` does.
        TtSerialParams params = window.serialParams();
        if (parser.isSet(baudOption)) {
            params.baud = parser.value(baudOption).toUInt();
        }
        window.connectSerial(parser.value(portOption), params);
    } else if (!parser.positionalArguments().isEmpty()) {
        QString target = parser.positionalArguments().constFirst();
        QString user;
        int port = 0;
        const int at = target.indexOf(QLatin1Char('@'));
        if (at >= 0) {
            user = target.left(at);
            target = target.mid(at + 1);
        }
        // Split on the *last* colon so a bracketed IPv6 literal survives; a
        // bare IPv6 address without brackets is ambiguous here exactly as it
        // is for `ssh`, and is spelled with -p there and in ~/.ssh/config.
        const int colon = target.lastIndexOf(QLatin1Char(':'));
        if (colon > target.lastIndexOf(QLatin1Char(']'))) {
            port = target.mid(colon + 1).toInt();
            target = target.left(colon);
        }
        if (parser.isSet(telnetOption)) {
            window.connectTelnet(target, static_cast<quint16>(port ? port : 23));
        } else {
            window.connectSsh(target, user, port);
        }
    }

    return app.exec();
}
