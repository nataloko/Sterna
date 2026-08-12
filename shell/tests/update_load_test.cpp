// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QApplication>
#include <QCoreApplication>
#include <QDir>
#include <QLibrary>
#include <QObject>
#include <QWidget>

#include <cstdio>

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
#ifdef Q_OS_WIN
    const QString path = QDir(QCoreApplication::applicationDirPath())
                             .filePath(QStringLiteral("sterna_updater.dll"));
#else
    const QString path = QStringLiteral(TT_UPDATE_LIBRARY);
#endif
    QLibrary library(path);
    using Factory = QObject *(*)(QWidget *);
    const auto factory =
        reinterpret_cast<Factory>(library.resolve("sterna_updater_new"));
    if (!factory) {
        std::fprintf(stderr, "updater load: %s\n",
                     library.errorString().toUtf8().constData());
        return 1;
    }
    QObject *updater = factory(nullptr);
    if (!updater || updater->metaObject()->indexOfMethod("check()") < 0) {
        std::fputs("updater load: factory returned the wrong object\n", stderr);
        delete updater;
        return 1;
    }
    delete updater;
    if (!library.unload()) {
        std::fprintf(stderr, "updater unload: %s\n",
                     library.errorString().toUtf8().constData());
        return 1;
    }
    std::puts("update load ok");
    return 0;
}
