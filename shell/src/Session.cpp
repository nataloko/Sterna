// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Session.h"

#include "Highlights.h"

#include <QClipboard>
#include <QGuiApplication>
#ifdef Q_OS_WIN
#include <QWinEventNotifier>
#else
#include <QSocketNotifier>
#endif
#include <QStringList>
#include <QTimer>
#include <QVector>

#include <optional>

namespace {

/// How long to wait before retrying output the far end would not take. Short
/// enough that a device releasing CTS is not noticed as lag, long enough that
/// a genuinely wedged line does not spin.
constexpr int kRetryIntervalMs = 20;

/// A write may block for this long before it returns short. Well under a frame
/// on purpose: this runs on the UI thread, and flow control is entitled to
/// hold the line for as long as it likes.
constexpr uint32_t kWriteTimeoutMs = 10;

/// How often the transport is given a wakeup the wire did not provide.
///
/// Only telnet's keepalive wants one, and it counts in whole seconds, so a
/// second is as fine as this needs to be — upstream's own thread wakes ten
/// times as often to answer the same question (`telnet.c:917`). One idle
/// timer for the life of the window is the price; the alternative is a
/// `TelKeepAliveInterval` that does nothing, because an idle socket produces
/// no descriptor wakeup and that is precisely the socket it exists for.
constexpr int kTickIntervalMs = 1000;

QStringList copiedStrings(const char *const *values)
{
    QStringList out;
    for (size_t i = 0; values && values[i]; i++) {
        out.append(QString::fromUtf8(values[i]));
    }
    return out;
}

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

    m_xferTimer = new QTimer(this);
    m_xferTimer->setSingleShot(true);
    connect(m_xferTimer, &QTimer::timeout, this, &Session::onTransferDeadline);

    m_tick = new QTimer(this);
    m_tick->setInterval(kTickIntervalMs);
    // Coarse on purpose: this must never be the reason a laptop wakes up, and
    // nothing behind it needs better than a second.
    m_tick->setTimerType(Qt::VeryCoarseTimer);
    connect(m_tick, &QTimer::timeout, this, [this] { tt_session_tick(m_session); });
    m_tick->start();

    // Ahead of anything the window connects, so that a slot reacting to a
    // settings change already sees the title those settings imply.
    connect(this, &Session::settingsChanged, this, &Session::refreshTitle);
    m_title = QString::fromUtf8(tt_session_title(m_session));
}

Session::~Session()
{
    // Order matters: the notifier watches a native object the session or the
    // pending connection owns, and freeing either closes it. A notifier left
    // armed on a closed object is a warning at best and a busy loop at worst.
    delete m_notifier;
    m_notifier = nullptr;
    tt_ssh_connect_free(m_ssh);
    m_ssh = nullptr;
    tt_session_free(m_session);
}

// --- reading the screen ------------------------------------------------------

int Session::cols() const { return static_cast<int>(tt_session_cols(m_session)); }
int Session::rows() const { return static_cast<int>(tt_session_rows(m_session)); }

void Session::scrollRegion(size_t *top, size_t *bottom) const
{
    tt_session_scroll_region(m_session, top, bottom);
}

const TtCell *Session::row(int y, size_t *outLen) const
{
    return tt_session_row(m_session, static_cast<size_t>(y), outLen);
}

const TtCell *Session::line(quint64 n, size_t *outLen) const
{
    return tt_session_line(m_session, n, outLen);
}

size_t Session::sixelImages(const TtSixelImage **out)
{
    return tt_session_sixel_images(m_session, out);
}

size_t Session::rowHighlights(int y, const TtHighlightSpan **out)
{
    size_t len = 0;
    const TtHighlightSpan *spans =
        tt_session_row_highlights(m_session, static_cast<size_t>(qMax(0, y)), &len);
    if (out) {
        *out = spans;
    }
    return spans ? len : 0;
}

void Session::setHighlights(const QVector<QuickHighlight> &rules)
{
    TtHighlights *list = buildHighlightList(rules);
    if (!list) {
        return;
    }
    // The core keeps its own compiled copy, so the list dies here.
    tt_session_set_highlights(m_session, list);
    tt_highlights_free(list);
    // Installing the matcher queues damage so existing text is repainted.
    // Drain it here: leaving it behind hands this call's repaint to the next
    // input event, the same latency bug `setSetting` and the send paths avoid.
    dispatch();
}

QString Session::highlightProblems() const
{
    const char *problems = tt_session_highlight_problems(m_session);
    return problems ? QString::fromUtf8(problems) : QString();
}

quint64 Session::lineAt(int y) const
{
    return tt_session_line_at(m_session, static_cast<size_t>(qMax(0, y)));
}

QString Session::urlAt(quint64 line, int x)
{
    const char *url = tt_session_url_at(m_session, line, static_cast<size_t>(qMax(0, x)));
    return url ? QString::fromUtf8(url) : QString();
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

bool Session::paletteRgb(uint32_t index, uint8_t *r, uint8_t *g, uint8_t *b) const
{
    return tt_session_palette_rgb(m_session, index, r, g, b);
}

bool Session::colorRgb(TtColorPair pair, bool background, uint8_t *r, uint8_t *g,
                       uint8_t *b) const
{
    return tt_session_color_rgb(m_session, pair, background, r, g, b);
}

TtTracking Session::mouseTracking() const { return tt_session_mouse_tracking(m_session); }

bool Session::wheelToCursor(TtModifiers mods) const
{
    return tt_session_wheel_to_cursor(m_session, mods);
}

QString Session::title() const { return m_title; }

void Session::refreshTitle()
{
    const QString title = QString::fromUtf8(tt_session_title(m_session));
    if (title != m_title) {
        m_title = title;
        emit titleChanged(m_title);
    }
}

bool Session::backspaceSendsBs() const
{
    return tt_session_backspace_sends_bs(m_session);
}

// --- the connection ----------------------------------------------------------

bool Session::isConnected() const { return tt_session_is_connected(m_session); }

bool Session::canDuplicate() const
{
    return isConnected() && m_duplicateKind != DuplicateKind::None;
}

bool Session::duplicateInto(Session *destination, QString *outError) const
{
    if (!destination || !canDuplicate()) {
        if (outError) {
            *outError = tr("Only a live SSH or telnet session can be duplicated.");
        }
        return false;
    }

    if (m_duplicateKind == DuplicateKind::Telnet) {
        TtTelnetParams params = m_duplicateTelnet.params;
        const QByteArray term = m_duplicateTelnet.termType.toUtf8();
        const QByteArray log = m_duplicateTelnet.logPath.toUtf8();
        params.term_type = m_duplicateTelnet.hasTermType ? term.constData() : nullptr;
        params.log_path = m_duplicateTelnet.hasLogPath ? log.constData() : nullptr;
        return destination->connectTelnet(m_duplicateTelnet.host,
                                          m_duplicateTelnet.port, params,
                                          outError);
    }

    TtSshParams params = m_duplicateSsh.params;
    const QByteArray host = m_duplicateSsh.host.toUtf8();
    const QByteArray user = m_duplicateSsh.user.toUtf8();
    const QByteArray term = m_duplicateSsh.term.toUtf8();
    params.host = host.constData();
    params.user = m_duplicateSsh.hasUser ? user.constData() : nullptr;
    params.term = m_duplicateSsh.hasTerm ? term.constData() : nullptr;

    QVector<QByteArray> identityBytes;
    QVector<const char *> identityPtrs;
    identityBytes.reserve(m_duplicateSsh.identities.size());
    identityPtrs.reserve(m_duplicateSsh.identities.size() + 1);
    for (const QString &path : m_duplicateSsh.identities) {
        identityBytes.append(path.toUtf8());
    }
    for (const QByteArray &path : identityBytes) {
        identityPtrs.append(path.constData());
    }
    identityPtrs.append(nullptr);
    params.identities = m_duplicateSsh.hasIdentities ? identityPtrs.constData()
                                                     : nullptr;

    QVector<QByteArray> knownBytes;
    QVector<const char *> knownPtrs;
    knownBytes.reserve(m_duplicateSsh.knownHosts.size());
    knownPtrs.reserve(m_duplicateSsh.knownHosts.size() + 1);
    for (const QString &path : m_duplicateSsh.knownHosts) {
        knownBytes.append(path.toUtf8());
    }
    for (const QByteArray &path : knownBytes) {
        knownPtrs.append(path.constData());
    }
    knownPtrs.append(nullptr);
    params.known_hosts = m_duplicateSsh.hasKnownHosts ? knownPtrs.constData()
                                                     : nullptr;
    return destination->startSsh(params, outError);
}

bool Session::supportsBreak() const { return tt_session_supports_break(m_session); }

TtLinkKind Session::linkKind() const { return tt_session_link_kind(m_session); }

QString Session::describe() const
{
    const char *d = tt_session_describe(m_session);
    return d ? QString::fromUtf8(d) : QString();
}

quint32 Session::serialBaud() const { return tt_session_serial_baud(m_session); }

/// What a log name's `&h`/`&p` and a `TitleFormat` endpoint expand to. Set
/// from each of the four connect paths rather than from the window, so that a
/// connection opened by the control socket or by a macro names itself the
/// same way one opened from the dialog does.
void Session::setConnectionName(const QString &host, quint16 port)
{
    m_connectionHost = host;
    m_connectionPort = port;
    const QByteArray utf8 = host.toUtf8();
    tt_session_set_connection_name(m_session, host.isEmpty() ? nullptr : utf8.constData(),
                                   port);
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
    // Upstream's `&h` on a serial line is `COM<n>`; the device's own name is
    // the counterpart, and the leading `/dev/` would be swept to underscores.
    setConnectionName(path.section(QLatin1Char('/'), -1), 0);
    m_duplicateKind = DuplicateKind::None;
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
    setConnectionName(host, port);
    rememberTelnet(host, port, params);
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
    // Nothing: upstream's escape has no arm for a local shell, so `&h` in a
    // log name expands to nothing rather than to the shell's path.
    setConnectionName(QString(), 0);
    m_duplicateKind = DuplicateKind::None;
    rearm();
    emit connectionChanged();
    return true;
}

QString Session::serialPath() const
{
    const char *path = tt_session_serial_path(m_session);
    return path ? QString::fromUtf8(path) : QString();
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
    // A deliberate disconnect still takes the clear-screen outcome branch,
    // but never AutoWinClose: that setting is for a line which ended by itself.
    // The caller already knows the connection changed, so the core does not
    // also manufacture a generic Disconnected notice for this path.
    pumpAndDispatch(0);
    emit connectionChanged();
}

// --- ssh ---------------------------------------------------------------------

bool Session::startSsh(const TtSshParams &params, QString *outError)
{
    cancelSsh();
    m_ssh = tt_ssh_connect_for_session(&params, m_session);
    if (!m_ssh) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    setConnectionName(QString::fromUtf8(params.host), params.port);
    rememberSsh(params);
    // The descriptor moves from the connection to the session at the moment
    // the shell starts, and it is the *same* descriptor — so `rearm` keeps
    // the notifier it already has rather than swapping one in mid-burst.
    rearm();
    emit connectionChanged();
    // Nothing will have happened yet, but polling once costs a function call
    // and covers the case where it already has.
    pollSsh();
    return true;
}

void Session::rememberTelnet(const QString &host, quint16 port,
                             const TtTelnetParams &params)
{
    m_duplicateTelnet = TelnetDuplicate{};
    m_duplicateTelnet.host = host;
    m_duplicateTelnet.port = port;
    m_duplicateTelnet.params = params;
    m_duplicateTelnet.hasTermType = params.term_type != nullptr;
    m_duplicateTelnet.hasLogPath = params.log_path != nullptr;
    if (params.term_type) {
        m_duplicateTelnet.termType = QString::fromUtf8(params.term_type);
    }
    if (params.log_path) {
        m_duplicateTelnet.logPath = QString::fromUtf8(params.log_path);
    }
    m_duplicateTelnet.params.term_type = nullptr;
    m_duplicateTelnet.params.log_path = nullptr;
    m_duplicateKind = DuplicateKind::Telnet;
}

void Session::rememberSsh(const TtSshParams &params)
{
    m_duplicateSsh = SshDuplicate{};
    m_duplicateSsh.params = params;
    m_duplicateSsh.host = QString::fromUtf8(params.host);
    m_duplicateSsh.hasUser = params.user != nullptr;
    m_duplicateSsh.hasTerm = params.term != nullptr;
    m_duplicateSsh.hasIdentities = params.identities != nullptr;
    m_duplicateSsh.hasKnownHosts = params.known_hosts != nullptr;
    if (params.user) {
        m_duplicateSsh.user = QString::fromUtf8(params.user);
    }
    if (params.term) {
        m_duplicateSsh.term = QString::fromUtf8(params.term);
    }
    m_duplicateSsh.identities = copiedStrings(params.identities);
    m_duplicateSsh.knownHosts = copiedStrings(params.known_hosts);
    m_duplicateSsh.params.host = nullptr;
    m_duplicateSsh.params.user = nullptr;
    m_duplicateSsh.params.term = nullptr;
    m_duplicateSsh.params.identities = nullptr;
    m_duplicateSsh.params.known_hosts = nullptr;
    m_duplicateKind = DuplicateKind::Ssh;
}

void Session::cancelSsh()
{
    if (!m_ssh) {
        return;
    }
    // Freeing the connection stops the attempt, and its native wakeup goes
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
        // The failed connection is the last owner of its wakeup, so stop Qt
        // watching it before `endSsh` closes it.
        delete m_notifier;
        m_notifier = nullptr;
        endSsh();
        rearm();
        emit sshFailed(error);
        emit connectionChanged();
        return;
    }
}

// --- input -------------------------------------------------------------------

// Every one of these ends in `dispatch` rather than `rearm`, because sending
// is not only sending: local echo puts the same bytes through the receive
// parser, so the screen has changed by the time the call returns and the
// events saying so are sitting in the core. See `Session::dispatch`.

void Session::sendKey(TtKey key)
{
    if (tt_session_send_key(m_session, key, nullptr) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    dispatch();
}

bool Session::loadKeyMap(const QString &path, QVector<quint16> *duplicates,
                         QString *outError)
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_key_map_load(m_session, utf8.constData()) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    if (duplicates) {
        duplicates->clear();
        const size_t count = tt_session_key_map_duplicate_count(m_session);
        duplicates->reserve(static_cast<qsizetype>(count));
        for (size_t i = 0; i < count; i++) {
            duplicates->append(tt_session_key_map_duplicate(m_session, i));
        }
    }
    return true;
}

KeyCodeAction Session::sendKeyCode(quint16 scan)
{
    TtKeyCodeResult result {};
    if (tt_session_send_key_code(m_session, scan, &result) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
        dispatch();
        return {};
    }
    KeyCodeAction out;
    out.kind = result.kind;
    out.value = result.value;
    if (result.text) {
        out.text = QString::fromUtf8(result.text);
    }
    dispatch();
    return out;
}

KeyCodeAction Session::runQuickButton(TtQuickButtonKind kind, const QString &value)
{
    TtKeyCodeResult result {};
    const QByteArray utf8 = value.toUtf8();
    if (tt_session_run_quick_button(m_session, kind, utf8.constData(), &result)
        != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
        dispatch();
        return {};
    }
    KeyCodeAction out;
    out.kind = result.kind;
    out.value = result.value;
    if (result.text) {
        out.text = QString::fromUtf8(result.text);
    }
    dispatch();
    return out;
}

bool Session::keyCodeBound(quint16 scan) const
{
    return tt_session_key_code_bound(m_session, scan);
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
    dispatch();
}

void Session::sendEditedLine(const QString &text)
{
    const QByteArray utf8 = text.toUtf8();
    if (tt_session_send_edited_line(m_session, utf8.constData(),
                                    static_cast<size_t>(utf8.size()))
        != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    // The forced echo queued ordinary damage. Drain it now: an idle line may
    // never wake the transport, and leaving it queued makes the next unrelated
    // input announce a stale second repaint.
    dispatch();
}

void Session::sendBytes(const QByteArray &bytes)
{
    if (bytes.isEmpty()) {
        return;
    }
    if (tt_session_send_bytes(
            m_session, reinterpret_cast<const uint8_t *>(bytes.constData()),
            static_cast<size_t>(bytes.size()))
        != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    dispatch();
}

void Session::paste(const QString &text, bool addCr)
{
    // An empty clipboard is where upstream gives up (`clipboar.c:236`), before
    // it would have appended the CR — so `Paste<CR>` over nothing sends
    // nothing rather than a bare Return.
    if (text.isEmpty()) {
        return;
    }
    const QByteArray utf8 = text.toUtf8();
    if (tt_session_paste(m_session, utf8.constData(),
                         static_cast<size_t>(utf8.size()), addCr) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
    dispatch();
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
    dispatch();
}

void Session::setCellPixels(int w, int h)
{
    tt_session_set_cell_pixels(m_session, w, h);
}

void Session::setWindowMetrics(const TtWindowMetrics &metrics)
{
    m_windowMetrics = metrics;
    tt_session_set_window_metrics(m_session, &metrics);
}

void Session::sendBreak()
{
    if (tt_session_send_break(m_session) != TT_OK) {
        emit notice(QString::fromUtf8(tt_last_error()));
    }
}

QString Session::logName(const QString &requested) const
{
    const QByteArray utf8 = requested.toUtf8();
    // `const_cast` for the same reason `logPath` does it: the ABI caches the
    // string it hands back, so it takes a mutable session while changing no
    // observable state.
    const char *name = tt_session_log_name(const_cast<TtSession *>(m_session),
                                           requested.isEmpty() ? nullptr : utf8.constData());
    return name ? QString::fromUtf8(name) : QString();
}

TtLogOptions Session::logDefaults() const
{
    TtLogOptions options = {};
    tt_log_options_default(&options);
    tt_session_log_defaults(m_session, &options);
    return options;
}

bool Session::startLog(const QString &path, QString *outError, const TtLogOptions *options)
{
    const QByteArray utf8 = path.toUtf8();
    // Null options: "however the settings say", which is what the command
    // line, `LogAutoStart` and a resumed session all want. The dialog is the
    // one caller that passes a struct.
    if (tt_session_log_start(m_session, utf8.constData(), options) != TT_OK) {
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

void Session::pauseLog(bool paused)
{
    if (logPaused() == paused) {
        return;
    }
    tt_session_log_pause(m_session, paused);
    // The same reason `startLog` emits: pausing is a state change the whole
    // window shows — the menu item's tick, the indicator's colour — and a
    // caller that refreshed only its own corner would leave the other stale.
    emit logStateChanged();
}

bool Session::logPaused() const
{
    return tt_session_log_paused(m_session);
}

// --- file transfer -----------------------------------------------------------

bool Session::sendFiles(const TtXferJob &job, const QStringList &paths, QString *outError)
{
    QVector<QByteArray> utf8;
    QVector<const char *> argv;
    utf8.reserve(paths.size());
    argv.reserve(paths.size());
    for (const QString &p : paths) {
        utf8.append(p.toUtf8());
        argv.append(utf8.last().constData());
    }
    if (tt_session_send_files(m_session, &job, argv.constData(),
                              static_cast<size_t>(argv.size())) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    // Straight into the loop: a sending protocol makes its first move on its
    // own, with nothing arriving to wake us.
    pumpAndDispatch(0);
    return true;
}

bool Session::receiveFiles(const TtXferJob &job, const QString &dir, const QString &name,
                           QString *outError)
{
    const QByteArray dirUtf8 = dir.toUtf8();
    const QByteArray nameUtf8 = name.toUtf8();
    if (tt_session_receive_files(m_session, &job, dirUtf8.constData(),
                                 name.isEmpty() ? nullptr : nameUtf8.constData()) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    pumpAndDispatch(0);
    return true;
}

void Session::cancelTransfer()
{
    tt_session_cancel_transfer(m_session);
    pumpAndDispatch(0);
}

bool Session::isTransferring() const
{
    return tt_session_transfer_deadline_ms(m_session) != -2;
}

TransferProgress Session::transferProgress() const
{
    TransferProgress out;
    TtTransferStatus st;
    // const_cast: the ABI caches the two strings it hands back, so it takes a
    // mutable session. Nothing observable changes — the same argument as
    // `logPath` above.
    if (!tt_session_transfer_status(const_cast<TtSession *>(m_session), &st)) {
        return out;
    }
    out.protocol = QString::fromUtf8(st.protocol ? st.protocol : "");
    out.file = QString::fromUtf8(st.file ? st.file : "");
    out.sending = st.sending;
    out.bytes = st.bytes;
    out.packets = st.packets;
    out.done = st.done;
    out.total = st.total;
    out.percent = st.percent;
    out.elapsedMs = st.elapsed_ms;
    return out;
}

void Session::onTransferDeadline()
{
    // The protocol's own retry clock ran out. Pumping is what fires it: the
    // core checks the deadline as part of driving the transfer.
    pumpAndDispatch(0);
}

void Session::feed(const QByteArray &bytes)
{
    tt_session_feed(m_session, reinterpret_cast<const uint8_t *>(bytes.constData()),
                    static_cast<size_t>(bytes.size()));
    pumpAndDispatch(0);
}

void Session::clearScreen()
{
    tt_session_clear_screen(m_session);
    dispatch();
}

void Session::clearBuffer()
{
    tt_session_clear_buffer(m_session);
    dispatch();
}

bool Session::cycleDebugMode()
{
    return tt_session_cycle_debug_mode(m_session);
}

void Session::unlinkMacro() { tt_session_unlink_macro(m_session); }

void Session::poll() { pumpAndDispatch(0); }

// --- settings ----------------------------------------------------------------

QString Session::setting(const QString &name) const
{
    const QByteArray utf8 = name.toUtf8();
    // `tt_session_setting` caches the string it hands back, so it takes a
    // mutable session; nothing observable changes. Same const_cast as the log
    // accessors above, and for the same reason.
    const char *value = tt_session_setting(const_cast<TtSession *>(m_session),
                                           utf8.constData());
    return value ? QString::fromUtf8(value) : QString();
}

QString Session::wordDelimiters() const
{
    // Cached on the core side and valid until the next call, like every other
    // borrowed string here.
    return QString::fromUtf8(tt_session_word_delimiters(const_cast<TtSession *>(m_session)));
}

bool Session::setSetting(const QString &name, const QString &value, QString *outError)
{
    const QByteArray n = name.toUtf8();
    const QByteArray v = value.toUtf8();
    if (tt_session_set_setting(m_session, n.constData(), v.constData()) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    emit settingsChanged();
    dispatch();
    return true;
}

bool Session::loadSettings(const QString &path, QString *outError)
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_settings_load(m_session, utf8.constData()) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    emit settingsChanged();
    dispatch();
    return true;
}

bool Session::copySettingsFrom(const Session &source, QString *outError)
{
    if (tt_session_copy_settings(m_session, source.m_session) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    emit settingsChanged();
    dispatch();
    return true;
}

bool Session::applyCommandLine(TtCmdLine *cmd, QString *outError)
{
    if (tt_cmdline_apply(cmd, m_session) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    emit settingsChanged();
    dispatch();
    return true;
}

TtStartupKind Session::startup(TtCmdLine *cmd, TtStartup *out)
{
    return tt_cmdline_startup(cmd, m_session, out);
}

bool Session::saveSettings(const QString &path, QString *outError) const
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_settings_save(m_session, utf8.constData()) != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    return true;
}

bool Session::saveSettingsForWindow(const QString &path, int x, int y,
                                    bool positionValid, QString *outError) const
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_settings_save_for_window(m_session, utf8.constData(), x, y,
                                            positionValid)
        != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    return true;
}

bool Session::saveWindowGeometry(const QString &path, int x, int y,
                                 bool positionValid, QString *outError) const
{
    const QByteArray utf8 = path.toUtf8();
    if (tt_session_window_geometry_save(m_session, utf8.constData(), x, y,
                                        positionValid)
        != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    return true;
}

bool Session::rememberSettings(const QVector<QPair<QString, QString>> &values,
                               const QString &path, QString *outError)
{
    // The UTF-8 has to outlive the call, and `TtSettingValue` holds borrowed
    // pointers — so the bytes live in vectors of their own rather than in
    // temporaries that would be gone by the time the core read them.
    QVector<QByteArray> names;
    QVector<QByteArray> texts;
    names.reserve(values.size());
    texts.reserve(values.size());
    for (const auto &pair : values) {
        names.append(pair.first.toUtf8());
        texts.append(pair.second.toUtf8());
    }
    QVector<TtSettingValue> items;
    items.reserve(values.size());
    for (int i = 0; i < values.size(); i++) {
        TtSettingValue item;
        item.name = names.at(i).constData();
        item.value = texts.at(i).constData();
        items.append(item);
    }

    const QByteArray utf8 = path.toUtf8();
    if (tt_session_settings_remember(m_session, utf8.constData(), items.constData(),
                                     static_cast<size_t>(items.size()))
        != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    // No `settingsChanged`, deliberately: nothing the window draws from is in
    // one of these keys, and that signal re-applies the *file's* terminal size,
    // the font, the title and the window flags — which would make remembering a
    // connection's speed resize the window somebody had just dragged.
    //
    // The drain is not optional in the same way. `tt_session_settings_remember`
    // applies the struct on its way to the file, so it pushes a `Damage` like
    // any other settings change; leaving it in the queue means the next thing
    // to drain — a keystroke's repaint — arrives carrying this change's
    // damage. That is the trap `AGENTS.md` records against `setSetting`,
    // `resize` and the settings loaders, and this path had it too.
    dispatch();
    return true;
}

bool Session::settingPresent(const QString &path, const QString &name,
                             bool *outPresent, QString *outError)
{
    if (!outPresent) {
        if (outError) {
            *outError = QStringLiteral("null settings presence output");
        }
        return false;
    }
    const QByteArray utf8 = path.toUtf8();
    const QByteArray setting = name.toUtf8();
    if (tt_settings_file_has(utf8.constData(), setting.constData(), outPresent)
        != TT_OK) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    return true;
}

TtSerialParams Session::serialParams() const
{
    TtSerialParams params;
    tt_session_serial_params(m_session, &params);
    return params;
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
    dispatch();
}

void Session::dispatch()
{
    const TtEvent *events = nullptr;
    const size_t n = tt_session_drain_events(m_session, &events);

    bool dirty = false;
    bool colorsMoved = false;
    bool windowOps = false;
    bool printerOps = false;
    bool connectionEnded = false;
    bool windowCloseRequested = false;
    bool transferMoved = false;
    std::optional<TransferResult> transferEnded;
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
        case TT_EVENT_KIND_STREAM_FILTER_FAILED:
            emit notice(tr("Lua stream filter disabled: %1")
                            .arg(QString::fromUtf8(events[i].text ? events[i].text : "")));
            break;
        case TT_EVENT_KIND_TRANSFER_PROGRESS:
            // Coalesced like damage: one pump can move a transfer several
            // times and a dialog only needs the latest.
            transferMoved = true;
            break;
        case TT_EVENT_KIND_BELL:
        case TT_EVENT_KIND_VISUAL_BELL:
            // Not coalesced with the others: the core has already decided
            // this is a bell worth making, and one pump produces at most one.
            emit bellRang(events[i].kind == TT_EVENT_KIND_VISUAL_BELL);
            break;
        case TT_EVENT_KIND_TRANSFER_DONE: {
            TtTransferResult r;
            if (tt_session_transfer_result(m_session, &r)) {
                transferEnded = TransferResult{
                    r.success, r.cancelled,
                    QString::fromUtf8(r.message ? r.message : ""), r.bytes, r.elapsed_ms};
            } else {
                transferEnded = TransferResult{};
            }
            break;
        }
        case TT_EVENT_KIND_CLIPBOARD_READ: {
            if (events[i].byte != 0) {
                emit notice(tr("Remote host read the clipboard"));
            }
            const QByteArray selection(events[i].text ? events[i].text : "");
            const QString text = QGuiApplication::clipboard()->text(QClipboard::Clipboard);
            const QByteArray utf8 = text.toUtf8();
            bool sent = false;
            if (tt_session_clipboard_reply(m_session, selection.constData(),
                                           utf8.constData(),
                                           static_cast<size_t>(utf8.size()), &sent)
                != TT_OK) {
                emit notice(QString::fromUtf8(tt_last_error()));
            }
            break;
        }
        case TT_EVENT_KIND_CLIPBOARD_WRITE: {
            const QString text = QString::fromUtf8(events[i].text ? events[i].text : "");
            if (events[i].byte != 0) {
                emit notice(tr("Remote host wrote the clipboard"));
            }
            if (text.isEmpty()) {
                QGuiApplication::clipboard()->clear(QClipboard::Clipboard);
            } else {
                QGuiApplication::clipboard()->setText(text, QClipboard::Clipboard);
            }
            break;
        }
        case TT_EVENT_KIND_CLIPBOARD_READ_REJECTED:
            emit notice(tr("Remote clipboard read rejected"));
            break;
        case TT_EVENT_KIND_CLIPBOARD_WRITE_REJECTED:
            emit notice(tr("Remote clipboard write rejected"));
            break;
        case TT_EVENT_KIND_CLOSE_REQUESTED:
            windowCloseRequested = true;
            break;
        case TT_EVENT_KIND_COLORS_CHANGED:
            // Coalesced like damage: one `OSC 4` can carry a whole palette and
            // there is one cache to refill however many colours moved.
            colorsMoved = true;
            break;
        case TT_EVENT_KIND_PRINTER:
            // Read once for the whole batch, below, for the same reason the
            // window operations are: the payload does not fit in `TtEvent`.
            printerOps = true;
            break;
        case TT_EVENT_KIND_WINDOW_REQUEST:
            // Read once for the whole batch, below. The event says only that
            // there is something to read: `TtEvent` has no room for two ints,
            // and giving it some would break the ABI for every other event.
            windowOps = true;
            break;
        }
    }

    // Before the repaint, because one of them is "repaint" and several change
    // the size the frame is about to be drawn at.
    if (printerOps) {
        const TtPrinterEvent *jobs = nullptr;
        const size_t count = tt_session_printer_events(m_session, &jobs);
        for (size_t i = 0; i < count; i++) {
            emit printerEvent(jobs[i]);
        }
    }

    if (windowOps) {
        const TtWindowRequest *requests = nullptr;
        const size_t count = tt_session_window_requests(m_session, &requests);
        for (size_t i = 0; i < count; i++) {
            emit windowOperationRequested(requests[i]);
        }
    }

    // Before the repaint, so the frame that shows the cells the host just sent
    // is drawn with the colours it sent them in.
    if (colorsMoved) {
        emit colorsChanged();
    }
    if (dirty) {
        emit damaged();
    }
    // Rearm before announcing the disconnect: the descriptor is already gone
    // and whatever the notice wakes up must not find a live notifier on it.
    rearm();
    // And before the signals, so a dialog opening its nested event loop on
    // `transferProgressed` finds the deadline timer already running.
    if (transferMoved && !transferEnded) {
        emit transferProgressed(transferProgress());
    }
    if (transferEnded) {
        emit transferFinished(*transferEnded);
    }
    if (connectionEnded) {
        // A local shell knows why it ended; a serial line does not. "bash
        // exited with status 1" is the difference between a window that
        // explains itself and one that just goes quiet.
        const QString note = closeNote();
        emit notice(note.isEmpty() ? tr("Disconnected") : note);
        emit connectionChanged();
    }
    if (windowCloseRequested) {
        emit closeRequested();
    }
}

void Session::rearm()
{
#ifdef Q_OS_WIN
    // SSH's connection and running transport share one manual-reset event.
    // Other Windows transports return null until their native asynchronous
    // byte-I/O paths land, so never manufacture a polling timer here.
    void *handle = m_ssh ? tt_ssh_connect_wait_handle(m_ssh)
                         : tt_session_wait_handle(m_session);

    if (m_notifier && m_notifier->handle() != handle) {
        m_notifier->setEnabled(false);
        delete m_notifier;
        m_notifier = nullptr;
    }
    if (handle && !m_notifier) {
        m_notifier = new QWinEventNotifier(handle, this);
        connect(m_notifier, &QWinEventNotifier::activated, this, &Session::onReadable);
    }
#else
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
#endif

    const bool stuck = tt_session_pending_out(m_session) > 0;
    if (stuck && !m_retry->isActive()) {
        m_retry->start();
    } else if (!stuck && m_retry->isActive()) {
        m_retry->stop();
    }

    // -2 is "no transfer", -1 is "a transfer with nothing armed". Only the
    // first stops the timer: a transfer waiting purely on the peer still needs
    // the notifier, and re-arming on every pump would be a timer in the idle
    // path, which this class does not have.
    const int64_t deadline = tt_session_transfer_deadline_ms(m_session);
    if (deadline < 0) {
        m_xferTimer->stop();
    } else {
        // A single shot, restarted after each pump, so the interval always
        // reflects what the protocol has *just* armed rather than what it
        // wanted a second ago. Floored at 1 ms: a zero-interval Qt timer fires
        // on every pass of the event loop, which is a spin.
        const int64_t ms = deadline < 1 ? 1 : (deadline > 60000 ? 60000 : deadline);
        m_xferTimer->start(static_cast<int>(ms));
    }
}
