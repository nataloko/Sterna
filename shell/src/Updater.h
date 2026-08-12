// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QUrl>

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

/// User-initiated, signed AppImage/NSIS updates.
///
/// No timer and no startup request: choosing Help > Check for Updates is the
/// permission to contact the release server. The manifest is verified before
/// its URL or size is trusted, and the downloaded program is verified again
/// before it can replace or execute anything.
class Updater : public QObject {
    Q_OBJECT

public:
    explicit Updater(QWidget *window);

public slots:
    void check();

private:
    using SmallReply = void (Updater::*)(const QByteArray &);

    void fetchSmall(const QUrl &url, qint64 limit, SmallReply next);
    void onManifest(const QByteArray &bytes);
    void onManifestSignature(const QByteArray &bytes);
    void offer(const UpdateArtifact &artifact);
    bool canInstall(QString *reason) const;
    void download();
    void drainDownload();
    void finishDownload();
    void installDownloaded();
    void installAppImage();
    void installWindows();
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
};
