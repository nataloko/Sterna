// Lua plugins through the window's real menus and event loop.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   QT_QPA_PLATFORM=offscreen ./build/plugin_test
//
// No server or hardware is needed. The lifecycle cases use the platform's
// local shell, `/bin/sh` through a pty or `cmd.exe` through ConPTY.

#include <QAction>
#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QKeySequence>
#include <QMenu>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "MainWindow.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

template <typename F>
bool spin(F done, int ms = 5000)
{
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(10, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

QString screenText(const Session &session)
{
    QString out;
    for (int y = 0; y < session.rows(); y++) {
        size_t len = 0;
        const TtCell *row = session.row(y, &len);
        if (!row) {
            continue;
        }
        for (size_t x = 0; x < len; x++) {
            if (row[x].width_class == TT_WIDTH_PAD) {
                continue;
            }
            const uint32_t c = row[x].text[0];
            out.append(c ? QChar(static_cast<char16_t>(c)) : QLatin1Char(' '));
        }
        out.append(QLatin1Char('\n'));
    }
    return out;
}

void test_window_plugins()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString plugins = QDir(dir.path()).filePath(QStringLiteral("plugins"));
    CHECK(QDir().mkpath(plugins));

    QFile source(QDir(plugins).filePath(QStringLiteral("10-window.lua")));
    CHECK(source.open(QIODevice::WriteOnly));
    source.write(
        "local count = 0\n"
        "local function say(name)\n"
        "  count = count + 1\n"
        "  print(name .. ' ' .. count)\n"
        "end\n"
        "sterna.menu { menu = 'Control/Plugins/Test', label = 'Count',\n"
        "  shortcut = 'Ctrl+Alt+C', action = function() say('menu') end }\n"
        "sterna.key('Ctrl+Alt+K', function() say('key') end)\n"
        "sterna.on('connect', function() say('connected') end)\n"
        "sterna.on('disconnect', function() say('disconnected') end)\n");
    source.close();

    MainWindow window(QDir(dir.path()).filePath(QStringLiteral("sterna.ini")),
                      plugins);
    QAction *menuAction =
        window.findChild<QAction *>(QStringLiteral("luaPluginAction0"));
    QAction *keyAction =
        window.findChild<QAction *>(QStringLiteral("luaPluginAction1"));
    CHECK(menuAction != nullptr);
    CHECK(keyAction != nullptr);
    if (!menuAction || !keyAction) {
        return;
    }

    CHECK(menuAction->text() == QStringLiteral("Count"));
    CHECK(menuAction->shortcut()
          == QKeySequence::fromString(QStringLiteral("Ctrl+Alt+C"),
                                      QKeySequence::PortableText));
    CHECK(keyAction->shortcut()
          == QKeySequence::fromString(QStringLiteral("Ctrl+Alt+K"),
                                      QKeySequence::PortableText));

    bool nested = false;
    QMenu *control = window.findChild<QMenu *>(QStringLiteral("controlMenu"));
    CHECK(control != nullptr);
    if (control) {
        for (QMenu *menu : control->findChildren<QMenu *>()) {
            if (menu->title() == QStringLiteral("Test")
                && menu->actions().contains(menuAction)) {
                nested = true;
            }
        }
    }
    CHECK(nested);

    menuAction->trigger();
    CHECK(spin([&] {
        return screenText(*window.session()).contains(QStringLiteral("menu 1"));
    }));
    keyAction->trigger();
    CHECK(spin([&] {
        return screenText(*window.session()).contains(QStringLiteral("key 2"));
    }));

    window.connectPty();
    CHECK(window.session()->isConnected());
    CHECK(spin([&] {
        return screenText(*window.session())
            .contains(QStringLiteral("connected 3"));
    }, 15000));

    window.session()->disconnectPort();
    CHECK(spin([&] {
        return screenText(*window.session())
            .contains(QStringLiteral("disconnected 4"));
    }));
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("sterna-plugin-test"));

    test_window_plugins();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("plugin ok\n");
    return 0;
}
