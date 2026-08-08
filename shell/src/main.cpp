// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QApplication>
#include <QCommandLineParser>
#include <QIcon>

#include "MainWindow.h"
#include "sterna.h"

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("sterna"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Sterna"));
    QCoreApplication::setApplicationVersion(QString::fromUtf8(tt_version()));
    app.setWindowIcon(QIcon(QStringLiteral(":/branding/sterna.svg")));

    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("A serial and SSH terminal."));
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
        QStringLiteral("Baud rate (default 9600)."), QStringLiteral("rate"));
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
        TtSerialParams params;
        tt_serial_params_default(&params);
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
