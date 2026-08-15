// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "FindBar.h"

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QTimer>
#include <QToolButton>

#include "TerminalView.h"

namespace {
/// How long after the last keystroke the search runs.
///
/// A pattern somebody is still typing usually matches nothing yet, and
/// "matches nothing" is exactly the case that costs a pass over the whole
/// scrollback — `erro` on the way to `error` would do it once per letter.
/// Short enough that finishing a word and reading the answer feels immediate.
constexpr int kDebounceMs = 150;

/// How many patterns are remembered. YAT keeps twelve and that is about right:
/// enough that yesterday's search is still there, few enough that the dropdown
/// is a list rather than a log.
constexpr int kHistoryMax = 12;

/// How far the label will walk to say which match this is.
///
/// A bound on a loop the *far end* drives — a pattern of `.` over a full
/// scrollback has one match per character — and not an opinion about how many
/// matches are worth having. Past it the label says "more than", which is a
/// true sentence, rather than a number that took a second to work out.
constexpr int kOrdinalMax = 10000;
} // namespace

FindBar::FindBar(TerminalView *view, Session *session)
    : QWidget(view)
    , m_view(view)
    , m_session(session)
{
    setObjectName(QStringLiteral("findBar"));
    // Painted rather than transparent: this floats over live text, and a bar
    // you could read the terminal through would be unreadable itself.
    setAutoFillBackground(true);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setFocusPolicy(Qt::NoFocus);

    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(6, 3, 6, 3);
    layout->setSpacing(6);

    m_pattern = new QComboBox(this);
    m_pattern->setObjectName(QStringLiteral("findPattern"));
    m_pattern->setEditable(true);
    // No autocompletion. A find field that finishes your regular expression
    // for you is a find field that searches for something you did not type.
    m_pattern->setInsertPolicy(QComboBox::NoInsert);
    m_pattern->setCompleter(nullptr);
    m_pattern->lineEdit()->setPlaceholderText(tr("Find"));
    m_pattern->lineEdit()->setClearButtonEnabled(true);
    m_pattern->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    m_pattern->setMinimumContentsLength(12);
    layout->addWidget(m_pattern, 1);

    m_previous = new QToolButton(this);
    m_previous->setObjectName(QStringLiteral("findPreviousButton"));
    m_previous->setText(tr("Previous"));
    m_previous->setToolTip(tr("The match before this one, wrapping at the top."));
    layout->addWidget(m_previous);

    m_next = new QToolButton(this);
    m_next->setObjectName(QStringLiteral("findNextButton"));
    m_next->setText(tr("Next"));
    m_next->setToolTip(tr("The match after this one, wrapping at the bottom."));
    layout->addWidget(m_next);

    m_case = new QCheckBox(tr("Case"), this);
    m_case->setObjectName(QStringLiteral("findCaseBox"));
    m_case->setToolTip(tr("Match upper and lower case exactly."));
    layout->addWidget(m_case);

    m_word = new QCheckBox(tr("Whole word"), this);
    m_word->setObjectName(QStringLiteral("findWholeWordBox"));
    m_word->setToolTip(tr("Only where the match has a word boundary at each end."));
    layout->addWidget(m_word);

    m_regex = new QCheckBox(tr("Regex"), this);
    m_regex->setObjectName(QStringLiteral("findRegexBox"));
    m_regex->setToolTip(
        tr("Read the pattern as a regular expression rather than as text."));
    layout->addWidget(m_regex);

    m_status = new QLabel(this);
    m_status->setObjectName(QStringLiteral("findStatus"));
    // `Ignored`, for the reason `PageStatusBar`'s name label is: the text here
    // can be a compiler's complaint about a regular expression, and a label
    // that quoted it as its width would push the bar wider than the terminal.
    m_status->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    m_status->setMinimumWidth(0);
    layout->addWidget(m_status, 1);

    auto *close = new QToolButton(this);
    close->setObjectName(QStringLiteral("findCloseButton"));
    close->setText(tr("Close"));
    layout->addWidget(close);

    m_debounce = new QTimer(this);
    m_debounce->setObjectName(QStringLiteral("findDebounceTimer"));
    m_debounce->setSingleShot(true);
    m_debounce->setInterval(kDebounceMs);
    connect(m_debounce, &QTimer::timeout, this, &FindBar::apply);

    connect(m_pattern->lineEdit(), &QLineEdit::textEdited, this,
            &FindBar::patternEdited);
    // Choosing out of the dropdown is a decision, not typing, so it searches at
    // once rather than waiting for the debounce that nothing will follow.
    connect(m_pattern, &QComboBox::activated, this, [this](int) { apply(); });
    connect(m_pattern->lineEdit(), &QLineEdit::returnPressed, this, [this] {
        // Enter is Next, and Shift+Enter is Previous — the shape every find bar
        // has. `returnPressed` does not carry the modifiers, so they are read
        // from the keyboard at the moment it fires.
        m_debounce->stop();
        step(QApplication::keyboardModifiers().testFlag(Qt::ShiftModifier));
    });
    connect(m_previous, &QToolButton::clicked, this, &FindBar::findPrevious);
    connect(m_next, &QToolButton::clicked, this, &FindBar::findNext);
    connect(close, &QToolButton::clicked, this, &FindBar::close);

    const auto boxed = [this] {
        m_debounce->stop();
        apply();
        emit optionsChanged(m_case->isChecked(), m_word->isChecked(),
                            m_regex->isChecked());
    };
    connect(m_case, &QCheckBox::toggled, this, boxed);
    connect(m_word, &QCheckBox::toggled, this, boxed);
    connect(m_regex, &QCheckBox::toggled, this, boxed);

    hide();
}

QSize FindBar::sizeHint() const
{
    // Width from the layout would be the sum of every child's preferred size,
    // and the terminal is not allowed to care how long a pattern is. One
    // column, so the bar is as tall as it needs and as wide as it is given.
    return QSize(0, QWidget::sizeHint().height());
}

void FindBar::open()
{
    // `isHidden` rather than `isVisible`: a window that has not been shown yet
    // has no visible children at all, and opening the bar has to mean the same
    // thing whether or not somebody is looking at it.
    if (isHidden()) {
        show();
        raise();
        m_view->positionFindBar();
        apply();
    }
    m_pattern->setFocus(Qt::ShortcutFocusReason);
    m_pattern->lineEdit()->selectAll();
}

void FindBar::close()
{
    if (isHidden()) {
        return;
    }
    m_debounce->stop();
    hide();
    // The matches stop being painted, but the current one stays selected: it
    // is what somebody was looking at, and it is what Copy should still take.
    m_session->clearFind();
    m_haveCurrent = false;
    // The session has no pattern now, so reopening has to give it one again.
    m_haveApplied = false;
    m_view->setFocus(Qt::OtherFocusReason);
}

void FindBar::setHistory(const QStringList &patterns)
{
    m_history = patterns.mid(0, kHistoryMax);
    const QString typed = m_pattern->currentText();
    // Rebuilding the list rewrites the edit field, so put back what was in it —
    // this is reached while somebody may be mid-search in another tab.
    const QSignalBlocker block(m_pattern);
    m_pattern->clear();
    m_pattern->addItems(m_history);
    m_pattern->setCurrentText(typed);
}

void FindBar::setOptions(bool caseSensitive, bool wholeWord, bool regex)
{
    const QSignalBlocker c(m_case);
    const QSignalBlocker w(m_word);
    const QSignalBlocker r(m_regex);
    m_case->setChecked(caseSensitive);
    m_word->setChecked(wholeWord);
    m_regex->setChecked(regex);
}

FindQuery FindBar::query() const
{
    FindQuery q;
    q.pattern = m_pattern->currentText();
    q.caseSensitive = m_case->isChecked();
    q.wholeWord = m_word->isChecked();
    q.regex = m_regex->isChecked();
    return q;
}

void FindBar::patternEdited()
{
    // The pattern is compiled straight away so a broken regular expression is
    // reported as it is typed, and only the *searching* waits: telling somebody
    // their parenthesis is unclosed 150 ms late reads as the field lagging.
    QString reason;
    if (!Session::checkFindPattern(query(), &reason)) {
        m_debounce->stop();
        setStatus(reason, true);
        return;
    }
    m_debounce->start();
}

bool FindBar::install()
{
    const FindQuery q = query();
    if (m_haveApplied && q == m_applied) {
        return true;
    }
    QString reason;
    if (!m_session->setFind(q, &reason)) {
        setStatus(reason, true);
        return false;
    }
    m_applied = q;
    m_haveApplied = true;
    // A new pattern is a new search: where the old one landed says nothing
    // about where this one should resume from.
    m_haveCurrent = false;
    return true;
}

void FindBar::apply()
{
    if (!install()) {
        return;
    }
    const FindQuery q = query();
    if (q.pattern.isEmpty()) {
        m_haveCurrent = false;
        setStatus(QString(), false);
        return;
    }
    // Search-as-you-type starts from the top of the window rather than from
    // the last match: the pattern has changed, so where the old one landed says
    // nothing about where this one should.
    FindMatch match;
    const quint64 top = m_session->lineAt(0);
    if (m_session->findNext(top, 0, false, true, &match)) {
        showMatch(match);
    } else {
        m_haveCurrent = false;
    }
    report();
}

void FindBar::step(bool backwards)
{
    const FindQuery q = query();
    if (q.pattern.isEmpty()) {
        return;
    }
    // Typing is debounced, and Enter can arrive inside that window — somebody
    // who types quickly and presses it would otherwise step through the last
    // pattern rather than the one on the screen in front of them.
    m_debounce->stop();
    if (!install()) {
        return;
    }
    // Committing the pattern: Enter and the two buttons are somebody saying
    // this is the search they meant, which is when it earns a place in the
    // dropdown. Every prefix they typed on the way does not.
    emit patternUsed(q.pattern);

    quint64 line = m_haveCurrent ? (backwards ? m_current.line : m_current.endLine)
                                 : m_session->lineAt(0);
    int x = m_haveCurrent ? (backwards ? m_current.from : m_current.to) : 0;
    FindMatch match;
    if (!m_session->findNext(line, x, backwards, true, &match)) {
        m_haveCurrent = false;
        report();
        return;
    }
    showMatch(match);
    report();
}

void FindBar::showMatch(const FindMatch &match)
{
    m_current = match;
    m_haveCurrent = true;
    m_view->revealLine(match.line);
    m_view->selectSpan(SelPoint {match.line, match.from},
                       SelPoint {match.endLine, match.to});
}

bool FindBar::locate(const FindMatch &match, int *ordinal, int *total)
{
    // Walked from the oldest line rather than counted as the steps happen:
    // output arriving between two steps can add matches on either side of this
    // one, so a number kept in a member would drift away from the total beside
    // it. One walk answers both questions — asking the core to count as well
    // would be a second pass over the same history for the same frame.
    FindMatch at;
    quint64 line = 0;
    int x = 0;
    int n = 0;
    int found = 0;
    while (n < kOrdinalMax && m_session->findNext(line, x, false, false, &at)) {
        n++;
        if (found == 0 && at == match) {
            found = n;
        }
        line = at.endLine;
        x = at.to;
    }
    *ordinal = found;
    *total = n;
    // The cap is a bound on a loop driven by the far end's output, not a
    // judgement about how many matches are interesting. Say when it bit.
    return n < kOrdinalMax;
}

void FindBar::report()
{
    int ordinal = 0;
    int total = 0;
    bool exact = true;
    if (m_haveCurrent) {
        exact = locate(m_current, &ordinal, &total);
    } else {
        total = m_session->findCount();
    }
    if (total == 0) {
        setStatus(tr("No matches"), false);
        return;
    }
    const QString many = exact ? tr("%n match(es)", "", total)
                               : tr("more than %n match(es)", "", total);
    setStatus(ordinal > 0 && exact ? tr("%1 of %2").arg(ordinal).arg(total) : many,
              false);
}

void FindBar::setStatus(const QString &text, bool problem)
{
    m_status->setText(text);
    // The reason a pattern would not compile, in full, where the strip can only
    // show the beginning of it.
    m_status->setToolTip(text);
    QPalette palette = m_status->palette();
    palette.setColor(QPalette::WindowText,
                     problem ? QColor(0xc0, 0x30, 0x30)
                             : QApplication::palette().color(QPalette::WindowText));
    m_status->setPalette(palette);
}

void FindBar::keyPressEvent(QKeyEvent *event)
{
    // Escape closes. It reaches here because the bar's own children have the
    // focus while it is open, so the terminal never sees it — which is what
    // keeps Escape a key the host receives at every other moment.
    if (event->key() == Qt::Key_Escape && event->modifiers() == Qt::NoModifier) {
        close();
        event->accept();
        return;
    }
    QWidget::keyPressEvent(event);
}
