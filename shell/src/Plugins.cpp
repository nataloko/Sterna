// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Plugins.h"

#ifdef Q_OS_WIN
#include <QWinEventNotifier>
#else
#include <QSocketNotifier>
#endif

#include "Macro.h"
#include "Session.h"

namespace {

QString string(const char *value)
{
    return value ? QString::fromUtf8(value) : QString();
}

} // namespace

Plugins::Plugins(Session *session, Macro *ui, const QString &directory,
                 QObject *parent)
    : QObject(parent)
    , m_session(session)
    , m_wasConnected(session->isConnected())
{
    const QByteArray path = directory.toUtf8();
    TtMacroUi callbacks = ui->ui();
    m_plugins = tt_plugins_load(m_session->handle(), path.constData(), &callbacks);
    if (!m_plugins) {
        m_error = QString::fromUtf8(tt_last_error());
        return;
    }

    const size_t count = tt_plugins_action_count(m_plugins);
    m_actions.reserve(static_cast<qsizetype>(count));
    for (size_t i = 0; i < count; i++) {
        TtPluginAction action = {};
        if (!tt_plugins_action(m_plugins, i, &action)) {
            continue;
        }
        m_actions.append({action.id, action.kind, string(action.plugin),
                          string(action.menu), string(action.label),
                          string(action.shortcut)});
    }

#ifdef Q_OS_WIN
    void *handle = tt_plugins_wait_handle(m_plugins);
    if (handle) {
        m_notifier = new QWinEventNotifier(handle, this);
        connect(m_notifier, &QWinEventNotifier::activated, this,
                &Plugins::service);
    }
#else
    const int fd = tt_plugins_poll_fd(m_plugins);
    if (fd >= 0) {
        m_notifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
        connect(m_notifier, &QSocketNotifier::activated, this,
                &Plugins::service);
    }
#endif

    connect(m_session, &Session::connectionChanged, this,
            &Plugins::onConnectionChanged);
}

Plugins::~Plugins()
{
    delete m_notifier;
    m_notifier = nullptr;
    if (m_plugins) {
        tt_plugins_free(m_plugins);
        m_plugins = nullptr;
        tt_session_unlink_plugins(m_session->handle());
    }
}

bool Plugins::busy() const
{
    return m_plugins && tt_plugins_busy(m_plugins);
}

bool Plugins::invoke(size_t id, QString *outError)
{
    if (!m_plugins) {
        if (outError) {
            *outError = m_error;
        }
        return false;
    }
    if (tt_plugins_invoke(m_plugins, id) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    m_active = busy();
    return true;
}

bool Plugins::emitHook(TtPluginHook hook)
{
    if (!m_plugins) {
        return false;
    }
    if (tt_plugins_emit(m_plugins, hook) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
        return false;
    }
    m_active |= busy();
    return true;
}

void Plugins::onConnectionChanged()
{
    const bool connected = m_session->isConnected();
    if (connected == m_wasConnected) {
        return;
    }
    m_wasConnected = connected;
    emitHook(connected ? TT_PLUGIN_HOOK_CONNECT : TT_PLUGIN_HOOK_DISCONNECT);
}

void Plugins::service()
{
    if (!m_plugins) {
        return;
    }
    // A dialog spins a nested event loop while the native wakeup stays ready.
    // The same guard as Macro keeps a second plugin job out of the first one's
    // modal dialog.
    if (m_notifier) {
        m_notifier->setEnabled(false);
    }
    tt_plugins_service(m_plugins, m_session->handle());
    if (m_notifier) {
        m_notifier->setEnabled(true);
    }
    m_session->poll();

    if (m_active && !busy()) {
        m_active = false;
        emit finished();
    }
}
