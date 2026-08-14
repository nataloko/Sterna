// The window, connecting over SSH, against a real server.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
//
//   cd ssh-audit && ./servers.sh start
//   D=$XDG_RUNTIME_DIR/sterna-ssh-audit
//   TT_SSH_HOST=127.0.0.1 TT_SSH_PORT=2222 TT_SSH_USER=$USER
//   TT_SSH_KEY=$D/id_ed25519 QT_QPA_PLATFORM=offscreen ./build/ssh_test
//   cd ssh-audit && ./servers.sh stop
//
// Without those variables it skips loudly, the same rule the serial rig and
// the core's own SSH tests follow.
//
// What this can check that `crates/tt-conn/tests/ssh.rs` cannot: that the
// *shell's* event loop drives the connection. The core's tests poll in a busy
// loop; this one waits on a `QSocketNotifier` exactly as the window does, so
// it is the only thing that would notice the descriptor being registered on
// the wrong fd, or the handover from the connection to the session losing a
// wakeup — which would look like a window that connects and then shows
// nothing.

#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QTimer>

#include <cstdio>
#include <cstdlib>

#include "Session.h"
#include "ConnectDialog.h"
#include "SshPrompts.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

QByteArray env(const char *name)
{
    const char *v = qgetenv(name).isEmpty() ? nullptr : qgetenv(name).constData();
    return v ? qgetenv(name) : QByteArray();
}

/// Spin the real event loop until `done` or the deadline. Nothing here polls
/// the session: every wakeup has to come from the notifier, which is the
/// point.
template <typename F>
bool spin(F done, int ms)
{
    QElapsedTimer timer;
    timer.start();
    while (!done() && timer.elapsed() < ms) {
        QEventLoop loop;
        QTimer::singleShot(20, &loop, &QEventLoop::quit);
        loop.exec(QEventLoop::AllEvents);
    }
    return done();
}

/// The whole visible screen as text, for looking for a marker in it.
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

} // namespace

/// `grab()` renders at the widget's *current* size, and a dialog that has
/// never been shown has not laid itself out — so wrapped labels overlap in the
/// image and nothing overlaps on screen. One `adjustSize` removes the
/// discrepancy.
void grabDialog(QWidget &&dialog, const QString &path)
{
    dialog.adjustSize();
    dialog.grab().save(path);
}

/// `--write <dir>` dumps the dialogs an SSH connection can raise, for
/// looking at the wording and the layout rather than guessing at them. They
/// are the parts of this feature that no assertion can judge.
void writeDialogs(const QString &dir)
{
    HostKeyRequest unknown;
    unknown.host = QStringLiteral("console-01.example.com");
    unknown.port = 22;
    unknown.algorithm = QStringLiteral("ssh-ed25519");
    unknown.fingerprint =
        QStringLiteral("SHA256:Gzq75YFfbr601YXYupc+1lo+XrFxne5w+oJpeyON5kg");
    unknown.verdict = TT_HOST_KEY_UNKNOWN;
    grabDialog(HostKeyDialog(unknown), dir + QStringLiteral("/hostkey-unknown.png"));

    HostKeyRequest changed = unknown;
    changed.verdict = TT_HOST_KEY_CHANGED;
    changed.recordedFingerprint =
        QStringLiteral("SHA256:/f8wu3OvjnNmXydHb05Z5qJojTVMiugU0ZWKNJvnzzE");
    changed.recordedAt = QStringLiteral("/home/nata/.ssh/known_hosts:12");
    grabDialog(HostKeyDialog(changed), dir + QStringLiteral("/hostkey-changed.png"));

    AuthRequest auth;
    auth.kind = TT_SSH_AUTH_KEYBOARD_INTERACTIVE;
    auth.name = QStringLiteral("Two-factor authentication");
    auth.instruction = QStringLiteral("Enter your password, then the code from "
                                      "your token.");
    auth.lines = {{QStringLiteral("Password:"), false},
                  {QStringLiteral("Token code:"), false}};
    grabDialog(AuthDialog(auth), dir + QStringLiteral("/auth.png"));
    grabDialog(ConnectDialog(), dir + QStringLiteral("/connect.png"));
    printf("wrote four PNGs to %s\n", qPrintable(dir));
}

int main(int argc, char **argv)
{
    QApplication app(argc, argv);

    for (int i = 1; i + 1 < argc; i++) {
        if (qstrcmp(argv[i], "--write") == 0) {
            writeDialogs(QString::fromUtf8(argv[i + 1]));
            return 0;
        }
    }

    const QByteArray host = env("TT_SSH_HOST");
    const QByteArray key = env("TT_SSH_KEY");
    if (host.isEmpty() || key.isEmpty()) {
        printf("SKIPPED: set TT_SSH_HOST and TT_SSH_KEY (see the file header)\n");
        return 0;
    }
    const QByteArray user = env("TT_SSH_USER");
    const QByteArray port = env("TT_SSH_PORT");

    Session session(80, 24);

    // A bare `Session` starts from `tt-vt`'s reference defaults, so the shipped
    // `CRReceive` has to be asked for: the window gets it by loading the
    // settings, and this test has no window. Set before connecting, so the
    // detector resolves on the server's own first line ending exactly as it
    // does in the application (deviation 9).
    QString settingError;
    CHECK(session.setSetting(QStringLiteral("terminal.cr_receive"),
                             QStringLiteral("AUTO"), &settingError));

    // The host key is answered from here rather than by a dialog: the point of
    // this test is the event loop, and a modal dialog under `offscreen` would
    // just hang. Answering it *from the signal* is still the real path.
    int hostKeyAsked = 0;
    QString fingerprint;
    QObject::connect(&session, &Session::sshHostKeyWanted,
                     [&](const HostKeyRequest &r) {
                         hostKeyAsked++;
                         fingerprint = r.fingerprint;
                         CHECK(r.host == QString::fromUtf8(host));
                         CHECK(r.verdict == TT_HOST_KEY_UNKNOWN);
                         // 2: accept once, so a test run never writes to the
                         // user's known_hosts.
                         session.answerHostKey(2);
                     });

    int authAsked = 0;
    QObject::connect(&session, &Session::sshAuthWanted, [&](const AuthRequest &) {
        // The key should have answered. Anything asked here means the auth
        // ordering is wrong, not that the prompt plumbing is.
        authAsked++;
        session.cancelSsh();
    });

    QString failure;
    QObject::connect(&session, &Session::sshFailed,
                     [&](const QString &e) { failure = e; });

    // A scratch known_hosts, so the verdict does not depend on what the
    // machine running this happens to have connected to before — and so a run
    // never writes to the user's file.
    const QByteArray knownHosts =
        (QDir::tempPath() + QStringLiteral("/tt-shell-ssh-known-hosts-%1")
                                .arg(QCoreApplication::applicationPid()))
            .toUtf8();
    QFile::remove(QString::fromUtf8(knownHosts));

    const char *identities[2] = {key.constData(), nullptr};
    const char *knownHostsFiles[2] = {knownHosts.constData(), nullptr};
    TtSshParams params;
    tt_ssh_params_default(&params);
    params.host = host.constData();
    params.port = static_cast<uint16_t>(port.isEmpty() ? 22 : port.toInt());
    params.user = user.isEmpty() ? nullptr : user.constData();
    params.identities = identities;
    params.known_hosts = knownHostsFiles;
    // The agent and the user's config belong to whoever runs this and must not
    // decide whether it passes.
    params.use_agent = false;
    params.use_ssh_config = false;
    params.connect_timeout_ms = 15000;

    QString error;
    CHECK(session.startSsh(params, &error));
    if (!error.isEmpty()) {
        fprintf(stderr, "startSsh: %s\n", qPrintable(error));
        return 1;
    }
    CHECK(session.isConnecting());

    const bool connected = spin(
        [&] { return session.isConnected() || !failure.isEmpty(); }, 20000);
    if (!failure.isEmpty()) {
        fprintf(stderr, "connect failed: %s\n", qPrintable(failure));
        return 1;
    }
    CHECK(connected);
    CHECK(hostKeyAsked == 1);
    CHECK(authAsked == 0);
    CHECK(fingerprint.startsWith(QStringLiteral("SHA256:")));
    CHECK(!session.isConnecting());
    // SSH has no break — RFC 4335 and russh does not implement it — so the
    // window must not offer the menu item.
    CHECK(!session.supportsBreak());
    CHECK(session.describe().contains(QLatin1Char('@')));
    printf("  connected to %s (%s)\n", qPrintable(session.describe()),
           qPrintable(fingerprint));

    // A login shell is not ready when the channel opens: the MOTD and the
    // first prompt arrive first, and anything typed before bash reads is
    // echoed by the pty and dropped.
    int quiet = 0;
    QObject::connect(&session, &Session::damaged, [&] { quiet = 0; });
    spin([&] { return ++quiet > 25; }, 8000);

    session.sendText(QStringLiteral("echo shell-ssh-ok\n"));
    const bool seen = spin(
        [&] { return screenText(session).contains(QStringLiteral("shell-ssh-ok")); },
        10000);
    CHECK(seen);
    if (!seen) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }

    // A bare CR is a carriage return, not a line ending — `CRReceive` ships as
    // Auto and this connection's line endings are CR LF, so it has resolved to
    // the reference mode by now (deviation 9). Without that, every prompt
    // redraw an interactive shell makes takes a line of its own, which is what
    // one keystroke in `fish` looks like. The markers are written as octal so
    // the echo of the command itself cannot satisfy the assertions.
    session.sendText(QStringLiteral("printf '\\101\\102\\103\\r\\104\\105\\106\\n'\n"));
    const bool overwritten = spin(
        [&] { return screenText(session).contains(QStringLiteral("DEF")); }, 10000);
    CHECK(overwritten);
    CHECK(!screenText(session).contains(QStringLiteral("ABC")));
    if (!overwritten || screenText(session).contains(QStringLiteral("ABC"))) {
        fprintf(stderr, "screen was:\n%s\n", qPrintable(screenText(session)));
    }

    // And disconnecting from the window's side leaves nothing armed — and does
    // not ask the window to close, which `AutoWinClose` would have done to a
    // network session before deviation 15.
    int closeRequests = 0;
    QObject::connect(&session, &Session::closeRequested, [&] { closeRequests++; });
    session.disconnectPort();
    CHECK(!session.isConnected());
    CHECK(!session.isConnecting());
    CHECK(closeRequests == 0);

    // "Accept once" means once: nothing was written down.
    CHECK(!QFile::exists(QString::fromUtf8(knownHosts)));

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("ssh ok\n");
    return 0;
}
