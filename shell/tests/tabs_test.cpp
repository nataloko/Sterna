// Multiple sessions in one window.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include <QAction>
#include <QApplication>
#include <QFile>
#include <QTabWidget>
#include <QTemporaryDir>

#include <cstdio>

#include "MainWindow.h"
#include "Session.h"
#include "TerminalPage.h"

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAILED %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

namespace {

QString screenText(const Session &session)
{
    QString out;
    for (int y = 0; y < session.rows(); y++) {
        size_t len = 0;
        const TtCell *row = session.row(y, &len);
        for (size_t x = 0; row && x < len; x++) {
            if (row[x].width_class != TT_WIDTH_PAD && row[x].text[0] != 0) {
                out += QChar(static_cast<char16_t>(row[x].text[0]));
            }
        }
        out += QLatin1Char('\n');
    }
    return out;
}

void test_tabs_are_independent_and_actions_follow_the_active_one()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString ini = dir.filePath(QStringLiteral("sterna.ini"));
    QFile file(ini);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[Tera Term]\r\nTerminalSize=40,10\r\n");
    file.close();

    MainWindow window(ini);
    auto *tabs = window.findChild<QTabWidget *>();
    auto *add = window.findChild<QAction *>(QStringLiteral("newTabAction"));
    auto *close = window.findChild<QAction *>(QStringLiteral("closeTabAction"));
    CHECK(tabs != nullptr);
    CHECK(add != nullptr);
    CHECK(close != nullptr);
    CHECK(tabs->count() == 1);
    CHECK(tabs->tabBarAutoHide());

    auto *first = static_cast<TerminalPage *>(tabs->widget(0));
    first->session()->feed(QByteArrayLiteral("first"));

    add->trigger();
    CHECK(tabs->count() == 2);
    CHECK(tabs->tabsClosable());
    auto *second = static_cast<TerminalPage *>(tabs->widget(1));
    CHECK(first != second);
    CHECK(window.session() == second->session());
    CHECK(second->session()->cols() == 40);
    CHECK(second->session()->rows() == 10);

    second->session()->feed(QByteArrayLiteral("second"));
    CHECK(screenText(*first->session()).contains(QStringLiteral("first")));
    CHECK(!screenText(*first->session()).contains(QStringLiteral("second")));
    CHECK(screenText(*second->session()).contains(QStringLiteral("second")));
    CHECK(!screenText(*second->session()).contains(QStringLiteral("first")));

    tabs->setCurrentIndex(0);
    CHECK(window.session() == first->session());
    tabs->setCurrentIndex(1);
    CHECK(window.session() == second->session());

    close->trigger();
    CHECK(tabs->count() == 1);
    CHECK(window.session() == first->session());
    CHECK(!tabs->tabsClosable());
}

} // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("tabs_test"));
    test_tabs_are_independent_and_actions_follow_the_active_one();
    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    puts("tabs ok");
    return 0;
}
