// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "PasteDialog.h"

#include <QDialogButtonBox>
#include <QFile>
#include <QFontDatabase>
#include <QLabel>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QTextStream>
#include <QVBoxLayout>

#include "I18n.h"

PasteDialog::PasteDialog(const QString &text, QSize size, QWidget *parent,
                         const I18n *i18n)
    : QDialog(parent)
{
    setWindowTitle(tr("Confirm paste"));
    setModal(true);

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(new QLabel(tr("Send this to the host?"), this));

    m_edit = new QPlainTextEdit(this);
    // The text a terminal is about to send, so it is shown in the face a
    // terminal sends it in — a proportional font would hide the two spaces
    // somebody is checking for.
    m_edit->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    m_edit->setLineWrapMode(QPlainTextEdit::NoWrap);
    m_edit->setPlainText(text);
    layout->addWidget(m_edit);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->button(QDialogButtonBox::Ok)
        ->setText(i18n ? i18n->text("BTN_OK", tr("OK")) : tr("OK"));
    buttons->button(QDialogButtonBox::Cancel)
        ->setText(i18n ? i18n->text("BTN_CANCEL", tr("Cancel"))
                       : tr("Cancel"));
    // Escape and the title bar's close both reach Cancel, which is the answer
    // that sends nothing — the safe way round for a box that exists to stop
    // something being sent by accident.
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    if (size.width() > 0 && size.height() > 0) {
        resize(size);
    }
}

QString PasteDialog::text() const
{
    return m_edit->toPlainText();
}

bool PasteDialog::shouldConfirm(const QString &text, const QString &dictionary)
{
    if (text.contains(QLatin1Char('\r')) || text.contains(QLatin1Char('\n'))) {
        return true;
    }
    if (dictionary.isEmpty()) {
        return false;
    }
    // One string per line, and a *substring* match: `search_dictW`
    // (`clipboar.c:71`) runs `wcsstr` for each line and stops at the first
    // hit. A file that cannot be read is no dictionary rather than an error.
    QFile file(dictionary);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return false;
    }
    QTextStream in(&file);
    while (!in.atEnd()) {
        const QString needle = in.readLine();
        if (!needle.isEmpty() && text.contains(needle)) {
            return true;
        }
    }
    return false;
}
