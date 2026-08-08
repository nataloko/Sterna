// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "Session.h"

#include <QSocketNotifier>
#include <QStringList>
#include <QTimer>

namespace {

/// How long to wait before retrying output the far end would not take. Short
/// enough that a device releasing CTS is not noticed as lag, long enough that
/// a genuinely wedged line does not spin.
constexpr int kRetryIntervalMs = 20;

/// A write may block for this long before it returns short. Well under a frame
/// on purpose: this runs on the UI thread, and flow control is entitled to
/// hold the line for as long as it likes.
constexpr uint32_t kWriteTimeoutMs = 10;

} // namespace

Session::Session(int cols, int rows, QObject *parent)
    : QObject(parent)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = static_cast<size_t>(cols);
    cfg.rows = static_cast<size_t>(rows);
    m_session = tt_session_new(&cfg);

    tt_session_set_write_timeout(m_session, kWriteTimeoutMs);

    m_retry = new QTimer(this);
    m_retry->setInterval(kRetryIntervalMs);
    connect(m_retry, &QTimer::timeout, this, &Session::onRetryPending);
}

Session::~Session()
{
    // Order matters: the notifier watches a descriptor the session or the
    // pending connection owns, and freeing either closes it. A notifier left
    // armed on a closed descriptor is a warning at best and a busy loop at
    // worst.
    delete m_notifier;
    m_notifier = nullptr;
    tt_ssh_connect_free(m_ssh);
    m_ssh = nullptr;
    tt_session_free(m_session);
}

// --- reading the screen ------------------------------------------------------

int Session::cols() const { return static_cast<int>(tt_session_cols(m_session)); }
int Session::rows() const { return static_cast<int>(tt_session_rows(m_session)); }

const TtCell *Session::row(int y, size_t *outLen) const
{
    return tt_session_row(m_session, static_cast<size_t>(y), outLen);
}

const TtCell *Session::line(quint64 n, size_t *outLen) const
{
    return tt_session_line(m_session, n, outLen);
}

quint64 Session::lineAt(int y) const
{
    return tt_session_line_at(m_session, static_cast<size_t>(qMax(0, y)));
}

quint64 Session::topLine() const { return tt_session_top_line(m_session); }

TtCursor Session::cursor() const
{
    TtCursor c {};
    tt_session_cursor(m_session, &c);
    return c;
}

int Session::cursorViewRow() const
{
    size_t y = 0;
    return tt_session_cursor_view_row(m_session, &y) ? static_cast<int>(y) : -1;
}

int Session::scrollbackLen() const
{
    return static_cast<int>(tt_session_scrollback_len(m_session));
}

int Session::viewOffset() const
{
    return static_cast<int>(tt_session_view_offset(m_session));
}

void Session::setViewOffset(int offset)
{
    tt_session_set_view_offset(m_session, static_cast<size_t>(qMax(0, offset)));
}

bool Session::reverseVideo() const { return tt_session_reverse_video(m_session); }

TtTracking Session::mouseTracking() const { return tt_session_mouse_tracking(m_session); }

QString Session::title() const { return m_title; }

bool Session::backspaceSendsBs() const
{
    return tt_session_backspace_sends_bs(m_session);
}

// --- the connection ----------------------------------------------------------

bool Session::isConnected() const { return tt_session_is_connected(m_session); }

bool Session::supportsBreak() const { return tt_session_supports_break(m_session); }

QString Session::describe() const
{
    const char *d = tt_session_describe(m_session);
    return d ? QString::fromUtf8(d) : QString();
}

bool Session::connectSerial(const QString &path, const TtSerialParams &params,
                            QString *outError)
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_connect_serial(m_session, utf8.constData(), &params) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    rearm();
    emit connectionChanged();
    return true;
}

bool Session::connectTelnet(const QString &host, quint16 port,
                            const TtTelnetParams &params, QString *outError)
{
    cancelSsh();
    const QByteArray utf8 = host.toUtf8();
    if (tt_session_connect_telnet(m_session, utf8.constData(), port, &params) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    rearm();
    emit connectionChanged();
    return true;
}

bool Session::connectPty(const QStringList &argv, QString *outError)
{
    cancelSsh();

    // The ABI borrows these, so both the bytes and the pointer array have to
    // outlive the call — hence two vectors rather than a temporary each.
    QVector<QByteArray> owned;
    QVector<const char *> pointers;
    owned.reserve(argv.size());
    pointers.reserve(argv.size());
    for (const QString &arg : argv) {
        owned.append(arg.toUtf8());
    }
    for (const QByteArray &arg : owned) {
        pointers.append(arg.constData());
    }

    TtPtyParams params;
    tt_pty_params_default(&params);
    if (!pointers.isEmpty()) {
        params.argv = pointers.constData();
        params.argc = static_cast<size_t>(pointers.size());
    }

    if (tt_session_connect_pty(m_session, &params) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    rearm();
    emit connectionChanged();
    return true;
}

QString Session::closeNote() const
{
    const char *note = tt_session_close_note(const_cast<TtSession *>(m_session));
    return note ? QString::fromUtf8(note) : QString();
}

void Session::disconnectPort()
{
    // A connection still being set up is a connection: "Disconnect" while a
    // password dialog is open has to stop the attempt, not do nothing.
    cancelSsh();
    // Drop the notifier first — `tt_session_disconnect` closes the descriptor
    // it is watching.
    delete m_notifier;
    m_notifier = nullptr;
    tt_session_disconnect(m_session);
    rearm();
    emit connectionChanged();
}

// --- ssh ---------------------------------------------------------------------

bool Session::startSsh(const TtSshParams &params, QString *outError)
{
    cancelSsh();
    m_ssh = tt_ssh_connect(&params);
    if (!m_ssh) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    // The descriptor moves from the connection to the session at the moment
    // the shell starts, and it is the *same* descriptor — so `rearm` keeps
    // the notifier it already has rather than swapping one in mid-burst.
    rearm();
    // Nothing will have happened yet, but polling once costs a function call
    // and covers the case where it already has.
    pollSsh();
    return true;
}

void Session::cancelSsh()
{
    if (!m_ssh) {
        return;
    }
    // Freeing the handle stops the attempt, and the descriptor it owns goes
    // with it — so the notifier has to go first.
    delete m_notifier;
    m_notifier = nullptr;
    endSsh();
    rearm();
}

void Session::endSsh()
{
    tt_ssh_connect_free(m_ssh);
    m_ssh = nullptr;
    m_sshWaiting = false;
}

void Session::answerHostKey(int decision)
{
    if (!m_ssh) {
        return;
    }
    tt_ssh_connect_answer_host_key(m_ssh, decision);
    m_sshWaiting = false;
    pollSsh();
}

void Session::answerAuth(const QStringList &answers)
{
    if (!m_ssh) {
        return;
    }
    QVector<QByteArray> utf8;
    QVector<const char *> ptrs;
    utf8.reserve(answers.size());
    ptrs.reserve(answers.size());
    for (const QString &a : answers) {
        utf8.append(a.toUtf8());
    }
    // Two passes: `utf8` must stop reallocating before any pointer into it is
    // taken, or every earlier one dangles.
    for (const QByteArray &a : utf8) {
        ptrs.append(a.constData());
    }
    tt_ssh_connect_answer_auth(m_ssh, ptrs.constData(),
                               static_cast<size_t>(ptrs.size()));
    m_sshWaiting = false;
    pollSsh();
}

void Session::pollSsh()
{
    // A dialog spins a nested event loop, so the notifier fires again while
    // one is open. Re-entering here would invalidate the strings that dialog
    // is showing and ask the same question twice.
    if (!m_ssh || m_sshWaiting) {
        return;
    }
    for (;;) {
        const TtSshStep step = tt_ssh_connect_poll(m_ssh, m_session);
        if (step == TT_SSH_WORKING) {
            return;
        }
        if (step == TT_SSH_HOST_KEY) {
            const TtSshHostKeyPrompt *p = tt_ssh_connect_host_key(m_ssh);
            if (!p) {
                return;
            }
            // Copied out, because everything the ABI handed back dies at the
            // next poll and the dialog outlives it.
            HostKeyRequest r;
            r.host = QString::fromUtf8(p->host);
            r.port = p->port;
            r.algorithm = QString::fromUtf8(p->algorithm);
            r.fingerprint = QString::fromUtf8(p->fingerprint);
            r.verdict = p->verdict;
            if (p->recorded_at) {
                r.recordedAt = QString::fromUtf8(p->recorded_at);
            }
            if (p->recorded_fingerprint) {
                r.recordedFingerprint = QString::fromUtf8(p->recorded_fingerprint);
            }
            if (p->also_known) {
                r.alsoKnown = QString::fromUtf8(p->also_known);
            }
            m_sshWaiting = true;
            emit sshHostKeyWanted(r);
            return;
        }
        if (step == TT_SSH_AUTH) {
            const TtSshAuthPrompt *p = tt_ssh_connect_auth(m_ssh);
            if (!p) {
                return;
            }
            AuthRequest r;
            r.kind = p->kind;
            r.name = QString::fromUtf8(p->name);
            r.instruction = QString::fromUtf8(p->instruction);
            if (p->path) {
                r.path = QString::fromUtf8(p->path);
            }
            for (size_t i = 0; i < p->prompt_count; i++) {
                r.lines.append({QString::fromUtf8(p->prompts[i].text),
                                p->prompts[i].echo});
            }
            m_sshWaiting = true;
            emit sshAuthWanted(r);
            return;
        }
        if (step == TT_SSH_READY) {
            endSsh();
            rearm();
            emit connectionChanged();
            // Whatever the far end sent while the handle was being freed is
            // still waiting, and the descriptor may not fire again for it.
            pumpAndDispatch(0);
            return;
        }
        // TT_SSH_FAILED, or anything a future core adds that this does not
        // know: stop, and say why.
        const QString error = QString::fromUtf8(tt_last_error());
        endSsh();
        delete m_notifier;
        m_notifier = nullptr;
        rearm();
        emit sshFailed(error);
        emit connectionChanged();
        return;
    }
}

// --- input -------------------------------------------------------------------

void Session::sendKey(TtKey key)
{
    if (tt_session_send_key(m_session, key, nullptr) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    rearm();
}

void Session::sendText(const QString &text)
{
    if (text.isEmpty()) {
        return;
    }
    const QByteArray utf8 = text.toUtf8();
    if (tt_session_send_text(m_session, utf8.constData(),
                             static_cast<size_t>(utf8.size())) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    rearm();
}

void Session::paste(const QString &text)
{
    if (text.isEmpty()) {
        return;
    }
    const QByteArray utf8 = text.toUtf8();
    if (tt_session_paste(m_session, utf8.constData(),
                         static_cast<size_t>(utf8.size())) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    rearm();
}

bool Session::mouse(TtMouseEvent event, uint8_t button, int px, int py,
                    TtModifiers mods)
{
    bool consumed = false;
    tt_session_mouse(m_session, event, button, px, py, mods, &consumed);
    rearm();
    return consumed;
}

void Session::focus(bool focused)
{
    tt_session_focus(m_session, focused);
    rearm();
}

void Session::resize(int cols, int rows)
{
    if (cols <= 0 || rows <= 0) {
        return;
    }
    tt_session_resize(m_session, static_cast<size_t>(cols), static_cast<size_t>(rows));
    emit damaged();
}

void Session::setCellPixels(int w, int h)
{
    tt_session_set_cell_pixels(m_session, w, h);
}

void Session::sendBreak(int ms)
{
    if (tt_session_send_break(m_session, static_cast<uint32_t>(ms)) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
}

bool Session::startLog(const QString &path, const TtLogOptions &options,
                       QString *outError)
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_log_start(m_session, utf8.constData(), &options) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    // Emitted here rather than left to the caller: the state changed, and a
    // window that only refreshes when *it* was the one to ask would miss a log
    // started from anywhere else — including the failure path, which stops one
    // nobody asked to stop.
    emit logStateChanged();
    return true;
}

void Session::stopLog()
{
    const bool was = isLogging();
    tt_session_log_stop(m_session);
    if (was) {
        emit logStateChanged();
    }
}

bool Session::isLogging() const
{
    return tt_session_log_path(const_cast<TtSession *>(m_session)) != nullptr;
}

QString Session::logPath() const
{
    const char *p = tt_session_log_path(const_cast<TtSession *>(m_session));
    return p ? QString::fromUtf8(p) : QString();
}

quint64 Session::logBytes() const
{
    return tt_session_log_bytes(m_session);
}

void Session::feed(const QByteArray &bytes)
{
    tt_session_feed(m_session, reinterpret_cast<const uint8_t *>(bytes.constData()),
                    static_cast<size_t>(bytes.size()));
    pumpAndDispatch(0);
}

// --- the loop ----------------------------------------------------------------

void Session::onReadable()
{
    if (m_ssh) {
        pollSsh();
        return;
    }

    // A budget of zero reads exactly once. Anything larger would let the pump
    // loop round to a second read, and that one blocks for the transport's
    // read timeout — on the UI thread. The descriptor stays readable while
    // there is more, so a burst arrives over several turns of the event loop
    // and the window keeps painting through it.
    pumpAndDispatch(0);
}

void Session::onRetryPending()
{
    pumpAndDispatch(0);
}

void Session::pumpAndDispatch(uint32_t budgetMs)
{
    if (tt_session_pump(m_session, budgetMs, nullptr) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }

    const TtEvent *events = nullptr;
    const size_t n = tt_session_drain_events(m_session, &events);

    bool dirty = false;
    bool connectionEnded = false;
    for (size_t i = 0; i < n; i++) {
        switch (events[i].kind) {
        case TT_EVENT_KIND_DAMAGE:
            // Coalesced rather than emitted per event: one pump can produce
            // several, and each one would otherwise cost a signal for a
            // repaint Qt is already going to merge.
            dirty = true;
            break;
        case TT_EVENT_KIND_TITLE:
            m_title = QString::fromUtf8(events[i].text ? events[i].text : "");
            emit titleChanged(m_title);
            break;
        case TT_EVENT_KIND_BREAK:
            emit notice(tr("Break received"));
            break;
        case TT_EVENT_KIND_BAD_BYTE:
            emit notice(tr("Framing or parity error (0x%1)")
                            .arg(events[i].byte, 2, 16, QLatin1Char('0')));
            break;
        case TT_EVENT_KIND_DISCONNECTED:
            connectionEnded = true;
            break;
        case TT_EVENT_KIND_RESIZE:
            // Passed on rather than acted on. The core does not resize itself
            // either — the window owns its size, and a grid that changed
            // underneath the painter would leave it drawing the wrong number
            // of cells.
            emit remoteResize(events[i].cols, events[i].rows);
            break;
        case TT_EVENT_KIND_LOG_FAILED:
            // The log is already closed, so this is the one chance to say so.
            // A window that kept claiming to be logging would let someone walk
            // away from a capture that stopped an hour ago.
            emit notice(tr("Logging stopped: %1")
                            .arg(QString::fromUtf8(events[i].text ? events[i].text : "")));
            emit logStateChanged();
            break;
        }
    }

    if (dirty) {
        emit damaged();
    }
    // Rearm before announcing the disconnect: the descriptor is already gone
    // and whatever the notice wakes up must not find a live notifier on it.
    rearm();
    if (connectionEnded) {
        // A local shell knows why it ended; a serial line does not. "bash
        // exited with status 1" is the difference between a window that
        // explains itself and one that just goes quiet.
        const QString note = closeNote();
        emit notice(note.isEmpty() ? tr("Disconnected") : note);
        emit connectionChanged();
    }
}

void Session::rearm()
{
    // While connecting, the connection owns the descriptor; afterwards the
    // session does, and it is the same one. Asking in this order means the
    // handover changes nothing the notifier can see.
    const int fd = m_ssh ? tt_ssh_connect_poll_fd(m_ssh)
                         : tt_session_poll_fd(m_session);

    if (m_notifier && m_notifier->socket() != fd) {
        // Deleted rather than re-pointed: a QSocketNotifier's descriptor is
        // fixed for its lifetime.
        m_notifier->setEnabled(false);
        delete m_notifier;
        m_notifier = nullptr;
    }
    if (fd >= 0 && !m_notifier) {
        m_notifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
        connect(m_notifier, &QSocketNotifier::activated, this, &Session::onReadable);
    }

    const bool stuck = tt_session_pending_out(m_session) > 0;
    if (stuck && !m_retry->isActive()) {
        m_retry->start();
    } else if (!stuck && m_retry->isActive()) {
        m_retry->stop();
    }
}
