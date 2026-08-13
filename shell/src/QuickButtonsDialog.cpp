// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonsDialog.h"

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QStackedWidget>
#include <QVBoxLayout>

#include "Session.h"
#include "TerminalView.h"

namespace {

/// The menu commands a button may invoke.
///
/// The ids are `tt_res.h`'s and the list is the subset `MainWindow::
/// invokeMenuCommand` actually implements — offering one it ignores would be
/// a button that does nothing, which is worse than not offering it. A user key
/// in a `KEYBOARD.CNF` can name any number; this is a picker, so it names the
/// ones that work.
struct MenuCommand {
    quint16 id;
    const char *label;
};

const MenuCommand kCommands[] = {
    {50430, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Send break")},
    {50120, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Start or stop logging")},
    {50130, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Send file...")},
    {50131, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Receive file...")},
    {50210, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Copy")},
    {50230, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Paste")},
    {50240, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Paste and send")},
    {50110, QT_TRANSLATE_NOOP("QuickButtonsDialog", "New connection...")},
    {50111, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Duplicate session")},
    {50112, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Local shell")},
    {50190, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Disconnect")},
    {50310, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Terminal settings...")},
    {50330, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Font...")},
    {50380, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Save setup")},
    {50395, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Load key map...")},
    {50470, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Run macro...")},
};

/// The convenience set. `Ctrl+Alt+digit` because Alt alone is Meta when
/// `MetaKey` is on, Ctrl alone is how a terminal sends control characters, and
/// the function keys belong to the host.
QKeySequence defaultShortcut(int index)
{
    static const int keys[] = {Qt::Key_1, Qt::Key_2, Qt::Key_3, Qt::Key_4,
                               Qt::Key_5, Qt::Key_6, Qt::Key_7, Qt::Key_8,
                               Qt::Key_9, Qt::Key_0};
    if (index < 0 || index >= static_cast<int>(std::size(keys))) {
        return {};
    }
    return QKeySequence(Qt::CTRL | Qt::ALT | keys[index]);
}

} // namespace

QuickButtonsDialog::QuickButtonsDialog(const QVector<QuickButton> &buttons,
                                       const Session *session,
                                       const QWidget *window, QWidget *parent)
    : QDialog(parent), m_buttons(buttons), m_session(session), m_window(window)
{
    setObjectName(QStringLiteral("quickButtonsDialog"));
    setWindowTitle(tr("Quick buttons"));

    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("quickButtonsList"));
    // Drag to reorder, which is the order they appear on the bar. A pair of
    // up/down buttons would say the same thing in two more widgets.
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setMinimumWidth(180);

    auto *add = new QPushButton(tr("Add"), this);
    add->setObjectName(QStringLiteral("quickButtonsAdd"));
    m_duplicate = new QPushButton(tr("Duplicate"), this);
    m_remove = new QPushButton(tr("Remove"), this);
    m_remove->setObjectName(QStringLiteral("quickButtonsRemove"));

    m_label = new QLineEdit(this);
    m_label->setObjectName(QStringLiteral("quickButtonLabel"));
    m_label->setPlaceholderText(tr("Shown on the button"));

    m_kind = new QComboBox(this);
    m_kind->setObjectName(QStringLiteral("quickButtonKind"));
    m_kind->addItem(tr("Send text"), TT_QUICK_BUTTON_TEXT);
    m_kind->addItem(tr("Send bytes"), TT_QUICK_BUTTON_BYTES);
    m_kind->addItem(tr("Run macro"), TT_QUICK_BUTTON_MACRO);
    m_kind->addItem(tr("Menu command"), TT_QUICK_BUTTON_COMMAND);

    // One field per kind rather than one field that means four things: a path
    // wants a Browse button and a command wants a list of the commands there
    // are, and neither is a line of text somebody should have to know the
    // spelling of.
    m_text = new QPlainTextEdit(this);
    m_text->setObjectName(QStringLiteral("quickButtonText"));
    m_text->setTabChangesFocus(true);
    m_text->setPlaceholderText(tr("What to send"));

    auto *pathRow = new QWidget(this);
    auto *pathLayout = new QHBoxLayout(pathRow);
    pathLayout->setContentsMargins(0, 0, 0, 0);
    m_path = new QLineEdit(pathRow);
    m_path->setObjectName(QStringLiteral("quickButtonPath"));
    auto *browse = new QPushButton(tr("Browse..."), pathRow);
    pathLayout->addWidget(m_path);
    pathLayout->addWidget(browse);

    m_command = new QComboBox(this);
    m_command->setObjectName(QStringLiteral("quickButtonCommand"));
    for (const MenuCommand &c : kCommands) {
        m_command->addItem(tr(c.label), c.id);
    }

    m_value = new QStackedWidget(this);
    m_value->addWidget(m_text);
    m_value->addWidget(pathRow);
    m_value->addWidget(m_command);

    m_enter = new QCheckBox(tr("Send Enter after"), this);
    m_enter->setObjectName(QStringLiteral("quickButtonEnter"));
    m_enter->setToolTip(tr("Adds a Return, the way pressing Enter would. Shift"
                           " and click sends it without one."));

    m_shortcut = new QKeySequenceEdit(this);
    m_shortcut->setObjectName(QStringLiteral("quickButtonShortcut"));

    m_warning = new QLabel(this);
    m_warning->setObjectName(QStringLiteral("quickButtonWarning"));
    m_warning->setWordWrap(true);
    m_warning->setStyleSheet(QStringLiteral("QLabel { color: #b71c1c; }"));

    auto *standard = new QPushButton(tr("Assign Ctrl+Alt+1...0"), this);
    standard->setToolTip(tr("Give the first ten buttons a shortcut each. Any "
                            "shortcut is a key the terminal stops receiving."));

    m_confirm = new QCheckBox(tr("Ask before running"), this);
    m_confirm->setObjectName(QStringLiteral("quickButtonConfirm"));

    m_fields = new QWidget(this);
    auto *form = new QFormLayout(m_fields);
    form->setContentsMargins(0, 0, 0, 0);
    form->addRow(tr("Label:"), m_label);
    form->addRow(tr("Does:"), m_kind);
    form->addRow(tr("Command:"), m_value);
    form->addRow(QString(), m_enter);
    form->addRow(tr("Shortcut:"), m_shortcut);
    form->addRow(QString(), m_warning);
    form->addRow(QString(), m_confirm);

    auto *buttonRow = new QHBoxLayout;
    buttonRow->addWidget(add);
    buttonRow->addWidget(m_duplicate);
    buttonRow->addWidget(m_remove);
    buttonRow->addStretch();

    auto *left = new QVBoxLayout;
    left->addWidget(m_list);
    left->addLayout(buttonRow);

    auto *right = new QVBoxLayout;
    right->addWidget(m_fields);
    right->addWidget(standard);
    right->addStretch();

    auto *columns = new QHBoxLayout;
    columns->addLayout(left, 1);
    columns->addLayout(right, 2);

    auto *box = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                     this);
    connect(box, &QDialogButtonBox::accepted, this, [this] {
        commit();
        accept();
    });
    connect(box, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(columns);
    layout->addWidget(box);

    connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
        if (!m_loading) {
            commit();
        }
        load(row);
    });
    // A drag reorders the model, not just the view.
    connect(m_list->model(), &QAbstractItemModel::rowsMoved, this,
            [this](const QModelIndex &, int from, int, const QModelIndex &, int to) {
                if (m_loading || from < 0 || from >= m_buttons.size()) {
                    return;
                }
                const QuickButton moved = m_buttons.takeAt(from);
                m_buttons.insert(to > from ? to - 1 : to, moved);
                m_current = m_list->currentRow();
            });
    connect(add, &QPushButton::clicked, this, &QuickButtonsDialog::addButton);
    connect(m_duplicate, &QPushButton::clicked, this,
            &QuickButtonsDialog::duplicateButton);
    connect(m_remove, &QPushButton::clicked, this,
            &QuickButtonsDialog::removeButton);
    connect(standard, &QPushButton::clicked, this,
            &QuickButtonsDialog::assignDefaultShortcuts);
    connect(browse, &QPushButton::clicked, this, [this] {
        const QString chosen = QFileDialog::getOpenFileName(
            this, tr("Macro"), QFileInfo(m_path->text()).absolutePath(),
            tr("Macros (*.ttl *.lua);;All files (*)"));
        if (!chosen.isEmpty()) {
            m_path->setText(chosen);
        }
    });

    // Every field writes back as it is typed, so the list's caption follows
    // the label and nothing has to be "applied" before switching rows.
    connect(m_label, &QLineEdit::textEdited, this, [this] {
        commit();
        if (m_current >= 0 && m_current < m_list->count()) {
            m_list->item(m_current)->setText(m_buttons[m_current].caption());
        }
    });
    connect(m_kind, &QComboBox::currentIndexChanged, this, [this] {
        applyKind();
        commit();
    });
    connect(m_text, &QPlainTextEdit::textChanged, this, [this] { commit(); });
    connect(m_path, &QLineEdit::textChanged, this, [this] { commit(); });
    connect(m_command, &QComboBox::currentIndexChanged, this, [this] { commit(); });
    connect(m_enter, &QCheckBox::toggled, this, [this] { commit(); });
    connect(m_confirm, &QCheckBox::toggled, this, [this] { commit(); });
    connect(m_shortcut, &QKeySequenceEdit::keySequenceChanged, this, [this] {
        checkShortcut();
        commit();
    });

    rebuildList();
    load(m_buttons.isEmpty() ? -1 : 0);
}

void QuickButtonsDialog::rebuildList()
{
    const bool wasLoading = m_loading;
    m_loading = true;
    const int keep = m_list->currentRow();
    m_list->clear();
    for (const QuickButton &button : m_buttons) {
        m_list->addItem(button.caption());
    }
    if (keep >= 0 && keep < m_list->count()) {
        m_list->setCurrentRow(keep);
    }
    m_loading = wasLoading;
}

void QuickButtonsDialog::selectRow(int index)
{
    if (index >= 0 && index < m_buttons.size()) {
        m_list->setCurrentRow(index);
    }
}

void QuickButtonsDialog::appendButton(const QuickButton &seed)
{
    m_buttons.append(seed);
    rebuildList();
    m_list->setCurrentRow(m_buttons.size() - 1);
    load(m_buttons.size() - 1);
    m_label->setFocus();
}

void QuickButtonsDialog::commit()
{
    if (m_loading || m_current < 0 || m_current >= m_buttons.size()) {
        return;
    }
    QuickButton &button = m_buttons[m_current];
    button.label = m_label->text();
    button.kind = static_cast<TtQuickButtonKind>(m_kind->currentData().toUInt());
    switch (button.kind) {
    case TT_QUICK_BUTTON_MACRO:
        button.text = m_path->text();
        break;
    case TT_QUICK_BUTTON_COMMAND:
        button.text = QString::number(m_command->currentData().toUInt());
        break;
    default:
        // The box holds line feeds because that is what a text edit produces;
        // a terminal wants carriage returns, and this is the boundary where
        // the difference belongs. See `Vt::encode_text`.
        button.text = m_text->toPlainText().replace(QLatin1Char('\n'),
                                                    QLatin1Char('\r'));
        if (m_enter->isChecked() && !button.text.endsWith(QLatin1Char('\r'))) {
            button.text += QLatin1Char('\r');
        } else if (!m_enter->isChecked()) {
            while (button.text.endsWith(QLatin1Char('\r'))) {
                button.text.chop(1);
            }
        }
        break;
    }
    button.shortcut = m_shortcut->keySequence().toString(QKeySequence::PortableText);
    button.confirm = m_confirm->isChecked();
    // `value` is the core's to produce; it is written when the window saves.
    button.value.clear();
}

void QuickButtonsDialog::load(int row)
{
    m_loading = true;
    m_current = row;
    const bool has = row >= 0 && row < m_buttons.size();
    m_fields->setEnabled(has);
    m_duplicate->setEnabled(has);
    m_remove->setEnabled(has);
    if (has) {
        const QuickButton &button = m_buttons[row];
        m_label->setText(button.label);
        const int kindRow = m_kind->findData(button.kind);
        m_kind->setCurrentIndex(kindRow < 0 ? 0 : kindRow);
        QString text = button.text;
        m_enter->setChecked(button.sendsEnter());
        if (button.sendsEnter()) {
            text.chop(1);
        }
        m_text->setPlainText(QString(text).replace(QLatin1Char('\r'),
                                                   QLatin1Char('\n')));
        m_path->setText(button.text);
        const int commandRow = m_command->findData(button.text.toUInt());
        m_command->setCurrentIndex(commandRow < 0 ? 0 : commandRow);
        m_shortcut->setKeySequence(
            QKeySequence::fromString(button.shortcut, QKeySequence::PortableText));
        m_confirm->setChecked(button.confirm);
        applyKind();
    } else {
        m_label->clear();
        m_text->clear();
        m_path->clear();
        m_shortcut->clear();
        m_confirm->setChecked(false);
        m_enter->setChecked(false);
    }
    m_loading = false;
    checkShortcut();
}

void QuickButtonsDialog::applyKind()
{
    const auto kind = static_cast<TtQuickButtonKind>(m_kind->currentData().toUInt());
    switch (kind) {
    case TT_QUICK_BUTTON_MACRO:
        m_value->setCurrentIndex(1);
        break;
    case TT_QUICK_BUTTON_COMMAND:
        m_value->setCurrentIndex(2);
        break;
    default:
        m_value->setCurrentIndex(0);
        break;
    }
    // Only the two sending kinds have a line ending to add.
    m_enter->setVisible(kind == TT_QUICK_BUTTON_TEXT
                        || kind == TT_QUICK_BUTTON_BYTES);
}

void QuickButtonsDialog::addButton()
{
    commit();
    QuickButton made;
    made.kind = TT_QUICK_BUTTON_TEXT;
    appendButton(made);
}

void QuickButtonsDialog::duplicateButton()
{
    commit();
    if (m_current < 0 || m_current >= m_buttons.size()) {
        return;
    }
    QuickButton copy = m_buttons[m_current];
    // Not the shortcut: two buttons cannot hold the same key, and taking it
    // from the original silently is the wrong half of that to choose.
    copy.shortcut.clear();
    m_buttons.insert(m_current + 1, copy);
    const int at = m_current + 1;
    rebuildList();
    m_list->setCurrentRow(at);
    load(at);
}

void QuickButtonsDialog::removeButton()
{
    if (m_current < 0 || m_current >= m_buttons.size()) {
        return;
    }
    const int at = m_current;
    m_buttons.remove(at);
    m_current = -1;
    rebuildList();
    const int next = qMin(at, m_buttons.size() - 1);
    m_list->setCurrentRow(next);
    load(next);
}

void QuickButtonsDialog::assignDefaultShortcuts()
{
    commit();
    for (int i = 0; i < m_buttons.size(); i++) {
        const QKeySequence sequence = defaultShortcut(i);
        if (sequence.isEmpty()) {
            break;
        }
        m_buttons[i].shortcut = sequence.toString(QKeySequence::PortableText);
    }
    load(m_current);
}

QString QuickButtonsDialog::shortcutComplaint(const QKeySequence &sequence,
                                              int forRow) const
{
    if (sequence.isEmpty()) {
        return {};
    }

    for (int i = 0; i < m_buttons.size(); i++) {
        if (i == forRow || m_buttons[i].shortcut.isEmpty()) {
            continue;
        }
        if (QKeySequence::fromString(m_buttons[i].shortcut,
                                     QKeySequence::PortableText)
            == sequence) {
            return tr("Already used by the quick button \"%1\".")
                .arg(m_buttons[i].caption());
        }
    }

    // The window's own menu items and anything a Lua plugin installed. Both
    // are ordinary `QAction`s on the window, which is why one walk finds them.
    if (m_window) {
        for (const QAction *action : m_window->findChildren<const QAction *>()) {
            if (action->shortcut() == sequence && !action->shortcut().isEmpty()
                && !action->objectName().startsWith(
                    QLatin1String("quickButton"))) {
                return tr("Already used by \"%1\".")
                    .arg(action->text().remove(QLatin1Char('&')));
            }
        }
    }

    // ...and the one nothing else can see: a key the *host* is expecting.
    // A QAction beats the terminal widget, so this would silently stop the
    // sequence reaching the far end.
    const quint16 scan = TerminalView::scanForSequence(sequence);
    if (scan != 0 && m_session && m_session->keyCodeBound(scan)) {
        return tr("The key map already assigns this key.");
    }

    const Qt::KeyboardModifiers mods = sequence[0].keyboardModifiers();
    const bool bare = (mods & ~Qt::ShiftModifier) == Qt::NoModifier;
    const int key = sequence[0].key();
    const bool function = key >= Qt::Key_F1 && key <= Qt::Key_F35;
    if (bare || function) {
        return tr("The terminal normally sends this key to the host, which "
                  "will stop while this button holds it.");
    }
    return {};
}

void QuickButtonsDialog::checkShortcut()
{
    const QString complaint =
        shortcutComplaint(m_shortcut->keySequence(), m_current);
    m_warning->setText(complaint);
    m_warning->setVisible(!complaint.isEmpty());
}
