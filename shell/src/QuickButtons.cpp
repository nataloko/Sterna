// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtons.h"

#include <QCoreApplication>

namespace {

/// How much of a command a toolbar button shows when there is no label.
///
/// Short on purpose. A bar of buttons is scanned rather than read, and a
/// button wide enough to hold `conf t / interface eth0 / no shutdown` is a
/// button that has pushed every other one off the window.
constexpr int kCaptionChars = 18;

/// The same for the one-line description, which lands in a tooltip and in the
/// confirmation box and can afford more.
constexpr int kDescribeChars = 120;

/// A single line, with the control characters made visible.
///
/// A command with a CR in it is normal — it is what "send Enter" means — and
/// printing the CR itself would leave a tooltip that says half of what the
/// button does with the cursor back at the start of it.
QString oneLine(const QString &text, int limit)
{
    QString out;
    out.reserve(text.size());
    for (const QChar c : text) {
        if (c == QLatin1Char('\r')) {
            out += QStringLiteral("⏎");
        } else if (c == QLatin1Char('\n')) {
            out += QStringLiteral("␊");
        } else if (c < QLatin1Char(' ') || c == QChar(0x7f)) {
            out += QStringLiteral("·");
        } else {
            out += c;
        }
    }
    if (out.size() > limit) {
        out.truncate(limit - 1);
        out += QStringLiteral("…");
    }
    return out;
}

} // namespace

QString QuickButton::caption() const
{
    if (!label.trimmed().isEmpty()) {
        return label;
    }
    if (kind == TT_QUICK_BUTTON_COMMAND) {
        return QCoreApplication::translate("QuickButton", "Command %1").arg(text);
    }
    return oneLine(text, kDescribeChars);
}

QString QuickButton::shortCaption() const
{
    if (!label.trimmed().isEmpty()) {
        return label;
    }
    return oneLine(caption(), kCaptionChars);
}

QString QuickButton::describe() const
{
    switch (kind) {
    case TT_QUICK_BUTTON_BYTES:
        return QCoreApplication::translate("QuickButton", "Send bytes: %1")
            .arg(oneLine(text, kDescribeChars));
    case TT_QUICK_BUTTON_MACRO:
        return QCoreApplication::translate("QuickButton", "Run macro: %1").arg(text);
    case TT_QUICK_BUTTON_COMMAND:
        return QCoreApplication::translate("QuickButton", "Menu command %1").arg(text);
    case TT_QUICK_BUTTON_TEXT:
    default:
        return QCoreApplication::translate("QuickButton", "Send: %1")
            .arg(oneLine(text, kDescribeChars));
    }
}

bool QuickButton::sendsEnter() const
{
    return (kind == TT_QUICK_BUTTON_TEXT || kind == TT_QUICK_BUTTON_BYTES)
        && text.endsWith(QLatin1Char('\r'));
}

QuickButton QuickButton::withoutEnter() const
{
    QuickButton copy = *this;
    if (copy.sendsEnter()) {
        copy.text.chop(1);
        // The stored form is what runs, so it is the one that has to lose the
        // CR — and it is escaped, so the tail is `$0D` rather than a byte.
        if (copy.value.endsWith(QLatin1String("$0D"), Qt::CaseInsensitive)) {
            copy.value.chop(3);
        } else if (copy.value.endsWith(QLatin1Char('\r'))) {
            copy.value.chop(1);
        }
    }
    return copy;
}

QVector<QuickButton> loadQuickButtons(const QString &settingsPath)
{
    QVector<QuickButton> out;
    TtQuickButtons *list = tt_quick_buttons_load(settingsPath.toUtf8().constData());
    if (!list) {
        return out;
    }
    const size_t count = tt_quick_buttons_len(list);
    out.reserve(static_cast<qsizetype>(count));
    for (size_t i = 0; i < count; i++) {
        const TtQuickButton *b = tt_quick_buttons_at(list, i);
        if (!b) {
            continue;
        }
        QuickButton button;
        button.label = QString::fromUtf8(b->label);
        button.kind = b->kind;
        button.value = QString::fromUtf8(b->value);
        button.text = QString::fromUtf8(b->text);
        button.shortcut = QString::fromUtf8(b->shortcut);
        button.confirm = b->confirm;
        out.append(button);
    }
    tt_quick_buttons_free(list);
    return out;
}

bool saveQuickButtons(const QString &settingsPath,
                      const QVector<QuickButton> &buttons, QString *outError)
{
    // A fresh list rather than the file's: what is saved is what the editor
    // holds, and the core replaces the whole section with it.
    TtQuickButtons *list = tt_quick_buttons_new();
    if (!list) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }

    bool ok = true;
    // The byte arrays outlive the call that reads them, which a temporary from
    // `toUtf8()` inside the struct initialiser would not.
    for (qsizetype i = 0; i < buttons.size() && ok; i++) {
        const QByteArray label = buttons[i].label.toUtf8();
        const QByteArray text = buttons[i].text.toUtf8();
        const QByteArray shortcut = buttons[i].shortcut.toUtf8();
        TtQuickButton entry {};
        entry.label = label.constData();
        entry.kind = buttons[i].kind;
        entry.value = nullptr; // Ignored: the core escapes `text` itself.
        entry.text = text.constData();
        entry.shortcut = shortcut.constData();
        entry.confirm = buttons[i].confirm;
        ok = tt_quick_buttons_set(list, static_cast<size_t>(i), &entry) == TT_OK;
    }
    if (ok) {
        ok = tt_quick_buttons_save(list, settingsPath.toUtf8().constData()) == TT_OK;
    }
    if (!ok && outError) {
        *outError = QString::fromUtf8(tt_last_error());
    }
    tt_quick_buttons_free(list);
    return ok;
}
