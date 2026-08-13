// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Highlights.h"

#include <QCoreApplication>
#include <QStringList>

namespace {

/// The ABI's packed colour, or its "leave this one alone" sentinel.
quint32 packColor(const QColor &color)
{
    if (!color.isValid()) {
        return TT_HIGHLIGHT_NO_COLOR;
    }
    return (static_cast<quint32>(color.red()) << 16)
           | (static_cast<quint32>(color.green()) << 8)
           | static_cast<quint32>(color.blue());
}

QColor unpackColor(quint32 value)
{
    if (value == TT_HIGHLIGHT_NO_COLOR) {
        return QColor();
    }
    return QColor(int((value >> 16) & 0xff), int((value >> 8) & 0xff), int(value & 0xff));
}

} // namespace

QString QuickHighlight::caption() const
{
    return label.isEmpty() ? pattern : label;
}

bool QuickHighlight::paints() const
{
    return fore.isValid() || back.isValid() || style != 0;
}

QString QuickHighlight::describe() const
{
    QStringList does;
    if (fore.isValid()) {
        does << QCoreApplication::translate("Highlights", "text %1").arg(fore.name());
    }
    if (back.isValid()) {
        does << QCoreApplication::translate("Highlights", "background %1").arg(back.name());
    }
    if (style & TT_HIGHLIGHT_BOLD) {
        does << QCoreApplication::translate("Highlights", "bold");
    }
    if (style & TT_HIGHLIGHT_UNDERLINE) {
        does << QCoreApplication::translate("Highlights", "underline");
    }
    if (style & TT_HIGHLIGHT_REVERSE) {
        does << QCoreApplication::translate("Highlights", "reverse");
    }
    if (does.isEmpty()) {
        does << QCoreApplication::translate("Highlights", "nothing yet");
    }

    const QString what = literal
                             ? QCoreApplication::translate("Highlights", "text \"%1\"").arg(pattern)
                             : QCoreApplication::translate("Highlights", "/%1/").arg(pattern);
    const QString where = wholeLine
                              ? QCoreApplication::translate("Highlights", "the whole line")
                              : QCoreApplication::translate("Highlights", "the match");
    return QCoreApplication::translate("Highlights", "%1 → %2: %3")
        .arg(what, where, does.join(QStringLiteral(", ")));
}

QVector<QuickHighlight> loadHighlights(const QString &settingsPath)
{
    QVector<QuickHighlight> out;
    const QByteArray path = settingsPath.toUtf8();
    TtHighlights *list = tt_highlights_load(path.constData());
    if (!list) {
        return out;
    }
    const size_t n = tt_highlights_len(list);
    out.reserve(int(n));
    for (size_t i = 0; i < n; i++) {
        const TtHighlight *rule = tt_highlights_at(list, i);
        if (!rule) {
            continue;
        }
        QuickHighlight made;
        made.label = QString::fromUtf8(rule->label);
        made.pattern = QString::fromUtf8(rule->pattern);
        made.literal = rule->literal;
        made.ignoreCase = rule->ignore_case;
        made.fore = unpackColor(rule->fore);
        made.back = unpackColor(rule->back);
        made.style = rule->style;
        made.wholeLine = rule->scope == TT_HIGHLIGHT_LINE;
        made.group = rule->group;
        made.enabled = rule->enabled;
        out.append(made);
    }
    tt_highlights_free(list);
    return out;
}

bool saveHighlights(const QString &settingsPath, const QVector<QuickHighlight> &rules,
                    QString *error)
{
    TtHighlights *list = tt_highlights_new();
    if (!list) {
        if (error) {
            *error = QString::fromUtf8(tt_last_error());
        }
        return false;
    }
    bool ok = true;
    for (int i = 0; i < rules.size() && ok; i++) {
        const QuickHighlight &rule = rules.at(i);
        // Named locals: the byte arrays outlive the call that reads them,
        // which a temporary from `toUtf8()` inside the struct would not.
        const QByteArray label = rule.label.toUtf8();
        const QByteArray pattern = rule.pattern.toUtf8();
        TtHighlight entry = {};
        entry.label = label.constData();
        entry.pattern = pattern.constData();
        entry.literal = rule.literal;
        entry.ignore_case = rule.ignoreCase;
        entry.fore = packColor(rule.fore);
        entry.back = packColor(rule.back);
        entry.style = rule.style;
        entry.scope = rule.wholeLine ? TT_HIGHLIGHT_LINE : TT_HIGHLIGHT_MATCH;
        entry.group = rule.group;
        entry.enabled = rule.enabled;
        ok = tt_highlights_set(list, size_t(i), &entry) == TT_OK;
    }
    if (ok) {
        const QByteArray path = settingsPath.toUtf8();
        ok = tt_highlights_save(list, path.constData()) == TT_OK;
    }
    if (!ok && error) {
        *error = QString::fromUtf8(tt_last_error());
    }
    tt_highlights_free(list);
    return ok;
}

bool checkHighlightPattern(const QString &pattern, bool literal, bool ignoreCase, QString *error)
{
    const QByteArray bytes = pattern.toUtf8();
    if (tt_highlight_check(bytes.constData(), literal, ignoreCase) == TT_OK) {
        return true;
    }
    if (error) {
        *error = QString::fromUtf8(tt_last_error());
    }
    return false;
}
