// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtons.h"

#include <QCoreApplication>
#include <QLocale>

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

QString QuickButton::repeatSummary() const
{
    if (!repeats()) {
        return {};
    }
    // Seconds, because that is the unit somebody thinks in and `xx.x` is what
    // the editor asks for; the file keeps milliseconds so it needs no decimal
    // separator and therefore no locale.
    const QString seconds = QLocale().toString(intervalMs / 1000.0, 'g', 4);
    if (repeatsForever()) {
        return QCoreApplication::translate("QuickButton",
                                           "Repeats every %1 s until stopped")
            .arg(seconds);
    }
    return QCoreApplication::translate("QuickButton", "Sends %1 times, every %2 s")
        .arg(repeat)
        .arg(seconds);
}

QString QuickButton::describe() const
{
    QString what;
    switch (kind) {
    case TT_QUICK_BUTTON_BYTES:
        what = QCoreApplication::translate("QuickButton", "Send bytes: %1")
                   .arg(oneLine(text, kDescribeChars));
        break;
    case TT_QUICK_BUTTON_MACRO:
        what = QCoreApplication::translate("QuickButton", "Run macro: %1").arg(text);
        break;
    case TT_QUICK_BUTTON_COMMAND:
        what = QCoreApplication::translate("QuickButton", "Menu command %1").arg(text);
        break;
    case TT_QUICK_BUTTON_TEXT:
    default:
        what = QCoreApplication::translate("QuickButton", "Send: %1")
                   .arg(oneLine(text, kDescribeChars));
        break;
    }
    // On its own line rather than appended, because this is the half that
    // decides whether pressing it is a keystroke or a quarter of an hour of
    // them — and it is the half the confirmation exists to show.
    const QString repeated = repeatSummary();
    return repeated.isEmpty() ? what : what + QLatin1Char('\n') + repeated;
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
        button.repeat = b->repeat;
        button.intervalMs = b->interval_ms;
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
        entry.repeat = buttons[i].repeat;
        entry.interval_ms = buttons[i].intervalMs;
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
