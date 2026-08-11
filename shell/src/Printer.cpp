#include "Printer.h"

#include <QFile>
#include <QFileInfo>
#include <QFont>
#include <QFontMetricsF>
#include <QPageLayout>
#include <QPageSize>
#include <QPainter>
#include <QPrinter>
#include <QTimer>

#include "Session.h"

namespace {

/// The face a printed page is set in.
///
/// **Not `PrnFont` yet.** That key goes through `ReadFont` (`ttset.c:1249`),
/// which is a name, two sizes and a Windows character set in one value, and the
/// settings schema has no type for it — so it is one of the two printer keys
/// that do not round-trip rather than one that does, and inventing a spelling
/// here would put a line in somebody's `TERATERM.INI` their own Tera Term
/// ignores. Monospace until it is transcribed: a terminal's output is columns.
QFont printerFont()
{
    QFont font;
    font.setStyleHint(QFont::Monospace);
    font.setFamily(QStringLiteral("monospace"));
    font.setFixedPitch(true);
    font.setPointSize(10);
    return font;
}

/// `PrnMargin` is four numbers in **hundredths of an inch** (`ttset.c:1255`),
/// left, right, top and bottom in that order — which is not the order
/// `QPageLayout` takes them in, and getting it wrong is a page that prints
/// perfectly and in the wrong place.
QMarginsF printerMargins(const Session &session)
{
    auto mm = [&](const char *name) {
        return session.setting(QLatin1String(name)).toDouble() * 0.254;
    };
    return QMarginsF(mm("printer.margin_left"), mm("printer.margin_top"),
                     mm("printer.margin_right"), mm("printer.margin_bottom"));
}

} // namespace

Printer::Printer(Session *session, QObject *parent)
    : QObject(parent)
    , m_session(session)
    , m_timer(new QTimer(this))
{
    m_timer->setSingleShot(true);
    connect(m_timer, &QTimer::timeout, this, &Printer::start);
}

Printer::~Printer() = default;

void Printer::handle(const TtPrinterEvent &event)
{
    switch (event.op) {
    case TT_PRINTER_OP_OPEN:
        // A second `Open` before the first job closed cannot happen — the core
        // shares one job between the two modes — but if it ever does, the
        // half-filled one is upstream's leaked spool file rather than a page.
        m_job.clear();
        m_open = true;
        break;
    case TT_PRINTER_OP_WRITE:
        if (event.text != nullptr) {
            m_job += QString::fromUtf8(event.text);
        }
        break;
    case TT_PRINTER_OP_CLOSE: {
        m_open = false;
        // `ClosePrnFile` starts a timer rather than printing, and the delay is
        // load-bearing: auto print closes and reopens a job around every
        // `CSI ? 1 i`, and without the wait each one would be a page.
        m_pendingJob += m_job;
        m_job.clear();
        m_pending = true;
        const int delay = m_session->setting(QStringLiteral("printer.passthrough_delay")).toInt();
        m_timer->start(qMax(0, delay) * 1000);
        break;
    }
    case TT_PRINTER_OP_SCREEN:
        printScreen(event.scroll_region != 0);
        break;
    }
}

void Printer::flushNow()
{
    if (m_pending) {
        m_timer->stop();
        start();
    }
}

void Printer::printScreen(bool scrollRegion)
{
    // `BuffPrint` renders the *screen*, so it does not go through the spool at
    // all upstream and does not go through the open job here either. What it
    // shares is the destination.
    size_t top = 0;
    size_t bottom = 0;
    m_session->scrollRegion(&top, &bottom);
    const size_t first = scrollRegion ? top : 0;
    const size_t last = scrollRegion ? bottom : static_cast<size_t>(m_session->rows()) - 1;

    QString text;
    for (size_t y = first; y <= last; y++) {
        size_t len = 0;
        const TtCell *cells = m_session->row(static_cast<int>(y), &len);
        QString line;
        for (size_t x = 0; x < len; x++) {
            if (cells[x].width_class == 2) { // the padding half of a wide cell
                continue;
            }
            for (size_t i = 0; i < TT_CELL_TEXT_MAX && cells[x].text[i] != 0; i++) {
                line += QString::fromUcs4(reinterpret_cast<const char32_t *>(&cells[x].text[i]), 1);
            }
        }
        while (line.endsWith(QLatin1Char(' '))) {
            line.chop(1);
        }
        text += line;
        text += QLatin1String("\r\n");
    }
    m_pendingJob += text;
    m_pending = true;
    // No delay: nobody asked for a screen and then asked again half a second
    // later, and the setting is named after the pass-through path.
    m_timer->start(0);
}

void Printer::start()
{
    const QString job = m_pendingJob;
    m_pendingJob.clear();
    m_pending = false;
    if (job.isEmpty()) {
        return;
    }
    const QString device = m_session->setting(QStringLiteral("printer.passthrough_port"));
    if (!device.isEmpty()) {
        // `PrintFileDirect` writes the spool through unchanged, so the job is
        // the bytes and not a rendering.
        QFile out(device);
        // Append rather than truncate: the destination is a device on a port
        // as often as it is a file, and truncating a device is meaningless
        // while truncating somebody's capture file is a loss.
        if (!out.open(QIODevice::WriteOnly | QIODevice::Append)) {
            emit notice(tr("Cannot open the printer port %1: %2")
                            .arg(device, out.errorString()));
            return;
        }
        // `UTF32ToMBCP(u32, CP_ACP)` on the way out of upstream's spool. There
        // is no ACP here, so this is the platform's own 8-bit encoding — the
        // same substitution the log's `&u` and the temporary directory make.
        const QByteArray bytes = job.toLocal8Bit();
        if (out.write(bytes) != bytes.size()) {
            emit notice(tr("Printing to %1 failed: %2").arg(device, out.errorString()));
        }
        return;
    }

    QPrinter printer(QPrinter::HighResolution);
    printer.setDocName(QStringLiteral("Sterna"));
    QPageLayout layout = printer.pageLayout();
    layout.setUnits(QPageLayout::Millimeter);
    layout.setMargins(printerMargins(*m_session));
    printer.setPageLayout(layout);
    if (printer.printerName().isEmpty()) {
        emit notice(tr("No printer is configured"));
        return;
    }

    QPainter painter;
    if (!painter.begin(&printer)) {
        emit notice(tr("Could not start printing"));
        return;
    }
    const QFont font = printerFont();
    painter.setFont(font);
    const QFontMetricsF metrics(font, &printer);
    const qreal lineHeight = metrics.height();
    const QRectF page = printer.pageLayout().paintRectPixels(printer.resolution());
    qreal y = metrics.ascent();

    // `PrnConvFF` (`ttset.c:1263`, default off) decides what a form feed is:
    // off it starts a page, and on it is one more line break. That is the way
    // round it reads in `teraprn.cpp:388`, where the *new page* branch is the
    // one guarded by the setting being zero.
    const bool convertFormFeed =
        m_session->setting(QStringLiteral("printer.convert_form_feed")) == QLatin1String("on");

    auto newPage = [&]() {
        printer.newPage();
        y = metrics.ascent();
    };
    QString line;
    auto flushLine = [&]() {
        if (y > page.height()) {
            newPage();
        }
        painter.drawText(QPointF(0, y), line);
        y += lineHeight;
        line.clear();
    };
    for (const QChar c : job) {
        if (c == QLatin1Char('\n')) {
            flushLine();
        } else if (c == QLatin1Char('\r')) {
            continue;
        } else if (c == QChar(0x0c)) {
            flushLine();
            if (!convertFormFeed) {
                newPage();
            }
        } else if (c == QLatin1Char('\t')) {
            line += QLatin1String("        ");
        } else if (c.isPrint()) {
            line += c;
        }
    }
    if (!line.isEmpty()) {
        flushLine();
    }
    painter.end();
}
