// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QUrl>

class QFile;
class QNetworkAccessManager;
class QNetworkReply;
class QProgressDialog;
class QTemporaryFile;
class QWidget;

struct UpdateArtifact {
    QString version;
    QUrl url;
    qint64 size = 0;
    QByteArray sha256;
    QByteArray signature;
};

enum class UpdateManifestResult {
    Error,
    Current,
    Available,
};

/// Parse metadata only after its detached signature has been verified.
///
/// Kept outside `Updater` so the security-sensitive bounds and version rules
/// have a small, network-free test boundary.
UpdateManifestResult parseUpdateManifest(const QByteArray &json,
                                         const QString &currentVersion,
                                         const QString &platform,
                                         UpdateArtifact *out,
                                         QString *error);

/// Strict base64 for one raw Ed25519 signature.
bool decodeUpdateSignature(const QByteArray &encoded, QByteArray *out);

/// Atomically replace one verified AppImage while preserving executable bits.
bool replaceVerifiedAppImage(const QString &source, const QString &target,
                             QString *error);

/// Give up ownership of a finished download, leaving the file on disk.
///
/// Takes ownership and destroys `download`, answering with its path. Destroying
/// it is the only way to close it: `QTemporaryFile::close()` keeps the handle so
/// that the unique name stays reserved and reopening is safe. Windows cannot
/// execute a file some other handle holds open for writing, so a downloaded
/// installer cannot be started until this has run. Auto-remove is cleared
/// first, so the file survives its object.
QString detachUpdateDownload(QTemporaryFile *download);

/// Signed AppImage/NSIS updates, asked for or scheduled.
///
/// No timer: this object makes exactly one request per [`check`] or
/// [`checkQuietly`], and the terminal decides when either happens — Help >
/// Check for Updates, or at most once a day at startup while
/// `updates.check_on_startup` is on. The manifest is verified before its URL or
/// size is trusted, and the downloaded program is verified again before it can
/// replace or execute anything, on both paths equally.
///
/// **Nothing is downloaded without being asked for.** A check reads a signed
/// manifest and a signature, together under 257 KiB; the artifact is fetched
/// only after the offer below has been accepted.
class Updater : public QObject {
    Q_OBJECT

public:
    explicit Updater(QWidget *window);

public slots:
    /// The button: says what it is doing, and says so when there is nothing to
    /// do or when it went wrong.
    void check();
    /// The startup check, which is silent until it has something to say.
    ///
    /// No progress dialog, no "Sterna is current", and no complaint about a
    /// release server that cannot be reached or a manifest that does not
    /// verify — the user did not ask, and a box on every launch is how people
    /// learn to turn a security feature off. An *available* update speaks, and
    /// from the offer onwards this is the same path the button takes, progress
    /// bar and failures included.
    void checkQuietly();

private:
    using SmallReply = void (Updater::*)(const QByteArray &);

    void start(bool quiet);
    void fetchSmall(const QUrl &url, qint64 limit, SmallReply next);
    void onManifest(const QByteArray &bytes);
    void onManifestSignature(const QByteArray &bytes);
    void offer(const UpdateArtifact &artifact);
    bool canInstall(QString *reason) const;
    void download();
    void drainDownload();
    void finishDownload();
    void installDownloaded();
    void installAppImage(const QString &path);
    /// `verified` is the read-only handle pinning the verified bytes at their
    /// path, and it must stay open across this call — the caller owns it.
    void installWindows(QFile &verified);
    void beginProgress(const QString &text, bool determinate);
    void reset();
    void cancel();
    void fail(const QString &message);
    void openReleases();

    QWidget *m_window = nullptr;
    QNetworkAccessManager *m_network = nullptr;
    QNetworkReply *m_reply = nullptr;
    QProgressDialog *m_progress = nullptr;
    QTemporaryFile *m_download = nullptr;
    QByteArray m_small;
    QByteArray m_manifest;
    qint64 m_smallLimit = 0;
    qint64 m_received = 0;
    QString m_downloadError;
    UpdateArtifact m_artifact;
    bool m_busy = false;
    bool m_cancelled = false;
    /// Silent until there is an update to offer. Cleared by [`reset`], which
    /// every path runs before it can show anything the user has to answer — so
    /// the offer, the download and the install speak whoever started the check.
    bool m_quiet = false;
};
