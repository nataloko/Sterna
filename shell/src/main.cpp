// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include <QApplication>
#include <QCommandLineParser>

#include "MainWindow.h"
#include "termitta.h"

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("termitta"));
    QCoreApplication::setApplicationVersion(QString::fromUtf8(tt_version()));

    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("A serial and SSH terminal. Stage 1: serial."));
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
    parser.addOption(portOption);
    parser.addOption(baudOption);
    parser.process(app);

    MainWindow window;
    window.show();

    if (parser.isSet(portOption)) {
        TtSerialParams params;
        tt_serial_params_default(&params);
        if (parser.isSet(baudOption)) {
            params.baud = parser.value(baudOption).toUInt();
        }
        window.connectSerial(parser.value(portOption), params);
    }

    return app.exec();
}
