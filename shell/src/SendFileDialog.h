// Sending a file a piece at a time: what to do, and how it is going.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>

#include "Session.h"
#include "sterna.h"

class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QLabel;
class QProgressBar;
class QPushButton;
class QSpinBox;
class QTimer;
class I18n;
class QFormLayout;

/// How to send it — the pace, and which of the two send paths.
///
/// Upstream's `sendfiledlg`, which carries the same four answers and writes
/// them back to the same four settings so that the next send starts where the
/// last one left off (`vtwin.cpp:4290`). The file itself is not here, for the
/// reason `XferOptionsDialog` gives: choosing a file is `QFileDialog`'s job.
class SendFileDialog : public QDialog {
    Q_OBJECT

public:
    /// `session` seeds the fields from `SendfileDelayType`, `SendfileDelayTick`,
    /// `SendfileSize`, `TransBin` and `LocalEcho`. Null is allowed and means the
    /// core's own defaults.
    explicit SendFileDialog(Session *session = nullptr, QWidget *parent = nullptr,
                            const I18n *i18n = nullptr);

    /// The options as configured. Ready for `Session::sendFile`.
    TtSendOptions options() const;

private slots:
    void paceChanged();

private:
    Session *m_session;
    QComboBox *m_pace;
    QSpinBox *m_interval;
    QSpinBox *m_group;
    QCheckBox *m_binary;
    QCheckBox *m_echo;
    QLabel *m_intervalLabel;
    QLabel *m_groupLabel;
};

/// Progress, and the three buttons that matter.
///
/// Modeless for the same reason `XferProgressDialog` is: the send is driven by
/// this window's event loop, so a dialog that blocks it blocks the send. What
/// is different here is that the terminal underneath is only *mute*, not deaf
/// — the far end's answers still reach the screen, which is the whole point of
/// feeding it a line at a time — so the dialog must not cover the terminal it
/// is being watched beside.
class SendProgressDialog : public QDialog {
    Q_OBJECT

public:
    SendProgressDialog(const QString &title, QWidget *parent = nullptr,
                       const I18n *i18n = nullptr);

    void update(const SendProgress &progress);
    /// Show how it ended and turn Stop into Close. The dialog stays up: a send
    /// that was cut off has something to say about how far it got, and a window
    /// that vanished at that moment would say it to nobody.
    void finish(const SendResult &result);

signals:
    void cancelled();
    void pauseToggled(bool paused);
    /// Time to read the progress again.
    ///
    /// The dialog asks rather than being told, because being told would mean an
    /// event per piece — one per *character* on a per-character pace, which is
    /// a thousand a second asking a label to repaint itself. Its own slow timer
    /// is the whole reason there is no progress signal on the core's side
    /// either.
    void poll();

private:
    QLabel *m_file;
    QLabel *m_stats;
    QProgressBar *m_bar;
    QPushButton *m_pause;
    QDialogButtonBox *m_buttons;
    QTimer *m_poll;
    const I18n *m_i18n;
    bool m_done = false;
};
