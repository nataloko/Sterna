// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Updater.h"
#include "sterna.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QTemporaryFile>

#include <cstdio>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#endif

namespace {

int failures = 0;

#define CHECK(expr)                                                            \
    do {                                                                       \
        if (!(expr)) {                                                         \
            std::fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__,       \
                         __LINE__, #expr);                                     \
            failures++;                                                       \
        }                                                                      \
    } while (0)

#ifdef Q_OS_WIN
/// Open a path exactly as the Windows loader does when it starts a program:
/// read and execute access, sharing only readers and deleters. It fails with a
/// sharing violation while any other handle holds the file open for writing,
/// which is the whole question `detachUpdateDownload` exists to answer.
bool loaderCanOpen(const QString &path)
{
    const std::wstring native = QDir::toNativeSeparators(path).toStdWString();
    const HANDLE handle =
        CreateFileW(native.c_str(), GENERIC_READ | FILE_EXECUTE,
                    FILE_SHARE_READ | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL, nullptr);
    if (handle == INVALID_HANDLE_VALUE) {
        return false;
    }
    CloseHandle(handle);
    return true;
}
#endif

QByteArray file(const QString &name)
{
    QFile source(QStringLiteral(TT_UPDATE_FIXTURE_DIR) + QLatin1Char('/') + name);
    CHECK(source.open(QIODevice::ReadOnly));
    return source.readAll();
}

QByteArray manifest(const QString &version = QStringLiteral("1.2.3"))
{
    QJsonObject artifact;
    artifact.insert(QStringLiteral("url"),
                    QStringLiteral("https://github.com/nataloko/Sterna/releases/"
                                   "download/v1.2.3/sterna.AppImage"));
    artifact.insert(QStringLiteral("size"), 12345);
    artifact.insert(
        QStringLiteral("sha256"),
        QStringLiteral("0123456789abcdef0123456789abcdef"
                       "0123456789abcdef0123456789abcdef"));
    artifact.insert(QStringLiteral("signature"),
                    QString::fromLatin1(QByteArray(64, '\0').toBase64()));
    QJsonObject platforms;
    platforms.insert(QStringLiteral("linux-x86_64"), artifact);
    QJsonObject root;
    root.insert(QStringLiteral("format"), 1);
    root.insert(QStringLiteral("version"), version);
    root.insert(QStringLiteral("platforms"), platforms);
    return QJsonDocument(root).toJson(QJsonDocument::Compact);
}

void signature_fixture_matches_the_compiled_key()
{
    const QByteArray message = file(QStringLiteral("test-message.txt"));
    QByteArray signature;
    CHECK(decodeUpdateSignature(file(QStringLiteral("test-signature.txt")),
                                &signature));
    CHECK(tt_update_verify(
        reinterpret_cast<const uint8_t *>(message.constData()),
        static_cast<size_t>(message.size()),
        reinterpret_cast<const uint8_t *>(signature.constData()),
        static_cast<size_t>(signature.size())));
    QByteArray changed = message;
    changed[0] ^= 1;
    CHECK(!tt_update_verify(
        reinterpret_cast<const uint8_t *>(changed.constData()),
        static_cast<size_t>(changed.size()),
        reinterpret_cast<const uint8_t *>(signature.constData()),
        static_cast<size_t>(signature.size())));
}

void manifest_is_bounded_and_platform_specific()
{
    UpdateArtifact artifact;
    QString error;
    CHECK(parseUpdateManifest(manifest(), QStringLiteral("1.2.2"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Available);
    CHECK(artifact.version == QStringLiteral("1.2.3"));
    CHECK(artifact.size == 12345);
    CHECK(artifact.sha256.size() == 32);
    CHECK(artifact.signature.size() == 64);
    CHECK(parseUpdateManifest(manifest(), QStringLiteral("1.2.3"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Current);
    CHECK(parseUpdateManifest(manifest(), QStringLiteral("2.0.0"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Current);

    CHECK(parseUpdateManifest(manifest(QStringLiteral("01.2.3")),
                              QStringLiteral("1.0.0"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Error);
    CHECK(parseUpdateManifest(manifest(), QStringLiteral("1.2.2"),
                              QStringLiteral("windows-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Error);

    QJsonObject root = QJsonDocument::fromJson(manifest()).object();
    QJsonObject platforms = root.value(QStringLiteral("platforms")).toObject();
    QJsonObject item = platforms.value(QStringLiteral("linux-x86_64")).toObject();
    item.insert(QStringLiteral("size"), 600 * 1024 * 1024);
    platforms.insert(QStringLiteral("linux-x86_64"), item);
    root.insert(QStringLiteral("platforms"), platforms);
    CHECK(parseUpdateManifest(QJsonDocument(root).toJson(),
                              QStringLiteral("1.2.2"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Error);

    item.insert(QStringLiteral("size"), 12345);
    item.insert(QStringLiteral("url"), QStringLiteral("http://example.com/x"));
    platforms.insert(QStringLiteral("linux-x86_64"), item);
    root.insert(QStringLiteral("platforms"), platforms);
    CHECK(parseUpdateManifest(QJsonDocument(root).toJson(),
                              QStringLiteral("1.2.2"),
                              QStringLiteral("linux-x86_64"), &artifact,
                              &error)
          == UpdateManifestResult::Error);
}

void appimage_replacement_is_atomic_and_executable()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString target = dir.filePath(QStringLiteral("Sterna.AppImage"));
    const QString source = dir.filePath(QStringLiteral("download"));
    {
        QFile file(target);
        CHECK(file.open(QIODevice::WriteOnly));
        CHECK(file.write("old") == 3);
        CHECK(file.setPermissions(QFileDevice::ReadOwner
                                  | QFileDevice::WriteOwner
                                  | QFileDevice::ExeOwner));
    }
    {
        QFile file(source);
        CHECK(file.open(QIODevice::WriteOnly));
        CHECK(file.write("new verified image") == 18);
    }
    QString error;
    CHECK(replaceVerifiedAppImage(source, target, &error));
    QFile installed(target);
    CHECK(installed.open(QIODevice::ReadOnly));
    CHECK(installed.readAll() == QByteArray("new verified image"));
    CHECK(installed.permissions() & QFileDevice::ExeOwner);
}

void a_detached_download_outlives_its_temporary_file()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    QString path;
    {
        auto *download = new QTemporaryFile(
            dir.filePath(QStringLiteral("sterna-update-XXXXXX.exe")));
        CHECK(download->open());
        CHECK(download->write("MZ verified installer") == 21);
        CHECK(download->flush());
        path = download->fileName();
        download->close();

#ifdef Q_OS_WIN
        // `close()` closed nothing: the object still holds the file open for
        // writing, so Windows refuses to make an image section of it and the
        // installer cannot be started. Asserted rather than assumed, so a
        // change in Qt shows up here instead of in a failed upgrade.
        CHECK(!loaderCanOpen(path));
#endif
        CHECK(detachUpdateDownload(download) == path);
    }

    CHECK(QFile::exists(path));
    QFile installer(path);
    CHECK(installer.open(QIODevice::ReadOnly));
    CHECK(installer.readAll() == QByteArray("MZ verified installer"));
#ifdef Q_OS_WIN
    // The updater keeps this read-only handle open across ShellExecuteEx. A
    // reader is not a writer, so the loader still gets what it asks for — and
    // Qt opens without FILE_SHARE_DELETE, so the bytes that were just verified
    // cannot be replaced at that path while the handle is held. Both halves are
    // asserted because the second is the reason the handle is kept rather than
    // closed with the rest of the verification.
    CHECK(loaderCanOpen(path));
    CHECK(!QFile::remove(path));
#endif
    installer.close();
    CHECK(QFile::remove(path));
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    signature_fixture_matches_the_compiled_key();
    manifest_is_bounded_and_platform_specific();
    appimage_replacement_is_atomic_and_executable();
    a_detached_download_outlives_its_temporary_file();
    if (failures) {
        return 1;
    }
    std::puts("update ok");
    return 0;
}
