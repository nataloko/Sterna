// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Recent.h"

#include <QFileInfo>
#include <QObject>
#include <QStringList>

namespace {

/// The six characters that mean something to the format, and `%` itself.
///
/// Only these: a device path is mostly `/`, `-`, `.` and `:`, and a record
/// somebody has to read in their own INI file should still look like the path
/// they typed. Non-ASCII goes through untouched for the same reason — the
/// file's encoding is the file's business.
const QLatin1String kDelimiters("%;?&=@");

QString esc(const QString &text)
{
    QString out;
    out.reserve(text.size());
    for (QChar c : text) {
        if (c.unicode() < 0x80 && kDelimiters.contains(QLatin1Char(c.toLatin1()))) {
            out += QString::asprintf("%%%02X", c.unicode());
        } else {
            out += c;
        }
    }
    return out;
}

QString unesc(const QString &text)
{
    QString out;
    out.reserve(text.size());
    for (int i = 0; i < text.size(); i++) {
        if (text.at(i) == QLatin1Char('%') && i + 2 < text.size()) {
            bool ok = false;
            const int value = QStringView(text).mid(i + 1, 2).toInt(&ok, 16);
            if (ok) {
                out += QChar(static_cast<char16_t>(value));
                i += 2;
                continue;
            }
        }
        out += text.at(i);
    }
    return out;
}

QString parityName(TtParity parity)
{
    switch (parity) {
    case TT_PARITY_ODD: return QStringLiteral("odd");
    case TT_PARITY_EVEN: return QStringLiteral("even");
    case TT_PARITY_MARK: return QStringLiteral("mark");
    case TT_PARITY_SPACE: return QStringLiteral("space");
    default: return QStringLiteral("none");
    }
}

bool parityFrom(const QString &name, TtParity *out)
{
    static const QHash<QString, TtParity> table = {
        {QStringLiteral("none"), TT_PARITY_NONE},
        {QStringLiteral("odd"), TT_PARITY_ODD},
        {QStringLiteral("even"), TT_PARITY_EVEN},
        {QStringLiteral("mark"), TT_PARITY_MARK},
        {QStringLiteral("space"), TT_PARITY_SPACE},
    };
    const auto at = table.constFind(name);
    if (at == table.constEnd()) {
        return false;
    }
    *out = *at;
    return true;
}

/// The schema's spellings, not this file's — `setbaud` and the serial dialog
/// already write `hard` into `[Tera Term] FlowCtrl`, and two vocabularies for
/// one enum is how a settings file ends up meaning two things.
QString flowName(TtFlowControl flow)
{
    switch (flow) {
    case TT_FLOW_CONTROL_XON_XOFF: return QStringLiteral("x");
    case TT_FLOW_CONTROL_RTS_CTS: return QStringLiteral("hard");
    case TT_FLOW_CONTROL_DSR_DTR: return QStringLiteral("dsrdtr");
    default: return QStringLiteral("none");
    }
}

bool flowFrom(const QString &name, TtFlowControl *out)
{
    static const QHash<QString, TtFlowControl> table = {
        {QStringLiteral("none"), TT_FLOW_CONTROL_NONE},
        {QStringLiteral("x"), TT_FLOW_CONTROL_XON_XOFF},
        {QStringLiteral("hard"), TT_FLOW_CONTROL_RTS_CTS},
        {QStringLiteral("dsrdtr"), TT_FLOW_CONTROL_DSR_DTR},
    };
    const auto at = table.constFind(name);
    if (at == table.constEnd()) {
        return false;
    }
    *out = *at;
    return true;
}

QString modeName(TtTelnetMode mode)
{
    switch (mode) {
    case TT_TELNET_NEGOTIATE: return QStringLiteral("negotiate");
    case TT_TELNET_FRAMED: return QStringLiteral("framed");
    case TT_TELNET_RAW: return QStringLiteral("raw");
    default: return QStringLiteral("auto");
    }
}

bool modeFrom(const QString &name, TtTelnetMode *out)
{
    static const QHash<QString, TtTelnetMode> table = {
        {QStringLiteral("auto"), TT_TELNET_AUTO},
        {QStringLiteral("negotiate"), TT_TELNET_NEGOTIATE},
        {QStringLiteral("framed"), TT_TELNET_FRAMED},
        {QStringLiteral("raw"), TT_TELNET_RAW},
    };
    const auto at = table.constFind(name);
    if (at == table.constEnd()) {
        return false;
    }
    *out = *at;
    return true;
}

/// `host[:port]`, splitting on the *last* colon so a bracketed IPv6 literal
/// survives. The same rule `main.cpp` applies to a positional argument, and
/// the same one `ssh` applies: a bare IPv6 address without brackets is
/// ambiguous here exactly as it is there.
void splitHost(const QString &text, QString *host, quint16 *port)
{
    *host = text;
    const int colon = text.lastIndexOf(QLatin1Char(':'));
    if (colon > text.lastIndexOf(QLatin1Char(']'))) {
        bool ok = false;
        const uint value = QStringView(text).mid(colon + 1).toUInt(&ok);
        if (ok && value <= 65535) {
            *port = static_cast<quint16>(value);
            *host = text.left(colon);
        }
    }
}

/// `a=1&b=2` into a table. Unknown keys are kept: a record written by a later
/// version should lose the fields this one cannot read and not the record.
QHash<QString, QString> query(const QString &text)
{
    QHash<QString, QString> out;
    const QStringList parts =
        text.split(QLatin1Char('&'), Qt::SkipEmptyParts);
    for (const QString &part : parts) {
        const int eq = part.indexOf(QLatin1Char('='));
        if (eq > 0) {
            out.insert(part.left(eq), unesc(part.mid(eq + 1)));
        }
    }
    return out;
}

} // namespace

RecentConnection RecentConnection::serial(const QString &path,
                                          const TtSerialParams &params)
{
    RecentConnection out;
    out.kind = Kind::Serial;
    out.path = path;
    out.baud = params.baud;
    out.bits = params.data_bits;
    out.parity = params.parity;
    out.stop = params.stop_bits;
    out.flow = params.flow;
    return out;
}

RecentConnection RecentConnection::ssh(const QString &host, const QString &user,
                                       quint16 port, const QString &identity,
                                       bool legacy)
{
    RecentConnection out;
    out.kind = Kind::Ssh;
    out.host = host;
    out.user = user;
    out.port = port;
    out.identity = identity;
    out.legacy = legacy;
    return out;
}

RecentConnection RecentConnection::telnet(const QString &host, quint16 port,
                                          TtTelnetMode mode)
{
    RecentConnection out;
    out.kind = Kind::Telnet;
    out.host = host;
    out.port = port;
    out.mode = mode;
    return out;
}

RecentConnection RecentConnection::shell()
{
    RecentConnection out;
    out.kind = Kind::Shell;
    return out;
}

TtSerialParams RecentConnection::appliedTo(TtSerialParams base) const
{
    if (kind != Kind::Serial) {
        return base;
    }
    base.baud = baud;
    base.data_bits = bits;
    base.parity = parity;
    base.stop_bits = stop;
    base.flow = flow;
    return base;
}

QString RecentConnection::encode() const
{
    switch (kind) {
    case Kind::Serial:
        return QStringLiteral("serial:%1?baud=%2&bits=%3&parity=%4&stop=%5&flow=%6")
            .arg(esc(path))
            .arg(baud)
            .arg(bits)
            .arg(parityName(parity), QString::number(stop), flowName(flow));
    case Kind::Ssh: {
        QString out = QStringLiteral("ssh://");
        if (!user.isEmpty()) {
            out += esc(user) + QLatin1Char('@');
        }
        out += esc(host);
        if (port != 0) {
            out += QLatin1Char(':') + QString::number(port);
        }
        QStringList fields;
        if (!identity.isEmpty()) {
            fields << QStringLiteral("identity=") + esc(identity);
        }
        if (legacy) {
            fields << QStringLiteral("legacy=1");
        }
        if (!fields.isEmpty()) {
            out += QLatin1Char('?') + fields.join(QLatin1Char('&'));
        }
        return out;
    }
    case Kind::Telnet:
        return QStringLiteral("telnet://%1:%2?mode=%3")
            .arg(esc(host), QString::number(port), modeName(mode));
    case Kind::Shell:
        break;
    }
    return QStringLiteral("shell:");
}

bool RecentConnection::decode(const QString &text, RecentConnection *out)
{
    const QString trimmed = text.trimmed();
    const int colon = trimmed.indexOf(QLatin1Char(':'));
    if (colon <= 0) {
        return false;
    }
    const QString scheme = trimmed.left(colon);
    QString rest = trimmed.mid(colon + 1);
    // `://` for the two that have an authority, a bare `:` for the two that do
    // not. Accepting either spelling for either would let `serial://x` through
    // as a path beginning with two slashes.
    const bool authority = rest.startsWith(QLatin1String("//"));
    if (authority) {
        rest = rest.mid(2);
    }

    QString head = rest;
    QHash<QString, QString> fields;
    const int mark = rest.indexOf(QLatin1Char('?'));
    if (mark >= 0) {
        head = rest.left(mark);
        fields = query(rest.mid(mark + 1));
    }

    RecentConnection made;
    if (scheme == QLatin1String("serial") && !authority) {
        made.kind = Kind::Serial;
        made.path = unesc(head);
        if (made.path.isEmpty()) {
            return false;
        }
        bool ok = false;
        made.baud = fields.value(QStringLiteral("baud")).toUInt(&ok);
        if (!ok || made.baud == 0) {
            return false;
        }
        const uint bitCount = fields.value(QStringLiteral("bits")).toUInt(&ok);
        if (!ok || bitCount < 5 || bitCount > 8) {
            return false;
        }
        made.bits = static_cast<quint8>(bitCount);
        const uint stopCount = fields.value(QStringLiteral("stop")).toUInt(&ok);
        if (!ok || stopCount < 1 || stopCount > 2) {
            return false;
        }
        made.stop = static_cast<quint8>(stopCount);
        if (!parityFrom(fields.value(QStringLiteral("parity")), &made.parity)
            || !flowFrom(fields.value(QStringLiteral("flow")), &made.flow)) {
            return false;
        }
    } else if (scheme == QLatin1String("ssh") && authority) {
        made.kind = Kind::Ssh;
        const int at = head.lastIndexOf(QLatin1Char('@'));
        if (at >= 0) {
            made.user = unesc(head.left(at));
            head = head.mid(at + 1);
        }
        QString host;
        splitHost(head, &host, &made.port);
        made.host = unesc(host);
        if (made.host.isEmpty()) {
            return false;
        }
        made.identity = fields.value(QStringLiteral("identity"));
        made.legacy = fields.value(QStringLiteral("legacy")) == QLatin1String("1");
    } else if (scheme == QLatin1String("telnet") && authority) {
        made.kind = Kind::Telnet;
        QString host;
        made.port = 23;
        splitHost(head, &host, &made.port);
        made.host = unesc(host);
        if (made.host.isEmpty()) {
            return false;
        }
        if (fields.contains(QStringLiteral("mode"))
            && !modeFrom(fields.value(QStringLiteral("mode")), &made.mode)) {
            return false;
        }
    } else if (scheme == QLatin1String("shell") && !authority) {
        made.kind = Kind::Shell;
        if (!head.isEmpty()) {
            return false;
        }
    } else {
        return false;
    }

    *out = made;
    return true;
}

bool RecentConnection::sameDestination(const RecentConnection &other) const
{
    if (kind != other.kind) {
        return false;
    }
    switch (kind) {
    case Kind::Serial:
        return path == other.path;
    case Kind::Ssh:
        return host == other.host && user == other.user && port == other.port;
    case Kind::Telnet:
        return host == other.host && port == other.port;
    case Kind::Shell:
        return true;
    }
    return false;
}

QString RecentConnection::label(const QHash<QString, QString> &deviceFor) const
{
    switch (kind) {
    case Kind::Serial: {
        const QString device = deviceFor.value(path, path);
        // The basename of a `device` is `ttyUSB0`; the basename of a
        // `by-path` name is the bus topology, which is worse than the whole
        // path because it looks like a name and is not one.
        const QString shown = deviceFor.contains(path)
            ? QFileInfo(device).fileName()
            : device;
        QString line = QStringLiteral("%1 %2%3%4")
                           .arg(baud)
                           .arg(bits)
                           .arg(parityName(parity).at(0).toUpper())
                           .arg(stop);
        if (flow != TT_FLOW_CONTROL_NONE) {
            line += QLatin1Char(' ') + flowName(flow);
        }
        return shown + QStringLiteral("  ") + line;
    }
    case Kind::Ssh: {
        QString out = QStringLiteral("ssh ");
        if (!user.isEmpty()) {
            out += user + QLatin1Char('@');
        }
        out += host;
        if (port != 0) {
            out += QLatin1Char(':') + QString::number(port);
        }
        return out;
    }
    case Kind::Telnet:
        return QStringLiteral("telnet %1:%2").arg(host).arg(port);
    case Kind::Shell:
        break;
    }
    return QObject::tr("Local shell");
}

QVector<RecentConnection> recent::decode(const QString &value)
{
    QVector<RecentConnection> out;
    const QStringList records =
        value.split(QLatin1Char(';'), Qt::SkipEmptyParts);
    for (const QString &record : records) {
        RecentConnection one;
        if (RecentConnection::decode(record, &one)) {
            out.append(one);
        }
    }
    return out;
}

QString recent::encode(const QVector<RecentConnection> &list)
{
    QStringList records;
    records.reserve(list.size());
    for (const RecentConnection &one : list) {
        records << one.encode();
    }
    return records.join(QLatin1Char(';'));
}

void recent::remember(QVector<RecentConnection> &list,
                      const RecentConnection &one)
{
    for (int i = list.size() - 1; i >= 0; i--) {
        if (list.at(i).sameDestination(one)) {
            list.remove(i);
        }
    }
    list.prepend(one);
    while (list.size() > Max) {
        list.removeLast();
    }
}
