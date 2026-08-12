// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Updater.h"

#include "sterna.h"

#include <QAbstractButton>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMessageBox>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProgressDialog>
#include <QPushButton>
#include <QRegularExpression>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTemporaryFile>
#include <QWidget>

#include <array>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <shellapi.h>
#endif

namespace {

constexpr qint64 MaxManifest = 256 * 1024;
constexpr qint64 MaxManifestSignature = 1024;
constexpr qint64 MaxArtifact = 128 * 1024 * 1024;

const QUrl ManifestUrl(
    QStringLiteral("https://github.com/nataloko/Sterna/releases/latest/download/"
                   "latest.json"));
const QUrl ManifestSignatureUrl(
    QStringLiteral("https://github.com/nataloko/Sterna/releases/latest/download/"
                   "latest.json.sig"));
const QUrl ReleasesUrl(
    QStringLiteral("https://github.com/nataloko/Sterna/releases/latest"));

bool version(const QString &text, std::array<quint64, 3> *out)
{
    static const QRegularExpression pattern(
        QStringLiteral("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\."
                       "(0|[1-9][0-9]*)$"));
    const QRegularExpressionMatch match = pattern.match(text);
    if (!match.hasMatch()) {
        return false;
    }
    std::array<quint64, 3> parsed = {};
    for (int i = 0; i < 3; i++) {
        bool ok = false;
        parsed[static_cast<size_t>(i)] = match.captured(i + 1).toULongLong(&ok);
        if (!ok) {
            return false;
        }
    }
    *out = parsed;
    return true;
}

bool signedBytes(const QByteArray &bytes, const QByteArray &signature)
{
    return tt_update_verify(
        reinterpret_cast<const uint8_t *>(bytes.constData()),
        static_cast<size_t>(bytes.size()),
        reinterpret_cast<const uint8_t *>(signature.constData()),
        static_cast<size_t>(signature.size()));
}

QString platformName()
{
#ifdef Q_OS_WIN
    return QStringLiteral("windows-x86_64");
#elif defined(Q_OS_LINUX)
    return QStringLiteral("linux-x86_64");
#else
    return {};
#endif
}

QNetworkRequest requestFor(const QUrl &url)
{
    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setTransferTimeout(30000);
    request.setRawHeader(
        "User-Agent",
        QByteArray("Sterna/") + QCoreApplication::applicationVersion().toUtf8()
            + " updater");
    request.setRawHeader("Accept", "application/octet-stream, application/json");
    return request;
}

QByteArray fileSha256(const QString &path, QString *error)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        *error = file.errorString();
        return {};
    }
    QCryptographicHash hash(QCryptographicHash::Sha256);
    while (!file.atEnd()) {
        const QByteArray bytes = file.read(1024 * 1024);
        if (bytes.isEmpty() && file.error() != QFileDevice::NoError) {
            *error = file.errorString();
            return {};
        }
        hash.addData(bytes);
    }
    return hash.result();
}

} // namespace

bool decodeUpdateSignature(const QByteArray &encoded, QByteArray *out)
{
    if (!out) {
        return false;
    }
    const QByteArray::FromBase64Result decoded = QByteArray::fromBase64Encoding(
        encoded.trimmed(), QByteArray::AbortOnBase64DecodingErrors);
    if (!decoded || decoded.decoded.size() != 64) {
        return false;
    }
    *out = decoded.decoded;
    return true;
}

UpdateManifestResult parseUpdateManifest(const QByteArray &json,
                                         const QString &currentVersion,
                                         const QString &platform,
                                         UpdateArtifact *out,
                                         QString *error)
{
    auto bad = [error](const QString &message) {
        if (error) {
            *error = message;
        }
        return UpdateManifestResult::Error;
    };
    if (!out) {
        return bad(QStringLiteral("no update destination"));
    }

    QJsonParseError parse;
    const QJsonDocument document = QJsonDocument::fromJson(json, &parse);
    if (parse.error != QJsonParseError::NoError || !document.isObject()) {
        return bad(QStringLiteral("the signed update manifest is not valid JSON"));
    }
    const QJsonObject root = document.object();
    const QJsonValue format = root.value(QStringLiteral("format"));
    if (!format.isDouble() || format.toDouble() != 1.0) {
        return bad(QStringLiteral("the signed update manifest has an unsupported format"));
    }

    const QString latestText = root.value(QStringLiteral("version")).toString();
    std::array<quint64, 3> latest = {};
    std::array<quint64, 3> current = {};
    if (!version(latestText, &latest) || !version(currentVersion, &current)) {
        return bad(QStringLiteral("the signed update manifest has an invalid version"));
    }
    if (latest <= current) {
        return UpdateManifestResult::Current;
    }
    if (platform.isEmpty()) {
        return bad(QStringLiteral("self-update is not supported on this platform"));
    }

    const QJsonObject platforms =
        root.value(QStringLiteral("platforms")).toObject();
    const QJsonObject item = platforms.value(platform).toObject();
    if (item.isEmpty()) {
        return bad(QStringLiteral("the signed update has no build for this platform"));
    }

    const QUrl url(item.value(QStringLiteral("url")).toString());
    if (!url.isValid() || url.scheme() != QLatin1String("https")
        || url.host().compare(QLatin1String("github.com"), Qt::CaseInsensitive)
            != 0) {
        return bad(QStringLiteral("the signed update has an unsafe download URL"));
    }

    const QJsonValue sizeValue = item.value(QStringLiteral("size"));
    const double sizeNumber = sizeValue.toDouble(-1);
    if (!sizeValue.isDouble() || sizeNumber < 1
        || sizeNumber > static_cast<double>(MaxArtifact)
        || sizeNumber != static_cast<double>(static_cast<qint64>(sizeNumber))) {
        return bad(QStringLiteral("the signed update has an invalid download size"));
    }

    const QByteArray sha =
        item.value(QStringLiteral("sha256")).toString().toLatin1();
    static const QRegularExpression shaPattern(QStringLiteral("^[0-9a-f]{64}$"));
    if (!shaPattern.match(QString::fromLatin1(sha)).hasMatch()) {
        return bad(QStringLiteral("the signed update has an invalid SHA-256"));
    }

    QByteArray signature;
    if (!decodeUpdateSignature(
            item.value(QStringLiteral("signature")).toString().toLatin1(),
            &signature)) {
        return bad(QStringLiteral("the signed update has an invalid artifact signature"));
    }

    out->version = latestText;
    out->url = url;
    out->size = static_cast<qint64>(sizeNumber);
    out->sha256 = QByteArray::fromHex(sha);
    out->signature = signature;
    if (error) {
        error->clear();
    }
    return UpdateManifestResult::Available;
}

bool replaceVerifiedAppImage(const QString &sourcePath, const QString &targetPath,
                             QString *error)
{
    const QFile::Permissions permissions = QFile::permissions(targetPath);
    QFile source(sourcePath);
    QSaveFile target(targetPath);
    if (!source.open(QIODevice::ReadOnly) || !target.open(QIODevice::WriteOnly)) {
        *error = source.isOpen() ? target.errorString() : source.errorString();
        return false;
    }
    while (!source.atEnd()) {
        const QByteArray bytes = source.read(1024 * 1024);
        if (bytes.isEmpty() && source.error() != QFileDevice::NoError) {
            target.cancelWriting();
            *error = source.errorString();
            return false;
        }
        if (target.write(bytes) != bytes.size()) {
            target.cancelWriting();
            *error = target.errorString();
            return false;
        }
    }
    if (!target.setPermissions(permissions)) {
        target.cancelWriting();
        *error = target.errorString();
        return false;
    }
    if (!target.commit()) {
        *error = target.errorString();
        return false;
    }
    error->clear();
    return true;
}

Updater::Updater(QWidget *window)
    : QObject(window)
    , m_window(window)
    , m_network(new QNetworkAccessManager(this))
{
}

void Updater::check()
{
    if (m_busy) {
        return;
    }
    m_busy = true;
    m_cancelled = false;
    beginProgress(tr("Checking for a signed Sterna update..."), false);
    fetchSmall(ManifestUrl, MaxManifest, &Updater::onManifest);
}

void Updater::fetchSmall(const QUrl &url, qint64 limit, SmallReply next)
{
    m_small.clear();
    m_smallLimit = limit;
    QNetworkReply *reply = m_network->get(requestFor(url));
    m_reply = reply;
    connect(reply, &QIODevice::readyRead, this, [this, reply] {
        if (m_reply != reply) {
            return;
        }
        const qint64 remaining = m_smallLimit - m_small.size();
        m_small += reply->read(qMax<qint64>(0, remaining) + 1);
        if (m_small.size() > m_smallLimit) {
            m_downloadError = tr("The update server returned too much data.");
            reply->abort();
        }
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, next] {
        if (m_reply != reply) {
            reply->deleteLater();
            return;
        }
        const qint64 remaining = m_smallLimit - m_small.size();
        m_small += reply->read(qMax<qint64>(0, remaining) + 1);
        if (m_small.size() > m_smallLimit && m_downloadError.isEmpty()) {
            m_downloadError = tr("The update server returned too much data.");
        }
        m_reply = nullptr;
        const QNetworkReply::NetworkError networkError = reply->error();
        const QString networkText = reply->errorString();
        reply->deleteLater();
        if (m_cancelled) {
            reset();
            return;
        }
        if (!m_downloadError.isEmpty()) {
            const QString message = m_downloadError;
            m_downloadError.clear();
            fail(message);
            return;
        }
        if (networkError != QNetworkReply::NoError) {
            fail(tr("Could not check for updates: %1").arg(networkText));
            return;
        }
        const QByteArray bytes = m_small;
        m_small.clear();
        (this->*next)(bytes);
    });
}

void Updater::onManifest(const QByteArray &bytes)
{
    m_manifest = bytes;
    fetchSmall(ManifestSignatureUrl, MaxManifestSignature,
               &Updater::onManifestSignature);
}

void Updater::onManifestSignature(const QByteArray &bytes)
{
    QByteArray signature;
    if (!decodeUpdateSignature(bytes, &signature)
        || !signedBytes(m_manifest, signature)) {
        fail(tr("The update manifest is not signed by Sterna. Nothing was "
                "downloaded."));
        return;
    }

    UpdateArtifact artifact;
    QString error;
    const UpdateManifestResult result = parseUpdateManifest(
        m_manifest, QCoreApplication::applicationVersion(), platformName(),
        &artifact, &error);
    m_manifest.clear();
    if (result == UpdateManifestResult::Error) {
        fail(error);
        return;
    }
    reset();
    if (result == UpdateManifestResult::Current) {
        QMessageBox::information(
            m_window, tr("Sterna update"),
            tr("Sterna %1 is current.")
                .arg(QCoreApplication::applicationVersion()));
        return;
    }
    offer(artifact);
}

void Updater::offer(const UpdateArtifact &artifact)
{
    m_artifact = artifact;
    QString reason;
    if (!canInstall(&reason)) {
        const QMessageBox::StandardButton answer = QMessageBox::question(
            m_window, tr("Sterna %1 is available").arg(artifact.version),
            reason + tr("\n\nOpen the release page instead?"));
        if (answer == QMessageBox::Yes) {
            openReleases();
        }
        return;
    }

#ifdef Q_OS_WIN
    const QString consequence =
        tr("The signed installer will ask for permission, then Sterna will "
           "close and restart.");
#else
    const QString consequence =
        tr("The signed AppImage will be replaced atomically. This running "
           "session stays open; the new version is used next time Sterna "
           "starts.");
#endif
    const QMessageBox::StandardButton answer = QMessageBox::question(
        m_window, tr("Sterna %1 is available").arg(artifact.version),
        tr("Download %1 MB?\n\n%2")
            .arg(QString::number(artifact.size / (1024.0 * 1024.0), 'f', 1),
                 consequence));
    if (answer == QMessageBox::Yes) {
        download();
    }
}

bool Updater::canInstall(QString *reason) const
{
#ifdef Q_OS_WIN
    const QString uninstaller = QDir(QCoreApplication::applicationDirPath())
                                    .filePath(QStringLiteral("uninstall.exe"));
    if (!QFileInfo::exists(uninstaller)) {
        *reason = tr("This copy is not an installed Windows build, so it will "
                     "not replace another installation.");
        return false;
    }
    return true;
#elif defined(Q_OS_LINUX)
    const QString appImage =
        QFileInfo(qEnvironmentVariable("APPIMAGE")).canonicalFilePath();
    const QFileInfo file(appImage);
    if (appImage.isEmpty() || !file.isFile()) {
        *reason = tr("This copy is not running from an AppImage, so it cannot "
                     "update itself in place.");
        return false;
    }
    if (!QFileInfo(file.absolutePath()).isWritable()) {
        *reason = tr("The AppImage directory is not writable, so Sterna cannot "
                     "replace itself safely.");
        return false;
    }
    return true;
#else
    *reason = tr("Self-update is not supported on this platform.");
    return false;
#endif
}

void Updater::download()
{
    m_busy = true;
    m_cancelled = false;
    m_received = 0;
    m_downloadError.clear();
    const QString suffix =
#ifdef Q_OS_WIN
        QStringLiteral(".exe");
#else
        QStringLiteral(".AppImage");
#endif
    const QString temp = QStandardPaths::writableLocation(
        QStandardPaths::TempLocation);
    m_download = new QTemporaryFile(
        QDir(temp).filePath(QStringLiteral("sterna-update-XXXXXX") + suffix),
        this);
    if (!m_download->open()) {
        fail(tr("Could not create the update download: %1")
                 .arg(m_download->errorString()));
        return;
    }

    beginProgress(tr("Downloading Sterna %1...").arg(m_artifact.version), true);
    QNetworkReply *reply = m_network->get(requestFor(m_artifact.url));
    m_reply = reply;
    connect(reply, &QIODevice::readyRead, this, &Updater::drainDownload);
    connect(reply, &QNetworkReply::downloadProgress, this,
            [this](qint64 received, qint64) {
                if (m_progress && m_artifact.size > 0) {
                    m_progress->setValue(static_cast<int>(
                        qMin<qint64>(1000, received * 1000 / m_artifact.size)));
                }
            });
    connect(reply, &QNetworkReply::finished, this, &Updater::finishDownload);
}

void Updater::drainDownload()
{
    if (!m_reply || !m_download) {
        return;
    }
    const qint64 remaining = m_artifact.size - m_received;
    const QByteArray bytes = m_reply->read(qMax<qint64>(0, remaining) + 1);
    m_received += bytes.size();
    if (m_received > m_artifact.size || m_received > MaxArtifact) {
        m_downloadError = tr("The update download is larger than its signed size.");
        m_reply->abort();
        return;
    }
    if (m_download->write(bytes) != bytes.size()) {
        m_downloadError = tr("Could not write the update download: %1")
                              .arg(m_download->errorString());
        m_reply->abort();
    }
}

void Updater::finishDownload()
{
    QNetworkReply *reply = m_reply;
    if (!reply) {
        return;
    }
    drainDownload();
    m_reply = nullptr;
    const QNetworkReply::NetworkError networkError = reply->error();
    const QString networkText = reply->errorString();
    reply->deleteLater();
    if (m_cancelled) {
        reset();
        return;
    }
    if (!m_downloadError.isEmpty()) {
        const QString message = m_downloadError;
        m_downloadError.clear();
        fail(message);
        return;
    }
    if (networkError != QNetworkReply::NoError) {
        fail(tr("Could not download the update: %1").arg(networkText));
        return;
    }
    if (m_received != m_artifact.size || !m_download->flush()) {
        fail(tr("The update download ended before its signed size."));
        return;
    }
    m_download->close();
    installDownloaded();
}

void Updater::installDownloaded()
{
    QString error;
    const QByteArray sha = fileSha256(m_download->fileName(), &error);
    if (!error.isEmpty()) {
        fail(tr("Could not verify the update: %1").arg(error));
        return;
    }
    if (sha != m_artifact.sha256) {
        fail(tr("The update's SHA-256 does not match the signed manifest. "
                "Nothing was installed."));
        return;
    }

    QFile file(m_download->fileName());
    if (!file.open(QIODevice::ReadOnly)) {
        fail(tr("Could not verify the update: %1").arg(file.errorString()));
        return;
    }
    uchar *bytes = file.map(0, m_artifact.size);
    const bool verified = bytes
        && tt_update_verify(bytes, static_cast<size_t>(m_artifact.size),
                            reinterpret_cast<const uint8_t *>(
                                m_artifact.signature.constData()),
                            static_cast<size_t>(m_artifact.signature.size()));
    if (bytes) {
        file.unmap(bytes);
    }
    if (!verified) {
        fail(tr("The update is not signed by Sterna. Nothing was installed."));
        return;
    }
    file.close();

#ifdef Q_OS_WIN
    installWindows();
#else
    installAppImage();
#endif
}

void Updater::installAppImage()
{
#ifdef Q_OS_LINUX
    const QString targetPath =
        QFileInfo(qEnvironmentVariable("APPIMAGE")).canonicalFilePath();
    QString error;
    if (!replaceVerifiedAppImage(m_download->fileName(), targetPath, &error)) {
        fail(tr("Could not replace the AppImage: %1").arg(error));
        return;
    }
    const QString installed = m_artifact.version;
    reset();
    QMessageBox::information(
        m_window, tr("Sterna updated"),
        tr("Sterna %1 was installed atomically. It will be used the next time "
           "Sterna starts; this session can stay open.")
            .arg(installed));
#endif
}

void Updater::installWindows()
{
#ifdef Q_OS_WIN
    const QString parameters =
        QStringLiteral("/S /UPDATEPID=%1 /RESTART")
            .arg(QCoreApplication::applicationPid());
    const std::wstring file = QDir::toNativeSeparators(m_download->fileName())
                                  .toStdWString();
    const std::wstring args = parameters.toStdWString();
    SHELLEXECUTEINFOW execute = {};
    execute.cbSize = sizeof execute;
    execute.fMask = SEE_MASK_NOCLOSEPROCESS;
    execute.hwnd = reinterpret_cast<HWND>(m_window->winId());
    execute.lpVerb = L"runas";
    execute.lpFile = file.c_str();
    execute.lpParameters = args.c_str();
    execute.nShow = SW_SHOWNORMAL;
    if (!ShellExecuteExW(&execute)) {
        fail(tr("Windows did not start the signed installer. Nothing was "
                "changed."));
        return;
    }
    if (execute.hProcess) {
        CloseHandle(execute.hProcess);
    }
    // The installer is executing this file, so QTemporaryFile must not try to
    // unlink it while the process is still starting. It lives in the ordinary
    // temp directory and is harmless after the upgrade.
    m_download->setAutoRemove(false);
    reset();
    QCoreApplication::quit();
#endif
}

void Updater::beginProgress(const QString &text, bool determinate)
{
    if (m_progress) {
        m_progress->deleteLater();
    }
    m_progress = new QProgressDialog(text, tr("Cancel"), 0,
                                     determinate ? 1000 : 0, m_window);
    m_progress->setWindowTitle(tr("Sterna update"));
    m_progress->setWindowModality(Qt::WindowModal);
    m_progress->setMinimumDuration(0);
    if (!determinate) {
        m_progress->setRange(0, 0);
    }
    connect(m_progress, &QProgressDialog::canceled, this, &Updater::cancel);
    m_progress->show();
}

void Updater::reset()
{
    m_busy = false;
    m_cancelled = false;
    m_manifest.clear();
    m_small.clear();
    m_downloadError.clear();
    if (m_progress) {
        m_progress->hide();
        m_progress->deleteLater();
        m_progress = nullptr;
    }
    if (m_download) {
        m_download->deleteLater();
        m_download = nullptr;
    }
}

void Updater::cancel()
{
    m_cancelled = true;
    if (m_reply) {
        m_reply->abort();
    } else {
        reset();
    }
}

void Updater::fail(const QString &message)
{
    reset();
    QMessageBox box(QMessageBox::Warning, tr("Sterna update"), message,
                    QMessageBox::Close, m_window);
    QAbstractButton *open =
        box.addButton(tr("Open release page"), QMessageBox::ActionRole);
    box.exec();
    if (box.clickedButton() == open) {
        openReleases();
    }
}

void Updater::openReleases()
{
    QDesktopServices::openUrl(ReleasesUrl);
}

/// The only symbol the terminal resolves from the on-demand updater library.
/// QObject keeps Qt's ABI at the seam; the terminal invokes the `check` slot by
/// name and never links Qt Network itself.
extern "C" Q_DECL_EXPORT QObject *sterna_updater_new(QWidget *parent)
{
    return new Updater(parent);
}
