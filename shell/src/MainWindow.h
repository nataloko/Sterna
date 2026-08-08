// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QMainWindow>
#include <QString>

#include "termitta.h"

class QLabel;
class Session;
class TerminalView;

/// One window, one session.
///
/// Tabs and multiple sessions are Stage 3. Keeping it one-to-one now means the
/// menu actions talk to a member rather than to a notion of a "current"
/// session, which is the thing that has to be threaded through everything
/// later — and threading it through six actions then is cheaper than carrying
/// the indirection through the whole of Stage 1.
class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    MainWindow();

    /// Connect at startup, for the command line.
    void connectSerial(const QString &path, const TtSerialParams &params);

private slots:
    void showConnectDialog();
    void disconnectPort();
    void sendBreak();
    void chooseFont();
    void onTitleChanged(const QString &title);
    void onNotice(const QString &text);
    void onConnectionChanged();

private:
    void buildMenus();
    void updateStatus();

    Session *m_session;
    TerminalView *m_view;
    QLabel *m_status;
    QAction *m_disconnectAction = nullptr;
    QAction *m_breakAction = nullptr;

    // Remembered so reopening the dialog does not start from the defaults
    // again. A session profile on disk is Stage 2's, with the settings schema.
    QString m_lastPort;
    TtSerialParams m_lastParams;
};
