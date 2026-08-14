// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>

class QPlainTextEdit;
class I18n;

/// `ConfirmChangePaste`'s dialog — `clipboarddlg` (`clipboar.c:174`).
///
/// The reason it exists is that a newline in pasted text is a *command*: a
/// terminal has no way to tell the host that what arrived was pasted, so a
/// shell runs every line of it the moment it lands. Showing the text and
/// letting it be edited first is the only defence a terminal can offer, and
/// upstream ships it on.
///
/// It is deliberately editable rather than a yes/no box: upstream's returns
/// the edited string, and the common use is deleting the trailing newline off
/// something copied out of a wiki so the command can be read before it runs.
class PasteDialog : public QDialog {
    Q_OBJECT

public:
    PasteDialog(const QString &text, QSize size, QWidget *parent = nullptr,
                const I18n *i18n = nullptr);

    /// The text as it stands, which may not be the text it was given.
    QString text() const;

    /// Does this paste need confirming? `CheckClipboardContentW`
    /// (`clipboar.c:126`), minus its `ConfirmChangePaste` gate, which the
    /// caller has already applied.
    ///
    /// The two paths ask different questions. A plain paste is confirmed when
    /// the text *contains* a line break (`clipboar.c:157` — `wcscspn` against
    /// `\r\n`). `Paste<CR>` is confirmed when one is being **added**, so
    /// `confirmCr` (`ConfirmChangePasteCR`) decides it alone and the text's
    /// own content has no say — upstream's own comment at `:136` weighs
    /// whether that is right and keeps it.
    ///
    /// Either way `dictionary` is consulted afterwards and can only turn
    /// confirmation on: a file of one string per line, matched as substrings,
    /// which is how a site adds `rm -rf` to the list. An unreadable or empty
    /// path is no dictionary and not an error, which is upstream's
    /// `LoadFileWW` returning NULL.
    static bool shouldConfirm(const QString &text, const QString &dictionary,
                              bool addCr = false, bool confirmCr = true);

private:
    QPlainTextEdit *m_edit;
};
