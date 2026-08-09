//! `parse_port_from_buf` (`servicenames.c:382`) — a TCP port written as a name.
//!
//! `/P=telnet`, `host:ssh` and `telnet://host:finger/` all resolve through the
//! same 317-entry table, which is upstream's own and **not** `/etc/services`:
//! `getservbyname` would answer differently on two machines and differently
//! again on Windows, and a shortcut somebody wrote in 2003 has to keep opening
//! the same port. The table is transcribed from
//! `teraterm/common/servicenames.c`, which is Robert O'Callahan's under a
//! BSD-3-clause notice — see `ATTRIBUTION.md`.
//!
//! Two quirks matter, and both are reachable from a shortcut. `ParsePortName`
//! (`ttset.c:3455`) takes the table's answer *or* `sscanf("%d")`, in that
//! order, so a number the table rejects for being out of range still gets
//! through the second door: `/P=99999` is 99999, truncated to a `WORD` by the
//! variable it lands in. And a name that matches nothing at all is 0, which the
//! resolution at the end of `_ParseParam` reads as "not given" and skips — so a
//! misspelt service name leaves the configured port alone rather than failing.

/// The table, sorted by name because upstream reaches it with `bsearch` — a
/// plain lookup and a binary search agree only while that holds, which is what
/// [`tests::the_table_is_sorted_and_whole`] is for.
static SERVICES: &[(&str, u16)] = &[
    ("arns", 384),
    ("at-echo", 204),
    ("at-nbp", 202),
    ("at-rtmp", 201),
    ("at-zis", 206),
    ("auditd", 705),
    ("auth", 113),
    ("authentication", 113),
    ("bftp", 152),
    ("bgp", 179),
    ("bootp", 67),
    ("bootpc", 68),
    ("bootps", 67),
    ("bshell", 881),
    ("bshelldbg", 1133),
    ("buildd", 877),
    ("cfinger", 2003),
    ("chargen", 19),
    ("chat", 531),
    ("client", 2030),
    ("cmd", 514),
    ("cmip-agent", 164),
    ("cmip-man", 163),
    ("coda_aux1", 1431),
    ("coda_aux2", 1433),
    ("coda_aux3", 1435),
    ("coda_backup", 1407),
    ("codacon", 1423),
    ("codger", 1285),
    ("codgerdbg", 1541),
    ("conference", 531),
    ("controller", 1389),
    ("courier", 530),
    ("cpsys", 1100),
    ("csd", 1346),
    ("csddbg", 1602),
    ("csnet-ns", 105),
    ("cso-ns", 105),
    ("cvskserver", 1999),
    ("daserver", 987),
    ("daytime", 13),
    ("dbp", 1429),
    ("dictionary", 10300),
    ("discard", 9),
    ("discuss", 2100),
    ("disptool0", 1399),
    ("disptool1", 1401),
    ("disptool2", 1403),
    ("disptool3", 1405),
    ("domain", 53),
    ("dos", 7000),
    ("echo", 7),
    ("eda1_mbx", 8100),
    ("eda2_mbx", 8101),
    ("eda_mbx", 8000),
    ("efs", 520),
    ("eklogin", 2105),
    ("ekshell", 545),
    ("erim", 1377),
    ("erimdbg", 1617),
    ("erlogin", 888),
    ("exec", 512),
    ("filesrv", 2001),
    ("finger", 79),
    ("ftp", 21),
    ("ftp-data", 20),
    ("gds_db", 1397),
    ("gopher", 70),
    ("greendbg", 1025),
    ("grmd", 5999),
    ("hcserver", 5710),
    ("hesupd", 751),
    ("hostname", 101),
    ("hostnames", 101),
    ("http", 80),
    ("https", 443),
    ("iasqlsvr", 7489),
    ("ident", 113),
    ("imap", 143),
    ("imap2", 143),
    ("imap3", 220),
    ("imap4", 143),
    ("imaps", 993),
    ("ingreslock", 1524),
    ("instsrv", 1234),
    ("ipt", 1387),
    ("ipx", 213),
    ("irc", 194),
    ("irc-alt", 6667),
    ("ishell", 883),
    ("iso-tsap", 102),
    ("itkt", 885),
    ("jeeves", 1439),
    ("joysticknav", 1389),
    ("joysticknavdbg", 1629),
    ("kcmd", 544),
    ("kdc", 750),
    ("kdebug", 10401),
    ("kerberos", 750),
    ("kerberos-adm", 749),
    ("kerberos-sec", 88),
    ("kerberos_master", 751),
    ("kjdbc", 1445),
    ("klogin", 543),
    ("kopexec", 561),
    ("kopshell", 562),
    ("kpasswd", 761),
    ("kpop", 1109),
    ("kpopr", 1110),
    ("kpwd", 761),
    ("krb5", 88),
    ("krb524", 4444),
    ("krb5_prop", 754),
    ("krb_prop", 754),
    ("krbupdate", 760),
    ("krcmd", 545),
    ("krcp", 1443),
    ("kreg", 760),
    ("kshell", 544),
    ("ktc", 750),
    ("kxct", 549),
    ("lanmgrx.osb", 5696),
    ("lcladm", 1441),
    ("link", 87),
    ("linuxconf", 98),
    ("listen", 2766),
    ("listener", 1025),
    ("loc-srv", 135),
    ("lockd", 4045),
    ("login", 513),
    ("mail", 25),
    ("man", 9535),
    ("mbatchd", 3881),
    ("motionnav", 1393),
    ("motionnavdbg", 1633),
    ("msdos", 7000),
    ("msp", 18),
    ("mtp", 57),
    ("name", 42),
    ("nameserver", 42),
    ("nanny", 773),
    ("nbdgm", 138),
    ("nbns", 137),
    ("nbssn", 139),
    ("ndim", 1419),
    ("netbios-dgm", 138),
    ("netbios-ns", 137),
    ("netbios-ssn", 139),
    ("netbios_dgm", 138),
    ("netbios_ns", 137),
    ("netbios_ssn", 139),
    ("netdist", 2106),
    ("netimage", 1287),
    ("netimagedbg", 1543),
    ("netnews", 532),
    ("netreg", 1353),
    ("netregdbg", 1609),
    ("netrjs", 77),
    ("netstat", 15),
    ("network_terminal", 1026),
    ("newdate", 526),
    ("news", 144),
    ("nextstep", 178),
    ("nft", 1536),
    ("nicname", 43),
    ("nntp", 119),
    ("nterm", 1026),
    ("ntp", 123),
    ("ntpd", 123),
    ("null", 9),
    ("odexm", 891),
    ("opcmd", 560),
    ("opshell", 560),
    ("pag", 879),
    ("papyrus", 893),
    ("parvis", 1379),
    ("parvisdbg", 1619),
    ("pcserver", 600),
    ("pcserverbulk", 2026),
    ("pcserverrpc", 2025),
    ("pharos", 1385),
    ("pierunt", 1373),
    ("pieruntdbg", 1613),
    ("piesrv", 1351),
    ("piesrvdbg", 1607),
    ("pmlockd", 1889),
    ("pop", 109),
    ("pop-2", 109),
    ("pop-3", 110),
    ("pop2", 109),
    ("pop3", 110),
    ("pop3s", 995),
    ("portmap", 111),
    ("portmapper", 111),
    ("postoffice", 109),
    ("print-srv", 170),
    ("printer", 515),
    ("prospero", 191),
    ("prospero-np", 1525),
    ("qotd", 17),
    ("quote", 17),
    ("rauth", 601),
    ("rcisimmux", 5347),
    ("rdeliver", 1530),
    ("rdp", 3389),
    ("readnews", 532),
    ("recserv", 7815),
    ("rem", 64),
    ("remote_file_sharing", 1025),
    ("remote_login", 1026),
    ("remotefs", 556),
    ("res", 3878),
    ("resolve", 875),
    ("resolvedbg", 1131),
    ("rfa", 4672),
    ("rfb", 5900),
    ("rfe", 5002),
    ("rfs", 1025),
    ("rfs_server", 556),
    ("rfsdbg", 1027),
    ("rje", 77),
    ("rlb", 1260),
    ("rndb2", 1425),
    ("rpc", 530),
    ("rpcbind", 111),
    ("rpl", 1347),
    ("rpldbg", 1603),
    ("rtelnet", 107),
    ("sbatchd", 3882),
    ("serv", 778),
    ("service_warp", 1375),
    ("service_warpdbg", 1615),
    ("sftp", 115),
    ("sgi-dgl", 5232),
    ("shell", 514),
    ("shelob", 1135),
    ("shelobsrv", 1137),
    ("sieve", 4190),
    ("sink", 9),
    ("sms_db", 775),
    ("sms_update", 777),
    ("smtp", 25),
    ("smtps", 465),
    ("smux", 199),
    ("snagas", 108),
    ("source", 19),
    ("spc", 6111),
    ("spooler", 515),
    ("ssh", 22),
    ("sshprop", 23523),
    ("statnav", 1391),
    ("statnavdbg", 1621),
    ("stm_switch", 1395),
    ("submission", 587),
    ("sunmatrox", 1283),
    ("sunmatroxdbg", 1539),
    ("sunrpc", 111),
    ("supdup", 95),
    ("supfiledbg", 1127),
    ("supfilesrv", 871),
    ("supnamedbg", 1125),
    ("supnamesrv", 869),
    ("support", 1529),
    ("systat", 11),
    ("ta-rauth", 601),
    ("tap", 113),
    ("task_control", 1381),
    ("tcpmux", 1),
    ("telnet", 23),
    ("telnet2", 24),
    ("telnets", 992),
    ("tempo", 526),
    ("text", 17),
    ("time", 37),
    ("timserver", 37),
    ("tnet", 1600),
    ("tsap", 102),
    ("ttylink", 87),
    ("ttytst", 19),
    ("ulistserv", 372),
    ("untp", 119),
    ("usenet", 119),
    ("users", 11),
    ("usim", 1400),
    ("uucp", 540),
    ("uucp-path", 117),
    ("uucpd", 540),
    ("vapor", 1387),
    ("vexec", 712),
    ("vice-exec", 712),
    ("vice-login", 713),
    ("vice-shell", 714),
    ("visim", 1371),
    ("visimdbg", 1611),
    ("vlogin", 713),
    ("vnc", 5900),
    ("vshell", 714),
    ("wais", 210),
    ("warplite", 1427),
    ("webster", 765),
    ("whois", 43),
    ("wiztemp", 1421),
    ("wm", 2000),
    ("wnn", 1383),
    ("wnn4", 22273),
    ("wnn4_cn", 22289),
    ("wnn4_jp", 22273),
    ("worldc", 1348),
    ("worldcdbg", 1604),
    ("writesrv", 2401),
    ("www", 80),
    ("x-server", 6000),
    ("x400", 103),
    ("x400-snd", 104),
    ("xdmcp", 177),
    ("xtermd", 873),
    ("z3950", 210),
];

/// `parse_port_from_buf` (`servicenames.c:382`) — the table, then `atoi`.
///
/// `-1` for anything that is neither, which is upstream's "not found" and is
/// not the same as 0. The name is lowercased into a 32-byte buffer first, so a
/// service name longer than 31 characters is compared truncated and cannot
/// match; reproduced, since the longest name in the table is 15.
pub fn parse_port_from_buf(buf: &[u8]) -> i32 {
    let lower: String = buf
        .iter()
        .take(31)
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect();
    if let Ok(i) = SERVICES.binary_search_by(|(name, _)| (*name).cmp(lower.as_str())) {
        return i32::from(SERVICES[i].1);
    }
    if buf.first().is_some_and(u8::is_ascii_digit) {
        // `atoi`, which stops at the first non-digit and does not complain.
        let n: i64 = lower
            .bytes()
            .take_while(u8::is_ascii_digit)
            .fold(0i64, |acc, b| {
                acc.saturating_mul(10)
                    .saturating_add(i64::from(b - b'0'))
                    .min(i64::from(i32::MAX))
            });
        if n > 0 && n < 65536 {
            return n as i32;
        }
    }
    -1
}

/// `ParsePortName` (`ttset.c:3455`) — the port a command line asked for, or 0.
///
/// The `sscanf` fallback is why this is not just [`parse_port_from_buf`]: a
/// numeric string the table refused — zero, negative, or 65536 and up — is
/// taken at face value here.
///
/// **It returns an `int` and the caller narrows it, and the order matters.**
/// `_ParseParam` stores it in a `WORD`, so `/P=65537` is port 1; but the bare
/// token after a host name is tested `> 0` *before* that truncation
/// (`ttset.c:3944`), so `myhost 65536` is a host with a port of 65536 — which
/// narrows to 0 and is then skipped as "not given" — rather than a second host
/// name called `65536`. Returning the narrowed value here would quietly swap
/// those two.
pub fn parse_port_name(buf: &[u8]) -> i32 {
    let port = parse_port_from_buf(buf);
    if port > 0 {
        return port;
    }
    scanf_int(buf).unwrap_or(0)
}

/// `swscanf(s, L"%d", &n)` — leading space, an optional sign, then digits, and
/// `None` when there were none. Used where upstream uses the return value to
/// decide whether the setting was given at all.
pub fn scanf_int(buf: &[u8]) -> Option<i32> {
    let mut i = 0;
    while matches!(buf.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    let neg = match buf.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut n: i64 = 0;
    while let Some(&b) = buf.get(i) {
        if !b.is_ascii_digit() {
            break;
        }
        n = n.saturating_mul(10).saturating_add(i64::from(b - b'0'));
        i += 1;
    }
    if i == start {
        return None;
    }
    let n = if neg { -n } else { n };
    Some(n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sorted, because a binary search over an unsorted table finds entries by
    /// luck; 317 entries, because a lost line is otherwise a service name that
    /// silently stops resolving.
    #[test]
    fn the_table_is_sorted_and_whole() {
        assert_eq!(SERVICES.len(), 317);
        assert!(SERVICES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn a_name_resolves_and_so_does_a_number() {
        assert_eq!(parse_port_name(b"telnet"), 23);
        assert_eq!(parse_port_name(b"ssh"), 22);
        assert_eq!(parse_port_name(b"TELNET"), 23);
        assert_eq!(parse_port_name(b"23"), 23);
        // The two entries that share a port are both reachable.
        assert_eq!(parse_port_name(b"auth"), 113);
        assert_eq!(parse_port_name(b"authentication"), 113);
    }

    /// The `sscanf` fallback, which is the whole reason `ParsePortName` exists
    /// rather than its callers using `parse_port_from_buf` directly.
    #[test]
    fn a_number_the_table_refuses_still_gets_through() {
        assert_eq!(parse_port_from_buf(b"99999"), -1);
        // ...whole, and it is the caller that narrows it to a port.
        assert_eq!(parse_port_name(b"99999"), 99999);
        assert_eq!(parse_port_name(b"65537") as u16, 1);
        assert_eq!(parse_port_name(b"-12"), -12);
        // Zero is "not given" once the resolution at the end sees it.
        assert_eq!(parse_port_name(b"0"), 0);
        // A name that matches nothing is the same as nothing.
        assert_eq!(parse_port_name(b"nosuchservice"), 0);
        assert_eq!(parse_port_name(b""), 0);
    }

    /// `atoi` stops at the first non-digit rather than rejecting the string,
    /// and the table is consulted with the whole of it first.
    #[test]
    fn a_number_with_a_tail_is_the_number() {
        assert_eq!(parse_port_from_buf(b"23x"), 23);
        assert_eq!(parse_port_name(b"23x"), 23);
        // A name with a numeric tail is a name first: `x-server` is 6000.
        assert_eq!(parse_port_name(b"x-server"), 6000);
    }

    #[test]
    fn scanf_int_is_what_decides_a_setting_was_given() {
        assert_eq!(scanf_int(b"42"), Some(42));
        assert_eq!(scanf_int(b"  -7"), Some(-7));
        assert_eq!(scanf_int(b""), None);
        assert_eq!(scanf_int(b"x9"), None);
        assert_eq!(scanf_int(b"9x"), Some(9));
    }
}
