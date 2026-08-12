// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Updater.h"
#include "sterna.h"

#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>

#include <cstdio>

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

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    signature_fixture_matches_the_compiled_key();
    manifest_is_bounded_and_platform_specific();
    appimage_replacement_is_atomic_and_executable();
    if (failures) {
        return 1;
    }
    std::puts("update ok");
    return 0;
}
