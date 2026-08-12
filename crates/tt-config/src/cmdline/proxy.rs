//! The third parser on the command line, which upstream keeps in a second
//! plugin — and which runs **before** the other two.
//!
//! `TTProxy`'s `TTXParseParam` (`TTProxy.h:97`) hooks `_ParseParam` exactly as
//! TTSSH's does and blanks out what it consumed, so all three compose through
//! the line. It is the outermost of them: `TTXInternalGetSetupHooks` installs
//! the plugins' hooks from the **end** of the table (`ttplug.cpp:664`), so the
//! last one to hook is the first one called — and TTProxy's `TTXExports` order
//! is 10 against TTSSH's 2500, with the comment `/* load first */` beside it.
//!
//! It takes three forms and only two of them are switches:
//!
//! - `/proxy=<url>`, which sets the proxy and **throws the real host away** —
//!   the arm discards `parseURL`'s return value, so anything after the proxy's
//!   `/` is lost.
//! - `/noproxy`, which upstream's own comment calls an alias for
//!   `-proxy=none://` and which is implemented as exactly that string.
//! - a bare `<scheme>://<proxy>/<realhost>` token, which sets the proxy *and*
//!   remembers the host. That host is applied to `ts.HostName` after the
//!   ordinary parser has run and **only if it found none** (`TTProxy.h:181`),
//!   which is why [`super::parse_all`] does it last.
//!
//! Four things here are easy to get wrong by reading quickly:
//!
//! - **`proxy` is matched case-insensitively, and the `=` is at a fixed
//!   offset.** `wcslen(option+1) >= 6 && option[6] == '='` and then `_wcsicmp`
//!   (`TTProxy.h:138`), so `/PROXY=` works where TTSSH's `/SSH` silently does
//!   nothing. `/noproxy` is `_wcsicmp` too.
//! - **An unrecognised scheme leaves the configured proxy alone.**
//!   `ProxyInfo::parse` returns NULL without a `://` or without a name in its
//!   own table, and `parseURL` assigns `defaultProxy` only when it got
//!   something back — which is the whole reason an ordinary `myhost` token
//!   does not clear the proxy. It is why [`ProxyOptions::proxy`] is an
//!   `Option`.
//! - **A URL that parsed and then had nothing after its `/` switches the proxy
//!   off.** `parseURL` tests `realhost.length() == 0` and assigns `TYPE_NONE`
//!   over the type it just read (`ProxyWSockHook.h:2143`), so
//!   `/proxy=socks5://p:1080/` is *no proxy* where `/proxy=socks5://p:1080` is
//!   a SOCKS5 one. The trailing slash is the whole difference, and nothing
//!   says a word. That is the thirty-seventh defect in `PLAN.md` and the one
//!   thing here this port does not reproduce — see [`ProxyOptions::url`].
//! - **A bare URL with no `/realhost` also switches it off**, by the arm above
//!   it: what is returned is then the whole URL, `://` and all, and
//!   `parseURL(url, FALSE)` reads that as "this was not a proxy URL after
//!   all". Upstream's documentation lists that form under "isn't supported",
//!   because it collides with Tera Term's own `telnet://host` — so it is a
//!   trap rather than a defect, and the token is left in the line for Tera
//!   Term to make what it can of.
//!
//! The first of TTProxy's two loops is not reproduced: it reads `/F=` so that a
//! settings file's `[TTProxy]` section is loaded before `/proxy=` overrides it,
//! and here that file is [`super::CommandLine::setup_file`] and the caller's to
//! load. The ordering rule survives it — apply the command line *over* the
//! settings, which is what [`ProxyOptions::apply`] is for.

use super::{apply_edits, eq_ci, find, percent_decode, switch_body, token_spans, Action};
use crate::{ProxyType, Settings};

/// The proxy a URL described — `ProxyInfo` (`ProxyWSockHook.h:118`) minus the
/// fields no URL can carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Proxy {
    pub kind: ProxyType,
    /// Empty when the type did not need one, which is `none://` and `ssl://`.
    pub host: Vec<u8>,
    /// **Zero until a colon was seen** — `ProxyInfo():type(TYPE_NONE), port(0)`
    /// (`:437`), and the port is parsed only in the arm a colon reaches. Zero
    /// means "the default for this type" here; upstream it means the first of
    /// the four defects listed in `crates/tt-conn/src/proxy.rs`.
    pub port: u16,
    /// Percent-decoded, unlike the same credentials given as settings.
    pub user: Option<Vec<u8>>,
    pub pass: Option<Vec<u8>>,
}

/// What TTProxy took out of a command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyOptions {
    /// `instance().defaultProxy`, assigned only by a URL that parsed. `None`
    /// leaves the settings file's proxy exactly as it was.
    pub proxy: Option<Proxy>,
    /// `getInstance().realhost` — the host recovered from the tail of a bare
    /// URL, which [`super::parse_all`] applies when the ordinary parser found
    /// no host of its own.
    ///
    /// Upstream copies it into `ts.HostName` verbatim, so a `realhost:23`
    /// keeps its colon and is looked up as a name with a colon in it. Its own
    /// documentation lists that form as invalid; reproduced, since the
    /// alternative is a command line that works here and nowhere else.
    pub host: Option<Vec<u8>>,
}

impl ProxyOptions {
    /// Apply over the settings, which is `instance().defaultProxy = proxy`.
    ///
    /// **The whole record is replaced**, so a `/proxy=` naming no credentials
    /// clears a `ProxyUser` and `ProxyPass` the file had — the parsed
    /// `ProxyInfo` is a fresh one and its user and pass are NULL. Same for the
    /// port: a URL with no `:port` leaves zero behind, which this port reads as
    /// the default for the type.
    pub fn apply(&self, settings: &mut Settings) {
        let Some(p) = &self.proxy else {
            return;
        };
        let text = |v: &[u8]| String::from_utf8_lossy(v).into_owned();
        settings.proxy_type = p.kind;
        settings.proxy_host = text(&p.host);
        settings.proxy_port = i32::from(p.port);
        settings.proxy_user = p.user.as_deref().map(text).unwrap_or_default();
        settings.proxy_pass = p.pass.as_deref().map(text).unwrap_or_default();
    }
}

/// `TTXParseParam` — the options, and the line with them taken out.
///
/// One pass, where TTSSH has two: upstream's first loop here reads `/F=` and
/// consumes nothing, so there is nothing for it to leave behind.
pub fn parse(line: &[u8]) -> (ProxyOptions, Vec<u8>) {
    let mut opts = ProxyOptions::default();
    let mut line = line.to_vec();
    let edits: Vec<_> = token_spans(&line, line.len() + 1)
        .into_iter()
        .filter_map(|(span, tok)| match opts.token(&tok) {
            Action::Keep => None,
            Action::Clear => Some((span, None)),
            Action::Replace(text) => Some((span, Some(text))),
        })
        .collect();
    apply_edits(&mut line, edits);
    (opts, line)
}

/// The same over arguments the platform has already split, returning the tokens
/// that survived — see [`super::ssh::parse_args`] for why the two halves
/// compose through the token list rather than through a rebuilt line.
pub fn parse_args<I, S>(args: I) -> (ProxyOptions, Vec<Vec<u8>>)
where
    I: IntoIterator<Item = S>,
    S: AsRef<[u8]>,
{
    let mut opts = ProxyOptions::default();
    let toks = args
        .into_iter()
        .filter_map(|a| {
            let tok = a.as_ref().to_vec();
            match opts.token(&tok) {
                Action::Keep => Some(tok),
                Action::Clear => None,
                Action::Replace(text) => Some(text.trim_ascii().to_vec()),
            }
        })
        .collect();
    (opts, toks)
}

impl ProxyOptions {
    /// One token of the second loop (`TTProxy.h:133`).
    fn token(&mut self, tok: &[u8]) -> Action {
        if let Some(body) = switch_body(tok) {
            // `wcslen(option + 1) >= 6 && option[6] == '='`, which is the `=`
            // at a fixed offset rather than a prefix test — so this arm can be
            // reached by exactly one five-letter word, and `/proxyx=` falls
            // past it to the `noproxy` test rather than into it.
            if body.len() >= 6 && body[5] == b'=' {
                if eq_ci(&body[..5], b"proxy") {
                    // `action = OPTION_CLEAR` sits outside the parse, so
                    // `/proxy=` with nothing behind it is consumed and changes
                    // nothing — and the real host a URL carried is discarded
                    // here, the return value going nowhere.
                    self.url(&body[6..], true);
                    return Action::Clear;
                }
            } else if eq_ci(body, b"noproxy") {
                self.url(b"none://", true);
                return Action::Clear;
            }
            return Action::Keep;
        }

        // Anything that is not a switch is offered to the URL parser, which is
        // how every ordinary host name on every command line reaches this code
        // and is declined by it.
        let Some(realhost) = self.url(tok, false) else {
            return Action::Keep;
        };
        let whole_url = find(&realhost, b"://").is_some();
        self.host = Some(realhost);
        match whole_url {
            // "-proxy= なしで、proto://proxy:port/ 以降の実ホストが含まれていない"
            // — left in the line for Tera Term to have a go at.
            true => Action::Replace(tok.to_vec()),
            false => Action::Clear,
        }
    }

    /// `parseURL` (`ProxyWSockHook.h:2135`) — parse, then decide whether what
    /// came back was a real host at all, then assign.
    ///
    /// `prefix` is upstream's, and it means "this came from `/proxy=`". Without
    /// it a URL that yielded no real host is read as not having been a proxy
    /// URL, and the type is thrown away — but the record is assigned either
    /// way, which is what makes that form *disable* a configured proxy rather
    /// than leave it alone.
    ///
    /// **The one divergence in this file is the second of those two arms.**
    /// Upstream throws the type away for an *empty* real host as well, without
    /// consulting `prefix` (`ProxyWSockHook.h:2143`) — so
    /// `/proxy=socks5://p:1080/` is no proxy where `/proxy=socks5://p:1080` is
    /// a SOCKS5 one, a trailing slash apart and in silence. That is the
    /// thirty-seventh defect on the list in `PLAN.md`, and it is the whole
    /// point of `prefix` that the two callers are not the same: `/proxy=`
    /// discards the real host, so testing it there is testing a value the arm
    /// has already decided not to use. The harm is one-sided — a launcher
    /// script written with the slash silently connects direct — and no
    /// documented form of the option has a trailing slash, so nothing means
    /// "disable" by it. `-noproxy` and `-proxy=none://` are how that is said.
    fn url(&mut self, url: &[u8], prefix: bool) -> Option<Vec<u8>> {
        let (mut proxy, realhost) = parse_info(url)?;
        if !prefix && find(&realhost, b"://").is_some() {
            proxy.kind = ProxyType::None;
        }
        let out = match realhost.is_empty() {
            true => {
                if !prefix {
                    proxy.kind = ProxyType::None;
                }
                None
            }
            false => Some(realhost),
        };
        self.proxy = Some(proxy);
        out
    }
}

/// `ProxyInfo::parse` (`ProxyWSockHook.h:271`) — the proxy, and the real host
/// behind it.
///
/// `None` is upstream's NULL, and it means *change nothing*: no `://`, a scheme
/// nobody recognises, a type that needs a host and has not got one, or a port
/// that will not parse.
fn parse_info(url: &[u8]) -> Option<(Proxy, Vec<u8>)> {
    let scheme = find(url, b"://")?;
    let (kind, needs_host) = parse_type(&url[..scheme])?;
    let mut proxy = Proxy {
        kind,
        ..Proxy::default()
    };
    let rest = &url[scheme + 3..];

    // The credentials are looked for in the **whole** remainder, `/realhost`
    // and all — `String(start)` is built before the host is parsed — so the
    // first `@` anywhere wins and `socks5://p:1080/user@host` has a username
    // of `p` and a password of `1080/user`.
    let mut p = 0;
    if let Some(at) = rest.iter().position(|&b| b == b'@') {
        let creds = &rest[..at];
        match creds.iter().position(|&b| b == b':') {
            None => proxy.user = Some(percent_decode(creds)),
            Some(colon) => {
                proxy.user = Some(percent_decode(&creds[..colon]));
                proxy.pass = Some(percent_decode(&creds[colon + 1..]));
            }
        }
        p = at + 1;
    }

    // The host, up to the first `/`. Every unbracketed colon assigns it again
    // and moves the start past itself, so it is the *last* colon that divides
    // host from port and `a:b:c` is a host of `b`. A `[…]` is stripped of its
    // brackets, but only in this arm — the no-colon case below keeps them,
    // which is upstream's and is why `socks5://[::1]/h` is a name nothing will
    // resolve.
    let mut start = p;
    let mut host = None;
    let mut bracket = false;
    while p < rest.len() && rest[p] != b'/' {
        if rest[p] == b'[' {
            bracket = true;
        } else if bracket && rest[p] == b']' {
            bracket = false;
        } else if !bracket && rest[p] == b':' {
            let piece = &rest[start..p];
            host = Some(match piece.first() {
                Some(b'[') => piece.get(1..piece.len().saturating_sub(1)).unwrap_or(b""),
                _ => piece,
            });
            start = p + 1;
        }
        p += 1;
    }
    if let Some(h) = host {
        proxy.host = h.to_vec();
    }

    // `none://` and `ssl://` skip this entirely, so they need neither a host
    // nor a parseable port and keep whatever the loop happened to leave.
    if needs_host {
        match host {
            None if start >= p => return None,
            None => proxy.host = rest[start..p].to_vec(),
            Some(_) => proxy.port = parse_port(&rest[start..p])?,
        }
    }

    // Everything after the first `/`, or — with no `/` at all — the URL itself,
    // which is the answer that makes a bare token disable the proxy.
    Some(match p < rest.len() {
        true => (proxy, rest[p + 1..].to_vec()),
        false => (proxy, url.to_vec()),
    })
}

/// `parseType` (`ProxyWSockHook.h:161`) — lower-cased and matched *exactly*
/// against upstream's own table. The `bool` is
/// `type != TYPE_NONE_FORCE && type != TYPE_SSL`, the test that decides whether
/// a host is required at all.
///
/// **The six SSL spellings are recognised and resolve to no proxy**, which is
/// two upstream facts at once. They parse into types upstream has enum values
/// for, so they clear a configured proxy exactly as the others do — but the
/// relay `switch` has no arm for any of them and they reach `default: result =
/// 0`, connected with no handshake performed, because `SSLSocket.h` sits in the
/// tree included by nothing. That is the thirty-sixth defect and it is not
/// reproduced: here they are a direct connection to the host the user named,
/// which is also what the settings schema does with the same spellings.
fn parse_type(s: &[u8]) -> Option<(ProxyType, bool)> {
    const TABLE: &[(&[u8], ProxyType, bool)] = &[
        (b"http", ProxyType::Http, true),
        (b"socks", ProxyType::Socks5, true),
        (b"socks4", ProxyType::Socks4, true),
        (b"telnet", ProxyType::Telnet, true),
        (b"socks5", ProxyType::Socks5, true),
        (b"none", ProxyType::None, false),
        (b"http+ssl", ProxyType::None, true),
        (b"socks+ssl", ProxyType::None, true),
        (b"socks4+ssl", ProxyType::None, true),
        (b"telnet+ssl", ProxyType::None, true),
        (b"socks5+ssl", ProxyType::None, true),
        (b"ssl", ProxyType::None, false),
        (b"none+ssl", ProxyType::None, false),
    ];
    let s = s.to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(name, ..)| *name == s.as_slice())
        .map(|&(_, kind, needs_host)| (kind, needs_host))
}

/// `parsePort` (`ProxyWSockHook.h:196`) — decimal, and stricter than it looks.
///
/// The first character must be `1`..`9`, so an **empty field and a leading zero
/// are both refused**: `socks5://p:0080/h` is not port 80, it is a URL that
/// does not parse, which leaves the proxy alone. (Upstream's later `digit == 0`
/// test is unreachable behind that first one.)
fn parse_port(s: &[u8]) -> Option<u16> {
    if !matches!(s.first(), Some(b'1'..=b'9')) {
        return None;
    }
    let mut n: u32 = 0;
    for &b in s {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n * 10 + u32::from(b - b'0');
        if n > 65535 {
            return None;
        }
    }
    Some(n as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(line: &str) -> (ProxyOptions, String) {
        let (o, rest) = parse(line.as_bytes());
        (o, String::from_utf8_lossy(&rest).into_owned())
    }

    fn proxy(line: &str) -> Proxy {
        opts(line).0.proxy.expect("the line named a proxy")
    }

    fn text(v: &Option<Vec<u8>>) -> String {
        String::from_utf8_lossy(v.as_deref().unwrap_or_default()).into_owned()
    }

    /// The documented form, and the option is taken out of the line rather than
    /// merely read out of it.
    #[test]
    fn the_documented_option_sets_the_proxy_and_leaves_the_line_clean() {
        let (o, rest) = opts("ttermpro -proxy=socks5://user:pass@proxy:1080 sshserver /ssh");
        assert_eq!(
            rest,
            "ttermpro                                      sshserver /ssh"
        );
        let p = o.proxy.expect("a proxy");
        assert_eq!(p.kind, ProxyType::Socks5);
        assert_eq!(p.host, b"proxy");
        assert_eq!(p.port, 1080);
        assert_eq!(text(&p.user), "user");
        assert_eq!(text(&p.pass), "pass");
        // Nothing was recovered as a host: `/proxy=` throws that half away.
        assert_eq!(o.host, None);
    }

    /// `/` and `-` both lead, and the word is case-insensitive — which `/SSH`
    /// in the other plugin is not.
    #[test]
    fn either_leader_and_any_case() {
        for line in [
            "tt /proxy=http://p:8080",
            "tt -proxy=http://p:8080",
            "tt /PROXY=http://p:8080",
            "tt -Proxy=HTTP://p:8080",
        ] {
            assert_eq!(proxy(line).kind, ProxyType::Http, "{line}");
        }
    }

    /// `-noproxy` is `-proxy=none://` and upstream implements it as exactly
    /// that string.
    #[test]
    fn noproxy_is_the_alias_its_comment_says_it_is() {
        assert_eq!(proxy("tt -noproxy").kind, ProxyType::None);
        assert_eq!(proxy("tt /NOPROXY"), proxy("tt /proxy=none://"));
        assert_eq!(opts("tt -noproxy").1, "tt         ");
    }

    /// An ordinary host name must not clear a configured proxy, which is the
    /// reason the parser answers `None` rather than a default.
    #[test]
    fn an_ordinary_token_changes_nothing() {
        for line in [
            "tt myhost",
            "tt myhost:22 /ssh",
            "tt ftp://p:21/h",
            "tt /F=x.ini",
        ] {
            let (o, rest) = opts(line);
            assert_eq!(o.proxy, None, "{line}");
            assert_eq!(o.host, None, "{line}");
            assert_eq!(rest, line, "{line}");
        }
    }

    /// The bare form, which is the only one that recovers a host.
    #[test]
    fn a_bare_url_names_the_proxy_and_the_host_behind_it() {
        let (o, rest) = opts("tt socks5://p:1080/realhost");
        assert_eq!(text(&o.host), "realhost");
        let p = o.proxy.expect("a proxy");
        assert_eq!(p.kind, ProxyType::Socks5);
        assert_eq!(p.host, b"p");
        assert_eq!(p.port, 1080);
        // Consumed, because the real host was found: Tera Term is given the
        // name afterwards instead.
        assert_eq!(rest, "tt                         ");
    }

    /// Upstream's documentation calls this form unsupported. What it does is
    /// switch the proxy off and leave the token for Tera Term to misread.
    #[test]
    fn a_bare_url_with_no_real_host_switches_the_proxy_off() {
        let (o, rest) = opts("tt socks5://p:1080");
        assert_eq!(o.proxy.expect("assigned anyway").kind, ProxyType::None);
        assert_eq!(text(&o.host), "socks5://p:1080");
        // OPTION_REPLACE, with the option unchanged: the span is exactly the
        // token and the separator in front of it, and the text goes back one
        // character in — so the line comes out identical to the one that went
        // in, which is what makes rewriting a token in place affordable.
        assert_eq!(rest, "tt socks5://p:1080");
    }

    /// The thirty-seventh defect, and the file's one divergence: upstream reads
    /// the trailing slash as "no proxy", where the slash is the only difference
    /// between these two lines and neither documented form has one.
    #[test]
    fn a_trailing_slash_does_not_disable_the_proxy_here() {
        assert_eq!(proxy("tt /proxy=socks5://p:1080").kind, ProxyType::Socks5);
        assert_eq!(proxy("tt /proxy=socks5://p:1080/").kind, ProxyType::Socks5);
        assert_eq!(proxy("tt /proxy=socks5://p:1080/").host, b"p");
        assert_eq!(proxy("tt /proxy=socks5://p:1080/").port, 1080);
        // The real host after it is still discarded, which is upstream's and is
        // not the same defect: `/proxy=` throws `parseURL`'s answer away.
        assert_eq!(opts("tt /proxy=socks5://p:1080/realhost").0.host, None);
        // And the bare form is untouched, because there the empty answer is
        // about a token Tera Term is going to see: `socks5://p:1080/` names no
        // host, so it names nothing, and the proxy goes off exactly as
        // upstream's does.
        assert_eq!(
            opts("tt socks5://p:1080/").0.proxy.expect("assigned").kind,
            ProxyType::None
        );
    }

    /// `/proxy=` with nothing behind it is consumed and changes nothing —
    /// the clear is not conditional on the URL having parsed.
    #[test]
    fn an_empty_url_is_consumed_and_does_nothing() {
        let (o, rest) = opts("tt /proxy= myhost");
        assert_eq!(o.proxy, None);
        assert_eq!(rest, "tt         myhost");
    }

    /// Every spelling in upstream's table, including the six whose relay does
    /// nothing at all.
    #[test]
    fn the_scheme_table_is_upstreams() {
        for (scheme, kind) in [
            ("http", ProxyType::Http),
            ("socks", ProxyType::Socks5),
            ("socks5", ProxyType::Socks5),
            ("socks4", ProxyType::Socks4),
            ("telnet", ProxyType::Telnet),
            ("http+ssl", ProxyType::None),
            ("socks+ssl", ProxyType::None),
            ("socks4+ssl", ProxyType::None),
            ("socks5+ssl", ProxyType::None),
            ("telnet+ssl", ProxyType::None),
        ] {
            let p = proxy(&format!("tt /proxy={scheme}://p:1080"));
            assert_eq!(p.kind, kind, "{scheme}");
            assert_eq!(p.host, b"p", "{scheme}");
        }
        // `none` and `ssl` are the two that need no host at all.
        for scheme in ["none", "ssl", "none+ssl"] {
            let p = proxy(&format!("tt /proxy={scheme}://"));
            assert_eq!(p.kind, ProxyType::None, "{scheme}");
            assert!(p.host.is_empty(), "{scheme}");
        }
        // And one that is in no table: nothing is assigned.
        assert_eq!(opts("tt /proxy=sock5://p:1080").0.proxy, None);
    }

    /// The `@` is looked for in the whole remainder, so the credentials can eat
    /// the path.
    #[test]
    fn the_credentials_are_searched_for_past_the_real_host() {
        let p = proxy("tt /proxy=socks5://p:1080/user@host");
        assert_eq!(text(&p.user), "p");
        assert_eq!(text(&p.pass), "1080/user");
        assert_eq!(p.host, b"host");
        assert_eq!(p.port, 0);
    }

    /// A password may hold a colon; a username may not.
    #[test]
    fn the_first_colon_divides_the_credentials() {
        let p = proxy("tt /proxy=http://u:a:b@p:80");
        assert_eq!(text(&p.user), "u");
        assert_eq!(text(&p.pass), "a:b");
    }

    /// Percent-decoded, which the same credentials given as settings are not.
    #[test]
    fn the_credentials_are_percent_decoded() {
        let p = proxy("tt /proxy=http://a%40b:p%3Aw%25@p:80");
        assert_eq!(text(&p.user), "a@b");
        assert_eq!(text(&p.pass), "p:w%");
    }

    /// A URL naming no credentials clears the ones the file had, because the
    /// whole record is replaced.
    #[test]
    fn a_url_with_no_credentials_clears_the_files() {
        let mut s = Settings {
            proxy_user: "from-the-file".into(),
            proxy_pass: "also".into(),
            ..Settings::default()
        };
        opts("tt /proxy=socks5://p:1080").0.apply(&mut s);
        assert_eq!(s.proxy_type, ProxyType::Socks5);
        assert_eq!(s.proxy_host, "p");
        assert_eq!(s.proxy_port, 1080);
        assert_eq!(s.proxy_user, "");
        assert_eq!(s.proxy_pass, "");
        // ...and a line that named no proxy at all leaves everything alone.
        let mut s = Settings {
            proxy_user: "from-the-file".into(),
            ..Settings::default()
        };
        opts("tt myhost").0.apply(&mut s);
        assert_eq!(s.proxy_user, "from-the-file");
    }

    /// The last colon divides host from port; the ones before it just move the
    /// host along.
    #[test]
    fn the_host_is_reassigned_at_every_colon() {
        assert_eq!(proxy("tt /proxy=http://a:b:80").host, b"b");
        assert_eq!(proxy("tt /proxy=http://a:b:80").port, 80);
    }

    /// A bracketed IPv6 literal loses its brackets when a port follows it and
    /// keeps them when one does not — which is upstream's, and the second half
    /// is a name nothing resolves.
    #[test]
    fn a_v6_literal_is_stripped_only_when_a_port_follows() {
        let p = proxy("tt /proxy=socks5://[::1]:1080");
        assert_eq!(p.host, b"::1");
        assert_eq!(p.port, 1080);
        assert_eq!(proxy("tt /proxy=socks5://[::1]").host, b"[::1]");
    }

    /// A port that will not parse is not a clamp and not a default: the whole
    /// URL is refused and the settings are left alone.
    #[test]
    fn a_bad_port_leaves_everything_alone() {
        for line in [
            "tt /proxy=http://p:0080",
            "tt /proxy=http://p:",
            "tt /proxy=http://p:0",
            "tt /proxy=http://p:65536",
            "tt /proxy=http://p:80a",
            "tt /proxy=http://",
        ] {
            assert_eq!(opts(line).0.proxy, None, "{line}");
        }
        assert_eq!(proxy("tt /proxy=http://p:65535").port, 65535);
    }

    /// A five-letter word and a fixed `=` position, so nothing else reaches the
    /// arm and `noproxy` is tested only when it did not.
    #[test]
    fn only_one_word_reaches_the_option() {
        for line in [
            "tt /proxyx=http://p:80",
            "tt /prox=http://p:80",
            "tt /noproxy=x",
        ] {
            let (o, rest) = opts(line);
            assert_eq!(o.proxy, None, "{line}");
            assert_eq!(rest, line, "{line}");
        }
    }

    /// The argument form, which has no line to blank.
    #[test]
    fn arguments_compose_through_the_token_list() {
        let (o, rest) = parse_args([&b"/proxy=http://p:8080"[..], b"myhost", b"/ssh"]);
        assert_eq!(o.proxy.expect("a proxy").kind, ProxyType::Http);
        assert_eq!(rest, vec![b"myhost".to_vec(), b"/ssh".to_vec()]);
    }
}
