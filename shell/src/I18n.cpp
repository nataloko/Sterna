// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "I18n.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>

I18n::I18n(QObject *parent) : QObject(parent) {}

I18n::~I18n()
{
    tt_i18n_free(m_catalog);
}

QString I18n::bundledDirectory()
{
    const QDir application(QCoreApplication::applicationDirPath());
    const QString installed = QDir::cleanPath(
        application.filePath(QStringLiteral("../share/sterna/lang")));
    if (QDir(installed).exists()) {
        return installed;
    }

#ifdef TT_SOURCE_LANG_DIR
    const QString source = QString::fromUtf8(TT_SOURCE_LANG_DIR);
    if (QDir(source).exists()) {
        return QDir::cleanPath(source);
    }
#endif

    // Windows will place the directory beside the executable. Keep this last
    // so an incomplete AppImage cannot accidentally borrow files from some
    // unrelated sibling `lang` directory.
    return application.filePath(QStringLiteral("lang"));
}

QString I18n::resolve(const QString &configured, const QString &settingsPath)
{
    QString relative = configured;
    relative.replace(QLatin1Char('\\'), QLatin1Char('/'));
    const QFileInfo given(relative);
    if (given.isAbsolute()) {
        return QDir::cleanPath(relative);
    }

    const QStringList candidates = {
        QDir(QFileInfo(settingsPath).absolutePath()).filePath(relative),
        QDir(QCoreApplication::applicationDirPath()).filePath(relative),
        QDir(bundledDirectory()).filePath(given.fileName()),
    };
    for (const QString &candidate : candidates) {
        if (QFileInfo(candidate).isFile()) {
            return QDir::cleanPath(candidate);
        }
    }
    // The most useful error names the installed location, not the process's
    // working directory or a source-tree fallback.
    return QDir::cleanPath(candidates.last());
}

bool I18n::load(const QString &configured, const QString &settingsPath,
                QString *outError)
{
    tt_i18n_free(m_catalog);
    m_catalog = nullptr;
    const QByteArray path = resolve(configured, settingsPath).toUtf8();
    m_catalog = tt_i18n_load(path.constData());
    if (!m_catalog && outError) {
        *outError = QString::fromUtf8(tt_last_error());
    }
    return m_catalog != nullptr;
}

QString I18n::text(const char *key, const QString &fallback,
                   const char *section) const
{
    if (!m_catalog) {
        return fallback;
    }
    const QByteArray source = fallback.toUtf8();
    size_t len = 0;
    const uint8_t *translated =
        tt_i18n_text(m_catalog, section, key, source.constData(), &len);
    return translated
               ? QString::fromUtf8(reinterpret_cast<const char *>(translated),
                                   static_cast<qsizetype>(len))
               : fallback;
}

QVector<LanguageChoice> I18n::availableLanguages()
{
    QVector<LanguageChoice> out;
    const QDir dir(bundledDirectory());
    const QStringList files = dir.entryList({QStringLiteral("*.lng")}, QDir::Files,
                                            QDir::Name | QDir::IgnoreCase);
    out.reserve(files.size());
    for (const QString &file : files) {
        const QByteArray path = dir.filePath(file).toUtf8();
        TtI18n *catalog = tt_i18n_load(path.constData());
        if (!catalog) {
            continue;
        }
        size_t len = 0;
        const uint8_t *raw =
            tt_i18n_text(catalog, "Info", "language", nullptr, &len);
        const QString name = raw
                                 ? QString::fromUtf8(
                                       reinterpret_cast<const char *>(raw),
                                       static_cast<qsizetype>(len))
                                 : QFileInfo(file).completeBaseName();
        tt_i18n_free(catalog);
        out.push_back({name, QStringLiteral("lang\\") + file});
    }
    return out;
}
