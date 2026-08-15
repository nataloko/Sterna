// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "LogDialog.h"

#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QSpinBox>
#include <QTimer>
#include <QVBoxLayout>

#include <limits>

#include "I18n.h"

namespace {

/// `LogRotateSizeType`: 0 bytes, 1 KB, 2 MB (`log_pp.cpp:71`).
///
/// **A multiplier and nothing else.** `LogRotateSize` is in bytes whatever
/// this says — upstream's dialog pre-multiplies on the way in and divides on
/// the way out — so a reader that scaled the stored number by the type would
/// turn a one-megabyte rotation into a terabyte one.
quint64 unitScale(int type)
{
    switch (type) {
    case 1:
        return 1024;
    case 2:
        return 1024 * 1024;
    default:
        return 1;
    }
}

/// The settings schema carries `LogRotateSize` as a signed integer. Keep the
/// dialog inside that range whichever display unit is selected; otherwise a
/// value the current capture accepts as `u64` wraps negative when it is written
/// back and silently disables rotation the next time the dialog opens.
int maxRotationUnits(int type)
{
    return static_cast<int>(std::numeric_limits<qint32>::max() / unitScale(type));
}

/// The setting spelling for each row of the timestamp combo, in the order the
/// combo lists them — which is upstream's order (`logdlg.cpp:284`).
const char *const kTimestampTypes[] = {"Local", "UTC", "LoggingElapsed", "ConnectionElapsed"};

} // namespace

LogOptionsDialog::LogOptionsDialog(Session *session, QWidget *parent, const I18n *i18n)
    : QDialog(parent), m_session(session), m_i18n(i18n)
{
    const auto text = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->text(key, fallback) : fallback;
    };
    const auto plainText = [i18n](const char *key, const QString &fallback) {
        return i18n ? i18n->plainText(key, fallback) : fallback;
    };

    setObjectName(QStringLiteral("logOptionsDialog"));
    setWindowTitle(plainText("DLG_FOPT_TITLE", tr("Log")));

    const TtLogOptions defaults =
        session ? session->logDefaults() : [] {
            TtLogOptions o = {};
            tt_log_options_default(&o);
            return o;
        }();

    // --- the file -----------------------------------------------------------

    m_file = new QLineEdit(this);
    m_file->setObjectName(QStringLiteral("logFile"));
    // The field holds an absolute path, and a box sized by the rest of the
    // dialog shows the tail of one — which is the half that matters least. Not
    // a minimum on the dialog: this is the widget with something long in it,
    // and the groups below should not be stretched to match a path.
    m_file->setMinimumWidth(fontMetrics().averageCharWidth() * 52);
    auto *browse = new QPushButton(text("BTN_BROWSE", tr("Browse...")), this);
    browse->setObjectName(QStringLiteral("logBrowse"));
    browse->setAutoDefault(false);
    connect(browse, &QPushButton::clicked, this, &LogOptionsDialog::browse);
    // Editing by hand is the signal that the clock should stop rewriting the
    // field. `textEdited` and not `textChanged`, which `refreshName` itself
    // would trip on every tick.
    connect(m_file, &QLineEdit::textEdited, this, [this] {
        m_nameEdited = true;
        refreshEnabled();
    });

    auto *fileRow = new QHBoxLayout;
    fileRow->addWidget(m_file, 1);
    fileRow->addWidget(browse);

    // --- write mode ---------------------------------------------------------

    auto *writeGroup = new QGroupBox(text("DLG_FOPT_APPEND_LABEL", tr("Write mode")), this);
    m_overwrite = new QRadioButton(plainText("DLG_FOPT_NEW_OVERWRITE", tr("New / Overwrite")),
                                   writeGroup);
    m_overwrite->setObjectName(QStringLiteral("logOverwrite"));
    m_append = new QRadioButton(plainText("DLG_FOPT_APPEND", tr("Append")), writeGroup);
    m_append->setObjectName(QStringLiteral("logAppend"));
    (defaults.append ? m_append : m_overwrite)->setChecked(true);
    auto *writeRow = new QHBoxLayout(writeGroup);
    writeRow->addWidget(m_overwrite);
    writeRow->addWidget(m_append);
    writeRow->addStretch();

    // --- text or binary -----------------------------------------------------

    auto *modeGroup = new QGroupBox(text("DLG_FOPT_BINARY_LABEL", tr("Text or Binary mode")), this);
    m_textMode = new QRadioButton(plainText("DLG_FOPT_TEXT", tr("Text")), modeGroup);
    m_textMode->setObjectName(QStringLiteral("logText"));
    m_binaryMode = new QRadioButton(plainText("DLG_FOPT_BINARY", tr("Binary")), modeGroup);
    m_binaryMode->setObjectName(QStringLiteral("logBinary"));
    (defaults.raw ? m_binaryMode : m_textMode)->setChecked(true);
    m_binaryMode->setToolTip(tr("Every byte as it arrived, escape sequences included, so "
                                "the session can be replayed. A binary log is never "
                                "timestamped."));
    auto *modeRow = new QHBoxLayout(modeGroup);
    modeRow->addWidget(m_textMode);
    modeRow->addWidget(m_binaryMode);
    modeRow->addStretch();

    // --- the ticks ----------------------------------------------------------

    m_bom = new QCheckBox(plainText("DLG_FOPT_BOM", tr("BOM")), this);
    m_bom->setObjectName(QStringLiteral("logBom"));
    m_bom->setChecked(defaults.bom);
    m_bom->setToolTip(tr("Start the file with a UTF-8 byte-order mark. Windows editors "
                         "may want one; most Linux tools would rather not see it."));

    m_plainText = new QCheckBox(plainText("DLG_FOPT_PLAIN", tr("Plain text")), this);
    m_plainText->setObjectName(QStringLiteral("logPlainText"));
    m_plainText->setChecked(session &&
                            session->setting(QStringLiteral("log.plain_text")) ==
                                QLatin1String("on"));

    m_includeScreen =
        new QCheckBox(plainText("DLG_FOPT_ALLBUFFINFIRST", tr("Include screen buffer")), this);
    m_includeScreen->setObjectName(QStringLiteral("logIncludeScreen"));
    m_includeScreen->setChecked(defaults.include_screen);
    m_includeScreen->setToolTip(tr("Write what is already on the screen and in the "
                                   "scrollback before the first new byte."));

    m_timestamp = new QCheckBox(plainText("DLG_FOPT_TIMESTAMP", tr("Timestamp")), this);
    m_timestamp->setObjectName(QStringLiteral("logTimestamp"));
    m_timestamp->setChecked(defaults.timestamp != TT_LOG_TIMESTAMP_NONE);

    m_timestampType = new QComboBox(this);
    m_timestampType->setObjectName(QStringLiteral("logTimestampType"));
    m_timestampType->addItem(plainText("DLG_FOPT_TIMESTAMP_LOCAL", tr("Local Time")));
    m_timestampType->addItem(plainText("DLG_FOPT_TIMESTAMP_UTC", tr("UTC")));
    m_timestampType->addItem(
        plainText("DLG_FOPT_TIMESTAMP_ELAPSED_LOGGING", tr("Elapsed Time (Logging)")));
    m_timestampType->addItem(
        plainText("DLG_FOPT_TIMESTAMP_ELAPSED_CONNECTION", tr("Elapsed Time (Connection)")));
    switch (defaults.timestamp) {
    case TT_LOG_TIMESTAMP_UTC:
        m_timestampType->setCurrentIndex(1);
        break;
    case TT_LOG_TIMESTAMP_ELAPSED:
        m_timestampType->setCurrentIndex(2);
        break;
    case TT_LOG_TIMESTAMP_ELAPSED_CONNECTION:
        m_timestampType->setCurrentIndex(3);
        break;
    default:
        m_timestampType->setCurrentIndex(0);
        break;
    }

    // --- rotation -----------------------------------------------------------

    auto *rotateGroup = new QGroupBox(this);
    m_rotate = new QCheckBox(plainText("DLG_TAB_LOG_ROTATE", tr("Log Rotate")), rotateGroup);
    m_rotate->setObjectName(QStringLiteral("logRotate"));
    m_rotate->setChecked(defaults.rotate_size > 0);

    m_rotateSize = new QSpinBox(rotateGroup);
    m_rotateSize->setObjectName(QStringLiteral("logRotateSize"));
    m_rotateUnit = new QComboBox(rotateGroup);
    m_rotateUnit->setObjectName(QStringLiteral("logRotateUnit"));
    m_rotateUnit->addItem(tr("Byte"));
    m_rotateUnit->addItem(tr("KB"));
    m_rotateUnit->addItem(tr("MB"));
    {
        // The stored number is bytes; the unit is how it is shown. Seeded from
        // the setting rather than from the resolved size, because the resolved
        // one is zero whenever rotation is off and the unit would reset.
        const int type =
            session ? session->setting(QStringLiteral("log.rotate_size_type")).toInt() : 0;
        m_rotateUnit->setCurrentIndex(qBound(0, type, 2));
        m_rotateSize->setRange(0, maxRotationUnits(m_rotateUnit->currentIndex()));
        const quint64 bytes =
            session ? session->setting(QStringLiteral("log.rotate_size")).toULongLong() : 0;
        m_rotateSize->setValue(static_cast<int>(bytes / unitScale(m_rotateUnit->currentIndex())));
    }

    m_rotateKeep = new QSpinBox(rotateGroup);
    m_rotateKeep->setObjectName(QStringLiteral("logRotateKeep"));
    // Upstream's `LogRotateStep` of zero is its internal cap of ten thousand
    // generations rather than none, which is a strange thing for a spin box to
    // show as `0` — so the zero says what it means.
    m_rotateKeep->setRange(0, std::numeric_limits<quint16>::max());
    m_rotateKeep->setSpecialValueText(tr("all (10000)"));
    m_rotateKeep->setValue(
        session ? session->setting(QStringLiteral("log.rotate_step")).toInt() : 0);

    auto *rotateRow = new QGridLayout(rotateGroup);
    rotateRow->addWidget(m_rotate, 0, 0, 1, 4);
    rotateRow->addWidget(
        new QLabel(text("DLG_TAB_LOG_ROTATE_SIZE_TEXT", tr("Size")), rotateGroup), 1, 0);
    rotateRow->addWidget(m_rotateSize, 1, 1);
    rotateRow->addWidget(m_rotateUnit, 1, 2);
    rotateRow->addWidget(new QLabel(text("DLG_TAB_LOG_ROTATESTEP", tr("Rotate")), rotateGroup), 2,
                         0);
    rotateRow->addWidget(m_rotateKeep, 2, 1);
    rotateRow->setColumnStretch(3, 1);

    // --- put it together ----------------------------------------------------

    auto *ticks = new QGridLayout;
    ticks->addWidget(m_plainText, 0, 0);
    ticks->addWidget(m_bom, 0, 1);
    ticks->addWidget(m_includeScreen, 1, 0, 1, 2);
    ticks->addWidget(m_timestamp, 2, 0);
    ticks->addWidget(m_timestampType, 2, 1);
    ticks->setColumnStretch(1, 1);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->button(QDialogButtonBox::Ok)->setText(text("BTN_OK", tr("OK")));
    buttons->button(QDialogButtonBox::Cancel)->setText(text("BTN_CANCEL", tr("Cancel")));
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(new QLabel(text("DLG_FOPT_FILENAME_TITLE", tr("Filename")), this));
    layout->addLayout(fileRow);
    layout->addWidget(writeGroup);
    layout->addWidget(modeGroup);
    layout->addLayout(ticks);
    layout->addWidget(rotateGroup);
    layout->addStretch();
    layout->addWidget(buttons);

    for (QRadioButton *b : {m_overwrite, m_append, m_textMode, m_binaryMode}) {
        connect(b, &QRadioButton::toggled, this, &LogOptionsDialog::refreshEnabled);
    }
    connect(m_timestamp, &QCheckBox::toggled, this, &LogOptionsDialog::refreshEnabled);
    connect(m_rotateUnit, &QComboBox::currentIndexChanged, this, [this](int type) {
        m_rotateSize->setMaximum(maxRotationUnits(type));
    });
    connect(m_rotate, &QCheckBox::toggled, this, [this](bool on) {
        // A rotation of zero bytes is no rotation — `LogRotateSize` defaults
        // to 0 and both engines treat it as off — so a tick with nothing
        // beside it is a switch that does nothing and says nothing. Ticking it
        // proposes a size; the number is then the user's, and unticking leaves
        // it where they put it.
        if (on && m_rotateSize->value() == 0) {
            m_rotateUnit->setCurrentIndex(2);
            m_rotateSize->setValue(1);
        }
        refreshEnabled();
    });

    refreshName();
    refreshEnabled();

    m_tick = new QTimer(this);
    m_tick->setObjectName(QStringLiteral("logNameTimer"));
    m_tick->setInterval(1000);
    connect(m_tick, &QTimer::timeout, this, &LogOptionsDialog::refreshName);
    m_tick->start();
}

QString LogOptionsDialog::path() const
{
    return m_file->text();
}

TtLogOptions LogOptionsDialog::options() const
{
    TtLogOptions options = m_session ? m_session->logDefaults() : TtLogOptions{};
    if (!m_session) {
        tt_log_options_default(&options);
    }
    options.raw = m_binaryMode->isChecked();
    options.append = m_append->isChecked();
    options.bom = m_bom->isChecked() && m_bom->isEnabled();
    options.include_screen = m_includeScreen->isChecked();
    options.timestamp = TT_LOG_TIMESTAMP_NONE;
    if (m_timestamp->isChecked() && !options.raw) {
        switch (m_timestampType->currentIndex()) {
        case 1:
            options.timestamp = TT_LOG_TIMESTAMP_UTC;
            break;
        case 2:
            options.timestamp = TT_LOG_TIMESTAMP_ELAPSED;
            break;
        case 3:
            options.timestamp = TT_LOG_TIMESTAMP_ELAPSED_CONNECTION;
            break;
        default:
            options.timestamp = TT_LOG_TIMESTAMP_LOCAL;
            break;
        }
    }
    if (m_rotate->isChecked()) {
        options.rotate_size =
            static_cast<quint64>(m_rotateSize->value()) * unitScale(m_rotateUnit->currentIndex());
        const int keep = m_rotateKeep->value();
        options.rotate_keep = keep == 0 ? 10000 : static_cast<quint32>(keep);
    } else {
        options.rotate_size = 0;
        options.rotate_keep = 0;
    }
    return options;
}

void LogOptionsDialog::applySettings() const
{
    if (!m_session) {
        return;
    }
    const auto onOff = [](bool on) {
        return on ? QStringLiteral("on") : QStringLiteral("off");
    };
    const auto set = [this](const QString &name, const QString &value) {
        QString error;
        m_session->setSetting(name, value, &error);
    };

    set(QStringLiteral("log.binary"), onOff(m_binaryMode->isChecked()));
    set(QStringLiteral("log.append"), onOff(m_append->isChecked()));
    // `SetLogFlags` leaves these two settings alone in binary mode. The
    // controls are disabled then too; writing a value somebody chose before
    // switching to Binary would make a greyed choice take effect later.
    if (!m_binaryMode->isChecked()) {
        set(QStringLiteral("log.plain_text"), onOff(m_plainText->isChecked()));
        set(QStringLiteral("log.timestamp"), onOff(m_timestamp->isChecked()));
    }
    set(QStringLiteral("log.include_screen_buffer"), onOff(m_includeScreen->isChecked()));
    // Written as a name rather than as an index. Upstream writes
    // `GetCurSel() - 1` here (`logdlg.cpp:106`) against the plain index it
    // reads back with at `:322`, which is a bug and not a convention.
    set(QStringLiteral("log.timestamp_type"),
        QString::fromLatin1(
            kTimestampTypes[qBound(0, m_timestampType->currentIndex(), 3)]));

    set(QStringLiteral("log.rotate"), m_rotate->isChecked() ? QStringLiteral("1")
                                                            : QStringLiteral("0"));
    set(QStringLiteral("log.rotate_size_type"),
        QString::number(m_rotateUnit->currentIndex()));
    set(QStringLiteral("log.rotate_size"),
        QString::number(static_cast<quint64>(m_rotateSize->value()) *
                        unitScale(m_rotateUnit->currentIndex())));
    set(QStringLiteral("log.rotate_step"), QString::number(m_rotateKeep->value()));
}

void LogOptionsDialog::refreshEnabled()
{
    const bool binary = m_binaryMode->isChecked();
    // A binary log is bytes as they arrived: there is nothing to strip and no
    // room to insert a stamp (`FixLogOption`, `filesys_log.cpp:243`).
    m_plainText->setEnabled(!binary);
    m_includeScreen->setEnabled(!binary);
    m_timestamp->setEnabled(!binary);
    m_timestampType->setEnabled(!binary && m_timestamp->isChecked());
    // A mark belongs at the head of a file, so only a new text one can carry
    // it — upstream's gate exactly (`filesys_log.cpp:382`).
    m_bom->setEnabled(!binary && m_overwrite->isChecked());

    m_rotateSize->setEnabled(m_rotate->isChecked());
    m_rotateUnit->setEnabled(m_rotate->isChecked());
    m_rotateKeep->setEnabled(m_rotate->isChecked());

    // Appending to a file that is not there is overwriting it with extra
    // steps, so upstream greys the choice until the name exists
    // (`logdlg.cpp:180`) — re-tested on every edit, which is why this runs
    // from `textEdited` too.
    const QFileInfo file(m_file->text());
    const bool exists = file.exists() && file.isFile();
    m_append->setEnabled(exists);
    if (!exists && m_append->isChecked()) {
        m_overwrite->setChecked(true);
    }
}

void LogOptionsDialog::refreshName()
{
    if (m_nameEdited || !m_session) {
        return;
    }
    // The core expands the template — `strftime`, then `&h`/`&p`/`&u`, then
    // the sweep for characters a file name cannot hold. Asking it every second
    // is what keeps a `%H%M%S` name current while the dialog sits open.
    const QString expanded = m_session->logName();
    if (expanded.isEmpty()) {
        return;
    }
    const QString leaf = QFileInfo(expanded).fileName();

    QString dir = m_file->text().isEmpty() ? QString() : QFileInfo(m_file->text()).absolutePath();
    if (dir.isEmpty()) {
        // First pass. **A configured `LogDefaultPath` wins**: somebody who
        // named a log directory in the settings has said where logs go, and a
        // remembered directory that quietly overrode it would make the setting
        // look broken. So the memory only answers when the setting is silent —
        // which is the case it was added for, since the fallback behind it is
        // `GetTermLogDir`'s chain and that ends in a per-user directory nobody
        // chose. `logName()` has already resolved all of that, so its own
        // directory is the answer whenever the setting had one.
        if (m_session->setting(QStringLiteral("log.default_path")).isEmpty()) {
            dir = m_session->setting(QStringLiteral("recent.log_dir"));
        }
        if (dir.isEmpty() || !QDir(dir).exists()) {
            dir = QFileInfo(expanded).absolutePath();
        }
    }
    const QString next = QDir(dir).filePath(leaf);
    if (next != m_file->text()) {
        m_file->setText(next);
        refreshEnabled();
    }
}

void LogOptionsDialog::browse()
{
    const QString chosen = QFileDialog::getSaveFileName(
        this,
        m_i18n ? m_i18n->plainText("FILEDLG_TRANS_TITLE_LOG", tr("Log session to"))
               : tr("Log session to"),
        m_file->text(), tr("Log files (*.log *.txt);;All files (*)"));
    if (chosen.isEmpty()) {
        return;
    }
    // A chosen name is an edited one: the clock must not overwrite it a second
    // later.
    m_nameEdited = true;
    m_file->setText(chosen);
    refreshEnabled();
}
