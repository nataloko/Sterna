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
#include <QLineEdit>
#include <QStatusBar>
#include <QTabWidget>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>

#include "MainWindow.h"
#include "Plugins.h"
#include "SettingsDialog.h"

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
        "local preferences = sterna.settings { title = 'Window plugin', "
        "section = 'Lua Window Test', fields = {\n"
        "  { name = 'enabled', label = 'Enabled', kind = 'bool', default = true },\n"
        "  { name = 'retries', label = 'Retries', kind = 'int', min = 1, max = 9, default = 3 },\n"
        "  { name = 'prefix', key = 'PromptPrefix', label = 'Prefix', "
        "description = 'Text before setting probes', kind = 'string', default = 'default:' },\n"
        "  { name = 'mode', label = 'Mode', kind = 'enum', "
        "choices = {'fast', 'safe'}, default = 'fast' },\n"
        "} }\n"
        "local inbound\n"
        "inbound = sterna.filter('input', function(bytes)\n"
        "  if bytes == 'probe' then return inbound.replacement end\n"
        "  if bytes == 'setting' then return preferences.prefix end\n"
        "  if bytes == 'boom' then error('broken filter') end\n"
        "  return bytes\n"
        "end)\n"
        "inbound.replacement = 'before'\n"
        "sterna.filter('output', function(bytes)\n"
        "  if bytes == 'FILTER\\r' then return \"printf 'out-filter\\\\n'\\r\" end\n"
        "  return bytes\n"
        "end)\n"
        "local function say(name)\n"
        "  count = count + 1\n"
        "  print(name .. ' ' .. count)\n"
        "end\n"
        "sterna.menu { menu = 'Control/Plugins/Test', label = 'Count',\n"
        "  shortcut = 'Ctrl+Alt+C', action = function()\n"
        "    inbound.replacement = 'after'; say('menu')\n"
        "  end }\n"
        "sterna.key('Ctrl+Alt+K', function() say('key') end)\n"
        "sterna.on('connect', function() say('connected') end)\n"
        "sterna.on('disconnect', function() say('disconnected') end)\n");
    source.close();

    const QString ini = QDir(dir.path()).filePath(QStringLiteral("sterna.ini"));
    QFile settings(ini);
    CHECK(settings.open(QIODevice::WriteOnly));
    settings.write("[Lua Window Test]\nPromptPrefix=saved:\nMode=safe\n");
    settings.close();

    MainWindow window(ini, plugins);
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

    Plugins *loaded = window.findChild<Plugins *>();
    CHECK(loaded != nullptr);
    if (loaded) {
        CHECK(loaded->settings().size() == 4);
        CHECK(loaded->setting(2) == QStringLiteral("saved:"));
        CHECK(loaded->setting(3) == QStringLiteral("safe"));

        SettingsDialog dialog(window.session(), loaded);
        QTabWidget *tabs = dialog.findChild<QTabWidget *>();
        CHECK(tabs != nullptr);
        bool foundPluginPage = false;
        if (tabs) {
            for (int i = 0; i < tabs->count(); i++) {
                foundPluginPage |= tabs->tabText(i) == QStringLiteral("Window plugin");
            }
        }
        CHECK(foundPluginPage);
        QLineEdit *prefix =
            dialog.findChild<QLineEdit *>(QStringLiteral("luaPluginSetting2"));
        CHECK(prefix != nullptr);
        if (prefix) {
            CHECK(prefix->text() == QStringLiteral("saved:"));
            prefix->setText(QStringLiteral("live:"));
            dialog.applyChanges();
            CHECK(loaded->setting(2) == QStringLiteral("live:"));
        }

        window.session()->feed(QByteArray("setting"));
        CHECK(screenText(*window.session()).contains(QStringLiteral("live:")));
        window.session()->feed(QByteArray("\033[2J\033[H"));
        QString saveError;
        CHECK(loaded->saveSettings(ini, &saveError));
        CHECK(saveError.isEmpty());
        CHECK(settings.open(QIODevice::ReadOnly));
        CHECK(settings.readAll().contains("PromptPrefix=live:"));
        settings.close();
    }

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

    window.session()->feed(QByteArray("probe"));
    CHECK(screenText(*window.session()).contains(QStringLiteral("before")));
    window.session()->feed(QByteArray("\033[2J\033[H"));

    menuAction->trigger();
    CHECK(spin([&] {
            return screenText(*window.session()).contains(QStringLiteral("menu 1"));
    }));
    window.session()->feed(QByteArray("\033[2J\033[H"));
    window.session()->feed(QByteArray("probe"));
    CHECK(screenText(*window.session()).contains(QStringLiteral("after")));
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

    window.session()->sendText(QStringLiteral("FILTER\r"));
    CHECK(spin([&] {
        return screenText(*window.session()).contains(QStringLiteral("out-filter"));
    }, 15000));

    window.session()->disconnectPort();
    CHECK(spin([&] {
        return screenText(*window.session())
            .contains(QStringLiteral("disconnected 4"));
    }));

    window.session()->feed(QByteArray("boom"));
    CHECK(window.statusBar()->currentMessage().contains(
        QStringLiteral("Lua stream filter disabled")));
    CHECK(screenText(*window.session()).contains(QStringLiteral("boom")));
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
