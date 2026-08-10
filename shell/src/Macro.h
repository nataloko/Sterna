// A TTL macro, running against this window's session.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QObject>
#include <QPoint>
#include <QString>
#include <QStringList>

#include "sterna.h"

class QDialog;
class QLabel;
#ifdef Q_OS_WIN
class QWinEventNotifier;
#else
class QSocketNotifier;
#endif
class QWidget;
class I18n;
class Session;

/// One running macro, and the window it asks things of.
///
/// The interpreter runs on a thread inside the core and blocks whenever it
/// wants something from out here; this class is the other half of that bargain.
/// It waits on the macro's descriptor with a `QSocketNotifier` — the same
/// no-timer arrangement `Session` uses, and for the same reason — and when it
/// fires, `tt_macro_service` runs whatever the macro asked for **on this
/// thread**. So a `messagebox` is an ordinary modal dialog: it spins a nested
/// event loop, the window goes on painting, and the script is parked on its own
/// thread until the user answers. Windows uses the same contract with a
/// waitable event and `QWinEventNotifier`.
///
/// The dialogs it can raise are the eleven the language has. What it cannot do
/// is listed at the bottom of `Macro.cpp`, with a reason each.
class Macro : public QObject {
    Q_OBJECT

public:
    /// `window` is what the dialogs are parented to, and what `showtt` and
    /// `getttpos` act on. It may be null — a `/V` session has no window — and
    /// the dialogs then open parentless, which is what upstream does too: a
    /// windowless Tera Term is still driven by a `ttpmacro.exe` that has its
    /// own dialogs. The two commands that are *about* the window refuse
    /// instead, because there is nothing to measure or raise.
    Macro(Session *session, QWidget *window, QObject *parent = nullptr,
          const I18n *i18n = nullptr);
    ~Macro() override;

    /// Start `args`' macro — `ttpmacro`'s command line, already split, without
    /// the program name. The first word that is not a switch names the file.
    ///
    /// False and `outError` when it could not be started at all: no file
    /// named, a file that will not open, or a thread that would not start.
    /// Everything after that is the macro's own business and arrives as
    /// `finished`.
    bool start(const QStringList &args, QString *outError);

    /// Ask it to stop at its next line — upstream's End button. Not immediate,
    /// and not an interruption of a dialog that is already up.
    void cancel();

    bool running() const;
    /// The last `setexitcode`, or 0.
    int exitCode() const;
    /// The macro's file, for a status line. Empty when none is running.
    QString name() const { return m_name; }

    // --- what a running macro asks of the window -----------------------------
    //
    // Called from the ABI's callbacks, on this thread, from inside `service`.
    // Public because the callbacks are free functions with C linkage; nothing
    // else should call them.

    bool showError(const TtMacroError *err);
    TtDialogEnd showMessage(const QString &text, const QString &title, bool yesNo);
    void showStatus(const QString &text, const QString &title);
    void closeStatus();
    bool raiseStatus();
    TtDialogEnd showList(const QString &text, const QString &title,
                         const QStringList &items, int selected,
                         const TtListBoxOpts &opts, int *outIndex);
    TtDialogEnd showInput(const QString &text, const QString &title,
                          const QString &initial, bool password, QString *outText);
    QString chooseFile(const QString &title, bool save, const QString &dir);
    QString chooseDirectory(const QString &title, const QString &dir);
    void setDialogPos(const TtDialogPos *pos);
    bool showWindow(TtShowWindow which);
    bool geometry(TtWindowGeometry *out) const;
    void enableKeyboard(bool on);

signals:
    /// The macro ended, for any reason — finished, stopped, or the window
    /// closed on it.
    void finished(int exitCode);
    /// `enablekeyb`. The window gates its terminal on this.
    void keyboardEnabled(bool on);
    /// Worth a line in the status bar.
    void notice(const QString &text);

private slots:
    void onServiceable();

private:
    /// Run the macro's pending jobs, then let the session catch up.
    void service();
    /// Free the handle, detach the terminal, and announce it.
    void stop();
    /// Put a dialog where `setdlgpos` asked, if it asked. Called after
    /// `adjustSize` so the placement is against a laid-out window — and
    /// `adjustSize` matters for its own reason: a dialog that has never been
    /// shown lays out wrong in `QWidget::grab`.
    void place(QWidget *dialog) const;

    Session *m_session;
    QWidget *m_window;
    const I18n *m_i18n;
    TtMacro *m_macro = nullptr;
#ifdef Q_OS_WIN
    QWinEventNotifier *m_notifier = nullptr;
#else
    QSocketNotifier *m_notifier = nullptr;
#endif
    QString m_name;

    /// `statusbox`'s modeless window, while one is up. Owned here: the macro
    /// closes it with `closesbox`, and nothing else may.
    QDialog *m_statusBox = nullptr;
    QLabel *m_statusLabel = nullptr;

    /// `setdlgpos`, if it has been called.
    bool m_hasPos = false;
    TtDialogPos m_pos {};
};
