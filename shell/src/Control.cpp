// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Control.h"

#include <QSocketNotifier>

#include "MainWindow.h"
#include "Session.h"

namespace {

Control *self(void *user) { return static_cast<Control *>(user); }

} // namespace

// The ABI's callbacks. Free functions with C linkage rather than static
// members, because that is the type the header declares — and `static`, so
// nothing outside this file can reach them.
extern "C" {

static TtStatus cbRunMacro(void *user, const char *const *argv, const char **error)
{
    QStringList args;
    for (size_t i = 0; argv && argv[i]; i++) {
        args << QString::fromUtf8(argv[i]);
    }
    const TtStatus status = self(user)->runMacro(args);
    if (status != TT_OK) {
        *error = self(user)->lastError();
    }
    return status;
}

static bool cbMacroRunning(void *user) { return self(user)->macroRunning(); }

static int32_t cbMacroExitCode(void *user) { return self(user)->macroExitCode(); }

static void cbStopMacro(void *user) { self(user)->stopMacro(); }

static TtStatus cbConnect(void *user, const char *line, const char **error)
{
    const TtStatus status = self(user)->connectLine(QByteArray(line ? line : ""));
    if (status != TT_OK) {
        *error = self(user)->lastError();
    }
    return status;
}

static bool cbCloseWindow(void *user) { return self(user)->closeWindow(); }

static const char *cbTitle(void *user) { return self(user)->title(); }

} // extern "C"

Control::Control(Session *session, MainWindow *window, QObject *parent)
    : QObject(parent), m_session(session), m_window(window)
{
}

Control::~Control()
{
    // Disabled before the handle goes: `tt_ctl_free` closes the descriptor
    // this notifier is watching.
    delete m_notifier;
    m_notifier = nullptr;
    if (m_ctl) {
        tt_ctl_free(m_ctl);
        m_ctl = nullptr;
    }
}

bool Control::start(const QString &name, QString *outError)
{
    if (m_ctl) {
        return true;
    }
    const QByteArray utf8 = name.toUtf8();
    TtCtlHost host = {};
    host.user = this;
    host.run_macro = cbRunMacro;
    host.macro_running = cbMacroRunning;
    host.macro_exit_code = cbMacroExitCode;
    host.stop_macro = cbStopMacro;
    host.connect = cbConnect;
    host.close_window = cbCloseWindow;
    host.title = cbTitle;

    m_ctl = tt_ctl_start(name.isEmpty() ? nullptr : utf8.constData(), &host);
    if (!m_ctl) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    m_path = QString::fromUtf8(tt_ctl_path(m_ctl));

    const int fd = tt_ctl_poll_fd(m_ctl);
    if (fd >= 0) {
        m_notifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
        connect(m_notifier, &QSocketNotifier::activated, this, &Control::onServiceable);
    }
    return true;
}

void Control::onServiceable() { service(); }

void Control::service()
{
    if (!m_ctl) {
        return;
    }
    // A dialog spins a nested event loop, and a `QSocketNotifier` is level
    // triggered — so without this the loop inside an SSH host-key prompt would
    // call back in here and start servicing a second request inside the first.
    // The same re-entrancy `Macro::service` and `Session::m_sshWaiting` exist
    // for.
    if (m_notifier) {
        m_notifier->setEnabled(false);
    }
    tt_ctl_service(m_ctl, m_session->handle());
    if (m_notifier) {
        m_notifier->setEnabled(true);
    }

    // The requests that just ran changed the session — sent, connected,
    // disconnected — and its own descriptor said nothing about any of it.
    m_session->poll();
}

TtStatus Control::runMacro(const QStringList &argv)
{
    QString error;
    bool busy = false;
    if (m_window->runMacroFile(argv, &error, &busy)) {
        return TT_OK;
    }
    m_error = error.toUtf8();
    return busy ? TT_ERR_BUSY : TT_ERR_IO;
}

bool Control::macroRunning() const { return m_window->macroRunning(); }

int Control::macroExitCode() const { return m_window->macroExitCode(); }

void Control::stopMacro() { m_window->stopMacro(); }

TtStatus Control::connectLine(const QByteArray &line)
{
    QString error;
    if (m_window->openCommandLine(line, &error)) {
        return TT_OK;
    }
    m_error = error.toUtf8();
    return TT_ERR_IO;
}

bool Control::closeWindow()
{
    // `close()` and not `deleteLater()`: the window's own close handler is
    // what stops the macro, writes the settings back and tears this socket
    // down, and a request that skipped it would leave all three undone.
    //
    // It is queued rather than immediate because we are *inside*
    // `tt_ctl_service`, which is inside a call this object owns — deleting
    // the window from here would unwind through a stack frame belonging to
    // something that is about to be freed.
    QMetaObject::invokeMethod(m_window, "close", Qt::QueuedConnection);
    return true;
}

const char *Control::title()
{
    m_title = m_window->windowTitle().toUtf8();
    return m_title.constData();
}
