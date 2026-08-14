// One connection somebody actually opened, and the list of them.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QHash>
#include <QString>
#include <QVector>

#include "sterna.h"

/// A destination, with the parameters it was opened with.
///
/// The nine `recent.*` keys beside this one remember *one* of each kind, which
/// is what seeds the connect dialog. A list is a different thing: it has to say
/// which kind each entry is, and it has to carry that entry's own parameters,
/// or picking one is a guess. A serial console at 9600 and a router at 115200
/// are two lines in the same list.
///
/// It holds exactly the fields the connect dialog asks for and no more.
/// Everything else a connection needs is a setting, and a setting that changes
/// should change for a remembered connection too — freezing a whole
/// `TtSerialParams` here would pin answers the settings file is supposed to
/// own, and a `DtrControl` that stopped following Setup would be a mystery
/// nobody could see the cause of.
struct RecentConnection {
    enum class Kind { Serial, Ssh, Telnet, Shell };

    Kind kind = Kind::Shell;

    /// [`Kind::Serial`]: the `open_path`, never the `/dev/ttyUSB<n>` name —
    /// that one is assigned in attach order and points somewhere else after a
    /// replug, which is the whole reason a remembered port needs the stable
    /// spelling.
    QString path;
    quint32 baud = 0;
    quint8 bits = 8;
    TtParity parity = TT_PARITY_NONE;
    quint8 stop = 1;
    TtFlowControl flow = TT_FLOW_CONTROL_NONE;

    /// [`Kind::Ssh`] and [`Kind::Telnet`].
    QString host;
    /// SSH only. **Empty is not an empty user name**: it means whatever
    /// `~/.ssh/config` says, and the record keeps that distinction because the
    /// connect call does.
    QString user;
    /// Zero is the same kind of absence: `~/.ssh/config`'s `Port`, then 22.
    quint16 port = 0;
    QString identity;
    bool legacy = false;
    TtTelnetMode mode = TT_TELNET_AUTO;

    static RecentConnection serial(const QString &path,
                                   const TtSerialParams &params);
    static RecentConnection ssh(const QString &host, const QString &user,
                                quint16 port, const QString &identity,
                                bool legacy);
    static RecentConnection telnet(const QString &host, quint16 port,
                                   TtTelnetMode mode);
    static RecentConnection shell();

    /// The remembered line settings laid over `base`, which is where every
    /// field this record does not hold comes from. The same shape as `--baud`,
    /// which overrides one field of the settings' parameters rather than
    /// replacing the set.
    TtSerialParams appliedTo(TtSerialParams base) const;

    /// The INI form. See the schema comment on `recent.connections`.
    QString encode() const;
    /// False for anything that does not parse — a settings file is hand-edited,
    /// so a bad record is dropped rather than repaired.
    static bool decode(const QString &text, RecentConnection *out);

    /// Would these open the same thing? Parameters are deliberately not part of
    /// the answer: reconnecting to the same port at a different baud replaces
    /// the entry rather than adding a second one, because the list is of places
    /// and the newest parameters are the ones that worked.
    bool sameDestination(const RecentConnection &other) const;

    /// What the dropdown shows. `deviceFor` maps `open_path` to the friendlier
    /// `device` name for ports that are plugged in right now; a port that is
    /// not gets its stored path, because a name it no longer answers to would
    /// be a worse lie than a long one.
    QString label(const QHash<QString, QString> &deviceFor = {}) const;
};

/// The list, newest first.
namespace recent {

/// Ten. Long enough to hold a working set, short enough that the dropdown does
/// not need scrolling and the INI line stays readable.
constexpr int Max = 10;

QVector<RecentConnection> decode(const QString &value);
QString encode(const QVector<RecentConnection> &list);

/// Put `one` at the front, drop any older record for the same destination, and
/// bound the length.
void remember(QVector<RecentConnection> &list, const RecentConnection &one);

} // namespace recent
