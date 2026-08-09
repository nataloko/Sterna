// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Macro.h"

#include <QApplication>
#include <QClipboard>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFontMetrics>
#include <QInputDialog>
#include <QLabel>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QScreen>
#include <QSocketNotifier>
#include <QVBoxLayout>
#include <QVector>

#include "Session.h"

namespace {

Macro *self(void *user) { return static_cast<Macro *>(user); }

QString str(const char *s) { return QString::fromUtf8(s ? s : ""); }

} // namespace

// The ABI's callbacks. Free functions with C linkage rather than static
// members, because that is the type the header declares — and `static`, so
// nothing outside this file can reach them.
extern "C" {

static bool cbError(void *user, const TtMacroError *err)
{
    return self(user)->showError(err);
}

static TtDialogEnd cbMessageBox(void *user, const char *text, const char *title)
{
    return self(user)->showMessage(str(text), str(title), false);
}

static TtDialogEnd cbYesNoBox(void *user, const char *text, const char *title)
{
    return self(user)->showMessage(str(text), str(title), true);
}

static TtStatus cbStatusBox(void *user, const char *text, const char *title)
{
    self(user)->showStatus(str(text), str(title));
    return TT_OK;
}

static TtStatus cbCloseStatusBox(void *user)
{
    self(user)->closeStatus();
    return TT_OK;
}

static TtStatus cbBringUpStatusBox(void *user)
{
    return self(user)->raiseStatus() ? TT_OK : TT_ERR_UNSUPPORTED;
}

static TtDialogEnd cbListBox(void *user, const char *text, const char *title,
                             const char *const *items, size_t count,
                             size_t selected, const TtListBoxOpts *opts,
                             size_t *outIndex)
{
    QStringList list;
    list.reserve(static_cast<int>(count));
    for (size_t i = 0; i < count; i++) {
        list << str(items[i]);
    }
    int index = static_cast<int>(selected);
    const TtDialogEnd end = self(user)->showList(str(text), str(title), list,
                                                 index, *opts, &index);
    if (end == TT_DIALOG_OK && index >= 0) {
        *outIndex = static_cast<size_t>(index);
    }
    return end;
}

static TtDialogEnd cbInputBox(void *user, const char *text, const char *title,
                              const char *initial, bool password,
                              const char **outText)
{
    // One macro, one thread, one dialog at a time — so a single buffer per
    // callback is enough, and the core copies out of it before returning.
    static QByteArray answer;
    QString typed;
    const TtDialogEnd end = self(user)->showInput(str(text), str(title),
                                                  str(initial), password, &typed);
    if (end == TT_DIALOG_OK) {
        answer = typed.toUtf8();
        *outText = answer.constData();
    }
    return end;
}

static const char *cbFilenameBox(void *user, const char *title, bool save,
                                 const char *initDir)
{
    static QByteArray chosen;
    const QString path = self(user)->chooseFile(str(title), save, str(initDir));
    if (path.isEmpty()) {
        return nullptr;
    }
    chosen = path.toUtf8();
    return chosen.constData();
}

static const char *cbDirnameBox(void *user, const char *title, const char *initDir)
{
    static QByteArray chosen;
    const QString path = self(user)->chooseDirectory(str(title), str(initDir));
    if (path.isEmpty()) {
        return nullptr;
    }
    chosen = path.toUtf8();
    return chosen.constData();
}

static void cbSetDialogPos(void *user, const TtDialogPos *pos)
{
    self(user)->setDialogPos(pos);
}

static TtStatus cbBeep(void *user, TtBeepSound sound)
{
    (void)user;
    // Five of the six are Windows system sounds and the sixth is the default
    // beep; nothing on this desktop distinguishes them, so all six are the
    // one beep the toolkit has.
    (void)sound;
    QApplication::beep();
    return TT_OK;
}

static TtStatus cbShowWindow(void *user, TtShowWindow which)
{
    return self(user)->showWindow(which) ? TT_OK : TT_ERR_UNSUPPORTED;
}

static bool cbGeometry(void *user, TtWindowGeometry *out)
{
    return self(user)->geometry(out);
}

static TtStatus cbEnableKeyboard(void *user, bool on)
{
    self(user)->enableKeyboard(on);
    return TT_OK;
}

static const char *cbClipboardText(void *user)
{
    (void)user;
    static QByteArray text;
    const QString s = QApplication::clipboard()->text();
    if (s.isEmpty()) {
        // Upstream's failed `GetClipboardData`, which is what an empty
        // clipboard amounts to — `clipb2var` reports it rather than handing
        // back nothing and calling it success.
        return nullptr;
    }
    text = s.toUtf8();
    return text.constData();
}

static bool cbSetClipboardText(void *user, const char *text)
{
    (void)user;
    QApplication::clipboard()->setText(str(text));
    return true;
}

} // extern "C"

Macro::Macro(Session *session, QWidget *window, QObject *parent)
    : QObject(parent)
    , m_session(session)
    , m_window(window)
{
}

Macro::~Macro()
{
    // Ordered like `Session`'s: the notifier watches a descriptor the macro
    // owns, and freeing the macro closes it.
    delete m_notifier;
    m_notifier = nullptr;
    if (m_macro) {
        tt_macro_free(m_macro);
        m_macro = nullptr;
        m_session->unlinkMacro();
    }
    delete m_statusBox;
}

bool Macro::start(const QStringList &args, QString *outError)
{
    if (m_macro) {
        if (outError) {
            *outError = tr("A macro is already running.");
        }
        return false;
    }

    // The array has to outlive the call, which is the only reason these are
    // held rather than built inline.
    QVector<QByteArray> owned;
    QVector<const char *> argv;
    owned.reserve(args.size());
    for (const QString &a : args) {
        owned << a.toUtf8();
        argv << owned.last().constData();
    }
    argv << nullptr;

    TtMacroUi ui = {};
    ui.user = this;
    ui.error = cbError;
    ui.message_box = cbMessageBox;
    ui.yes_no_box = cbYesNoBox;
    ui.status_box = cbStatusBox;
    ui.close_status_box = cbCloseStatusBox;
    ui.bringup_status_box = cbBringUpStatusBox;
    ui.list_box = cbListBox;
    ui.input_box = cbInputBox;
    ui.filename_box = cbFilenameBox;
    ui.dirname_box = cbDirnameBox;
    ui.set_dialog_pos = cbSetDialogPos;
    ui.beep = cbBeep;
    ui.show_window = cbShowWindow;
    ui.terminal_geometry = cbGeometry;
    ui.enable_keyboard = cbEnableKeyboard;
    ui.clipboard_text = cbClipboardText;
    ui.set_clipboard_text = cbSetClipboardText;

    m_macro = tt_macro_start(m_session->handle(), argv.constData(), &ui);
    if (!m_macro) {
        if (outError) {
            *outError = QString::fromUtf8(tt_last_error());
        }
        return false;
    }

    // The first non-switch argument, which is what the core opened.
    m_name.clear();
    for (const QString &a : args) {
        if (!a.startsWith(QLatin1Char('/'))) {
            m_name = QFileInfo(a).fileName();
            break;
        }
    }

    const int fd = tt_macro_poll_fd(m_macro);
    if (fd >= 0) {
        m_notifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
        connect(m_notifier, &QSocketNotifier::activated, this, &Macro::onServiceable);
    }
    // It has been running since `tt_macro_start` returned, so anything it has
    // already asked for is waiting — and a one-line macro may have finished
    // already, in which case this is where that is noticed.
    service();
    return true;
}

void Macro::cancel()
{
    if (m_macro) {
        tt_macro_cancel(m_macro);
    }
}

bool Macro::running() const { return m_macro && tt_macro_running(m_macro); }

int Macro::exitCode() const { return m_macro ? tt_macro_exit_code(m_macro) : 0; }

void Macro::onServiceable() { service(); }

void Macro::service()
{
    if (!m_macro) {
        return;
    }
    // A dialog spins a nested event loop, and a `QSocketNotifier` is level
    // triggered — so without this the loop inside the dialog would call back
    // in here and run a second dialog inside the first. The same re-entrancy
    // the SSH host-key prompt needed a guard for.
    if (m_notifier) {
        m_notifier->setEnabled(false);
    }
    tt_macro_service(m_macro, m_session->handle());
    if (m_notifier) {
        m_notifier->setEnabled(true);
    }

    // The jobs that just ran changed the session — sent, connected, printed —
    // and its own descriptor said nothing about any of it.
    m_session->poll();

    if (!tt_macro_running(m_macro)) {
        stop();
    }
}

void Macro::stop()
{
    if (!m_macro) {
        return;
    }
    // Disabled before `deleteLater` and not simply deleted: this runs inside
    // the notifier's own signal, and the handle it watches is about to be
    // freed with the descriptor under it.
    if (m_notifier) {
        m_notifier->setEnabled(false);
        m_notifier->deleteLater();
        m_notifier = nullptr;
    }
    const int code = tt_macro_exit_code(m_macro);
    tt_macro_free(m_macro);
    m_macro = nullptr;
    m_session->unlinkMacro();
    closeStatus();

    // A keyboard locked by `enablekeyb 0` is released here, and upstream does
    // not do this — `KeybEnabled` is only put back by Control > Reset
    // terminal (`vtwin.cpp:4874`), which this port does not have, so a macro
    // that died between the two calls would leave a terminal nobody can type
    // into. `enablekeyb.html` describes the lock as lasting "while the macro
    // is sending the data", so this follows the manual, which is what the
    // port does in the three other places the two disagree.
    emit keyboardEnabled(true);
    m_name.clear();
    emit finished(code);
}

// --- what a running macro asks of the window ---------------------------------

void Macro::place(QWidget *dialog) const
{
    if (!m_hasPos || !dialog) {
        return;
    }
    // Against a laid-out dialog: an anchored placement needs its size, and a
    // dialog that has never been shown does not have one yet.
    dialog->adjustSize();
    if (m_pos.position == 0) {
        dialog->move(m_pos.x, m_pos.y);
        return;
    }

    QRect area;
    if (m_pos.position >= 6 && m_window && m_window->isVisible()
        && !m_window->isMinimized()) {
        area = m_window->frameGeometry();
    } else if (QScreen *screen = m_window ? m_window->screen()
                                          : QApplication::primaryScreen()) {
        // Upstream falls back to the display when there is no window to
        // measure, or when it is minimised or hidden (`ttmdlg.cpp:247`) —
        // a window nobody can see is not a position.
        area = screen->availableGeometry();
    } else {
        return;
    }

    const QSize size = dialog->size();
    QPoint at;
    switch ((m_pos.position - 1) % 5) {
    case 0:
        at = area.topLeft();
        break;
    case 1:
        at = QPoint(area.right() - size.width(), area.top());
        break;
    case 2:
        at = QPoint(area.left(), area.bottom() - size.height());
        break;
    case 3:
        at = QPoint(area.right() - size.width(), area.bottom() - size.height());
        break;
    default:
        at = area.center() - QPoint(size.width() / 2, size.height() / 2);
        break;
    }
    dialog->move(at + QPoint(m_pos.offset_x, m_pos.offset_y));
}

void Macro::setDialogPos(const TtDialogPos *pos)
{
    m_hasPos = pos != nullptr;
    if (pos) {
        m_pos = *pos;
    }
}

bool Macro::showError(const TtMacroError *err)
{
    QMessageBox box(m_window);
    box.setIcon(QMessageBox::Warning);
    box.setWindowTitle(tr("Macro error"));
    box.setText(str(err->message));
    // `code` 0 is an error from a language `ttmparse.h` never numbered, which
    // is every Lua one — and those carry their own `file:line:` in the message
    // rather than in the fields. Repeating a position the message already has,
    // as "line 0" and a blank line, would be worse than saying nothing.
    if (err->code != 0) {
        const QString where = tr("%1, line %2")
                                  .arg(QFileInfo(str(err->file)).fileName())
                                  .arg(err->line_no);
        box.setInformativeText(tr("%1\n\n%2").arg(where, str(err->line).trimmed()));
    }
    // Upstream's two buttons, and Continue is the one that is not the
    // default: a script that has gone wrong should stop unless the user says
    // otherwise.
    QPushButton *stop = box.addButton(tr("Stop"), QMessageBox::AcceptRole);
    box.addButton(tr("Continue"), QMessageBox::RejectRole);
    box.setDefaultButton(stop);
    place(&box);
    box.exec();
    return box.clickedButton() == stop;
}

TtDialogEnd Macro::showMessage(const QString &text, const QString &title, bool yesNo)
{
    QMessageBox box(m_window);
    box.setWindowTitle(title.isEmpty() ? tr("Macro") : title);
    box.setText(text);
    if (!yesNo) {
        box.setIcon(QMessageBox::Information);
        box.addButton(QMessageBox::Ok);
        place(&box);
        box.exec();
        return TT_DIALOG_OK;
    }

    box.setIcon(QMessageBox::Question);
    QPushButton *yes = box.addButton(QMessageBox::Yes);
    box.addButton(QMessageBox::No);
    place(&box);
    box.exec();
    // **No and the close box are one answer here and two upstream**, where
    // closing a `yesnobox` ends the macro and No does not. Qt gives Escape and
    // the title bar's close to the reject-role button, so the two cannot be
    // told apart; the safer of them is No, which lets the script decide.
    return box.clickedButton() == yes ? TT_DIALOG_OK : TT_DIALOG_CANCEL;
}

void Macro::showStatus(const QString &text, const QString &title)
{
    if (!m_statusBox) {
        m_statusBox = new QDialog(m_window);
        // Modeless on purpose: the macro goes on running and updates the text
        // as it does, which is the whole difference between this and
        // `messagebox`.
        m_statusBox->setWindowModality(Qt::NonModal);
        auto *layout = new QVBoxLayout(m_statusBox);
        m_statusLabel = new QLabel(m_statusBox);
        m_statusLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
        m_statusLabel->setWordWrap(true);
        layout->addWidget(m_statusLabel);
    }
    m_statusBox->setWindowTitle(title.isEmpty() ? tr("Macro") : title);
    m_statusLabel->setText(text);
    place(m_statusBox);
    m_statusBox->show();
    m_statusBox->raise();
}

void Macro::closeStatus()
{
    if (m_statusBox) {
        m_statusBox->hide();
    }
}

bool Macro::raiseStatus()
{
    if (!m_statusBox || !m_statusBox->isVisible()) {
        return false;
    }
    m_statusBox->raise();
    m_statusBox->activateWindow();
    return true;
}

TtDialogEnd Macro::showList(const QString &text, const QString &title,
                            const QStringList &items, int selected,
                            const TtListBoxOpts &opts, int *outIndex)
{
    QDialog dialog(m_window);
    dialog.setWindowTitle(title.isEmpty() ? tr("Macro") : title);
    auto *layout = new QVBoxLayout(&dialog);
    if (!text.isEmpty()) {
        auto *label = new QLabel(text, &dialog);
        label->setWordWrap(true);
        layout->addWidget(label);
    }
    auto *list = new QListWidget(&dialog);
    list->addItems(items);
    if (selected >= 0 && selected < items.size()) {
        list->setCurrentRow(selected);
    }
    layout->addWidget(list);
    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    if (opts.double_click) {
        connect(list, &QListWidget::itemDoubleClicked, &dialog, &QDialog::accept);
    }

    // `listboxsize=WxH` is in characters, so it is measured in the dialog's
    // own font rather than the terminal's — the box is a dialog, not a view
    // of the session.
    if (opts.width > 0 && opts.height > 0) {
        const QFontMetrics fm(list->font());
        list->setMinimumSize(fm.horizontalAdvance(QLatin1Char('0'))
                                 * static_cast<int>(opts.width),
                             fm.lineSpacing() * static_cast<int>(opts.height));
    }
    if (opts.maximized) {
        dialog.showMaximized();
    } else if (opts.minimized) {
        dialog.showMinimized();
    }
    place(&dialog);

    if (dialog.exec() != QDialog::Accepted) {
        return TT_DIALOG_CANCEL;
    }
    *outIndex = list->currentRow();
    return *outIndex >= 0 ? TT_DIALOG_OK : TT_DIALOG_CANCEL;
}

TtDialogEnd Macro::showInput(const QString &text, const QString &title,
                             const QString &initial, bool password,
                             QString *outText)
{
    QInputDialog dialog(m_window);
    dialog.setWindowTitle(title.isEmpty() ? tr("Macro") : title);
    dialog.setLabelText(text);
    dialog.setTextValue(initial);
    dialog.setTextEchoMode(password ? QLineEdit::Password : QLineEdit::Normal);
    dialog.setInputMode(QInputDialog::TextInput);
    place(&dialog);
    if (dialog.exec() != QDialog::Accepted) {
        return TT_DIALOG_CANCEL;
    }
    *outText = dialog.textValue();
    return TT_DIALOG_OK;
}

QString Macro::chooseFile(const QString &title, bool save, const QString &dir)
{
    // The platform's own dialog, which `setdlgpos` cannot place — it may not
    // be a window in this process at all.
    return save ? QFileDialog::getSaveFileName(m_window, title, dir)
                : QFileDialog::getOpenFileName(m_window, title, dir);
}

QString Macro::chooseDirectory(const QString &title, const QString &dir)
{
    return QFileDialog::getExistingDirectory(m_window, title, dir);
}

bool Macro::showWindow(TtShowWindow which)
{
    if (!m_window) {
        return false;
    }
    switch (which) {
    case TT_SHOW_VT_HIDE:
        m_window->hide();
        return true;
    case TT_SHOW_VT_MINIMIZE:
        m_window->showMinimized();
        return true;
    case TT_SHOW_VT_RESTORE:
        m_window->showNormal();
        m_window->raise();
        m_window->activateWindow();
        return true;
    default:
        // The four TEK arms and the three log-window ones. Refused rather than
        // ignored: a macro asking to raise a TEK window on a build that has
        // none has not been misunderstood, it has asked for something that is
        // not there.
        return false;
    }
}

bool Macro::geometry(TtWindowGeometry *out) const
{
    if (!m_window) {
        return false;
    }
    // Upstream's order — iconic, then zoomed, then visible — so a window that
    // is both minimised and maximised reports minimised.
    if (m_window->isMinimized()) {
        out->state = TT_WINDOW_MINIMIZED;
    } else if (m_window->isMaximized()) {
        out->state = TT_WINDOW_MAXIMIZED;
    } else if (m_window->isVisible()) {
        out->state = TT_WINDOW_NORMAL;
    } else {
        out->state = TT_WINDOW_HIDDEN;
    }
    const QRect frame = m_window->frameGeometry();
    const QRect client = m_window->geometry();
    out->x = frame.x();
    out->y = frame.y();
    out->width = frame.width();
    out->height = frame.height();
    out->client_x = client.x();
    out->client_y = client.y();
    out->client_width = client.width();
    out->client_height = client.height();
    return true;
}

void Macro::enableKeyboard(bool on)
{
    emit keyboardEnabled(on);
    emit notice(on ? tr("Keyboard released by the macro")
                   : tr("Keyboard locked by the macro"));
}

// What this frontend does not answer, and why — the same list `tt-macro`'s
// host keeps, one layer out:
//
// `callmenu` — the ids are `teraterm.rc`'s and there are about ninety of them.
//   Answering the ones this window has a menu item for means a table mapping
//   Windows command ids onto `QAction`s, which is worth writing when there is
//   a menu to map rather than ahead of one.
// `show` — the macro's own control window. Upstream's `ttpmacro.exe` has one,
//   with the End button on it; here the macro is a thread in this process and
//   the terminal's own menu is where End belongs.
// `setexitcode` — stored by the core, which is where a frontend that wants it
//   at the end reads it from. Nothing here exits on a macro's word yet.
// `connect_ssh` — the one connection a macro cannot open for itself, because
//   a host key or a password is a prompt. It needs a transport handed back
//   across the seam and the seam has no transport type; until then a
//   `connect '… /ssh'` reports 1, which a script can test for.
