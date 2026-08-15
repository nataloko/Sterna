// The log dialog: where the capture goes and how it is written.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>

#include "Session.h"
#include "sterna.h"

class QCheckBox;
class QComboBox;
class QLineEdit;
class QRadioButton;
class QSpinBox;
class QTimer;
class I18n;

/// Everything Tera Term's `File > Log...` asks, in one modal box.
///
/// **Upstream's is a plain dialog too.** Tera Term 4 customised the Win32
/// common save dialog with an option strip; Tera Term 5 replaced it with
/// `IDD_LOGDLG` (`logdlg.cpp:267`) — a filename field, a `...` button that
/// opens the real picker, and the options underneath. That is the shape here,
/// which is also the only shape Qt can offer: the desktop's file dialog is a
/// portal on the other side of D-Bus and nothing can be bolted to it.
///
/// Two of upstream's controls are missing on purpose. **Hide dialog** hides
/// the logging status window, and Sterna has none — a terminal's own status
/// line carries the `REC` counter instead, so there is nothing for the tick to
/// act on. The **UTF-8/UTF-16LE/UTF-16BE** combo has no meaning here either: a
/// log is written from a Rust `String`, so it is UTF-8 and only the mark is a
/// question.
///
/// One is here that upstream keeps on its Setup page: **rotation**. Whether a
/// capture should roll over is a fact about the capture — a week on a console
/// against ten seconds of a boot log — so it is asked where the capture is
/// started, seeded from the same three keys the Setup page edits and written
/// back to them.
class LogOptionsDialog : public QDialog {
    Q_OBJECT

public:
    /// `session` supplies the starting values and the expanded file name.
    /// Null is allowed — a dialog put up with nothing open is still a dialog —
    /// and then everything is the core's own default.
    explicit LogOptionsDialog(Session *session = nullptr, QWidget *parent = nullptr,
                              const I18n *i18n = nullptr);

    /// The file to log to, absolute.
    QString path() const;
    /// The options as configured. Ready for `Session::startLog`.
    TtLogOptions options() const;

    /// Write back every control that has a `TERATERM.INI` key behind it.
    ///
    /// Upstream's `SetLogFlags` (`logdlg.cpp:82`) does the same to `ts` — live
    /// only, so Setup > Save still decides whether any of it reaches the file.
    /// Its own source calls this out as questionable, since a per-session
    /// dialog is moving global settings; it is reproduced because the
    /// alternative is a dialog that forgets what you told it last time.
    void applySettings() const;

private slots:
    /// Upstream's `ArrangeControls` (`logdlg.cpp:167`): which controls a
    /// choice makes meaningless.
    void refreshEnabled();

private:
    /// Re-expand the name part of the field against the clock.
    ///
    /// On a 1 s timer while the user has not touched the field
    /// (`logdlg.cpp:481`), so a `%H%M%S` template names the file the OK button
    /// is pressed on rather than the one the dialog opened on. The directory
    /// is left alone: it is the user's answer, not the clock's.
    void refreshName();
    void browse();

    Session *m_session;
    const I18n *m_i18n;
    /// Set once the field is edited by hand, which stops [`refreshName`].
    bool m_nameEdited = false;

    QLineEdit *m_file;
    QRadioButton *m_overwrite;
    QRadioButton *m_append;
    QRadioButton *m_textMode;
    QRadioButton *m_binaryMode;
    QCheckBox *m_bom;
    QCheckBox *m_plainText;
    QCheckBox *m_includeScreen;
    QCheckBox *m_timestamp;
    QComboBox *m_timestampType;
    QCheckBox *m_rotate;
    QSpinBox *m_rotateSize;
    QComboBox *m_rotateUnit;
    QSpinBox *m_rotateKeep;
    QTimer *m_tick;
};
