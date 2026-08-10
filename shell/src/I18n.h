// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QString>
#include <QVector>

#include "sterna.h"

struct LanguageChoice {
    /// The translator-facing `[Info] language` value.
    QString name;
    /// The shared-INI spelling, such as `lang\ja_JP.lng`.
    QString setting;
};

/// One Tera Term `.lng` catalog, owned on the C++ side of the flat ABI.
class I18n final : public QObject {
public:
    explicit I18n(QObject *parent = nullptr);
    ~I18n() override;

    /// Load `configured`, resolving a relative Windows-style value the way a
    /// copied `TERATERM.INI` expects. A failure clears the old catalog, so the
    /// caller falls back to its source-language text as upstream does.
    bool load(const QString &configured, const QString &settingsPath,
              QString *outError = nullptr);

    /// One translated string, or `fallback`. Embedded NULs survive in the
    /// QString because the ABI supplies the byte length explicitly.
    QString text(const char *key, const QString &fallback,
                 const char *section = "Tera Term") const;

    /// The installed catalogs, named by their own metadata.
    static QVector<LanguageChoice> availableLanguages();

    /// Directory holding the shipped catalogs. Public for integration tests
    /// and for file-dialog callers that may later want a custom language.
    static QString bundledDirectory();

private:
    static QString resolve(const QString &configured, const QString &settingsPath);

    TtI18n *m_catalog = nullptr;
};
