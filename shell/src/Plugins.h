// Lua plugins, loaded once for one terminal page.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "sterna.h"

class Macro;
class Session;
#ifdef Q_OS_WIN
class QWinEventNotifier;
#else
class QSocketNotifier;
#endif

/// A copied plugin declaration. The ABI strings are borrowed from the plugin
/// handle; the window keeps these values after calls back into the core.
struct PluginActionInfo {
    size_t id = 0;
    TtPluginActionKind kind = TT_PLUGIN_ACTION_MENU;
    QString plugin;
    QString menu;
    QString label;
    QString shortcut;

    bool operator==(const PluginActionInfo &other) const
    {
        return kind == other.kind && plugin == other.plugin
               && menu == other.menu && label == other.label
               && shortcut == other.shortcut;
    }
};

/// A copied custom settings declaration. Its current value remains in the
/// core's shared control; the dialog asks `Plugins::setting` when it opens.
struct PluginSettingInfo {
    size_t id = 0;
    size_t pageId = 0;
    QString plugin;
    QString page;
    QString section;
    QString key;
    QString name;
    QString label;
    QString description;
    QString defaultValue;
    TtSettingKind kind = TT_SETTING_KIND_STR;
    int min = 0;
    int max = 0;
    QStringList choices;
};

/// Persistent Lua VMs and the notifier which services their host calls.
///
/// There is one per page because a plugin callback is attached to one
/// terminal's receive stream. The files are loaded in filename order by the
/// core; this class only copies their declarations into Qt values and puts the
/// worker's native wakeup into the event loop.
class Plugins : public QObject {
    Q_OBJECT

public:
    Plugins(Session *session, Macro *ui, const QString &directory,
            const QString &settingsPath,
            QObject *parent = nullptr);
    ~Plugins() override;

    const QVector<PluginActionInfo> &actions() const { return m_actions; }
    const QVector<PluginSettingInfo> &settings() const { return m_settings; }
    QString setting(size_t id);
    bool setSetting(size_t id, const QString &value, QString *outError = nullptr);
    bool copySettingsFrom(const Plugins &source, QString *outError = nullptr);
    bool saveSettings(const QString &path, QString *outError = nullptr) const;
    QString error() const { return m_error; }
    bool busy() const;

    /// Start one menu/key callback. False and `outError` when another callback
    /// is already running or the worker has stopped.
    bool invoke(size_t id, QString *outError = nullptr);

signals:
    void finished();
    void notice(const QString &text);

private slots:
    void service();
    void onConnectionChanged();

private:
    bool emitHook(TtPluginHook hook);

    Session *m_session;
    TtPlugins *m_plugins = nullptr;
#ifdef Q_OS_WIN
    QWinEventNotifier *m_notifier = nullptr;
#else
    QSocketNotifier *m_notifier = nullptr;
#endif
    QVector<PluginActionInfo> m_actions;
    QVector<PluginSettingInfo> m_settings;
    QString m_error;
    bool m_wasConnected = false;
    bool m_active = false;
};
