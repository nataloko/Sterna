// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "Session.h"

#include <QSocketNotifier>
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
    // Order matters: the notifier watches a descriptor the session owns, and
    // freeing the session closes it. A notifier left armed on a closed
    // descriptor is a warning at best and a busy loop at worst.
    delete m_notifier;
    m_notifier = nullptr;
    tt_session_free(m_session);
}

// --- reading the screen ------------------------------------------------------

int Session::cols() const { return static_cast<int>(tt_session_cols(m_session)); }
int Session::rows() const { return static_cast<int>(tt_session_rows(m_session)); }

const TtCell *Session::row(int y, size_t *outLen) const
{
    return tt_session_row(m_session, static_cast<size_t>(y), outLen);
}

TtCursor Session::cursor() const
{
    TtCursor c {};
    tt_session_cursor(m_session, &c);
    return c;
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

void Session::disconnectPort()
{
    // Drop the notifier first — `tt_session_disconnect` closes the descriptor
    // it is watching.
    delete m_notifier;
    m_notifier = nullptr;
    tt_session_disconnect(m_session);
    rearm();
    emit connectionChanged();
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

void Session::feed(const QByteArray &bytes)
{
    tt_session_feed(m_session, reinterpret_cast<const uint8_t *>(bytes.constData()),
                    static_cast<size_t>(bytes.size()));
    pumpAndDispatch(0);
}

// --- the loop ----------------------------------------------------------------

void Session::onReadable()
{
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
        }
    }

    if (dirty) {
        emit damaged();
    }
    // Rearm before announcing the disconnect: the descriptor is already gone
    // and whatever the notice wakes up must not find a live notifier on it.
    rearm();
    if (connectionEnded) {
        emit notice(tr("Disconnected"));
        emit connectionChanged();
    }
}

void Session::rearm()
{
    const int fd = tt_session_poll_fd(m_session);

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
